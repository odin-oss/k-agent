use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkPolicyError {
    #[error("Invalid hash: must be exactly 6 characters")]
    InvalidHash,
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),
}

pub struct CreateNetworkPolicyProps {
    pub hash: String,
}

pub struct DeleteNetworkPolicyProps {
    pub hash: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkPolicyResponse {
    pub result: Value,
    pub r#type: String,
    pub name: String,
}

/**
 * This object is responsible for managing the network policies in Kubernetes cluster.
 */
pub struct NetworkPolicy {
    client: Client,
    base_url: String,
}
impl NetworkPolicy {
    pub fn new(client: Client, base_url: impl Into<String>) -> Self {
        Self { client, base_url: base_url.into() }
    }

    /**
     * Validates that the hash is exactly 6 characters long.
     */
    fn validate_hash(hash: &str) -> Result<(), NetworkPolicyError> {
        if hash.len() != 6 {
            return Err(NetworkPolicyError::InvalidHash);
        }
        Ok(())
    }

    /**
     * Launches the creation of a network policy in Kubernetes.
     * This will be called when an application is created, and it will deploy the network policy in the cluster.
     */
    pub async fn create(&self, props: CreateNetworkPolicyProps) -> Result<NetworkPolicyResponse, NetworkPolicyError> {
        Self::validate_hash(&props.hash)?;

        let body = json!({
            "metadata": {
                "name": format!("kubec-np-odin-kafka-from-n{}", props.hash),
                "namespace": "odin"
            },
            "spec": {
                "podSelector": {
                    "matchLabels": {
                        "app": "odin-kafka"
                    }
                },
                "policyTypes": ["Ingress"],
                "ingress": [{
                    "from": [{
                        "namespaceSelector": {
                            "matchLabels": {
                                "kubernetes.io/metadata.name": format!("n{}", props.hash)
                            }
                        }
                    }]
                }]
            }
        });

        let url = format!(
            "{}/apis/networking.k8s.io/v1/namespaces/odin/networkpolicies",
            self.base_url
        );
        let result = self.client.post(&url).json(&body).send().await?.json::<Value>().await?;

        Ok(NetworkPolicyResponse {
            result,
            r#type: "NetworkPolicy".to_string(),
            name: format!("network-policy-{}", props.hash),
        })
    }

    /**
     * Launches the deletion of a network policy in Kubernetes.
     * This will be called when an application is deleted, and it will remove the network policy from the cluster.
     */
    pub async fn delete(&self, props: DeleteNetworkPolicyProps) -> Result<NetworkPolicyResponse, NetworkPolicyError> {
        Self::validate_hash(&props.hash)?;

        let url = format!(
            "{}/apis/networking.k8s.io/v1/namespaces/odin/networkpolicies/kubec-np-odin-kafka-from-n{}",
            self.base_url, props.hash
        );
        let result = self.client.delete(&url).send().await?.json::<Value>().await?;

        Ok(NetworkPolicyResponse {
            result,
            r#type: "NetworkPolicy".to_string(),
            name: format!("network-policy-{}", props.hash),
        })
    }
}
