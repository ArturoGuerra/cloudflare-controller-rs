use cloudflare::framework::{
    auth::Credentials,
    endpoint::Endpoint,
    response::{ApiErrors, ApiFailure, ApiResponse, ApiResult, ApiSuccess},
    Environment,
};

pub mod auth;
pub mod tunnel;

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
