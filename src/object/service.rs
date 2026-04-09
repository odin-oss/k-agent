use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("Invalid hash: must be exactly 6 characters")]
    InvalidHash,
    #[error("Invalid field '{0}': must not be empty")]
    EmptyField(String),
    #[error("Invalid port: must be a positive number")]
    InvalidPort,
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),
}

// ── Supporting types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SvcType {
    LoadBalancer,
    ClusterIp,
}

// ── Props ─────────────────────────────────────────────────────────────────────

pub struct CreateServiceProps {
    pub hash: String,
    pub label: String,
    pub port_externe: u32,
    pub port_interne: u32,
    pub svc_type: SvcType,
    pub shutable: bool,
}

// ── Response ──────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct ServiceResponse {
    pub result: Value,
    pub r#type: String,
    pub name: String,
}

// ── Struct ────────────────────────────────────────────────────────────────────

pub struct Service {
    client: Client,
    base_url: String,
}

impl Service {
    pub fn new(client: Client, base_url: impl Into<String>) -> Self {
        Self { client, base_url: base_url.into() }
    }

    // ── Validation ────────────────────────────────────────────────────────────

    fn validate_hash(hash: &str) -> Result<(), ServiceError> {
        if hash.len() != 6 {
            return Err(ServiceError::InvalidHash);
        }
        Ok(())
    }

    fn validate_not_empty(value: &str, field: &str) -> Result<(), ServiceError> {
        if value.is_empty() {
            return Err(ServiceError::EmptyField(field.to_string()));
        }
        Ok(())
    }

    // ── Public API ────────────────────────────────────────────────────────────

    pub async fn create(&self, props: CreateServiceProps) -> Result<ServiceResponse, ServiceError> {
        Self::validate_hash(&props.hash)?;
        Self::validate_not_empty(&props.label, "label")?;
        if props.port_externe == 0 || props.port_interne == 0 {
            return Err(ServiceError::InvalidPort);
        }

        let prefix = match props.svc_type {
            SvcType::ClusterIp => "ci",
            SvcType::LoadBalancer => "lb",
        };
        let type_str = match props.svc_type {
            SvcType::ClusterIp => "ClusterIP",
            SvcType::LoadBalancer => "LoadBalancer",
        };
        let service_name = format!("{}{}{}{}", prefix, props.label, props.hash, props.port_externe);

        let body = json!({
            "apiVersion": "v1",
            "kind": "Service",
            "metadata": {
                "name": service_name,
                "namespace": format!("n{}", props.hash),
                "labels": {
                    "type": "Service",
                    "hash": props.hash,
                    "shutable": if props.shutable { "true" } else { "false" }
                }
            },
            "spec": {
                "ports": [{
                    "port": props.port_interne,
                    "protocol": "TCP"
                }],
                "selector": {
                    "app": format!("{}{}", props.label, props.hash)
                },
                "sessionAffinity": "ClientIP",
                "type": type_str
            }
        });

        let url = format!(
            "{}/api/v1/namespaces/n{}/services",
            self.base_url, props.hash
        );
        let result = self.client.post(&url).json(&body).send().await?.json::<Value>().await?;

        Ok(ServiceResponse {
            result,
            r#type: "Service".to_string(),
            name: service_name,
        })
    }

    pub async fn delete(&self, hash: &str) -> Result<Vec<ServiceResponse>, ServiceError> {
        Self::validate_hash(hash)?;

        let names = self.get(hash).await?;
        let mut responses = vec![];
        for name in names {
            responses.push(self.del(hash, &name).await?);
        }
        Ok(responses)
    }

    pub async fn get_services(&self, hash: &str) -> Result<Value, ServiceError> {
        Self::validate_hash(hash)?;

        let url = format!(
            "{}/api/v1/namespaces/n{}/services",
            self.base_url, hash
        );
        let mut result = self.client.get(&url).send().await?.json::<Value>().await?;

        if let Some(items) = result["items"].as_array_mut() {
            for item in items.iter_mut() {
                item["kind"] = json!("Service");
            }
        }

        Ok(result)
    }

    // ── Private API ───────────────────────────────────────────────────────────

    async fn get(&self, hash: &str) -> Result<Vec<String>, ServiceError> {
        let url = format!(
            "{}/api/v1/namespaces/n{}/services?labelSelector=hash={}",
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

    async fn del(&self, hash: &str, name: &str) -> Result<ServiceResponse, ServiceError> {
        let url = format!(
            "{}/api/v1/namespaces/n{}/services/{}",
            self.base_url, hash, name
        );
        let result = self.client.delete(&url).send().await?.json::<Value>().await?;
        Ok(ServiceResponse {
            result,
            r#type: "Service".to_string(),
            name: name.to_string(),
        })
    }
}
