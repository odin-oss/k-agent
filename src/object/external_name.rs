use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum ExternalNameError {
    #[error("Invalid hash: must be exactly 6 characters")]
    InvalidHash,
    #[error("Invalid field '{0}': must not be empty")]
    EmptyField(String),
    #[error("Invalid port: must be a positive number")]
    InvalidPort,
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),
}

// ── Props ─────────────────────────────────────────────────────────────────────

pub struct CreateExternalNameProps {
    pub hash: String,
    pub label: String,
    pub port_externe: u32,
}

// ── Response ──────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct ExternalNameResponse {
    pub result: Value,
    pub r#type: String,
    pub name: String,
}

// ── Struct ────────────────────────────────────────────────────────────────────

pub struct ExternalName {
    client: Client,
    base_url: String,
}

impl ExternalName {
    pub fn new(client: Client, base_url: impl Into<String>) -> Self {
        Self { client, base_url: base_url.into() }
    }

    // ── Validation ────────────────────────────────────────────────────────────

    fn validate_hash(hash: &str) -> Result<(), ExternalNameError> {
        if hash.len() != 6 {
            return Err(ExternalNameError::InvalidHash);
        }
        Ok(())
    }

    fn validate_not_empty(value: &str, field: &str) -> Result<(), ExternalNameError> {
        if value.is_empty() {
            return Err(ExternalNameError::EmptyField(field.to_string()));
        }
        Ok(())
    }

    // ── Public API ────────────────────────────────────────────────────────────

    pub async fn create(&self, props: CreateExternalNameProps) -> Result<ExternalNameResponse, ExternalNameError> {
        Self::validate_hash(&props.hash)?;
        Self::validate_not_empty(&props.label, "label")?;
        if props.port_externe == 0 {
            return Err(ExternalNameError::InvalidPort);
        }

        let service_name = format!("ci{}{}{}-proxy", props.label, props.hash, props.port_externe);
        let external_name = format!(
            "ci{}{}{}.n{}.svc.cluster.local",
            props.label, props.hash, props.port_externe, props.hash
        );

        let body = json!({
            "apiVersion": "v1",
            "kind": "Service",
            "metadata": {
                "name": service_name,
                "namespace": "odin",
                "labels": {
                    "type": "ExternalName",
                    "hash": props.hash,
                    "shutable": "true"
                }
            },
            "spec": {
                "externalName": external_name,
                "ports": [{ "port": props.port_externe, "protocol": "TCP" }],
                "type": "ExternalName"
            }
        });

        let url = format!("{}/api/v1/namespaces/odin/services", self.base_url);
        let result = self.client.post(&url).json(&body).send().await?.json::<Value>().await?;

        Ok(ExternalNameResponse {
            result,
            r#type: "ExternalName".to_string(),
            name: service_name,
        })
    }

    pub async fn delete(&self, hash: &str) -> Result<Vec<ExternalNameResponse>, ExternalNameError> {
        Self::validate_hash(hash)?;

        let names = self.get(hash).await?;
        let mut responses = vec![];
        for name in names {
            responses.push(self.del(&name).await?);
        }
        Ok(responses)
    }

    // ── Private API ───────────────────────────────────────────────────────────

    async fn get(&self, hash: &str) -> Result<Vec<String>, ExternalNameError> {
        let url = format!(
            "{}/api/v1/namespaces/odin/services?labelSelector=type=ExternalName,hash={}",
            self.base_url, hash
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

    async fn del(&self, name: &str) -> Result<ExternalNameResponse, ExternalNameError> {
        let url = format!("{}/api/v1/namespaces/odin/services/{}", self.base_url, name);
        let result = self.client.delete(&url).send().await?.json::<Value>().await?;
        Ok(ExternalNameResponse {
            result,
            r#type: "ExternalName".to_string(),
            name: name.to_string(),
        })
    }
}