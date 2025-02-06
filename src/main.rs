use cloudflare::Client as CloudflareClient;
use kube::Client as K8sClient;
use kube::CustomResourceExt;
use std::fs::File;
use std::io::Write;

use futures::try_join;

pub mod cloudflare;
pub mod controllers;
pub mod crd;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let kubernetes_client = K8sClient::try_default().await?;
    let cloudflare_client = CloudflareClient::try_default()?;

    let tunnel_controller =
        controllers::TunnelController::try_new(kubernetes_client.clone()).await?;
    let ingress_controller =
        controllers::IngressController::try_new(kubernetes_client.clone(), cloudflare_client)
            .await?;

    try_join!(ingress_controller.start(), tunnel_controller.start())?;

    Ok(())
}
