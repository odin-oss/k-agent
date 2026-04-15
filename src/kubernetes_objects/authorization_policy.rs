use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthorizationPolicyError {
    #[error("Invalid hash: must be exactly 6 characters")]
    InvalidHash,
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),
    #[error("Unexpected response: {0}")]
    UnexpectedResponse(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthorizationPolicyResponse {
    pub result: Value,
    pub r#type: String,
    pub name: String,
}

/**
 * This object is responsible for communicating with the Kubernetes API to manage Istio AuthorizationPolicies.
 */
pub struct AuthorizationPolicy {
    client: Client,
    base_url: String,
}
impl AuthorizationPolicy {
    pub fn new(client: Client, base_url: impl Into<String>) -> Self {
        Self {
            client,
            base_url: base_url.into(),
        }
    }

    /**
     * Validates that the hash is exactly 6 characters long.
     */
    fn validate_hash(hash: &str) -> Result<(), AuthorizationPolicyError> {
        if hash.len() != 6 {
            return Err(AuthorizationPolicyError::InvalidHash);
        }
        Ok(())
    }

    /**
     * Creates an Istio AuthorizationPolicy in Kubernetes using the provided hash.
     */
    pub async fn create(&self, hash: &str) -> Result<AuthorizationPolicyResponse, AuthorizationPolicyError> {
        Self::validate_hash(hash)?;

        let body = json!({
            "kind": "AuthorizationPolicy",
            "apiVersion": "security.istio.io/v1",
            "metadata": {
                "name": format!("istio-ap-odin-kafka-odn-{}", hash),
                "namespace": "odin"
            },
            "spec": {
                "action": "ALLOW",
                "selector": {
                    "matchLabels": {
                        "app": "odin-kafka"
                    }
                },
                "rules": [{
                    "from": [{
                        "source": {
                            "namespaces": [format!("odn-{}", hash)]
                        }
                    }],
                    "to": [{
                        "operation": {
                            "ports": ["9092"]
                        }
                    }]
                }]
            }
        });

        let url = format!(
            "{}/apis/security.istio.io/v1/namespaces/odin/authorizationpolicies",
            self.base_url
        );

        let result = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await?
            .json::<Value>()
            .await?;

        Ok(AuthorizationPolicyResponse {
            result,
            r#type: "AuthorizationPolicy".to_string(),
            name: format!("authorization-policy-{}", hash),
        })
    }

    /**
     * Deletes an Istio AuthorizationPolicy in Kubernetes based on the provided hash.
     */
    pub async fn delete(&self, hash: &str) -> Result<AuthorizationPolicyResponse, AuthorizationPolicyError> {
        Self::validate_hash(hash)?;

        let url = format!(
            "{}/apis/security.istio.io/v1/namespaces/odin/authorizationpolicies/istio-ap-odin-kafka-odn-{}",
            self.base_url, hash
        );

        let result = self
            .client
            .delete(&url)
            .send()
            .await?
            .json::<Value>()
            .await?;

        Ok(AuthorizationPolicyResponse {
            result,
            r#type: "AuthorizationPolicy".to_string(),
            name: format!("authorization-policy-{}", hash),
        })
    }
}