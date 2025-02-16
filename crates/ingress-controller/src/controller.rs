use crate::cloudflare::{tunnel::CloudflareTunnel, Client as CloudflareClient};
use crate::crd::tunnel::{Tunnel, TunnelCrd};
use futures::{Stream, StreamExt, TryFutureExt, TryStream, TryStreamExt};
use k8s_openapi::api::networking::v1::{Ingress, IngressClass};
use kube::runtime::controller::Action;
use kube::runtime::reflector::ObjectRef;
use kube::runtime::Controller;
use kube::CustomResourceExt;
use kube::Resource;
use kube::{
    api::{Api, ResourceExt},
    runtime::{
        reflector::{self, reflector, Lookup, Store},
        utils::EventDecode,
        watcher::{self, watcher, Event},
        WatchStreamExt,
    },
    Client,
};
use std::future::{ready, Future, IntoFuture};
use std::pin::Pin;
use std::sync::Arc;

const INGRESS_CONTROLLER: &str = "cloudflare.ar2ro.io/ingress-controller";

trait StoreIngressClassExt<T> {
    fn ingress_class_names(&self) -> Vec<String>;
}

trait IngressClassExt {
    fn controller_name(&self) -> Option<&String>;
}

trait IngressExt {
    fn ingress_class_name(&self) -> Option<&String>;
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Kube Error: {0}")]
    KubeError(#[source] kube::Error),
    #[error("missing default tunnel")]
    MissingDefaultTunnel,
    #[error("invalid ingress class parameters: {0}")]
    InvalidIngressClassParameters(&'static str),
}

pub struct IngressController {
    kubernetes_client: Client,
    cloudflare_client: CloudflareClient,
}

struct Context {
    controller: IngressController,
    ingress_api: Api<Ingress>,
    ingress_store: Store<Ingress>,
    ingress_class_api: Api<IngressClass>,
    ingress_class_store: Store<IngressClass>,
}

impl IntoFuture for IngressController {
    type Output = anyhow::Result<()>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output>>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(self.start())
    }
}

async fn reconcile(ingress: Arc<Ingress>, ctx: Arc<Context>) -> Result<Action, Error> {
    // Checks if ingress belongs to us and exists early if it doesnt.
    // INFO: Return early if we don't own this ingress class.

    let ingress_class = match ingress.ingress_class_name() {
        Some(class_name) => {
            let obj_ref = ObjectRef::new(class_name);
            match ctx.ingress_class_store.get(&obj_ref) {
                Some(ingress_class) => ingress_class,
                None => return Ok(Action::await_change()),
            }
        }
        None => return Ok(Action::await_change()),
    };

    let tunnel_crd = match ingress_class.spec.as_ref().unwrap().parameters.as_ref() {
        Some(parameters) => {
            // INFO: K8s default value for this is Cluster so we set that.
            let default_scope = "Cluster".to_string();
            let default_api_group = "cloudfare.ar2ro.io".to_string();
            let scope = parameters.scope.as_ref().unwrap_or(&default_scope);
            let kind = &parameters.kind;
            let api_group = parameters.api_group.as_ref().unwrap_or(&default_api_group);

            let tunnel = if Tunnel::crd().spec.group.eq(api_group)
                && Tunnel::crd().spec.names.kind.eq(kind)
                && Tunnel::crd().spec.scope.eq(scope)
            {
                let api: Api<Tunnel> = if "Namespace".eq(scope.as_str()) {
                    Api::namespaced(
                        ctx.controller.kubernetes_client.clone(),
                        parameters.namespace.as_ref().unwrap().as_str(),
                    )
                } else {
                    Api::all(ctx.controller.kubernetes_client.clone())
                };

                match api.get(parameters.name.as_str()).await {
                    Ok(tunnel) => tunnel,
                    Err(err) => return Err(Error::KubeError(err)),
                }
            } else {
                return Err(Error::InvalidIngressClassParameters(
                    "parameters don't match Tunnel Crd spec",
                ));
            };

            tunnel
        }
        // TODO: Get default tunnel or return an error if its not found.
        None => return Err(Error::MissingDefaultTunnel),
    };

    let tunnel_uuid = match tunnel_crd.spec.uuid {
        Some(tunnel_uuid) => tunnel_uuid,
        // Requeue in 2 minutes as the tunnel is not ready.
        None => return Ok(Action::requeue(std::time::Duration::from_secs(60 * 2))),
    };

    Ok(Action::requeue(std::time::Duration::from_secs(60)))
}

fn error_policy<'a>(ingress: Arc<Ingress>, error: &Error, ctx: Arc<Context>) -> Action {
    Action::requeue(std::time::Duration::from_secs(60))
}

impl StoreIngressClassExt<IngressClass> for Store<IngressClass> {
    fn ingress_class_names(&self) -> Vec<String> {
        self.state()
            .into_iter()
            .filter(|ingress| {
                ingress
                    .controller_name()
                    .map(|controller_name| controller_name.eq(INGRESS_CONTROLLER))
                    .unwrap_or(false)
            })
            .map(|ingress| ingress.name_any())
            .collect::<Vec<_>>()
    }
}

impl IngressClassExt for IngressClass {
    fn controller_name(&self) -> Option<&String> {
        self.spec
            .as_ref()
            .map(|spec| spec.controller.as_ref().map(|controller| controller))
            .flatten()
    }
}

impl IngressExt for Ingress {
    fn ingress_class_name(&self) -> Option<&String> {
        self.spec
            .as_ref()
            .map(|spec| spec.ingress_class_name.as_ref().map(|name| name))
            .flatten()
    }
}

impl IngressController {
    async fn sync_ingress(&self) -> anyhow::Result<()> {
        Ok(())
    }

    pub async fn start(self) -> anyhow::Result<()> {
        let wc = watcher::Config::default().timeout(20);

        let ingress_class_api: Api<IngressClass> = Api::all(self.kubernetes_client.clone());
        let ingress_api: Api<Ingress> = Api::all(self.kubernetes_client.clone());

        let (ingress_class_store, ingress_class_writer) = reflector::store();
        let (ingress_store, ingress_writer) = reflector::store();

        // NOTE: This needs to be started before the controller or it will stall.
        let ingress_class_watcher = watcher(ingress_class_api.clone(), wc.clone())
            .reflect(ingress_class_writer)
            .default_backoff()
            .touched_objects()
            .for_each(|_| ready(()));

        let ingress_class_store_clone = ingress_class_store.clone();
        let ingress_watcher = watcher(ingress_api.clone(), wc.clone())
            .default_backoff()
            .reflect(ingress_writer)
            .touched_objects()
            .try_filter(move |ingress| {
                ready(ingress.ingress_class_name().map_or_else(
                    || false,
                    |name| {
                        ingress_class_store_clone
                            .ingress_class_names()
                            .contains(name)
                    },
                ))
            });

        // NOTE: Starts ingress class watcher and waits for it to be populated.
        tokio::spawn(ingress_class_watcher);
        ingress_class_store.wait_until_ready().await?;

        let ctx = Arc::new(Context {
            controller: self,
            ingress_store: ingress_store.clone(),
            ingress_api,
            ingress_class_store: ingress_class_store.clone(),
            ingress_class_api: ingress_class_api.clone(),
        });

        // Controller is trigged when a change to the stream happens and when
        Controller::for_stream(ingress_watcher, ingress_store)
            .owns(ingress_class_api, wc.clone())
            .run(reconcile, error_policy, ctx)
            .for_each(|_| ready(()))
            .await;
        Ok(())
    }

    pub async fn try_new(
        kubernetes_client: Client,
        cloudflare_client: CloudflareClient,
    ) -> anyhow::Result<IngressController> {
        Ok(IngressController {
            kubernetes_client,
            cloudflare_client,
        })
    }
}
