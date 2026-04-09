use std::env;
use std::time::Duration;
use tokio::time;

mod object {
    pub mod authorization_policy;
    pub mod config_map;
    pub mod deployment;
    pub mod ingress;
    pub mod namespace;
    pub mod network_policy;
    pub mod pvc;
    pub mod registry_hub;
    pub mod service;
    pub mod storage_carrier;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Getting env variables from .env file
    dotenvy::dotenv().ok();
    let hub_url: String =
        env::var("ODIN_HUB_URL").expect("ODIN_HUB_URL must be set in .env file or environment.");
    let agent_label: Option<String> = env::var("AGENT_LABEL").ok();
    let agent_uuid: Option<String> = env::var("AGENT_UUID").ok();

    println!("-----");
    // Creating the reqwest::Client
    let client: reqwest::Client = reqwest::Client::new();

    if let Some(ref uuid) = agent_uuid {
        println!("[ODIN][K-AGENT] Agent UUID: {uuid}");

        // Run ping_alive forever (blocks main)
        ping_alive(&client, hub_url.clone()).await;
    } else if let Some(ref label) = agent_label {
        println!(
            "[ODIN][K-AGENT] No AGENT_UUID set, but AGENT_LABEL found: {label}. Need to register new agent."
        );
        register_agent(&client, hub_url.clone(), label.clone()).await?;
    } else {
        println!("[ODIN][K-AGENT] No AGENT_UUID or AGENT_LABEL set.");
    }

    Ok(())
}

async fn ping_alive(client: &reqwest::Client, hub_url: String) {
    let mut interval = time::interval(Duration::from_secs(5));
    loop {
        interval.tick().await;
        match client.get(&format!("{}/alive", hub_url)).send().await {
            Ok(resp) => println!("Ping /alive: {}", resp.status()),
            Err(e) => eprintln!("Ping /alive failed: {e}"),
        }
    }
}

async fn register_agent(
    client: &reqwest::Client,
    hub_url: String,
    label: String,
) -> Result<String, Box<dyn std::error::Error>> {
    let body = serde_json::json!({ "label": label, "type": "kubernetes" });
    let resp = client
        .post(&format!("{}/agent", hub_url))
        .json(&body)
        .send()
        .await?;
    if resp.status().is_success() {
        let json_resp: serde_json::Value = resp.json().await?;
        if let Some(uuid) = json_resp["data"]["id_agent"].as_str() {
            println!("Please now agent this UUID in the AGENT_UUID: {uuid}");
            Ok(uuid.to_string())
        } else {
            Err("Response JSON does not contain 'data.id_agent' field.".into())
        }
    } else {
        Err(format!("Failed to register agent. Status: {}", resp.status()).into())
    }
}
