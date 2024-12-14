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
