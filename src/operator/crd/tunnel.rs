use k8s_openapi::apimachinery::pkg::apis::meta::v1::Condition;
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[kube(
    group = "cloudflare.ar2ro.io",
    version = "v1",
    kind = "CfTunnel",
    doc = "Custom resource representation of a Cloudflare Tunnel",
    status = "TunnelStatus",
    scale = r#"{"specReplicasPath":".spec.replicas", "statusReplicasPath":".status.replicas"}"#,
    namespaced
)]
pub struct TunnelCrd {
    pub replicas: i32,
    pub credentials: String,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub tunnel_secret: Option<String>,
    pub tags: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TunnelStatus {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(schema_with = "conditions")]
    pub conditions: Vec<Condition>,
}

pub fn conditions(_: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
    serde_json::from_value(serde_json::json!({
        "type": "array",
        "x-kubernetes-list-type": "map",
        "x-kubernetes-list-map-keys": ["type"],
        "items": {
            "type": "object",
            "properties": {
                "lastTransitionTime": { "format": "date-time", "type": "string" },
                "message": { "type": "string" },
                "observedGeneration": { "type": "integer", "format": "int64", "default": 0 },
                "reason": { "type": "string" },
                "status": { "type": "string" },
                "type": { "type": "string" }
            },
            "required": [
                "lastTransitionTime",
                "message",
                "reason",
                "status",
                "type"
            ],
        },
    }))
    .unwrap()
}
