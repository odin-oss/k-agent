use std::env;
use std::sync::LazyLock;

use crate::kubernetes_objects::ingress::{IngressPort, PortType};
use crate::service::AppInterface;

mod agent;
mod kubernetes;
mod service;
mod kubernetes_objects {
    pub mod authorization_policy;
    pub mod config_map;
    pub mod deployment;
    pub mod external_name;
    pub mod ingress;
    pub mod namespace;
    pub mod network_policy;
    pub mod pvc;
    pub mod registry_hub;
    pub mod service;
    pub mod storage_carrier;
}

pub struct Configurations {
    // AGENT VARIABLES
    AGENT_LABEL: String,
    AGENT_UUID: Option<String>,
    MOTHERSHIP_URL: String,
    MOTHERSHIP_AUTH_TOKEN: Option<String>,
    // KUBERNETES VARIABLES
    KUBERNETES_VOLUME_TYPE: Option<String>,
    KUBERNETES_STORAGE_CLASSNAME: Option<String>,
    KUBERNETES_URL: Option<String>,
    KUBERNETES_TOKEN: Option<String>,
    KUBERNETES_MASTER_IP: Option<String>,
    KUBERNETES_TOKEN_PATH: Option<String>,
    KUBERNETES_CA_CERT_PATH: Option<String>,
    KUBERNETES_ISTIO_ACTIVATED: bool,
    // KONG / INGRESS VARIABLES
    KONG_URL: Option<String>,
    TLS_ODIN_MONOLITH: Option<String>,
    // REGISTRY HUB VARIABLES
    TZ: String,

    // REGISTRY VARIABLES
    REGISTRY_URL: Option<String>,
    REGISTRY_USERNAME: Option<String>,
    REGISTRY_PASSWORD: Option<String>,

    is_registered: bool,
    is_ok: bool,
}

static CONFIG: LazyLock<Configurations> = LazyLock::new(|| init_config());

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Getting env variables from .env file
    dotenvy::dotenv().ok();
    let conf: &Configurations = &CONFIG;

    println!("-----");
    // Creating the reqwest::Client
    let client: reqwest::Client = reqwest::Client::new();

    /**if conf.is_registered {
        agent::ping_alive(&client, conf.MOTHERSHIP_URL.clone()).await;
    } else {
        agent::register_agent(&client, conf.MOTHERSHIP_URL.clone(), conf.AGENT_LABEL.clone()).await?;
    } **/
    // Build the Orchestrator
    let kubernetes_url: String = conf.KUBERNETES_URL.clone().unwrap_or_default();
    let orchestrator = service::Orchestrator {
        deployment: kubernetes_objects::deployment::Deployment::new(
            client.clone(),
            kubernetes_url.clone(),
        ),
        service: kubernetes_objects::service::Service::new(client.clone(), kubernetes_url.clone()),
        namespace: kubernetes_objects::namespace::Namespace::new(
            client.clone(),
            kubernetes_url.clone(),
        ),
        external_name: kubernetes_objects::external_name::ExternalName::new(
            client.clone(),
            kubernetes_url.clone(),
        ),
        ingress: kubernetes_objects::ingress::Ingress::new(
            client.clone(),
            conf.KONG_URL.clone().unwrap_or_default(),
            conf.TLS_ODIN_MONOLITH.clone().unwrap_or_default(),
        ),
        pvc: kubernetes_objects::pvc::Pvc::new(
            client.clone(),
            kubernetes_url.clone(),
            conf.KUBERNETES_STORAGE_CLASSNAME
                .clone()
                .unwrap_or_default(),
            conf.KUBERNETES_VOLUME_TYPE.clone().unwrap_or_default(),
        ),
        registry_hub: kubernetes_objects::registry_hub::RegistryHub::new(
            client.clone(),
            kubernetes_url.clone(),
            conf.REGISTRY_URL.clone().unwrap_or_default(),
            conf.REGISTRY_USERNAME.clone().unwrap_or_default(),
            conf.REGISTRY_PASSWORD.clone().unwrap_or_default(),
        ),
        authorization_policy: kubernetes_objects::authorization_policy::AuthorizationPolicy::new(
            client.clone(),
            kubernetes_url.clone(),
        ),
        network_policy: kubernetes_objects::network_policy::NetworkPolicy::new(
            client.clone(),
            kubernetes_url.clone(),
        ),
    };

    // Execute application creation
    let create_props = service::CreateProps {
        hash: "xnxnxn".to_string(),
        interfaces: vec![
            AppInterface {
        label: "first-deployment".to_string(),
        registry_link: "registry.gitlab.com/caelus-team/application-cirrus/applications/epitech-demo:prerelease-1.0.4".to_string(),
        service_command: "/usr/bin/supervisord".to_string(),
        node_selectors: vec![
            kubernetes_objects::deployment::NodeSelector {
                key: "node-role.kubernetes.io/odn-default".to_string(),
                value: "".to_string(),
            }
        ],
        ports: vec![
            IngressPort {
                port: 8080,
                label: "toooop".to_string(),
                port_type: PortType { label: "strip_path".to_string()},
            }
        ],
        liveness_probe_initial_delay: 300,
        liveness_probe_period: 10,
        readiness_probe_initial_delay: 5,
        readiness_probe_period: 3,
        need_compute_gpu: false,
        ram_limit: "2Gi".to_string(),
        cpu_limit: "1".to_string(),
        cpu_request: "950m".to_string(),
        ram_request: "500Mi".to_string(),
        ingress_bandwidth: "1Gbps".to_string(),
        egress_bandwidth: "1Gbps".to_string(),
        volume_type: kubernetes_objects::deployment::VolumeType::Block,
        envs: vec![
            kubernetes_objects::deployment::VariableEnvironment {
                key: "ENV_VAR".to_string(),
                value: "value".to_string(),
            }
        ],
        args: vec![
            kubernetes_objects::deployment::Argument {
                value: "--arg1".to_string(),
            },
            kubernetes_objects::deployment::Argument {
                value: "--arg2".to_string(),
            }
        ],
            }
        ],
        generated_label: "ulfi-ulfi-ulfi".to_string(),
        username: "ulfi".to_string(),
        password: "ulfi".to_string(),
        istio_activated: conf.KUBERNETES_ISTIO_ACTIVATED,
    };

    match orchestrator.create(create_props).await {
        Ok(hash) => println!("[ODIN][K-AGENT] Application created successfully with hash: {hash}"),
        Err(e) => eprintln!("[ODIN][K-AGENT] Application creation failed: {e}"),
    }

    Ok(())
}

/**
 * Reads configuration values from environment variables and returns a Configurations struct.
 */
fn init_config() -> Configurations {
    // Checking mandatory environment variables
    let agent_label: String =
        env::var("AGENT_LABEL").expect("AGENT_LABEL environment variable is required but not set.");
    let mothership_url: String = env::var("MOTHERSHIP_URL")
        .expect("MOTHERSHIP_URL environment variable is required but not set.");

    let mut conf: Configurations = Configurations {
        AGENT_LABEL: agent_label.into(),
        AGENT_UUID: env::var("AGENT_UUID").ok(),
        MOTHERSHIP_URL: mothership_url.into(),
        MOTHERSHIP_AUTH_TOKEN: env::var("MOTHERSHIP_AUTH_TOKEN").ok(),
        KUBERNETES_VOLUME_TYPE: env::var("KUBERNETES_VOLUME_TYPE").ok(),
        KUBERNETES_STORAGE_CLASSNAME: env::var("KUBERNETES_STORAGE_CLASSNAME").ok(),
        KUBERNETES_URL: env::var("KUBERNETES_URL").ok(),
        KUBERNETES_TOKEN: env::var("KUBERNETES_TOKEN").ok(),
        KUBERNETES_MASTER_IP: env::var("KUBERNETES_MASTER_IP").ok(),
        KUBERNETES_TOKEN_PATH: env::var("KUBERNETES_TOKEN_PATH").ok(),
        KUBERNETES_CA_CERT_PATH: env::var("KUBERNETES_CA_CERT_PATH").ok(),
        KUBERNETES_ISTIO_ACTIVATED: env::var("KUBERNETES_ISTIO_ACTIVATED").unwrap_or_default()
            == "true",
        KONG_URL: env::var("KONG_URL").ok(),
        TLS_ODIN_MONOLITH: env::var("TLS_ODIN_MONOLITH").ok(),
        TZ: env::var("TZ").unwrap_or_default(),
        REGISTRY_URL: env::var("REGISTRY_URL").ok(),
        REGISTRY_USERNAME: env::var("REGISTRY_USERNAME").ok(),
        REGISTRY_PASSWORD: env::var("REGISTRY_PASSWORD").ok(),
        is_registered: false,
        is_ok: true,
    };

    // Checking global variables for agent registration
    if let Some(ref uuid) = conf.AGENT_UUID
        && let Some(ref _token) = conf.MOTHERSHIP_AUTH_TOKEN
    {
        println!("[ODIN][K-AGENT] Agent UUID: {uuid}");
        conf.is_registered = true;
    } else if let Some(ref uuid) = conf.AGENT_UUID {
        println!(
            "[ODIN][K-AGENT] AGENT_UUID set but no MOTHERSHIP_AUTH_TOKEN found. AGENT_UUID: {uuid}."
        );
        conf.is_ok = false;
    } else if !conf.AGENT_LABEL.is_empty() {
        println!(
            "[ODIN][K-AGENT] No AGENT_UUID set, but AGENT_LABEL found: {}. Need to register new agent.",
            conf.AGENT_LABEL
        );
        conf.is_registered = false;
    }

    conf
}
