use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use thiserror::Error;
use tokio::fs;

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum ConfigMapError {
    #[error("Invalid hash: must be exactly 6 characters")]
    InvalidHash,
    #[error("Invalid field '{0}': must not be empty")]
    EmptyField(String),
    #[error("Failed to read file at '{0}': {1}")]
    FileRead(String, std::io::Error),
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),
}

// ── Props ─────────────────────────────────────────────────────────────────────

pub struct CreateConfigMapProps {
    pub hash: String,
    pub path: String,
    pub namespace: String,
    pub name: String,
    pub filename: String,
    pub shutable: bool,
}

pub struct UpdateConfigMapProps {
    pub path: String,
    pub namespace: String,
    pub name: String,
    pub filename: String,
    pub shutable: bool,
}

// ── Response ──────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigMapResponse {
    pub result: Value,
    pub r#type: String,
    pub name: String,
}

// ── Struct ────────────────────────────────────────────────────────────────────

pub struct ConfigMap {
    client: Client,
    base_url: String,
}

impl ConfigMap {
    pub fn new(client: Client, base_url: impl Into<String>) -> Self {
        Self {
            client,
            base_url: base_url.into(),
        }
    }

    // ── Validation ────────────────────────────────────────────────────────────

    fn validate_hash(hash: &str) -> Result<(), ConfigMapError> {
        if hash.len() != 6 {
            return Err(ConfigMapError::InvalidHash);
        }
        Ok(())
    }

    fn validate_not_empty(value: &str, field: &str) -> Result<(), ConfigMapError> {
        if value.is_empty() {
            return Err(ConfigMapError::EmptyField(field.to_string()));
        }
        Ok(())
    }

    async fn read_file(path: &str) -> Result<String, ConfigMapError> {
        fs::read_to_string(Path::new(path))
            .await
            .map_err(|e| ConfigMapError::FileRead(path.to_string(), e))
    }

    // ── Create ────────────────────────────────────────────────────────────────

    pub async fn create(&self, props: CreateConfigMapProps) -> Result<ConfigMapResponse, ConfigMapError> {
        Self::validate_hash(&props.hash)?;
        Self::validate_not_empty(&props.namespace, "namespace")?;
        Self::validate_not_empty(&props.name, "name")?;
        Self::validate_not_empty(&props.filename, "filename")?;

        let data = Self::read_file(&props.path).await?;

        let body = json!({
            "apiVersion": "v1",
            "data": {
                props.filename: data
            },
            "metadata": {
                "name": props.name,
                "namespace": props.namespace,
                "labels": {
                    "type": "ConfigMap",
                    "hash": props.hash,
                    "shutable": if props.shutable { "true" } else { "false" }
                }
            }
        });

        let url = format!(
            "{}/api/v1/namespaces/{}/configmaps",
            self.base_url, props.namespace
        );

        let result = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await?
            .json::<Value>()
            .await?;

        Ok(ConfigMapResponse {
            result,
            r#type: "ConfigMap".to_string(),
            name: props.name,
        })
    }

    // ── Update ────────────────────────────────────────────────────────────────

    pub async fn update(&self, props: UpdateConfigMapProps) -> Result<Value, ConfigMapError> {
        Self::validate_not_empty(&props.namespace, "namespace")?;
        Self::validate_not_empty(&props.name, "name")?;
        Self::validate_not_empty(&props.filename, "filename")?;

        let data = Self::read_file(&props.path).await?;

        let body = json!({
            "data": {
                props.filename: data
            },
            "metadata": {
                "name": props.name,
                "namespace": props.namespace,
                "labels": {
                    "type": "ConfigMap",
                    "shutable": if props.shutable { "true" } else { "false" }
                }
            }
        });

        let url = format!(
            "{}/api/v1/namespaces/{}/configmaps/{}",
            self.base_url, props.namespace, props.name
        );

        let result = self
            .client
            .put(&url)
            .json(&body)
            .send()
            .await?
            .json::<Value>()
            .await?;

        Ok(result)
    }
}