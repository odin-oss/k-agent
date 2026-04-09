use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum StorageCarrierError {
    #[error("Invalid hash: must be exactly 6 characters")]
    InvalidHash,
    #[error("Invalid field '{0}': must not be empty")]
    EmptyField(String),
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),
}

// ── Props ─────────────────────────────────────────────────────────────────────

pub struct SmashExportProps {
    pub hash: String,
    pub upload_id: String,
    pub label: String,
    pub app_deletion: bool,
    pub folder_path: String,
    pub storage_carrier_image: String,
    pub storage_carrier_image_tag: String,
    pub smash_api_key: String,
    pub smash_region: String,
    pub smash_teamid: String,
    pub web_title: String,
    pub upload_description: String,
    pub export_language: String,
    pub availability: String,
    pub sender_name: String,
    pub sender_email: String,
    pub receiver_email: String,
}

// ── Response ──────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct StorageCarrierResponse {
    pub result: Value,
    pub r#type: String,
    pub name: String,
}

// ── Struct ────────────────────────────────────────────────────────────────────

pub struct StorageCarrier {
    client: Client,
    base_url: String,
    kafka_broker: String,
    kafka_topic: String,
}

impl StorageCarrier {
    pub fn new(
        client: Client,
        base_url: impl Into<String>,
        kafka_broker: impl Into<String>,
        kafka_topic: impl Into<String>,
    ) -> Self {
        Self {
            client,
            base_url: base_url.into(),
            kafka_broker: kafka_broker.into(),
            kafka_topic: kafka_topic.into(),
        }
    }

    // ── Validation ────────────────────────────────────────────────────────────

    fn validate_hash(hash: &str) -> Result<(), StorageCarrierError> {
        if hash.len() != 6 {
            return Err(StorageCarrierError::InvalidHash);
        }
        Ok(())
    }

    fn validate_not_empty(value: &str, field: &str) -> Result<(), StorageCarrierError> {
        if value.is_empty() {
            return Err(StorageCarrierError::EmptyField(field.to_string()));
        }
        Ok(())
    }

    // ── Public API ────────────────────────────────────────────────────────────

    pub async fn smash_export(&self, props: SmashExportProps) -> Result<StorageCarrierResponse, StorageCarrierError> {
        Self::validate_hash(&props.hash)?;
        Self::validate_not_empty(&props.upload_id, "upload_id")?;
        Self::validate_not_empty(&props.label, "label")?;

        let job_name = format!("storage-carrier-{}-{}", props.hash, props.upload_id);
        let pvc_name = format!("{}{}-pvc", props.label, props.hash);

        let body = json!({
            "metadata": {
                "name": job_name,
                "namespace": format!("n{}", props.hash),
                "labels": {
                    "hash": props.hash,
                    "app": "odin-storage-carrier"
                }
            },
            "spec": {
                "backoffLimit": 2,
                "ttlSecondsAfterFinished": 60,
                "template": {
                    "spec": {
                        "volumes": [{
                            "name": pvc_name,
                            "persistentVolumeClaim": {
                                "claimName": pvc_name
                            }
                        }],
                        "containers": [{
                            "name": job_name,
                            "image": format!("{}:{}", props.storage_carrier_image, props.storage_carrier_image_tag),
                            "env": [
                                { "name": "ENV_HASH", "value": props.hash },
                                { "name": "UPLOAD_ID", "value": props.upload_id },
                                { "name": "APP_DELETION", "value": props.app_deletion.to_string() },
                                { "name": "FOLDER_PATH", "value": props.folder_path },
                                { "name": "SMASH_API_KEY", "value": props.smash_api_key },
                                { "name": "SMASH_REGION", "value": props.smash_region },
                                { "name": "ENV_NAME", "value": props.web_title },
                                { "name": "SMASH_UPLOAD_DESCRIPTION", "value": props.upload_description },
                                { "name": "SMASH_UPLOAD_TEAMID", "value": props.smash_teamid },
                                { "name": "SMASH_UPLOAD_LANGUAGE", "value": props.export_language },
                                { "name": "SMASH_UPLOAD_AVAILABILITY", "value": props.availability },
                                { "name": "SMASH_UPLOAD_SENDER_NAME", "value": props.sender_name },
                                { "name": "SMASH_UPLOAD_SENDER_EMAIL", "value": props.sender_email },
                                { "name": "SMASH_UPLOAD_RECEIVER_EMAIL", "value": props.receiver_email },
                                { "name": "KAFKA_BROKERS", "value": &self.kafka_broker },
                                { "name": "KAFKA_TOPIC", "value": &self.kafka_topic },
                                { "name": "KAFKA_CLIENT_ID", "value": "storage-carrier" },
                            ],
                            "resources": {
                                "limits": { "cpu": "2", "memory": "1Gi" },
                                "requests": { "cpu": "100m", "memory": "100Mi" }
                            },
                            "volumeMounts": [{
                                "name": pvc_name,
                                "mountPath": props.folder_path
                            }]
                        }],
                        "restartPolicy": "Never",
                        "imagePullSecrets": [{ "name": "registryhub" }]
                    }
                }
            }
        });

        let url = format!(
            "{}/apis/batch/v1/namespaces/n{}/jobs",
            self.base_url, props.hash
        );
        let result = self.client.post(&url).json(&body).send().await?.json::<Value>().await?;

        Ok(StorageCarrierResponse {
            result,
            r#type: "Job".to_string(),
            name: job_name,
        })
    }
}
