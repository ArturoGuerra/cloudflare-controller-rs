use crate::cftunnel::{Auth, CloudflareTunnel};
use crate::operator::controller::Context;
use crate::operator::crd::tunnel::{self, Tunnel};
use crate::operator::resources::{deployment, secret};
use crate::operator::Error;
use cloudflare::endpoints::cfd_tunnel::ConfigurationSrc;
use cloudflare::framework::response::ApiFailure;
use k8s_openapi::ByteString;
use kube::api::{Patch, PatchParams};
use kube::core::object::HasSpec;
use kube::runtime::controller::Action;
use kube::{Api, Resource, ResourceExt};
use reqwest::StatusCode;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::time::Duration;

const RECONCILE_TIMER: u64 = 60;

#[derive(Debug)]
enum TunnelAction {
    Delete,
    Create,
    Sync,
}

impl From<&Arc<Tunnel>> for TunnelAction {
    fn from(s: &Arc<Tunnel>) -> TunnelAction {
        if s.meta().deletion_timestamp.is_some() {
            TunnelAction::Delete
        } else if s.meta().finalizers.is_none() {
            TunnelAction::Create
        } else {
            TunnelAction::Sync
        }
    }
}

#[inline]
pub async fn create_tunnel(
    generator: Arc<Tunnel>,
    ctx: Arc<Context>,
    name: &str,
    namespace: &str,
) -> Result<Action, Error> {
    let auth: Auth = match ctx
        .credentials_api
        .get_opt(&generator.spec.credentials)
        .await
    {
        Ok(result) => match result {
            Some(credentials) => credentials.into(),
            None => {
                return Err(Error::MissingCredentials(
                    generator.spec.credentials.clone(),
                ))
            }
        },
        Err(err) => return Err(Error::KubeError(err)),
    };

    let tunnel_secret = generator
        .spec
        .tunnel_secret
        .as_ref()
        .map(|bytes| bytes.as_bytes());

    let tunnel = match generator.spec.uuid {
        Some(uuid) => match ctx
            .cloudflare_client
            .get_tunnel(&auth, uuid.to_string().as_ref())
            .await
        {
            Ok(tunnel) => tunnel,
            Err(err) => return Err(Error::CloudflareApiFailure(err)),
        },

        None => match ctx
            .cloudflare_client
            .create_tunnel(&auth, name, tunnel_secret, ConfigurationSrc::Cloudflare)
            .await
        {
            Ok(tunnel) => {
                let crd_api: Api<Tunnel> =
                    Api::namespaced(ctx.kubernetes_client.clone(), namespace);

                let mut crd = (*generator).clone();
                crd.spec.uuid = Some(tunnel.id);
                let patch: Patch<Tunnel> = Patch::Merge(crd);
                match crd_api.patch(name, &PatchParams::default(), &patch).await {
                    Ok(_) => tunnel,
                    Err(err) => return Err(Error::KubeError(err)),
                }
            }
            Err(err) => return Err(Error::CloudflareApiFailure(err)),
        },
    };

    let tunnel_token: String = match ctx
        .cloudflare_client
        .get_tunnel_token(&auth, tunnel.id.to_string().as_ref())
        .await
    {
        Ok(token) => token.into(),
        Err(err) => return Err(Error::CloudflareApiFailure(err)),
    };

    let mut labels = BTreeMap::new();
    labels.insert("app.kubernetes.io/name".into(), name.into());
    labels.insert(
        "app.kubernetes.io/managed-by".into(),
        "cloudflare-tunnel-operator".into(),
    );

    let mut secrets = BTreeMap::new();
    secrets.insert(
        "TUNNEL_TOKEN".to_owned(),
        ByteString(tunnel_token.clone().into_bytes()),
    );

    println!("Okay we should start creating our resources now!");

    if let Err(err) = secret::create(
        name,
        namespace,
        generator.clone(),
        ctx.clone(),
        labels.clone(),
        secrets,
    )
    .await
    {
        return Err(Error::KubeError(err));
    }

    if let Err(err) = deployment::create(
        name,
        namespace,
        generator.clone(),
        labels.clone(),
        ctx.clone(),
    )
    .await
    {
        return Err(Error::KubeError(err));
    }

    println!(
        "Successfully created Tunnel, name: {}, namespace: {}, UUID: {}",
        name, namespace, tunnel_token
    );

    match tunnel::add_finalizer(name, namespace, ctx.clone()).await {
        Ok(_) => Ok(Action::requeue(Duration::from_secs(RECONCILE_TIMER))),
        Err(err) => Err(Error::KubeError(err)),
    }
}

#[inline]
async fn delete_tunnel(
    generator: Arc<Tunnel>,
    ctx: Arc<Context>,
    name: &str,
    namespace: &str,
) -> Result<Action, Error> {
    if let Some(uuid) = generator.spec.uuid {
        match ctx
            .credentials_api
            .get_opt(&generator.spec().credentials)
            .await
        {
            Ok(credentials) => {
                if let Some(credentials) = credentials {
                    let auth: Auth = credentials.into();
                    if let Err(err) = ctx.cloudflare_client.delete_tunnel(&auth, uuid).await {
                        match &err {
                            ApiFailure::Error(status, errors) => match *status {
                                StatusCode::NOT_FOUND => println!(
                                "Ignoring cloudflare NotFound errors while deleting tunnel, {:?}",
                                errors
                            ),

                                StatusCode::FORBIDDEN => println!(
                                "Ignoring cloudflare Forbidden errors while deleting tunnel, {:?}",
                                errors
                            ),
                                _ => return Err(Error::CloudflareApiFailure(err)),
                            },
                            _ => return Err(Error::CloudflareApiFailure(err)),
                        }
                    }
                }
            }
            Err(err) => {
                return Err(Error::KubeError(err));
            }
        };
    };

    if let Err(err) = deployment::delete(ctx.clone(), name, namespace).await {
        return Err(Error::KubeError(err));
    }

    if let Err(err) = secret::delete(ctx.clone(), name, namespace).await {
        return Err(Error::KubeError(err));
    }

    // This should be the last thing we do as the controller wont requeue this resource
    // again
    match tunnel::remove_finalizer(name, namespace, ctx.clone()).await {
        Ok(_) => Ok(Action::await_change()),
        Err(err) => Err(Error::KubeError(err)),
    }
}

pub async fn reconciler(generator: Arc<Tunnel>, ctx: Arc<Context>) -> Result<Action, Error> {
    let namespace: String = match generator.namespace() {
        Some(namespace) => namespace,
        None => return Err(Error::MissingNamespace("Tunnel")),
    };

    let name = generator.name_any();

    println!("Processing ({}) from ({})", &name, &namespace);

    let action = TunnelAction::from(&generator);
    println!("Action: {:?}", &action);
    match action {
        TunnelAction::Create => create_tunnel(generator, ctx, &name, &namespace).await,
        TunnelAction::Delete => delete_tunnel(generator, ctx, &name, &namespace).await,
        TunnelAction::Sync => Ok(Action::requeue(Duration::from_secs(RECONCILE_TIMER))),
    }
}

pub fn on_err(_generator: Arc<Tunnel>, error: &Error, _ctx: Arc<Context>) -> Action {
    println!("Error: {}", error);
    match error {
        Error::MissingCredentials(v) => {
            println!("Missing credentials {}, requeuing in 120 seconds", v);
            Action::requeue(Duration::from_secs(120))
        }
        _ => Action::await_change(),
    }
}
