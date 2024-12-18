use crate::operator::controller::Context;
use crate::operator::crd::tunnel::Tunnel;
use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::core::v1::{
    ConfigMapEnvSource, Container, EnvFromSource, HTTPGetAction, PodSpec, PodTemplateSpec, Probe,
    SecretEnvSource,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::api::{Api, PostParams};
use kube::api::{DeleteParams, ObjectMeta};
use std::collections::BTreeMap;
use std::sync::Arc;

pub async fn create(
    name: &str,
    namespace: &str,
    generator: Arc<Tunnel>,
    labels: BTreeMap<String, String>,
    ctx: Arc<Context>,
) -> Result<(), kube::Error> {
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
        //        EnvFromSource {
        //            config_map_ref: Some(ConfigMapEnvSource {
        //                name: name.to_owned(),
        //                optional: Some(false),
        //            }),
        //            ..EnvFromSource::default()
        //        },
    ];

    let probe = Probe {
        http_get: Some(HTTPGetAction {
            port: IntOrString::Int(2000),
            path: Some("/ready".to_owned()),
            ..HTTPGetAction::default()
        }),
        ..Probe::default()
    };

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
                metadata: Some(ObjectMeta {
                    name: Some(name.to_owned()),
                    namespace: Some(namespace.to_owned()),
                    labels: Some(labels.clone()),
                    ..ObjectMeta::default()
                }),
                spec: Some(PodSpec {
                    containers: vec![Container {
                        name: "cloudflared".to_owned(),
                        image: Some(image),
                        env_from: Some(env),
                        command: Some(vec![
                            "cloudflared".into(),
                            "tunnel".into(),
                            "--no-autoupdate".into(),
                            "--metrics".into(),
                            "0.0.0.0:2000".into(),
                            "run".into(),
                        ]),
                        liveness_probe: Some(probe),
                        ..Container::default()
                    }],
                    ..PodSpec::default()
                }),
            },
            ..DeploymentSpec::default()
        }),
        ..Deployment::default()
    };

    let deployment_api: Api<Deployment> = Api::namespaced(ctx.kubernetes_client.clone(), namespace);
    match deployment_api
        .create(&PostParams::default(), &deployment)
        .await
    {
        Ok(_) => Ok(()),
        Err(err) => Err(err),
    }
}

pub async fn delete(ctx: Arc<Context>, name: &str, namespace: &str) -> Result<(), kube::Error> {
    let deployment_api: Api<Deployment> = Api::namespaced(ctx.kubernetes_client.clone(), namespace);

    match deployment_api.delete(name, &DeleteParams::default()).await {
        Ok(_) => Ok(()),
        Err(err) => match &err {
            kube::Error::Api(apierr) => match &apierr.code {
                400..=403 => Ok(()),
                _ => Err(err),
            },
            _ => Err(err),
        },
    }
}
