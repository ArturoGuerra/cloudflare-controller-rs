use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OriginRequestAccess {
    pub aud_tag: Vec<String>,
    pub required: bool,
    pub team_name: String,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OriginRequest {
    pub access: Option<OriginRequestAccess>,
    pub ca_pool: Option<String>,
    pub connection_timeout: i32,
    pub disable_chunked_encoding: bool,
    pub http2origin: bool,
    pub http_host_header: Option<String>,
    pub keep_alive_connections: i32,
    pub keep_alive_timeout: i32,
    pub no_happy_eyeballs: bool,
    pub no_tls_verify: bool,
    pub origin_server_name: Option<String>,
    pub proxy_type: Option<String>,
    pub tcp_keep_alive: i32,
    pub tls_timeout: i32,
}

#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[kube(
    group = "cloudflare.ar2ro.io",
    version = "v1",
    kind = "TunnelIngress",
    doc = "Custom resource representation of a Cloudflare Tunnel Ingress Rule",
    singular = "tunnelingress",
    plural = "tunnelingress",
    selectable = ".spec.tunnel",
    namespaced
)]
pub struct TunnelIngressCrd {
    pub tunnel: String,
    pub hostname: Option<String>,
    pub origin_request: Option<OriginRequest>,
    pub path: Option<String>,
    pub service: String,
}
