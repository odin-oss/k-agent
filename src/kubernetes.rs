use reqwest::Client;
use serde_json::Value;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum KubernetesApiError {
    #[error("Kubernetes is not activated")]
    NotActivated,
    #[error("Kubernetes API connection refused: {0}")]
    ConnectionRefused(String),
    #[error("Kubernetes API invalid URL: {0}")]
    InvalidUrl(String),
    #[error("Kubernetes API not responding: {0}")]
    NotResponding(String),
    #[error("Kubernetes API timed out or bad gateway")]
    TimedOut,
    #[error("Kubernetes API x509 certificate error: {0}")]
    X509Certificate(String),
    #[error("Object already exists: {0}")]
    AlreadyExists(String),
    #[error("Kubernetes API error ({0}): {1}")]
    ApiError(u16, String),
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),
    #[error("Kubernetes error: {0}")]
    Other(String),
}

pub struct KubernetesConfig {
    pub activated: bool,
    pub url: String,
    pub token: String,
}

/**
 * This object is responsible for communicating with the Kubernetes API. 
 * It has a fetch function that takes care of all the logic related to making requests, handling errors, and retrying when necessary. 
 * The rest of the codebase can use this client to interact with Kubernetes without worrying about the underlying details.
 */
pub struct KubernetesClient {
    client: Client,
    config: KubernetesConfig,
    retries: u32,
    retry_delay: Duration,
}
impl KubernetesClient {
    /**
     * Contructor for the KubernetesClient.
     */
    pub fn new(client: Client, config: KubernetesConfig) -> Self {
        Self {
            client,
            config,
            retries: 20,
            retry_delay: Duration::from_millis(3000),
        }
    }

    /**
     * Custome fetch function that handles all interactions with the Kubernetes API, including error handling and retries.
     */
    pub async fn fetch(
        &self,
        url: &str,
        method: &str,
        body: Option<&Value>,
    ) -> Result<Value, KubernetesApiError> {
        let full_url: String = format!("{}{}", self.config.url, url);
        for attempt in 1..=self.retries {
            let result = self.do_fetch(&full_url, method, body).await;
            match result {
                Ok(val) => return Ok(val),
                Err(e) => {
                    if attempt < self.retries && Self::is_retryable(&e) {
                        tokio::time::sleep(self.retry_delay).await;
                        continue;
                    }
                    return Err(e);
                }
            }
        }
        Err(KubernetesApiError::Other(
            "Exhausted all retry attempts".into(),
        ))
    }

    /**
     *  Here is the actual function that will be used to communicate with the Kubernetes API.
     */
    async fn do_fetch(
        &self,
        full_url: &str,
        method: &str,
        body: Option<&Value>,
    ) -> Result<Value, KubernetesApiError> {
        let mut request: reqwest::RequestBuilder = match method {
            "GET" => self.client.get(full_url),
            "POST" => self.client.post(full_url),
            "PUT" => self.client.put(full_url),
            "PATCH" => self.client.patch(full_url),
            "DELETE" => self.client.delete(full_url),
            _ => self.client.get(full_url),
        };

        request = request
            .header("Content-Type", "application/json")
            .bearer_auth(&self.config.token);

        if method != "GET" && method != "DELETE" {
            if let Some(b) = body {
                request = request.json(b);
            }
        }

        let res = request.send().await.map_err(|e| {
            let msg = e.to_string();
            if msg.contains("ECONNREFUSED") || msg.contains("Connection refused") {
                KubernetesApiError::ConnectionRefused(msg)
            } else if msg.contains("Invalid URL") || msg.contains("invalid URL") {
                KubernetesApiError::InvalidUrl(msg)
            } else {
                KubernetesApiError::RequestFailed(e)
            }
        })?;

        let status = res.status().as_u16();

        // Retryable status codes
        if status == 429 || status == 504 || status == 502 {
            return Err(KubernetesApiError::TimedOut);
        }

        // Conflict → already exists
        if status == 409 {
            let text = res.text().await.unwrap_or_default();
            return Err(KubernetesApiError::AlreadyExists(text));
        }

        // Parse response
        let content_type = res
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        if content_type.contains("application/json") {
            let data: Value = res.json().await?;

            if status >= 400 {
                return Err(KubernetesApiError::ApiError(
                    status,
                    data.to_string(),
                ));
            }

            // Check for AlreadyExists reason in JSON body
            if data.get("reason").and_then(|v| v.as_str()) == Some("AlreadyExists") {
                let msg = data["message"].as_str().unwrap_or("").to_string();
                return Err(KubernetesApiError::AlreadyExists(msg));
            }

            Ok(data)
        } else {
            let text = res.text().await.unwrap_or_default();

            if status >= 400 {
                return Err(KubernetesApiError::ApiError(status, text));
            }

            if text.contains("actively refused") {
                return Err(KubernetesApiError::ConnectionRefused(text));
            }
            if text.contains("x509: cannot verify signature") {
                return Err(KubernetesApiError::X509Certificate(text));
            }

            Ok(Value::String(text))
        }
    }

    /**
     * Looking at the error type to determine if the request should be retried or not.
     */
    fn is_retryable(err: &KubernetesApiError) -> bool {
        matches!(
            err,
            KubernetesApiError::TimedOut
                | KubernetesApiError::RequestFailed(_)
                | KubernetesApiError::NotResponding(_)
                | KubernetesApiError::ConnectionRefused(_)
        )
    }
}
