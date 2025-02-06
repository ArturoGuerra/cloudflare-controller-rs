use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::apimachinery::pkg::{apis::meta::v1::LabelSelector, util::intstr::IntOrString};
use k8s_openapi::{
    api::core::v1::{
        Container, EnvFromSource, HTTPGetAction, PodSpec, PodTemplateSpec, Probe, Secret,
        SecretEnvSource,
    },
    ByteString,
};
use kube::api::{DeleteParams, ObjectMeta, Patch, PatchParams, PostParams};
use kube::{Api, CustomResource, ResourceExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

const FINALIZER_NAME: &str = "tunnel.cloudflare.ar2ro.io/finalizer";

#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[kube(
    group = "cloudflare.ar2ro.io",
    version = "v1",
    kind = "Tunnel",
    doc = "Custom resource representation of a Cloudflare Tunnel",
    scale = r#"{"specReplicasPath":".spec.replicas", "statusReplicasPath":".status.replicas"}"#,
    namespaced
)]
pub struct TunnelCrd {
    pub uuid: Option<Uuid>,
    pub replicas: i32,
    pub credentials: String,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub tunnel_secret: Option<String>,
    pub tags: Option<HashMap<String, String>>,
}

pub struct Resources {
    pub deployment: Deployment,
    pub secret: Secret,
}

impl Tunnel {
    pub async fn create_resources(
        &self,
        kubernetes_client: kube::Client,
        labels: BTreeMap<String, String>,
        secrets: BTreeMap<String, ByteString>,
    ) -> Result<Resources, kube::Error> {
        let name = self.name_any();
        let namespace = self.metadata.namespace.clone().unwrap();
        let postparams = PostParams::default();

        let secret = Secret {
            metadata: ObjectMeta {
                name: Some(self.name_any()),
                namespace: Some(namespace.clone()),
                labels: Some(labels.clone()),
                ..ObjectMeta::default()
            },
            data: Some(secrets),
            ..Secret::default()
        };

        let image = match &self.spec.image {
            Some(image) => image.to_owned(),
            None => "cloudflare/cloudflared:latest".to_owned(),
        };

        let env = vec![EnvFromSource {
            secret_ref: Some(SecretEnvSource {
                name: name.clone(),
                optional: Some(false),
            }),
            ..EnvFromSource::default()
        }];

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
                replicas: Some(self.spec.replicas),
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

        let deployment_api: Api<Deployment> =
            Api::namespaced(kubernetes_client.clone(), &namespace);

        let deployment = match deployment_api.create(&postparams, &deployment).await {
            Ok(deployment) => deployment,
            Err(err) => return Err(err),
        };

        let secret_api: Api<Secret> = Api::namespaced(kubernetes_client.clone(), &namespace);
        let secret = match secret_api.create(&postparams, &secret).await {
            Ok(secret) => secret,
            Err(err) => return Err(err),
        };

        Ok(Resources { deployment, secret })
    }

    pub async fn delete_resources(
        &self,
        kubernetes_client: kube::Client,
    ) -> Result<(), kube::Error> {
        let name = self.name_any();
        let namespace = self.metadata.namespace.clone().unwrap();
        let deleteparams = DeleteParams::default();

        let deployment_api: Api<Secret> = Api::namespaced(kubernetes_client.clone(), &namespace);

        match deployment_api.delete(&name, &deleteparams).await {
            Ok(_) => {}
            Err(err) => return Err(err),
        };

        let secret_api: Api<Secret> = Api::namespaced(kubernetes_client.clone(), &namespace);
        match secret_api.delete(&name, &deleteparams).await {
            Ok(_) => {}
            Err(err) => return Err(err),
        };

        Ok(())
    }

    pub async fn add_finalizer(
        &self,
        kubernetes_client: kube::Client,
    ) -> Result<Tunnel, kube::Error> {
        let tunnel_api: Api<Tunnel> = Api::namespaced(
            kubernetes_client.clone(),
            self.metadata.namespace.clone().unwrap().as_ref(),
        );

        let patch: Value = json!({
            "metadata": {
                "finalizers": [FINALIZER_NAME]
            }
        });

        let patch: Patch<&Value> = Patch::Merge(&patch);
        match tunnel_api
            .patch(self.name_any().as_ref(), &PatchParams::default(), &patch)
            .await
        {
            Ok(tunnel) => Ok(tunnel),
            Err(err) => Err(err),
        }
    }

    pub async fn remove_finalizer(
        &self,
        kubernetes_client: kube::Client,
    ) -> Result<Tunnel, kube::Error> {
        let tunnel_api: Api<Tunnel> = Api::namespaced(
            kubernetes_client.clone(),
            self.metadata.namespace.clone().unwrap().as_ref(),
        );

        let patch: Value = json!({
            "metadata": {
                "finalizers": null,
           }
        });

        let patch: Patch<&Value> = Patch::Merge(&patch);

        match tunnel_api
            .patch(
                self.metadata.namespace.clone().unwrap().as_ref(),
                &PatchParams::default(),
                &patch,
            )
            .await
        {
            Ok(tunnel) => Ok(tunnel),
            Err(err) => Err(err),
        }
    }
}
