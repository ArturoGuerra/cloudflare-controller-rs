use crate::controllers::tunnel::Context;
use crate::crd::tunnel::Tunnel;
use k8s_openapi::{api::core::v1::Secret, ByteString};
use kube::api::{Api, DeleteParams, ObjectMeta, PostParams};
use std::{collections::BTreeMap, sync::Arc};

pub async fn create(
    name: &str,
    namespace: &str,
    generator: Arc<Tunnel>,
    ctx: Arc<Context>,
    labels: BTreeMap<String, String>,
    secrets: BTreeMap<String, ByteString>,
) -> Result<Secret, kube::Error> {
    let secret = Secret {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            namespace: Some(namespace.to_owned()),
            labels: Some(labels),
            ..ObjectMeta::default()
        },
        data: Some(secrets),
        ..Secret::default()
    };

    let secret_api: Api<Secret> = Api::namespaced(ctx.kubernetes_client.clone(), namespace);
    match secret_api.create(&PostParams::default(), &secret).await {
        Ok(secret) => Ok(secret),
        Err(err) => Err(err),
    }
}

pub async fn delete(ctx: Arc<Context>, name: &str, namespace: &str) -> Result<(), kube::Error> {
    let secret_api: Api<Secret> = Api::namespaced(ctx.kubernetes_client.clone(), namespace);
    match secret_api.delete(name, &DeleteParams::default()).await {
        Ok(_) => Ok(()),
        Err(err) => Err(err),
    }
}
