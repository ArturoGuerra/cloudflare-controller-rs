use futures::{future::select_all, Future};
use kube::Client;
use std::future::IntoFuture;
use std::pin::Pin;

pub mod ingress;
pub mod tunnel;

pub use ingress::IngressController;
pub use tunnel::TunnelController;

#[allow(async_fn_in_trait)]
pub trait Controller {
    async fn try_new(client: Client) -> anyhow::Result<impl Controller + IntoFuture>;
}

async fn start_tunnel_controller(client: Client) {
    let tunnel_controller = TunnelController::try_new(client).await.unwrap();
    tunnel_controller.await;
}

pub async fn start() -> anyhow::Result<()> {
    let client = Client::try_default().await?;
    let futures: Vec<Pin<Box<dyn Future<Output = ()>>>> =
        vec![Box::pin(start_tunnel_controller(client.clone()))];
    select_all(futures).await;
    Ok(())
}
