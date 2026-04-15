use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;


#[derive(Debug, Error)]
pub enum DeploymentError {
    #[error("Invalid hash: must be exactly 6 characters")]
    InvalidHash,
    #[error("Invalid registry link: expected 'image:tag' format")]
    InvalidRegistryLink,
    #[error("Invalid port: '{0}' is not a number")]
    InvalidPort(String),
    #[error("Malformed body: {0}")]
    MalformedBody(String),
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),
}

#[derive(Debug, Clone)]
pub struct Port {
    pub port: String,
}

#[derive(Debug, Clone)]
pub struct VariableEnvironment {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct Argument {
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct NodeSelector {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VolumeType {
    Block,
    Mount,
}

pub struct CreateDeploymentProps {
    pub hash: String,
    pub registry_link: String,
    pub username: String,
    pub password: String,
    pub service_command: String,
    pub label: String,
    pub ports: Vec<Port>,
    pub envs: Vec<VariableEnvironment>,
    pub args: Vec<Argument>,
    pub node_selectors: Vec<NodeSelector>,
    pub generated_label: String,
    pub has_storage: bool,
    pub readiness_probe_initial_delay: u32,
    pub liveness_probe_initial_delay: u32,
    pub readiness_probe_period: u32,
    pub liveness_probe_period: u32,
    pub need_compute_gpu: bool,
    pub ram_limit: String,
    pub ram_request: String,
    pub cpu_limit: String,
    pub cpu_request: String,
    pub egress_bandwidth: String,
    pub ingress_bandwidth: String,
    pub volume_type: VolumeType,
}

pub struct RegistryLink {
    pub image: String,
    pub image_tag: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeploymentResponse {
    pub result: Value,
    pub r#type: String,
    pub name: String,
}

/**
 * This object is responsible for communicating with the Kubernetes API to manage deployments.
 */
pub struct Deployment {
    client: Client,
    base_url: String,
}
impl Deployment {
    pub fn new(client: Client, base_url: impl Into<String>) -> Self {
        Self { client, base_url: base_url.into() }
    }

    /**
     * Validates that the hash is exactly 6 characters long.
     */
    fn validate_hash(hash: &str) -> Result<(), DeploymentError> {
        if hash.len() != 6 {
            return Err(DeploymentError::InvalidHash);
        }
        Ok(())
    }

    /**
     * Parses a registry link in the format "image:tag".
     */
    fn parse_registry_link(link: &str) -> Result<RegistryLink, DeploymentError> {
        let parts: Vec<&str> = link.splitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(DeploymentError::InvalidRegistryLink);
        }
        Ok(RegistryLink {
            image: parts[0].to_string(),
            image_tag: parts[1].to_string(),
        })
    }

    /**
     * Validates that the bandwidth value is in the correct format (e.g., "100M" or "1G").
     */
    fn validate_bandwidth(value: &str) -> bool {
        let re = regex::Regex::new(r"^\d+[MG]$").unwrap();
        re.is_match(value)
    }

    /**
     * Adds node selectors to the deployment spec.
     */
    fn add_node_selectors(
        mut body: Value,
        node_selectors: &[NodeSelector],
    ) -> Result<Value, DeploymentError> {
        let spec = body["spec"]["template"]["spec"]
            .as_object_mut()
            .ok_or_else(|| DeploymentError::MalformedBody("spec.template.spec missing".into()))?;

        // Group values by key
        let mut united: Vec<(String, Vec<String>)> = vec![];
        for ns in node_selectors {
            if let Some(entry) = united.iter_mut().find(|(k, _)| k == &ns.key) {
                entry.1.push(ns.value.clone());
            } else {
                united.push((ns.key.clone(), vec![ns.value.clone()]));
            }
        }

        let node_selector_terms: Vec<Value> = united
            .iter()
            .map(|(key, values)| {
                json!({
                    "matchExpressions": [{
                        "key": key,
                        "operator": "In",
                        "values": values
                    }]
                })
            })
            .collect();

        spec.insert(
            "affinity".to_string(),
            json!({
                "nodeAffinity": {
                    "requiredDuringSchedulingIgnoredDuringExecution": {
                        "nodeSelectorTerms": node_selector_terms
                    }
                }
            }),
        );

        Ok(body)
    }

    /**
     * Adds service commands to the deployment spec.
     */
    fn add_service_commands(
        mut body: Value,
        service_command: &str,
    ) -> Result<Value, DeploymentError> {
        if service_command.is_empty() {
            return Ok(body);
        }
        let commands: Vec<&str> = service_command.split(' ').collect();
        body["spec"]["template"]["spec"]["containers"][0]["command"] = json!(commands);
        Ok(body)
    }

    /**
     * Adds arguments to the deployment spec.
     */
    fn add_arguments(
        mut body: Value,
        args: &[Argument],
        hash: &str,
        username: &str,
        password: &str,
        label: &str,
        generated_label: &str,
    ) -> Result<Value, DeploymentError> {
        if args.is_empty() {
            return Ok(body);
        }
        let parsed_args: Vec<String> = args
            .iter()
            .map(|arg| {
                Self::parse_generic_tags(&arg.value, hash, username, password, label, generated_label)
            })
            .collect();
        body["spec"]["template"]["spec"]["containers"][0]["args"] = json!(parsed_args);
        Ok(body)
    }

    /**
     * Adds ports to the deployment spec.
     */
    fn add_ports(mut body: Value, ports: &[Port]) -> Result<Value, DeploymentError> {
        if ports.is_empty() {
            return Ok(body);
        }
        for p in ports {
            p.port.parse::<u32>().map_err(|_| DeploymentError::InvalidPort(p.port.clone()))?;
        }
        let port_list: Vec<Value> = ports
            .iter()
            .map(|p| {
                let port_num: u32 = p.port.parse().unwrap();
                json!({ "containerPort": port_num, "protocol": "TCP" })
            })
            .collect();
        body["spec"]["template"]["spec"]["containers"][0]["ports"] = json!(port_list);
        Ok(body)
    }

    /**
     * Adds environment variables to the deployment spec.
     */
    fn add_envs(
        mut body: Value,
        envs: &[VariableEnvironment],
        hash: &str,
        username: &str,
        password: &str,
        label: &str,
        generated_label: &str,
    ) -> Result<Value, DeploymentError> {
        if envs.is_empty() {
            return Ok(body);
        }
        let env_list: Vec<Value> = envs
            .iter()
            .map(|env| {
                json!({
                    "name": env.key,
                    "value": Self::parse_generic_tags(&env.value, hash, username, password, label, generated_label)
                })
            })
            .collect();
        body["spec"]["template"]["spec"]["containers"][0]["env"] = json!(env_list);
        Ok(body)
    }

    /**
     * Adds GPU resources to the deployment spec.
     */
    fn add_compute_gpu(mut body: Value) -> Value {
        body["spec"]["template"]["spec"]["runtimeClassName"] = json!("nvidia");
        body["spec"]["template"]["spec"]["containers"][0]["resources"]["limits"]
            ["nvidia.com/gpu"] = json!(1);
        body
    }

    /**
     * Adds storage to the deployment spec.
     */
    fn add_storage(
        mut body: Value,
        username: &str,
        label: &str,
        hash: &str,
        has_storage: bool,
        volume_type: &VolumeType,
    ) -> Result<Value, DeploymentError> {
        if !has_storage {
            return Ok(body);
        }
        let pvc_name = format!("{}{}-pvc", label, hash);
        let home_path = format!("/home/{}", username);

        if *volume_type == VolumeType::Block {
            body["spec"]["template"]["spec"]["containers"][0]["volumeDevices"] = json!([{
                "devicePath": home_path,
                "name": pvc_name
            }]);
        } else {
            body["spec"]["template"]["spec"]["containers"][0]["volumeMounts"] = json!([{
                "mountPath": home_path,
                "name": pvc_name
            }]);
        }

        body["spec"]["template"]["spec"]["volumes"] = json!([{
            "name": pvc_name,
            "persistentVolumeClaim": { "claimName": pvc_name }
        }]);

        Ok(body)
    }

    /**
     * Parses generic tags in a string, replacing placeholders with actual values.
     */
    fn parse_generic_tags(
        value: &str,
        hash: &str,
        username: &str,
        password: &str,
        label: &str,
        generated_label: &str,
    ) -> String {
        value
            .replace("{{hash}}", hash)
            .replace("{{username}}", username)
            .replace("{{password}}", password)
            .replace("{{label}}", label)
            .replace("{{generated_label}}", generated_label)
    }

    /**
     * Creates a deployment in Kubernetes.
     */
    pub async fn create(&self, props: CreateDeploymentProps) -> Result<DeploymentResponse, DeploymentError> {
        Self::validate_hash(&props.hash)?;
        let registry = Self::parse_registry_link(&props.registry_link)?;

        let mut body = json!({
            "metadata": {
                "name": format!("{}{}", props.label, props.hash),
                "namespace": format!("odn-{}", props.hash),
                "labels": {
                    "app": format!("{}{}", props.label, props.hash),
                    "type": "Deployment",
                    "hash": props.hash,
                    "shutable": "true"
                }
            },
            "spec": {
                "replicas": 1,
                "automountServiceAccountToken": false,
                "selector": {
                    "matchLabels": {
                        "app": format!("{}{}", props.label, props.hash)
                    }
                },
                "template": {
                    "metadata": {
                        "labels": {
                            "app": format!("{}{}", props.label, props.hash),
                            "type": "Deployment",
                            "hash": props.hash
                        },
                        "name": format!("{}{}", props.label, props.hash),
                        "namespace": format!("odn-{}", props.hash),
                        "annotations": {
                            "kubernetes.io/ingress-bandwidth": props.ingress_bandwidth,
                            "kubernetes.io/egress-bandwidth": props.egress_bandwidth
                        }
                    },
                    "spec": {
                        "securityContext": {
                            "runAsUser": 1000,
                            "runAsGroup": 1000,
                            "fsGroup": 1000
                        },
                        "containers": [{
                            "name": format!("pod{}{}", props.label, props.hash),
                            "image": format!("{}:{}", registry.image, registry.image_tag),
                            "imagePullPolicy": "IfNotPresent",
                            "readinessProbe": {
                                "exec": { "command": ["/bin/sh", "/var/probe.sh"] },
                                "initialDelaySeconds": props.readiness_probe_initial_delay,
                                "periodSeconds": props.readiness_probe_period
                            },
                            "livenessProbe": {
                                "exec": { "command": ["/bin/sh", "/var/probe.sh"] },
                                "initialDelaySeconds": props.liveness_probe_initial_delay,
                                "periodSeconds": props.liveness_probe_period
                            },
                            "resources": {
                                "limits": { "cpu": props.cpu_limit, "memory": props.ram_limit },
                                "requests": { "cpu": props.cpu_request, "memory": props.ram_request }
                            }
                        }],
                        "imagePullSecrets": [{ "name": "registryhub" }]
                    }
                }
            }
        });

        if props.need_compute_gpu {
            body = Self::add_compute_gpu(body);
        }
        body = Self::add_node_selectors(body, &props.node_selectors)?;
        body = Self::add_service_commands(body, &props.service_command)?;
        body = Self::add_arguments(body, &props.args, &props.hash, &props.username, &props.password, &props.label, &props.generated_label)?;
        body = Self::add_ports(body, &props.ports)?;
        body = Self::add_envs(body, &props.envs, &props.hash, &props.username, &props.password, &props.label, &props.generated_label)?;
        body = Self::add_storage(body, &props.username, &props.label, &props.hash, props.has_storage, &props.volume_type)?;

        let url = format!("{}/apis/apps/v1/namespaces/odn-{}/deployments", self.base_url, props.hash);
        println!("[ODIN][K-AGENT][Deployment] POST {url}");
        let response = self.client.post(&url).json(&body).send().await?;
        let status = response.status();
        let result = response.json::<Value>().await?;
        if !status.is_success() {
            eprintln!("[ODIN][K-AGENT][Deployment] API returned {status}: {result}");
        } else {
            println!("[ODIN][K-AGENT][Deployment] Created successfully: {}", result["metadata"]["name"]);
        }

        Ok(DeploymentResponse {
            result,
            r#type: "Deployment".to_string(),
            name: format!("{}{}", props.label, props.hash),
        })
    }

    /**
     * Deletes a deployment in Kubernetes.
     */
    pub async fn delete(&self, hash: &str) -> Result<Vec<DeploymentResponse>, DeploymentError> {
        Self::validate_hash(hash)?;

        let url = format!(
            "{}/apis/apps/v1/namespaces/odn-{}/deployments?labelSelector=type=Deployment,hash={}",
            self.base_url, hash, hash
        );
        let result = self.client.get(&url).send().await?.json::<Value>().await?;
        let names: Vec<String> = result["items"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|item| item["metadata"]["name"].as_str().map(String::from))
            .collect();

        if names.is_empty() {
            return Ok(vec![]);
        }
        let mut responses = vec![];
        for name in names {
            let url = format!(
                "{}/apis/apps/v1/namespaces/odn-{}/deployments/{}",
                self.base_url, hash, name
            );
            let result = self.client.delete(&url).send().await?.json::<Value>().await?;
            responses.push(DeploymentResponse {
                result,
                r#type: "Deployment".to_string(),
                name,
            });
        }
        Ok(responses)
    }

    /**
     * Scales a deployment in Kubernetes by updating the number of replicas.
     */
    pub async fn scale(&self, hash: &str, replicas: u32) -> Result<Vec<DeploymentResponse>, DeploymentError> {
        Self::validate_hash(hash)?;

        let url = format!(
            "{}/apis/apps/v1/namespaces/odn-{}/deployments?labelSelector=type=Deployment,hash={}",
            self.base_url, hash, hash
        );
        println!("[ODIN][K-AGENT][Deployment] GET {url}");
        let response = self.client.get(&url).send().await?;
        let status = response.status();
        let result = response.json::<Value>().await?;
        if !status.is_success() {
            eprintln!("[ODIN][K-AGENT][Deployment] Scale list API returned {status}: {result}");
        }
        let names: Vec<String> = result["items"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|item| item["metadata"]["name"].as_str().map(String::from))
            .collect();

        if names.is_empty() {
            println!("[ODIN][K-AGENT][Deployment] No deployments found for hash {hash} to scale");
            return Ok(vec![]);
        }

        println!("[ODIN][K-AGENT][Deployment] Scaling {} deployment(s) to {replicas} replica(s): {:?}", names.len(), names);
        let mut responses = vec![];
        for name in names {
            let body = json!({
                "kind": "Scale",
                "apiVersion": "autoscaling/v1",
                "metadata": { "name": name, "namespace": format!("odn-{}", hash) },
                "spec": { "replicas": replicas }
            });
            let url = format!(
                "{}/apis/apps/v1/namespaces/odn-{}/deployments/{}/scale",
                self.base_url, hash, name
            );
            println!("[ODIN][K-AGENT][Deployment] PUT {url}");
            let response = self.client.put(&url).json(&body).send().await?;
            let status = response.status();
            let result = response.json::<Value>().await?;
            if !status.is_success() {
                eprintln!("[ODIN][K-AGENT][Deployment] Scale API returned {status}: {result}");
            } else {
                println!("[ODIN][K-AGENT][Deployment] Scaled {name} to {replicas} replica(s)");
            }
            responses.push(DeploymentResponse {
                result,
                r#type: "Deployment".to_string(),
                name,
            });
        }
        Ok(responses)
    }

    /**
     * Retrieves all pods associated with a given hash in Kubernetes.
     * This will be called when an application needs to list its pods.
     */
    pub async fn get_pods(&self, hash: &str) -> Result<Value, DeploymentError> {
        Self::validate_hash(hash)?;
        let url = format!("{}/api/v1/namespaces/odn-{}/pods", self.base_url, hash);
        let mut result = self.client.get(&url).send().await?.json::<Value>().await?;
        if let Some(items) = result["items"].as_array_mut() {
            for item in items.iter_mut() {
                item["kind"] = json!("Pod");
            }
        }
        Ok(result)
    }

    /**
     * Retrieves all deployments associated with a given hash in Kubernetes.
     */
    pub async fn get_deployments(&self, hash: &str) -> Result<Value, DeploymentError> {
        Self::validate_hash(hash)?;
        let url = format!("{}/apis/apps/v1/namespaces/odn-{}/deployments", self.base_url, hash);
        let mut result = self.client.get(&url).send().await?.json::<Value>().await?;
        if let Some(items) = result["items"].as_array_mut() {
            for item in items.iter_mut() {
                item["kind"] = json!("Deployment");
            }
        }
        Ok(result)
    }

    /**
     * Retrieves all replicasets associated with a given hash in Kubernetes.
     */
    pub async fn get_replicasets(&self, hash: &str) -> Result<Value, DeploymentError> {
        Self::validate_hash(hash)?;
        let url = format!("{}/apis/apps/v1/namespaces/odn-{}/replicasets", self.base_url, hash);
        let mut result = self.client.get(&url).send().await?.json::<Value>().await?;
        if let Some(items) = result["items"].as_array_mut() {
            for item in items.iter_mut() {
                item["kind"] = json!("ReplicaSet");
            }
        }
        Ok(result)
    }

    /**
     * Retrieves the names of all deployments associated with a given hash in Kubernetes.
     */
    async fn get(&self, hash: &str) -> Result<Vec<String>, DeploymentError> {
        let url = format!(
            "{}/apis/apps/v1/namespaces/odn-{}/deployments?labelSelector=type=Deployment,hash={}",
            self.base_url, hash, hash
        );
        let result = self.client.get(&url).send().await?.json::<Value>().await?;
        let names = result["items"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|item| item["metadata"]["name"].as_str().map(String::from))
            .collect();
        Ok(names)
    }

    /**
     * Deletes a deployment in Kubernetes by name.
     */
    async fn del(&self, hash: &str, name: &str) -> Result<DeploymentResponse, DeploymentError> {
        let url = format!(
            "{}/apis/apps/v1/namespaces/odn-{}/deployments/{}",
            self.base_url, hash, name
        );
        let result = self.client.delete(&url).send().await?.json::<Value>().await?;
        Ok(DeploymentResponse {
            result,
            r#type: "Deployment".to_string(),
            name: name.to_string(),
        })
    }

    /**
     * Scales a deployment in Kubernetes by updating the number of replicas.
     */
    async fn put(&self, hash: &str, name: &str, replicas: u32) -> Result<DeploymentResponse, DeploymentError> {
        let body = json!({
            "kind": "Scale",
            "apiVersion": "autoscaling/v1",
            "metadata": { "name": name, "namespace": format!("odn-{}", hash) },
            "spec": { "replicas": replicas }
        });
        let url = format!(
            "{}/apis/apps/v1/namespaces/odn-{}/deployments/{}/scale",
            self.base_url, hash, name
        );
        let result = self.client.put(&url).json(&body).send().await?.json::<Value>().await?;
        Ok(DeploymentResponse {
            result,
            r#type: "Deployment".to_string(),
            name: name.to_string(),
        })
    }
}