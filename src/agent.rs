use std::time::Duration;
use tokio::time;

/**
 * Continuously pings the /alive endpoint of the ODIN Hub every 5 seconds to indicate that the agent is alive.
 * # Arguments
 * * `client` - A reference to the reqwest::Client used for making HTTP requests
 * * `hub_url` - The base URL of the ODIN Hub to which the /alive endpoint belongs
 * # Behavior
 * This function runs indefinitely, sending a GET request to the /alive endpoint every 5 seconds
 */
pub async fn ping_alive(client: &reqwest::Client, hub_url: String) {
    let mut interval: time::Interval = time::interval(Duration::from_secs(5));
    loop {
        interval.tick().await;
        match client.get(&format!("{}/alive", hub_url)).send().await {
            Ok(resp) => println!("Ping /alive: {}", resp.status()),
            Err(e) => eprintln!("Ping /alive failed: {e}"),
        }
    }
}

/**
 * Registers a new agent with the given label and returns the assigned UUID. 
 * The agent type is hardcoded to "kubernetes".
 * # Arguments
 * * `client` - A reference to the reqwest::Client used for making HTTP requests
 * * `hub_url` - The base URL of the ODIN Hub to which the agent will be registered
 * * `label` - The label to assign to the new agent
 * # Returns
 * A Result containing the assigned UUID as a String if successful, or an error if the registration
 */
pub async fn register_agent(
    client: &reqwest::Client,
    hub_url: String,
    label: String,
) -> Result<String, Box<dyn std::error::Error>> {
    let body: serde_json::Value = serde_json::json!({ "label": label, "type": "kubernetes" });
    let resp: reqwest::Response = client
        .post(&format!("{}/agent", hub_url))
        .json(&body)
        .send()
        .await?;
    if resp.status().is_success() {
        let json_resp: serde_json::Value = resp.json().await?;
        if let Some(uuid) = json_resp["data"]["id_agent"].as_str() {
            println!("Please now set this UUID in the AGENT_UUID environment variable : {uuid}");
            Ok(uuid.to_string())
        } else {
            Err("Response JSON does not contain 'data.id_agent' field.".into())
        }
    } else {
        Err(format!("Failed to register agent. Status: {}", resp.status()).into())
    }
}
