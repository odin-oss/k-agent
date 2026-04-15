use base64::Engine;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RegistryHubError {
    #[error("Invalid hash: must be exactly 6 characters")]
    InvalidHash,
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),
}

pub struct CreateRegistryHubProps {
    pub hash: String,
}

pub struct DeleteRegistryHubProps {
    pub hash: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegistryHubResponse {
    pub result: Value,
    pub r#type: String,
    pub name: String,
}

/**
 * This object is responsible for communicating with the Kubernetes API to manage the registry hub secret.
 * It has two main functions: create and delete, which will be called when an application is created or deleted, respectively. 
 */
pub struct RegistryHub {
    client: Client,
    base_url: String,
    registry_url: String,
    registry_username: String,
    registry_password: String,
}
impl RegistryHub {
    pub fn new(
        client: Client,
        base_url: impl Into<String>,
        registry_url: impl Into<String>,
        registry_username: impl Into<String>,
        registry_password: impl Into<String>,
    ) -> Self {
        Self {
            client,
            base_url: base_url.into(),
            registry_url: registry_url.into(),
            registry_username: registry_username.into(),
            registry_password: registry_password.into(),
        }
    }

    /**
     * Validates that the hash is exactly 6 characters long.
     */
    fn validate_hash(hash: &str) -> Result<(), RegistryHubError> {
        if hash.len() != 6 {
            return Err(RegistryHubError::InvalidHash);
        }
        Ok(())
    }
    /**
     * Launches the creation of the registry hub secret in Kubernetes. 
     * This will be called when an application is created, and it will store the registry credentials in the cluster.
     * It deploys a Kubernetes secret of type `kubernetes.io/dockerconfigjson` with the registry credentials encoded in base64, from the var env.
     */
    pub async fn create(&self, props: CreateRegistryHubProps) -> Result<RegistryHubResponse, RegistryHubError> {
        Self::validate_hash(&props.hash)?;

        let docker_config = json!({
            "auths": {
                &self.registry_url: {
                    "username": &self.registry_username,
                    "password": &self.registry_password,
                }
            }
        });
        let encoded = base64::engine::general_purpose::STANDARD.encode(docker_config.to_string());

        let body = json!({
            "data": {
                ".dockerconfigjson": encoded,
            },
            "metadata": {
                "name": "registryhub",
                "namespace": format!("odn-{}", props.hash),
                "labels": {
                    "type": "RegistryHub",
                    "hash": props.hash,
                }
            },
            "type": "kubernetes.io/dockerconfigjson"
        });

        let url: String = format!(
            "{}/api/v1/namespaces/odn-{}/secrets",
            self.base_url, props.hash
        );
        let result: Value = self.client.post(&url).json(&body).send().await?.json::<Value>().await?;
        Ok(RegistryHubResponse {
            result,
            r#type: "RegistryHub".to_string(),
            name: "registryhub".to_string(),
        })
    }

    /**
     * Launches the deletion of the registry hub secret in Kubernetes. 
     * This will be called when an application is deleted, and it will remove the registry credentials from the cluster.
     */
    pub async fn delete(&self, props: DeleteRegistryHubProps) -> Result<RegistryHubResponse, RegistryHubError> {
        Self::validate_hash(&props.hash)?;

        let url = format!(
            "{}/api/v1/namespaces/odn-{}/secrets/registryhub",
            self.base_url, props.hash
        );
        let result: Value = self.client.delete(&url).send().await?.json::<Value>().await?;
        Ok(RegistryHubResponse {
            result,
            r#type: "RegistryHub".to_string(),
            name: "registryhub".to_string(),
        })
    }
}
