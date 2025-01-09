use crate::crd::credentials::{self, Credentials as CredentialsCrd};
use async_trait::async_trait;
use cloudflare::{
    endpoints::cfd_tunnel::{
        create_tunnel, delete_tunnel, get_tunnel, get_tunnel_token, update_configuration,
        ConfigurationSrc, Tunnel, TunnelConfiguration, TunnelToken,
    },
    framework::{
        auth::Credentials,
        endpoint::Endpoint,
        response::{ApiErrors, ApiFailure, ApiResponse, ApiResult, ApiSuccess},
        Environment,
    },
};
use uuid::Uuid;

pub struct Auth {
    account_id: String,
    kind: Credentials,
}

impl From<CredentialsCrd> for Auth {
    fn from(s: CredentialsCrd) -> Auth {
        let account_id = s.spec.account_id;
        let kind = match s.spec.auth {
            credentials::AuthKind::ServiceKey(key) => Credentials::Service { key },
            credentials::AuthKind::UserAuthKey { email, key } => {
                Credentials::UserAuthKey { email, key }
            }
            credentials::AuthKind::UserAuthToken(token) => Credentials::UserAuthToken { token },
        };

        Auth { account_id, kind }
    }
}

#[async_trait]
pub trait CloudflareTunnel: Send + Sync {
    async fn create_tunnel<'a>(
        &self,
        auth: &Auth,
        name: &str,
        tunnel_secret: Option<&'a [u8]>,
        config_src: ConfigurationSrc,
    ) -> Result<Tunnel, ApiFailure>;
    async fn delete_tunnel(&self, auth: &Auth, tunnel_id: Uuid) -> Result<(), ApiFailure>;
    async fn update_configuration(
        &self,
        auth: &Auth,
        tunnel_id: Uuid,
        config: TunnelConfiguration,
    ) -> Result<Option<TunnelConfiguration>, ApiFailure>;
    async fn get_tunnel_token(
        &self,
        auth: &Auth,
        tunnel_id: &str,
    ) -> Result<TunnelToken, ApiFailure>;
    async fn get_tunnel(&self, auth: &Auth, tunnel_id: &str) -> Result<Tunnel, ApiFailure>;
}

pub struct Client {
    http_client: reqwest::Client,
}

impl Client {
    pub fn try_default() -> anyhow::Result<Self> {
        let headers = reqwest::header::HeaderMap::default();
        let http_client = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;
        Ok(Self { http_client })
    }
}

impl Client {
    async fn request<ResultType: ApiResult>(
        &self,
        credentials: &Credentials,
        endpoint: &(dyn Endpoint<ResultType> + Send + Sync),
    ) -> ApiResponse<ResultType> {
        let mut request = self
            .http_client
            .request(endpoint.method(), endpoint.url(&Environment::Production));

        if let Some(body) = endpoint.body() {
            request = request.body(body);
            request = request.header(
                reqwest::header::CONTENT_TYPE,
                endpoint.content_type().as_ref(),
            );
        }

        let auth = |mut auth: reqwest::RequestBuilder, credentials: &Credentials| {
            for (k, v) in credentials.headers() {
                auth = auth.header(k, v);
            }

            auth
        };

        let request = auth(request, credentials);

        let response = request.send().await?;
        map_api_response(response).await
    }
}

#[async_trait]
impl CloudflareTunnel for Client {
    async fn create_tunnel<'a>(
        &self,
        auth: &Auth,
        name: &str,
        tunnel_secret: Option<&'a [u8]>,
        config_src: ConfigurationSrc,
    ) -> Result<Tunnel, ApiFailure> {
        let params = create_tunnel::Params {
            name,
            tunnel_secret,
            config_src: &config_src,
            metadata: None,
        };

        let endpoint = create_tunnel::CreateTunnel {
            account_identifier: &auth.account_id,
            params,
        };

        match self.request(&auth.kind, &endpoint).await {
            Ok(result) => Ok(result.result),
            Err(err) => Err(err),
        }
    }

    async fn delete_tunnel(&self, auth: &Auth, tunnel_id: Uuid) -> Result<(), ApiFailure> {
        let params = delete_tunnel::Params { cascade: true };

        let tunnel_id = tunnel_id.to_string();
        let endpoint = delete_tunnel::DeleteTunnel {
            account_identifier: &auth.account_id,
            tunnel_id: &tunnel_id,
            params,
        };

        match self.request(&auth.kind, &endpoint).await {
            Ok(_) => Ok(()),
            Err(err) => Err(err),
        }
    }

    async fn update_configuration(
        &self,
        auth: &Auth,
        tunnel_id: Uuid,
        config: TunnelConfiguration,
    ) -> Result<Option<TunnelConfiguration>, ApiFailure> {
        let params = update_configuration::Params { config };

        let endpoint = update_configuration::UpdateTunnelConfiguration {
            account_identifier: &auth.account_id,
            tunnel_id,
            params,
        };

        match self.request(&auth.kind, &endpoint).await {
            Ok(res) => Ok(res.result.config),
            Err(err) => Err(err),
        }
    }

    async fn get_tunnel_token(
        &self,
        auth: &Auth,
        tunnel_id: &str,
    ) -> Result<TunnelToken, ApiFailure> {
        let endpoint = get_tunnel_token::TunnelToken {
            account_identifier: &auth.account_id,
            tunnel_id,
        };

        match self.request::<TunnelToken>(&auth.kind, &endpoint).await {
            Ok(res) => Ok(res.result),
            Err(err) => Err(err),
        }
    }

    async fn get_tunnel(&self, auth: &Auth, tunnel_id: &str) -> Result<Tunnel, ApiFailure> {
        let endpoint = get_tunnel::GetTunnel {
            account_identifier: &auth.account_id,
            tunnel_id,
        };

        match self.request::<Tunnel>(&auth.kind, &endpoint).await {
            Ok(res) => Ok(res.result),
            Err(err) => Err(err),
        }
    }
}

async fn map_api_response<ResultType: ApiResult>(
    resp: reqwest::Response,
) -> ApiResponse<ResultType> {
    let status = resp.status();
    if status.is_success() {
        let parsed: Result<ApiSuccess<ResultType>, reqwest::Error> = resp.json().await;
        match parsed {
            Ok(api_resp) => Ok(api_resp),
            Err(e) => Err(ApiFailure::Invalid(e)),
        }
    } else {
        let parsed: Result<ApiErrors, reqwest::Error> = resp.json().await;
        let errors = parsed.unwrap_or_default();
        Err(ApiFailure::Error(status, errors))
    }
}
