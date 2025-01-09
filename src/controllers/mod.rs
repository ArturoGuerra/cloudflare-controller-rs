use std::future::IntoFuture;

pub mod ingress;
pub mod tunnel;

pub use tunnel::TunnelController;

#[allow(async_fn_in_trait)]
pub trait Controller {
    async fn try_default() -> anyhow::Result<impl Controller + IntoFuture>;
}
