use crate::Error;
use cloudflare::framework::auth::Credentials as CloudflareCredentials;
use kube::Api;
use kube_derive::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum AuthKind {
    UserAuthToken(String),
    UserAuthKey { email: String, key: String },
    ServiceKey(String),
}

#[derive(CustomResource, Serialize, Deserialize, Debug, PartialEq, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[kube(
    group = "cloudflare.ar2ro.io",
    version = "v1",
    kind = "Credentials",
    plural = "credentials",
    singular = "credentials",
    doc = "Custom resource representation of Cloudflare Credentials",
    derive = "PartialEq",
    scale = r#"{"specReplicasPath":".spec.replicas", "statusReplicasPath":".status.replicas"}"#
)]
pub struct CredentialsCrd {
    pub account_id: String,
    pub auth: AuthKind,
}

#[allow(async_fn_in_trait)]
pub trait CredentialsApiExt {
    async fn get_credentials(&self, name: &str) -> Result<(String, CloudflareCredentials), Error>;
}

impl From<Credentials> for (String, CloudflareCredentials) {
    fn from(item: Credentials) -> (String, CloudflareCredentials) {
        let account_id = item.spec.account_id;

        let credentials = match item.spec.auth {
            AuthKind::UserAuthToken(token) => CloudflareCredentials::UserAuthToken { token },
            AuthKind::UserAuthKey { email, key } => {
                CloudflareCredentials::UserAuthKey { email, key }
            }
            AuthKind::ServiceKey(key) => CloudflareCredentials::Service { key },
        };

        (account_id, credentials)
    }
}

impl CredentialsApiExt for Api<Credentials> {
    async fn get_credentials(&self, name: &str) -> Result<(String, CloudflareCredentials), Error> {
        match self.get_opt(name).await.map_err(Error::KubeError)? {
            Some(credentials) => Ok(credentials.into()),
            None => Err(Error::MissingCredentials(name.to_string())),
        }
    }
}
