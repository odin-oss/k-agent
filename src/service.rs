use thiserror::Error;

use crate::kubernetes_objects::authorization_policy::AuthorizationPolicy;
use crate::kubernetes_objects::deployment::{CreateDeploymentProps, Deployment};
use crate::kubernetes_objects::external_name::{CreateExternalNameProps, ExternalName};
use crate::kubernetes_objects::ingress::{AddInKongProps, DeleteFromKongProps, Ingress, IngressPort};
use crate::kubernetes_objects::namespace::{CreateNamespaceProps, DeleteNamespaceProps, Namespace};
use crate::kubernetes_objects::network_policy::NetworkPolicy;
use crate::kubernetes_objects::pvc::{CreatePvcProps, Pvc};
use crate::kubernetes_objects::registry_hub::{CreateRegistryHubProps, DeleteRegistryHubProps, RegistryHub};
use crate::kubernetes_objects::service::{CreateServiceProps, Service, SvcType};

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("Invalid hash: must be exactly 6 characters")]
    InvalidHash,
    #[error("Deployment error: {0}")]
    Deployment(String),
    #[error("Service error: {0}")]
    Service(String),
    #[error("Namespace error: {0}")]
    Namespace(String),
    #[error("ExternalName error: {0}")]
    ExternalName(String),
    #[error("Ingress error: {0}")]
    Ingress(String),
    #[error("PVC error: {0}")]
    Pvc(String),
    #[error("RegistryHub error: {0}")]
    RegistryHub(String),
    #[error("AuthorizationPolicy error: {0}")]
    AuthorizationPolicy(String),
    #[error("NetworkPolicy error: {0}")]
    NetworkPolicy(String),
    #[error("Task join error: {0}")]
    JoinError(String),
}

// ── Interface (app definition from environment) ───────────────────────────────

#[derive(Debug, Clone)]
pub struct AppInterface {
    pub label: String,
    pub registry_link: String,
    pub service_command: String,
    pub ports: Vec<IngressPort>,
    pub envs: Vec<crate::kubernetes_objects::deployment::VariableEnvironment>,
    pub args: Vec<crate::kubernetes_objects::deployment::Argument>,
    pub node_selectors: Vec<crate::kubernetes_objects::deployment::NodeSelector>,
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
    pub volume_type: crate::kubernetes_objects::deployment::VolumeType,
}

// ── Create Props ──────────────────────────────────────────────────────────────

pub struct CreateProps {
    pub hash: String,
    pub interfaces: Vec<AppInterface>,
    pub generated_label: String,
    pub username: String,
    pub password: String,
    pub istio_activated: bool,
}

// ── Orchestrator ──────────────────────────────────────────────────────────────

pub struct Orchestrator {
    pub deployment: Deployment,
    pub service: Service,
    pub namespace: Namespace,
    pub external_name: ExternalName,
    pub ingress: Ingress,
    pub pvc: Pvc,
    pub registry_hub: RegistryHub,
    pub authorization_policy: AuthorizationPolicy,
    pub network_policy: NetworkPolicy,
}

impl Orchestrator {
    fn validate_hash(hash: &str) -> Result<(), OrchestratorError> {
        if hash.len() != 6 {
            return Err(OrchestratorError::InvalidHash);
        }
        Ok(())
    }

    // ── Deletion ──────────────────────────────────────────────────────────────

    /// Execute the full deletion workflow for an application.
    /// Deletes external names, namespace, registry hub, deployments, services,
    /// and Kong ingress in parallel. Optionally deletes authorization policy if Istio is active.
    pub async fn exec_deletion(
        &self,
        hash: &str,
        istio_activated: bool,
    ) -> Result<(), OrchestratorError> {
        Self::validate_hash(hash)?;

        let (ext_res, ns_res, reg_res, dep_res, svc_res, kong_res) = tokio::join!(
            self.external_name.delete(hash),
            self.namespace
                .delete(DeleteNamespaceProps { hash: hash.to_string() }),
            self.registry_hub
                .delete(DeleteRegistryHubProps { hash: hash.to_string() }),
            self.deployment.delete(hash),
            self.service.delete(hash),
            self.ingress
                .delete_from_kong(DeleteFromKongProps { hash: hash.to_string() }),
        );

        ext_res.map_err(|e| OrchestratorError::ExternalName(e.to_string()))?;
        ns_res.map_err(|e| OrchestratorError::Namespace(e.to_string()))?;
        reg_res.map_err(|e| OrchestratorError::RegistryHub(e.to_string()))?;
        dep_res.map_err(|e| OrchestratorError::Deployment(e.to_string()))?;
        svc_res.map_err(|e| OrchestratorError::Service(e.to_string()))?;
        kong_res.map_err(|e| OrchestratorError::Ingress(e.to_string()))?;

        if istio_activated {
            self.authorization_policy
                .delete(hash)
                .await
                .map_err(|e| OrchestratorError::AuthorizationPolicy(e.to_string()))?;
        }

        Ok(())
    }

    // ── Start (scale up) ──────────────────────────────────────────────────────

    /// Scale all deployments for a given hash to 1 replica.
    pub async fn exec_start(&self, hash: &str) -> Result<(), OrchestratorError> {
        Self::validate_hash(hash)?;
        self.deployment
            .scale(hash, 1)
            .await
            .map_err(|e| OrchestratorError::Deployment(e.to_string()))?;
        Ok(())
    }

    // ── Shutdown (scale down) ─────────────────────────────────────────────────

    /// Scale all deployments for a given hash to 0 replicas.
    pub async fn exec_shutdown(&self, hash: &str) -> Result<(), OrchestratorError> {
        Self::validate_hash(hash)?;
        self.deployment
            .scale(hash, 0)
            .await
            .map_err(|e| OrchestratorError::Deployment(e.to_string()))?;
        Ok(())
    }

    // ── Create ────────────────────────────────────────────────────────────────

    /// Execute the full application creation workflow.
    /// Creates namespace, registry hub, and optionally authorization policy first,
    /// then creates services, external names, PVCs, deployments, and Kong ingress in parallel.
    pub async fn create(&self, props: CreateProps) -> Result<String, OrchestratorError> {
        Self::validate_hash(&props.hash)?;

        // Step 1: create namespace and registry hub (must exist before other objects)
        self.namespace
            .create(CreateNamespaceProps {
                hash: props.hash.clone(),
            })
            .await
            .map_err(|e| OrchestratorError::Namespace(format!("{e:?}")))?;

        self.registry_hub
            .create(CreateRegistryHubProps {
                hash: props.hash.clone(),
            })
            .await
            .map_err(|e| OrchestratorError::RegistryHub(format!("{e:?}")))?;

        if props.istio_activated {
            self.authorization_policy
                .create(&props.hash)
                .await
                .map_err(|e| OrchestratorError::AuthorizationPolicy(format!("{e:?}")))?;
        }

        // Detect which apps have HSTORAGE env var
        let storage_labels: Vec<String> = props
            .interfaces
            .iter()
            .filter(|app| app.envs.iter().any(|env| env.key == "HSTORAGE"))
            .map(|app| app.label.to_lowercase().replace(' ', ""))
            .collect();

        // Step 2: create all objects for each interface in parallel
        let mut errors: Vec<OrchestratorError> = Vec::new();

        for app in &props.interfaces {
            let label = app.label.to_lowercase().replace(' ', "");
            let has_storage = storage_labels.contains(&label);

            // SSH service (port 22)
            let ssh_res = self
                .service
                .create(CreateServiceProps {
                    hash: props.hash.clone(),
                    label: label.clone(),
                    port_externe: 22,
                    port_interne: 22,
                    svc_type: SvcType::ClusterIp,
                    shutable: false,
                })
                .await;
            if let Err(e) = ssh_res {
                errors.push(OrchestratorError::Service(format!("[label={label}] SSH port 22: {e:?}")));
            }

            // Kong ingress
            let kong_res = self
                .ingress
                .add_in_kong(AddInKongProps {
                    hash: props.hash.clone(),
                    ports: app.ports.clone(),
                    label: label.clone(),
                })
                .await;
            if let Err(e) = kong_res {
                errors.push(OrchestratorError::Ingress(format!("[label={label}] {e:?}")));
            }

            // Per-port services and external names
            for port in &app.ports {
                let svc_res = self
                    .service
                    .create(CreateServiceProps {
                        hash: props.hash.clone(),
                        label: label.clone(),
                        port_externe: port.port,
                        port_interne: port.port,
                        svc_type: SvcType::ClusterIp,
                        shutable: false,
                    })
                    .await;
                if let Err(e) = svc_res {
                    errors.push(OrchestratorError::Service(format!("[label={label}, port={}] {e:?}", port.port)));
                }

                let ext_res: Result<_, _> = self
                    .external_name
                    .create(CreateExternalNameProps {
                        hash: props.hash.clone(),
                        label: label.clone(),
                        port_externe: port.port,
                    })
                    .await;
                if let Err(e) = ext_res {
                    errors.push(OrchestratorError::ExternalName(format!("[label={label}, port={}] {e:?}", port.port)));
                }
            }

            // PVC if storage is needed
            if has_storage {
                let pvc_res = self
                    .pvc
                    .create(CreatePvcProps {
                        hash: props.hash.clone(),
                        label: label.clone(),
                    })
                    .await;
                if let Err(e) = pvc_res {
                    errors.push(OrchestratorError::Pvc(format!("[label={label}] {e:?}")));
                }
            }

            // Deployment
            let target = if label.contains("ssh-") {
                label.split("ssh-").nth(1).unwrap_or("").to_string()
            } else {
                String::new()
            };

            let dep_res = self
                .deployment
                .create(CreateDeploymentProps {
                    hash: props.hash.clone(),
                    registry_link: app.registry_link.clone(),
                    username: props.username.clone(),
                    password: props.password.clone(),
                    service_command: app.service_command.clone(),
                    label: label.clone(),
                    ports: app
                        .ports
                        .iter()
                        .map(|p| crate::kubernetes_objects::deployment::Port {
                            port: p.port.to_string(),
                        })
                        .collect(),
                    envs: app.envs.clone(),
                    args: app.args.clone(),
                    node_selectors: app.node_selectors.clone(),
                    generated_label: props.generated_label.clone(),
                    has_storage,
                    readiness_probe_initial_delay: app.readiness_probe_initial_delay,
                    liveness_probe_initial_delay: app.liveness_probe_initial_delay,
                    readiness_probe_period: app.readiness_probe_period,
                    liveness_probe_period: app.liveness_probe_period,
                    need_compute_gpu: app.need_compute_gpu,
                    ram_limit: app.ram_limit.clone(),
                    ram_request: app.ram_request.clone(),
                    cpu_limit: app.cpu_limit.clone(),
                    cpu_request: app.cpu_request.clone(),
                    egress_bandwidth: app.egress_bandwidth.clone(),
                    ingress_bandwidth: app.ingress_bandwidth.clone(),
                    volume_type: app.volume_type.clone(),
                })
                .await;
            if let Err(e) = dep_res {
                errors.push(OrchestratorError::Deployment(format!("[label={label}] {e:?}")));
            }
        }

        if !errors.is_empty() {
            eprintln!(
                "[ODIN][K-AGENT] {} error(s) during creation of hash {}:",
                errors.len(),
                props.hash
            );
            for e in &errors {
                eprintln!("  - {e}");
                eprintln!("    debug: {e:?}");
            }
        }

        Ok(props.hash)
    }
}
