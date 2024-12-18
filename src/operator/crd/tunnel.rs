use crate::operator::controller::Context;
use kube::api::{Patch, PatchParams};
use kube::{Api, CustomResource};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

const FINALIZER_NAME: &str = "tunnel.cloudflare.ar2ro.io/finalizer";

#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[kube(
    group = "cloudflare.ar2ro.io",
    version = "v1",
    kind = "Tunnel",
    doc = "Custom resource representation of a Cloudflare Tunnel",
    scale = r#"{"specReplicasPath":".spec.replicas", "statusReplicasPath":".status.replicas"}"#,
    namespaced
)]
pub struct TunnelCrd {
    pub uuid: Option<Uuid>,
    pub replicas: i32,
    pub credentials: String,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub tunnel_secret: Option<String>,
    pub tags: Option<HashMap<String, String>>,
}

pub async fn add_finalizer(
    name: &str,
    namespace: &str,
    context: Arc<Context>,
) -> Result<Tunnel, kube::Error> {
    let tunnel_api: Api<Tunnel> = Api::namespaced(context.kubernetes_client.clone(), namespace);

    let patch: Value = json!({
        "metadata": {
            "finalizers": [FINALIZER_NAME]
        }
    });

    let patch: Patch<&Value> = Patch::Merge(&patch);
    match tunnel_api
        .patch(name, &PatchParams::default(), &patch)
        .await
    {
        Ok(tunnel) => Ok(tunnel),
        Err(err) => Err(err),
    }
}

pub async fn remove_finalizer(
    name: &str,
    namespace: &str,
    context: Arc<Context>,
) -> Result<(), kube::Error> {
    let tunnel_api: Api<Tunnel> = Api::namespaced(context.kubernetes_client.clone(), namespace);

    let patch: Value = json!({
        "metadata": {
            "finalizers": null,
       }
    });

    let patch: Patch<&Value> = Patch::Merge(&patch);

    match tunnel_api
        .patch(name, &PatchParams::default(), &patch)
        .await
    {
        Ok(_) => Ok(()),
        Err(err) => Err(err),
    }
}
