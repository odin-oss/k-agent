use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IngressError {
    #[error("Invalid hash: must be exactly 6 characters")]
    InvalidHash,
    #[error("Invalid field '{0}': must not be empty")]
    EmptyField(String),
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),
}

#[derive(Debug, Clone)]
pub struct PortType {
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct IngressPort {
    pub port: u32,
    pub label: String,
    pub port_type: PortType,
}

pub struct AddInKongProps {
    pub hash: String,
    pub ports: Vec<IngressPort>,
    pub label: String,
}

pub struct DeleteFromKongProps {
    pub hash: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IngressResponse {
    pub result: Value,
    pub r#type: String,
    pub name: String,
}

pub struct Ingress {
    client: Client,
    kong_url: String,
    tls_odin_monolith: String,
}

impl Ingress {
    pub fn new(client: Client, kong_url: impl Into<String>, tls_odin_monolith: impl Into<String>) -> Self {
        Self {
            client,
            kong_url: kong_url.into(),
            tls_odin_monolith: tls_odin_monolith.into(),
        }
    }

    fn validate_hash(hash: &str) -> Result<(), IngressError> {
        if hash.len() != 6 {
            return Err(IngressError::InvalidHash);
        }
        Ok(())
    }

    fn validate_not_empty(value: &str, field: &str) -> Result<(), IngressError> {
        if value.is_empty() {
            return Err(IngressError::EmptyField(field.to_string()));
        }
        Ok(())
    }

    pub async fn add_in_kong(&self, props: AddInKongProps) -> Result<Vec<IngressResponse>, IngressError> {
        Self::validate_hash(&props.hash)?;
        Self::validate_not_empty(&props.label, "label")?;

        // Creating Services in Kong
        for port in &props.ports {
            let url = format!("{}/services", self.kong_url);
            let service_name = format!(
                "{}-{}-{}",
                props.hash,
                props.label,
                port.label.to_lowercase()
            );
            let service_url = format!(
                "http://ci{}{}{}-proxy:{}",
                props.label, props.hash, port.port, port.port
            );
            let body = json!({
                "name": service_name,
                "url": service_url,
            });
            self.client.post(&url).json(&body).send().await?;
        }

        // Getting Services from Kong to retrieve their IDs
        let mut services = vec![];
        for port in &props.ports {
            let service_name = format!(
                "{}-{}-{}",
                props.hash,
                props.label,
                port.label.to_lowercase()
            );
            let url = format!("{}/services/{}", self.kong_url, service_name);
            let svc = self.client.get(&url).send().await?.json::<Value>().await?;
            services.push(svc);
        }

        // Creating Routes and Plugins in Kong
        let mut responses = vec![];
        for (i, svc) in services.iter().enumerate() {
            let svc_id = svc["id"].as_str().unwrap_or_default();
            let port = &props.ports[i];

            // Enable odin-auth plugin for authentication
            let plugin_url = format!("{}/services/{}/plugins", self.kong_url, svc_id);
            let plugin_body = json!({
                "name": "odin-auth",
                "config": {
                    "auth_url": format!("https://{}/auth", self.tls_odin_monolith),
                },
            });
            let plugin_result = self.client
                .post(&plugin_url)
                .json(&plugin_body)
                .send()
                .await?
                .json::<Value>()
                .await?;
            responses.push(IngressResponse {
                result: plugin_result,
                r#type: "Plugin".to_string(),
                name: "odin-auth".to_string(),
            });

            // Create Route
            let route_url = format!("{}/services/{}/routes", self.kong_url, svc_id);
            let path_base = format!(
                "/{}/{}-{}",
                props.hash,
                props.label,
                port.label.to_lowercase()
            );
            let strip_path = port.port_type.label.to_lowercase() == "strip_path";
            let route_body = json!({
                "paths": [
                    format!("{}/", path_base),
                    path_base,
                ],
                "strip_path": strip_path,
                "preserve_host": true,
            });
            let route_result = self.client
                .post(&route_url)
                .json(&route_body)
                .send()
                .await?
                .json::<Value>()
                .await?;
            responses.push(IngressResponse {
                result: route_result,
                r#type: "Route".to_string(),
                name: format!("{}-{}", props.hash, port.label.to_lowercase()),
            });
        }

        Ok(responses)
    }

    pub async fn delete_from_kong(&self, props: DeleteFromKongProps) -> Result<Vec<IngressResponse>, IngressError> {
        Self::validate_hash(&props.hash)?;

        // Get all services and routes
        let services_url = format!("{}/services", self.kong_url);
        let routes_url = format!("{}/routes", self.kong_url);

        let services_result = self.client.get(&services_url).send().await?.json::<Value>().await?;
        let routes_result = self.client.get(&routes_url).send().await?.json::<Value>().await?;

        // Filter routes that contain the hash in their paths
        let empty_routes = vec![];
        let filtered_routes: Vec<&Value> = routes_result["data"]
            .as_array()
            .unwrap_or(&empty_routes)
            .iter()
            .filter(|route| {
                route["paths"]
                    .as_array()
                    .map(|paths| {
                        paths.iter().any(|p| {
                            p.as_str().map(|s| s.contains(&props.hash)).unwrap_or(false)
                        })
                    })
                    .unwrap_or(false)
            })
            .collect();

        // Filter services that contain the hash in their host
        let empty_services = vec![];
        let filtered_services: Vec<&Value> = services_result["data"]
            .as_array()
            .unwrap_or(&empty_services)
            .iter()
            .filter(|svc| {
                svc["host"]
                    .as_str()
                    .map(|h| h.contains(&props.hash))
                    .unwrap_or(false)
            })
            .collect();

        let mut responses = vec![];

        // Delete filtered routes
        for route in &filtered_routes {
            let route_id = route["id"].as_str().unwrap_or_default();
            let url = format!("{}/routes/{}", self.kong_url, route_id);
            let result = self.client.delete(&url).send().await?.json::<Value>().await?;
            responses.push(IngressResponse {
                result,
                r#type: "Route".to_string(),
                name: route_id.to_string(),
            });
        }

        // Delete filtered services
        for svc in &filtered_services {
            let svc_id = svc["id"].as_str().unwrap_or_default();
            let url = format!("{}/services/{}", self.kong_url, svc_id);
            let result = self.client.delete(&url).send().await?.json::<Value>().await?;
            responses.push(IngressResponse {
                result,
                r#type: "Service".to_string(),
                name: svc_id.to_string(),
            });
        }

        Ok(responses)
    }
}
