use async_trait::async_trait;
use cloudflare::{
    endpoints::cfd_tunnel::{
        create_tunnel, delete_tunnel, get_configuration, get_tunnel_token, update_configuration,
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

#[async_trait]
pub trait CloudflareTunnel: Send + Sync {
    async fn create_tunnel<'a>(
        &self,
        account_id: &str,
        credentials: &Credentials,
        name: &str,
        tunnel_secret: Option<&'a [u8]>,
        config_src: ConfigurationSrc,
    ) -> anyhow::Result<Tunnel>;
    async fn delete_tunnel(
        &self,
        account_id: &str,
        credentials: &Credentials,
        tunnel_id: Uuid,
    ) -> anyhow::Result<()>;
    async fn update_configuration(
        &self,
        account_id: &str,
        credentials: &Credentials,
        tunnel_id: Uuid,
        config: TunnelConfiguration,
    ) -> anyhow::Result<Option<TunnelConfiguration>>;
    async fn get_tunnel_token(
        &self,
        account_id: &str,
        credentials: &Credentials,
        tunnel_id: &str,
    ) -> anyhow::Result<TunnelToken>;
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
        account_id: &str,
        credentials: &Credentials,
        name: &str,
        tunnel_secret: Option<&'a [u8]>,
        config_src: ConfigurationSrc,
    ) -> anyhow::Result<Tunnel> {
        let params = create_tunnel::Params {
            name,
            tunnel_secret,
            config_src: &config_src,
            metadata: None,
        };

        let endpoint = create_tunnel::CreateTunnel {
            account_identifier: account_id,
            params,
        };

        match self.request(credentials, &endpoint).await {
            Ok(result) => Ok(result.result),
            Err(err) => Err(err.into()),
        }
    }

    async fn delete_tunnel(
        &self,
        account_id: &str,
        credentials: &Credentials,
        tunnel_id: Uuid,
    ) -> anyhow::Result<()> {
        let params = delete_tunnel::Params { cascade: true };

        let tunnel_id = tunnel_id.to_string();
        let endpoint = delete_tunnel::DeleteTunnel {
            account_identifier: account_id,
            tunnel_id: &tunnel_id,
            params,
        };

        match self.request(credentials, &endpoint).await {
            Ok(_) => Ok(()),
            Err(err) => Err(err.into()),
        }
    }

    async fn update_configuration(
        &self,
        account_id: &str,
        credentials: &Credentials,
        tunnel_id: Uuid,
        config: TunnelConfiguration,
    ) -> anyhow::Result<Option<TunnelConfiguration>> {
        let params = update_configuration::Params { config };

        let endpoint = update_configuration::UpdateTunnelConfiguration {
            account_identifier: account_id,
            tunnel_id,
            params,
        };

        match self.request(credentials, &endpoint).await {
            Ok(res) => Ok(res.result.config),
            Err(err) => Err(err.into()),
        }
    }

    async fn get_tunnel_token(
        &self,
        account_id: &str,
        credentials: &Credentials,
        tunnel_id: &str,
    ) -> anyhow::Result<TunnelToken> {
        let endpoint = get_tunnel_token::TunnelToken {
            account_identifier: account_id,
            tunnel_id,
        };

        match self.request::<TunnelToken>(credentials, &endpoint).await {
            Ok(res) => Ok(res.result),
            Err(err) => Err(err.into()),
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
        Err(ApiFailure::Error(status.into(), errors))
    }
}
