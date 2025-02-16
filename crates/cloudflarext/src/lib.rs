use cloudflare::framework::{
    auth::Credentials,
    endpoint::Endpoint,
    response::{ApiErrors, ApiFailure, ApiResponse, ApiResult, ApiSuccess},
    Environment, Error, HttpApiClientConfig,
};

pub mod cfd_tunnel;

trait CredentialsExt {
    fn header_map(&self) -> http::HeaderMap;
}

impl CredentialsExt for Credentials {
    fn header_map(&self) -> http::HeaderMap {
        let mut headers = http::HeaderMap::new();
        match self {
            Credentials::UserAuthKey { email, key } => {
                headers.insert("X-Auth-Email", email.parse().unwrap());
                headers.insert("X-Auth-Key", key.parse().unwrap());
            }
            Credentials::Service { key } => {
                headers.insert("X-Auth-User-Service-Key", key.parse().unwrap());
            }
            Credentials::UserAuthToken { token } => {
                headers.insert(
                    "Authorization",
                    format!("Bearer {}", &token).parse().unwrap(),
                );
            }
        };

        headers
    }
}

pub struct AuthlessClient {
    environment: Environment,
    http_client: reqwest::Client,
}

impl AuthlessClient {
    pub fn try_new(
        config: HttpApiClientConfig,
        environment: Environment,
    ) -> Result<AuthlessClient, Error> {
        let builder = reqwest::Client::builder().default_headers(config.default_headers);
        let http_client = builder.build()?;
        Ok(AuthlessClient {
            environment,
            http_client,
        })
    }

    pub async fn request<ResultType>(
        &self,
        credentials: &Credentials,
        endpoint: &(dyn Endpoint<ResultType> + Send + Sync),
    ) -> ApiResponse<ResultType>
    where
        ResultType: ApiResult,
    {
        let mut request = self
            .http_client
            .request(endpoint.method(), endpoint.url(&self.environment));

        if let Some(body) = endpoint.body() {
            request = request.body(body).header(
                reqwest::header::CONTENT_TYPE,
                endpoint.content_type().as_ref(),
            );
        }

        let response = request.headers(credentials.header_map()).send().await?;
        map_api_response(response).await
    }
}

// If the response is 2XX and parses, return Success.
// If the response is 2XX and doesn't parse, return Invalid.
// If the response isn't 2XX, return Failure, with API errors if they were included.
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
