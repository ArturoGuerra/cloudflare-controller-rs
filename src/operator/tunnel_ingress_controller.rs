use kube::runtime::controller::Action;

use crate::operator::controller::Context;
use crate::operator::crd::tunnel_configuration::TunnelIngress;
use crate::operator::Error;
use std::sync::Arc;
use tokio::time::Duration;

pub async fn reconciler(generator: Arc<TunnelIngress>, ctx: Arc<Context>) -> Result<Action, Error> {
    Ok(Action::requeue(Duration::from_secs(30 * 100)))
}

pub fn on_err(generator: Arc<TunnelIngress>, error: &Error, ctx: Arc<Context>) -> Action {
    Action::requeue(Duration::from_secs(30 * 100))
}
