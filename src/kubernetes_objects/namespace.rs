use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NamespaceError {
    #[error("Invalid hash: must be between 4 and 6 characters")]
    InvalidHash,
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),
}

pub struct CreateNamespaceProps {
    pub hash: String,
}
pub struct DeleteNamespaceProps {
    pub hash: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NamespaceResponse {
    pub result: Value,
    pub r#type: String,
    pub name: String,
}
/**
 * This object is responsible for communicating with the Kubernetes API to manage the namespaces in the cluster.
 */
pub struct Namespace {
    client: Client,
    base_url: String,
}
impl Namespace {
    pub fn new(client: Client, base_url: impl Into<String>) -> Self {
        Self { client, base_url: base_url.into() }
    }

    /**
     * Validates that the hash is between 4 and 6 characters long.
     */
    fn validate_hash(hash: &str, min: usize, max: usize) -> Result<(), NamespaceError> {
        if hash.len() < min || hash.len() > max {
            return Err(NamespaceError::InvalidHash);
        }
        Ok(())
    }

    /**
     * Launches the creation of a namespace in Kubernetes.
     * This will be called when an application is created, and it will deploy the namespace in the cluster.
     */
    pub async fn create(&self, props: CreateNamespaceProps) -> Result<NamespaceResponse, NamespaceError> {
        Self::validate_hash(&props.hash, 4, 6)?;
        let name = if props.hash == "odin" {
            "odin".to_string()
        } else {
            format!("n{}", props.hash)
        };
        let body = json!({
            "apiVersion": "v1",
            "kind": "Namespace",
            "metadata": {
                "name": name,
                "labels": {
                    "kubernetes.io/metadata.name": name,
                    "type": "Namespace",
                    "hash": props.hash,
                    "name": "user-app-namespace"
                }
            },
            "status": {
                "phase": "Active"
            }
        });

        let url = format!("{}/api/v1/namespaces", self.base_url);
        let result = self.client.post(&url).json(&body).send().await?.json::<Value>().await?;

        Ok(NamespaceResponse {
            result,
            r#type: "Namespace".to_string(),
            name,
        })
    }

    /**
     * Launches the deletion of a namespace in Kubernetes.
     * This will be called when an application is deleted, and it will remove the namespace from the cluster.
     */
    pub async fn delete(&self, props: DeleteNamespaceProps) -> Result<NamespaceResponse, NamespaceError> {
        Self::validate_hash(&props.hash, 6, 6)?;

        let name = format!("n{}", props.hash);
        let url = format!("{}/api/v1/namespaces/{}", self.base_url, name);
        let result = self.client.delete(&url).send().await?.json::<Value>().await?;

        Ok(NamespaceResponse {
            result,
            r#type: "Namespace".to_string(),
            name,
        })
    }
}
