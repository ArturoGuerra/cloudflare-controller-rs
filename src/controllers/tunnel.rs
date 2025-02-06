use crate::cloudflare::{auth::Auth, tunnel::CloudflareTunnel, Client as CloudflareClient};
use crate::crd::credentials::Credentials;
use crate::crd::tunnel::Tunnel;
use cloudflare::endpoints::cfd_tunnel::ConfigurationSrc;
use cloudflare::framework::response::ApiFailure;
use futures::{Future, StreamExt};
use k8s_openapi::api::{
    apps::v1::Deployment,
    core::v1::{ConfigMap, Secret},
};
use k8s_openapi::ByteString;
use kube::api::{Patch, PatchParams};
use kube::core::object::HasSpec;
use kube::runtime::controller::Action;
use kube::{
    client::Client, runtime::watcher::Config, runtime::Controller as KubeController, Api, Resource,
    ResourceExt,
};
use reqwest::StatusCode;
use std::collections::BTreeMap;
use std::future::IntoFuture;
use std::pin::Pin;
use std::sync::Arc;
use tokio::time::Duration;

const RECONCILE_TIMER: u64 = 60;

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

pub struct TunnelController(Arc<Context>);

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

pub struct Context {
    pub kubernetes_client: Client,
    pub cloudflare_client: CloudflareClient,
    pub credentials_api: Api<Credentials>,
    pub tunnel_api: Api<Tunnel>,
}

#[inline]
pub async fn create_tunnel(generator: Arc<Tunnel>, ctx: Arc<Context>) -> Result<Action, Error> {
    let name = generator.name_any();
    let namespace = generator.metadata.namespace.clone().unwrap();
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
            .create_tunnel(&auth, &name, tunnel_secret, ConfigurationSrc::Cloudflare)
            .await
        {
            Ok(tunnel) => {
                let crd_api: Api<Tunnel> =
                    Api::namespaced(ctx.kubernetes_client.clone(), &namespace);

                let mut crd = (*generator).clone();
                crd.spec.uuid = Some(tunnel.id);
                let patch: Patch<Tunnel> = Patch::Merge(crd);
                match crd_api.patch(&name, &PatchParams::default(), &patch).await {
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
    labels.insert("app.kubernetes.io/name".into(), name.clone());
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

    if let Err(err) = generator
        .create_resources(ctx.kubernetes_client.clone(), labels, secrets)
        .await
    {
        return Err(Error::KubeError(err));
    }

    println!(
        "Successfully created Tunnel, name: {}, namespace: {}, UUID: {}",
        name, namespace, tunnel_token
    );

    match generator.add_finalizer(ctx.kubernetes_client.clone()).await {
        Ok(_) => Ok(Action::requeue(Duration::from_secs(RECONCILE_TIMER))),
        Err(err) => Err(Error::KubeError(err)),
    }
}

#[inline]
async fn delete_tunnel(generator: Arc<Tunnel>, ctx: Arc<Context>) -> Result<Action, Error> {
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

    if let Err(err) = generator
        .delete_resources(ctx.kubernetes_client.clone())
        .await
    {
        return Err(Error::KubeError(err));
    }

    // This should be the last thing we do as the controller wont requeue this resource
    // again
    match generator
        .remove_finalizer(ctx.kubernetes_client.clone())
        .await
    {
        Ok(_) => Ok(Action::await_change()),
        Err(err) => Err(Error::KubeError(err)),
    }
}

pub async fn reconciler(generator: Arc<Tunnel>, ctx: Arc<Context>) -> Result<Action, Error> {
    let action = TunnelAction::from(&generator);
    println!("Action: {:?}", &action);
    match action {
        TunnelAction::Create => create_tunnel(generator, ctx).await,
        TunnelAction::Delete => delete_tunnel(generator, ctx).await,
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

impl TunnelController {
    pub fn get_context(&self) -> Arc<Context> {
        self.0.clone()
    }

    pub async fn start(self) -> anyhow::Result<()> {
        println!("Starting Tunnel Controller");
        let deployment_api: Api<Deployment> = Api::all(self.0.kubernetes_client.clone());
        let configmap_api: Api<ConfigMap> = Api::all(self.0.kubernetes_client.clone());
        let secret_api: Api<Secret> = Api::all(self.0.kubernetes_client.clone());
        KubeController::new(self.0.tunnel_api.clone(), Config::default())
            .owns(deployment_api, Config::default())
            .owns(configmap_api, Config::default())
            .owns(secret_api, Config::default())
            .run(reconciler, on_err, self.0.clone())
            .for_each(|result| async move {
                match result {
                    Ok(result) => println!("Successfully reconciled tunnel: {:?}", result),
                    Err(err) => println!("Failed to reconcile tunnel: {:?}", err),
                }
            })
            .await;

        Ok(())
    }
}

impl TunnelController {
    pub async fn try_new(client: Client) -> anyhow::Result<TunnelController> {
        let context = Context::try_new(client).await?;
        Ok(Self(Arc::new(context)))
    }
}

impl IntoFuture for TunnelController {
    type Output = anyhow::Result<()>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output>>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(self.start())
    }
}

impl Context {
    pub async fn try_new(client: Client) -> anyhow::Result<Self> {
        let cloudflare_client = CloudflareClient::try_default()?;

        let credentials_api: Api<Credentials> = Api::all(client.clone());
        let tunnel_api: Api<Tunnel> = Api::all(client.clone());

        Ok(Self {
            kubernetes_client: client,
            cloudflare_client,
            credentials_api,
            tunnel_api,
        })
    }
}
