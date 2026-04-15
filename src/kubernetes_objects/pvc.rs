use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PvcError {
    #[error("Invalid hash: must be exactly 6 characters")]
    InvalidHash,
    #[error("Invalid field '{0}': must not be empty")]
    EmptyField(String),
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),
}

pub struct CreatePvcProps {
    pub hash: String,
    pub label: String,
}

pub struct GetPvcProps {
    pub hash: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PvcResponse {
    pub result: Value,
    pub r#type: String,
    pub name: String,
}

/**
 * This object is responsible for communicating with the Kubernetes API to manage persistent volume claims (PVCs).
 */
pub struct Pvc {
    client: Client,
    base_url: String,
    storage_classname: String,
    volume_type: String,
}
impl Pvc {
    pub fn new(
        client: Client,
        base_url: impl Into<String>,
        storage_classname: impl Into<String>,
        volume_type: impl Into<String>,
    ) -> Self {
        Self {
            client,
            base_url: base_url.into(),
            storage_classname: storage_classname.into(),
            volume_type: volume_type.into(),
        }
    }

    /**
     * Validates that the hash is exactly 6 characters long.
     */
    fn validate_hash(hash: &str) -> Result<(), PvcError> {
        if hash.len() != 6 {
            return Err(PvcError::InvalidHash);
        }
        Ok(())
    }

    /**
     * Validates that a given value is not empty.
     */
    fn validate_not_empty(value: &str, field: &str) -> Result<(), PvcError> {
        if value.is_empty() {
            return Err(PvcError::EmptyField(field.to_string()));
        }
        Ok(())
    }

    /**
     * Launches the creation of a PVC in Kubernetes.
     * This will be called when an application is created, and it will deploy the PVC in the cluster.
     */
    pub async fn create(&self, props: CreatePvcProps) -> Result<PvcResponse, PvcError> {
        Self::validate_hash(&props.hash)?;
        Self::validate_not_empty(&props.label, "label")?;

        let pvc_name: String = format!("{}{}-pvc", props.label, props.hash);
        let namespace: String = format!("odn-{}", props.hash);

        let mut body = json!({
            "metadata": {
                "name": pvc_name,
                "namespace": namespace,
                "labels": {
                    "type": "PVC",
                    "hash": props.hash,
                    "shutable": "false"
                }
            },
            "spec": {
                "accessModes": ["ReadWriteOnce"],
                "resources": {
                    "requests": {
                        "storage": "1Gi"
                    }
                },
                "storageClassName": self.storage_classname
            }
        });

        if self.volume_type == "Block" {
            body["spec"]["volumeMode"] = json!("Block");
        }

        let url = format!(
            "{}/api/v1/namespaces/{}/persistentvolumeclaims",
            self.base_url, namespace
        );
        let result = self.client.post(&url).json(&body).send().await?.json::<Value>().await?;

        Ok(PvcResponse {
            result,
            r#type: "PVC".to_string(),
            name: pvc_name,
        })
    }

    /**
     * Retrieves the PVCs in Kubernetes for a given hash. 
     * This will be used to check if the PVC already exists when creating an application, and to retrieve the PVC information when needed.
     */
    pub async fn get(&self, props: GetPvcProps) -> Result<Value, PvcError> {
        Self::validate_hash(&props.hash)?;

        let url = format!(
            "{}/api/v1/namespaces/odn-{}/persistentvolumeclaims",
            self.base_url, props.hash
        );
        let mut result = self.client.get(&url).send().await?.json::<Value>().await?;

        if let Some(items) = result["items"].as_array_mut() {
            for item in items.iter_mut() {
                item["kind"] = json!("PersistentVolumeClaim");
            }
        }
        Ok(result)
    }
}
