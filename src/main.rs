use kube::Client;
use kube::CustomResourceExt;
use std::fs::File;
use std::io::Write;

pub mod cloudflare;
pub mod controller;
pub mod crd;
pub mod resources;

use controller::Controller;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    //    let credentials_crd = serde_json::to_string_pretty(&crd::credentials::Credentials::crd())?;
    //
    //    let mut file = File::create("./credentials_crd.yaml").expect("unable to create file");
    //
    //    file.write_all(credentials_crd.into_bytes().as_ref())
    //        .unwrap();
    //
    //    let tunnels_crd = serde_json::to_string_pretty(&crd::tunnel::Tunnel::crd())?;
    //
    //    let mut file = File::create("./tunnels_crd.yaml").expect("unable to create file");
    //
    //    file.write_all(tunnels_crd.into_bytes().as_ref()).unwrap();
    //
    //    let tunnel_ingress_crd =
    //        serde_json::to_string_pretty(&crd::tunnel_configuration::TunnelIngress::crd())?;
    //
    //    let mut file = File::create("./tunnel_ingress_crd.yaml").expect("unable to create file");
    //
    //    file.write_all(tunnel_ingress_crd.into_bytes().as_ref())
    //        .unwrap();

    //controllers::start().await.unwrap();
    let client = Client::try_default().await?;

    let ingress_controller = controller::IngressController::try_new(client).await?;
    ingress_controller.start().await;

    Ok(())
}
