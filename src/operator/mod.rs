use cloudflare::framework::response::ApiFailure;

pub mod controller;
pub mod crd;
mod resources;
mod tunnel_controller;
mod tunnel_ingress_controller;

/// All errors possible to occur during reconciliation
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // Any error originating from the `kube-rs` crate
    #[error("Kubernetes reported error: {0}")]
    KubeError(#[from] kube::Error),
    // Any error that the cloudflare api returns
    #[error("Cloudflare api returned an error {0}")]
    CloudflareApiFailure(#[from] ApiFailure),
    #[error("missing namespace for resource {0}")]
    MissingNamespace(&'static str),
    #[error("Missing credentials CRD {0}")]
    MissingCredentials(String),
}
