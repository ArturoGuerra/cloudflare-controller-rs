use crate::cftunnel::Client as CloudflareClient;
use kube::client::Client;

pub mod controller;
pub mod crd;
pub mod resources;

mod tunnel_controller;
mod tunnel_ingress_controller;

/// All errors possible to occur during reconciliation
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Any error originating from the `kube-rs` crate
    #[error("Kubernetes reported error: {source}")]
    KubeError {
        #[from]
        source: kube::Error,
    },
    /// Error in user input or Echo resource definition, typically missing fields.
    #[error("Invalid Echo CRD: {0}")]
    UserInputError(String),
    #[error("missing namespace for resource {0}")]
    MissingNamespaceError(&'static str),
    #[error("missing credentials")]
    MissingCredentialsError,
    #[error("error creating tunnel")]
    FailedToCreateTunnelError,
}

pub struct Data {
    cloudflare_client: CloudflareClient,
    client: Client,
}
