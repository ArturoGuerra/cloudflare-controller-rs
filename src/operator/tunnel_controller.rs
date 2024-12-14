use cloudflare::endpoints::cfd_tunnel::ConfigurationSrc;
use cloudflare::framework::auth::Credentials;
use k8s_openapi::api::apps::v1::Deployment;
use kube::api::GetParams;
use kube::runtime::controller::Action;
use kube::{Resource, ResourceExt};

use crate::cftunnel::CloudflareTunnel;
use crate::operator::controller::Context;
use crate::operator::crd::credentials::AuthKind;
use crate::operator::crd::tunnel::{CfTunnel, TunnelCrd, TunnelStatus};
use crate::operator::resources::deployment;
use crate::operator::Error;
use std::ops::Deref;
use std::sync::Arc;
use tokio::time::Duration;

enum TunnelAction {
    Delete,
    Create,
    Nop,
}

impl From<&Arc<CfTunnel>> for TunnelAction {
    fn from(s: &Arc<CfTunnel>) -> TunnelAction {
        if s.meta().deletion_timestamp.is_some() {
            TunnelAction::Delete
        } else if s.meta().finalizers.is_some() {
            TunnelAction::Create
        } else {
            TunnelAction::Nop
        }
    }
}

pub async fn reconciler(generator: Arc<CfTunnel>, ctx: Arc<Context>) -> Result<Action, Error> {
    println!("Generator: {:?}", &generator);

    let namespace: String = match generator.namespace() {
        Some(namespace) => namespace,
        None => return Err(Error::MissingNamespaceError("Tunnel")),
    };

    let name = generator.name_any();

    println!("Processing ({}) from ({})", &name, &namespace);

    match TunnelAction::from(&generator) {
        TunnelAction::Create => {
            let credentials = match ctx.credentials_api.get_opt(&name).await {
                Ok(result) => match result {
                    Some(credentials) => credentials,
                    None => return Err(Error::MissingCredentialsError),
                },
                Err(err) => return Err(Error::KubeError { source: err }),
            };

            let tunnel_secret = generator
                .spec
                .tunnel_secret
                .as_ref()
                .map(|bytes| bytes.as_bytes());

            let auth = match &credentials.spec.auth {
                AuthKind::ServiceKey(key) => Credentials::Service {
                    key: key.to_owned(),
                },
                AuthKind::UserAuthKey { email, key } => Credentials::UserAuthKey {
                    email: email.to_owned(),
                    key: key.to_owned(),
                },
                AuthKind::UserAuthToken(token) => Credentials::UserAuthToken {
                    token: token.to_owned(),
                },
            };

            let tunnel = match ctx
                .cloudflare_client
                .create_tunnel(
                    &credentials.spec.account_id,
                    &auth,
                    &name,
                    tunnel_secret,
                    ConfigurationSrc::Cloudflare,
                )
                .await
            {
                Ok(tunnel) => tunnel,
                // TODO: read error
                Err(_) => return Err(Error::FailedToCreateTunnelError),
            };

            let tunnel_token = match ctx
                .cloudflare_client
                .get_tunnel_token(
                    &credentials.spec.account_id,
                    &auth,
                    tunnel.id.to_string().as_ref(),
                )
                .await
            {
                Ok(token) => token,
                // TODO: read error
                Err(_) => return Err(Error::FailedToCreateTunnelError),
            };

            let d



            Ok(Action::requeue(Duration::from_secs(10)))
        }
        TunnelAction::Delete => Ok(Action::await_change()),
        TunnelAction::Nop => Ok(Action::requeue(Duration::from_secs(10))),
    }
}

pub fn on_err(generator: Arc<CfTunnel>, error: &Error, ctx: Arc<Context>) -> Action {
    Action::requeue(Duration::from_secs(30 * 100))
}
