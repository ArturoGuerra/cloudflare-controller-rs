use crate::cloudflare::{Auth, Client as CloudflareClient, CloudflareTunnel};
use crate::controller::ingress;
use futures::{Stream, StreamExt, TryFutureExt, TryStream, TryStreamExt};
use k8s_openapi::api::networking::v1::{Ingress, IngressClass};
use kube::runtime::controller::Action;
use kube::runtime::Controller;
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
use tokio::task;

const INGRESS_CONTROLLER: &str = "cloudflare.ar2ro.io/ingress-controller";
const CLASSLESS_INGRESS_POLICY: bool = false;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Kube Error: {0}")]
    KubeError(#[source] kube::Error),
}

pub struct IngressController {
    kubernetes_client: Client,
    cloudflare_client: CloudflareClient,
}

struct Context {
    controller: IngressController,
    ingress_api: Api<Ingress>,
    ingress_store: Store<Ingress>,
}

impl IntoFuture for IngressController {
    type Output = anyhow::Result<()>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output>>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(self.start())
    }
}

async fn reconcile(ingress: Arc<Ingress>, ctx: Arc<Context>) -> Result<Action, Error> {
    println!("Ingress: {:?}", ingress.name_any());
    Ok(Action::requeue(std::time::Duration::from_secs(60)))
}

fn error_policy<'a>(ingress: Arc<Ingress>, error: &Error, ctx: Arc<Context>) -> Action {
    Action::requeue(std::time::Duration::from_secs(60))
}

trait StoreIngressClassExt<T> {
    fn filterd(&self) -> Vec<Arc<T>>;
    fn ingress_class_names(&self) -> Vec<String>;
}

trait IngressClassExt {
    fn filter(&self) -> bool;
}

trait IngressExt {
    fn ingress_class_name(&self) -> Option<&String>;
}

impl StoreIngressClassExt<IngressClass> for Store<IngressClass> {
    fn filterd(&self) -> Vec<Arc<IngressClass>> {
        self.state()
            .into_iter()
            .filter(|ingress| ingress.filter())
            .collect::<Vec<_>>()
    }

    fn ingress_class_names(&self) -> Vec<String> {
        self.state()
            .into_iter()
            .filter(|ingress| ingress.filter())
            .map(|ingress| ingress.name_any())
            .collect::<Vec<_>>()
    }
}

impl IngressClassExt for IngressClass {
    fn filter(&self) -> bool {
        self.spec
            .as_ref()
            .map(|spec| {
                spec.controller
                    .as_ref()
                    .map(|controller| controller.eq(INGRESS_CONTROLLER))
            })
            .flatten()
            .unwrap_or(CLASSLESS_INGRESS_POLICY)
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

        // TODO: Replace class filter with something that can be overwritten.
        // NOTE: This needs to be started before the controller or it will stall.
        let ingress_class_watcher = watcher(ingress_class_api.clone(), wc.clone())
            .reflect(ingress_class_writer)
            .default_backoff()
            .touched_objects()
            .for_each(|_| ready(()));

        // NOTE: This is a cheap operation.
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

        let ctx = Arc::new(Context {
            controller: self,
            ingress_store: ingress_store.clone(),
            ingress_api,
        });

        // NOTE: Starts ingress class watcher and waits for it to be populated.
        tokio::spawn(ingress_class_watcher);
        ingress_class_store.wait_until_ready().await?;
        println!(
            "Managing the following Ingres Classes: {:?}",
            ingress_class_store.ingress_class_names()
        );

        // Controller is trigged when a change to the stream happens and when
        Controller::for_stream(ingress_watcher, ingress_store.clone())
            .owns(ingress_class_api.clone(), wc.clone())
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
