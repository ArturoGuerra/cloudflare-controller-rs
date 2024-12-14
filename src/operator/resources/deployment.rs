use crate::operator::controller::Context;
use crate::operator::crd::tunnel::CfTunnel;
use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::core::v1::{
    ConfigMapEnvSource, Container, EnvFromSource, PodSpec, PodTemplateSpec, SecretEnvSource,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use kube::api::Api;
use kube::api::ObjectMeta;
use std::collections::BTreeMap;
use std::sync::Arc;

pub async fn create(
    name: &str,
    namespace: &str,
    generator: Arc<CfTunnel>,
    labels: BTreeMap<String, String>,
    ctx: Arc<Context>,
) -> anyhow::Result<()> {
    let image = match &generator.spec.image {
        Some(image) => image.to_owned(),
        None => "cloudflare/cloudflared:latest".to_owned(),
    };

    let env = vec![
        EnvFromSource {
            secret_ref: Some(SecretEnvSource {
                name: name.to_owned(),
                optional: Some(false),
            }),
            ..EnvFromSource::default()
        },
        EnvFromSource {
            config_map_ref: Some(ConfigMapEnvSource {
                name: name.to_owned(),
                optional: Some(false),
            }),
            ..EnvFromSource::default()
        },
    ];

    let deployment = Deployment {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            namespace: Some(namespace.to_owned()),
            labels: Some(labels.clone()),
            ..ObjectMeta::default()
        },
        spec: Some(DeploymentSpec {
            replicas: Some(generator.spec.replicas),
            selector: LabelSelector {
                match_labels: Some(labels.clone()),
                ..LabelSelector::default()
            },
            template: PodTemplateSpec {
                spec: Some(PodSpec {
                    containers: vec![Container {
                        name: "cloudflared".to_owned(),
                        image: Some(image),
                        env_from: Some(env),
                        ..Container::default()
                    }],
                    ..PodSpec::default()
                }),
                ..PodTemplateSpec::default()
            },
            ..DeploymentSpec::default()
        }),
        ..Deployment::default()
    };

    let deployment_api: Api<Deployment> = Api::namespaced(ctx.kubernetes_client.clone(), namespace);

    Ok(())
}
