use cloudflare::Client as CloudflareClient;
use kube::Client as K8sClient;
use kube::CustomResourceExt;
use std::fs::File;
use std::io::Write;

pub mod cloudflare;
pub mod controllers;
pub mod crd;
pub mod resources;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let kubernetes_client = K8sClient::try_default().await?;
    let cloudflare_client = CloudflareClient::try_default()?;

    let ingress_controller =
        controllers::IngressController::try_new(kubernetes_client, cloudflare_client).await?;

    ingress_controller.start().await?;

    Ok(())
}
