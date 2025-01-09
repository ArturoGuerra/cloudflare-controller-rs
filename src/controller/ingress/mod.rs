use crate::controller::{ingress, Controller};
use futures::{Stream, TryStream, TryStreamExt};
use k8s_openapi::api::networking::v1::{Ingress, IngressClass};
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

pub struct IngressController {
    kubernetes_client: Client,
    ingress_api: Api<Ingress>,
    ingress_class_api: Api<IngressClass>,
}

struct Context {
    ingress_class_store: Store<IngressClass>,
    ingress_store: Store<Ingress>,
}

impl IntoFuture for IngressController {
    type Output = anyhow::Result<()>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output>>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(self.start())
    }
}

impl IngressController {
    async fn sync_ingress(&self) -> anyhow::Result<()> {
        Ok(())
    }

    pub async fn start(self) -> anyhow::Result<()> {
        let wc = watcher::Config::default().timeout(20);

        let (ingress_class_store, ingress_class_writer) = reflector::store();
        let ingress_class_rf = reflector(
            ingress_class_writer,
            watcher(self.ingress_class_api.clone(), wc.clone()),
        );

        let (ingress_store, ingress_writer) = reflector::store();
        let ingress_rf = reflector(ingress_writer, watcher(self.ingress_api.clone(), wc));

        let ingress_class_reflector_task = task::spawn(async move {
            ingress_class_rf
                .touched_objects()
                .try_filter(|ingress_class| {
                    ready(match &ingress_class.spec {
                        Some(spec) => match &spec.controller {
                            Some(controller) => controller.eq(INGRESS_CONTROLLER),
                            None => false,
                        },
                        None => false,
                    })
                })
                .try_collect::<Vec<_>>()
                .await
                .unwrap();
        });

        let ingress_class_store_rf = ingress_class_store.clone();
        let ingress_reflector_task = task::spawn(async move {
            ingress_class_store_rf.wait_until_ready().await.unwrap();
            let ingress_class_names = ingress_class_store_rf
                .state()
                .iter()
                .filter_map(|ic| match &ic.metadata.name {
                    Some(name) => Some(name.clone()),
                    None => None,
                })
                .collect::<Vec<String>>();
            ingress_rf
                .touched_objects()
                .try_filter(move |ingress| match &ingress.spec {
                    Some(spec) => match &spec.ingress_class_name {
                        Some(ingress_class_name) => {
                            if ingress_class_names.contains(ingress_class_name) {
                                return ready(true);
                            }

                            ready(false)
                        }
                        None => ready(false),
                    },
                    None => ready(false),
                })
                .try_collect::<Vec<_>>()
                .await
                .unwrap();
        });

        Ok(())
    }

    pub async fn try_new(kubernetes_client: Client) -> anyhow::Result<IngressController> {
        let ingress_api: Api<Ingress> = Api::all(kubernetes_client.clone());

        let ingress_class_api: Api<IngressClass> = Api::all(kubernetes_client.clone());

        let controller = IngressController {
            kubernetes_client,
            ingress_api,
            ingress_class_api,
        };

        Ok(controller)
    }
}
