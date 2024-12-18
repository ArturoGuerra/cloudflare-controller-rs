use crate::cftunnel::Client as CloudflareClient;
use crate::operator::crd::{
    credentials::Credentials, tunnel::Tunnel, tunnel_configuration::TunnelIngress,
};
use futures::StreamExt;
use futures::{future::select_all, Future};
use k8s_openapi::api::{
    apps::v1::Deployment,
    core::v1::{ConfigMap, Secret},
};
use kube::{client::Client, runtime::watcher::Config, runtime::Controller as KubeController, Api};
use std::future::IntoFuture;
use std::pin::Pin;
use std::sync::Arc;

use super::tunnel_controller;
use super::tunnel_ingress_controller;

pub struct Controller(Arc<Context>);

pub struct Context {
    pub kubernetes_client: Client,
    pub cloudflare_client: CloudflareClient,
    pub credentials_api: Api<Credentials>,
    pub tunnel_api: Api<Tunnel>,
    pub tunnel_ingress_api: Api<TunnelIngress>,
}

impl Controller {
    pub async fn try_default() -> anyhow::Result<Self> {
        let context = Context::try_default().await?;
        Ok(Self(Arc::new(context)))
    }

    pub fn get_context(&self) -> Arc<Context> {
        self.0.clone()
    }

    async fn tunnel_controller(&self) {
        println!("Starting Tunnel Controller");
        let deployment_api: Api<Deployment> = Api::all(self.0.kubernetes_client.clone());
        let configmap_api: Api<ConfigMap> = Api::all(self.0.kubernetes_client.clone());
        let secret_api: Api<Secret> = Api::all(self.0.kubernetes_client.clone());
        KubeController::new(self.0.tunnel_api.clone(), Config::default())
            .owns(deployment_api, Config::default())
            .owns(configmap_api, Config::default())
            .owns(secret_api, Config::default())
            .run(
                tunnel_controller::reconciler,
                tunnel_controller::on_err,
                self.0.clone(),
            )
            .for_each(|result| async move {
                match result {
                    Ok(result) => println!("Successfully reconciled tunnel: {:?}", result),
                    Err(err) => println!("Failed to reconcile tunnel: {:?}", err),
                }
            })
            .await;
    }

    async fn tunnel_ingress_controller(&self) {
        println!("Starting Tunnel Ingress Controller");
        let secret_api: Api<Secret> = Api::all(self.0.kubernetes_client.clone());
        KubeController::new(self.0.tunnel_ingress_api.clone(), Config::default())
            .owns(secret_api, Config::default())
            .run(
                tunnel_ingress_controller::reconciler,
                tunnel_ingress_controller::on_err,
                self.0.clone(),
            )
            .for_each(|result| async move {
                match result {
                    Ok(result) => println!("Successfully reconciled tunnel ingress: {:?}", result),
                    Err(err) => println!("Failed to reconcile tunnel ingress: {:?}", err),
                }
            })
            .await;
    }

    async fn future(self) -> anyhow::Result<()> {
        let futures: Vec<Pin<Box<dyn Future<Output = ()>>>> = vec![
            Box::pin(self.tunnel_controller()),
            Box::pin(self.tunnel_ingress_controller()),
        ];

        select_all(futures).await;

        Ok(())
    }
}

impl IntoFuture for Controller {
    type Output = anyhow::Result<()>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output>>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(self.future())
    }
}

impl Context {
    pub async fn try_default() -> anyhow::Result<Self> {
        let kubernetes_client = Client::try_default().await?;
        let cloudflare_client = CloudflareClient::try_default()?;

        let credentials_api: Api<Credentials> = Api::all(kubernetes_client.clone());
        let tunnel_api: Api<Tunnel> = Api::all(kubernetes_client.clone());
        let tunnel_ingress_api: Api<TunnelIngress> = Api::all(kubernetes_client.clone());

        Ok(Self {
            kubernetes_client,
            cloudflare_client,
            credentials_api,
            tunnel_api,
            tunnel_ingress_api,
        })
    }
}
