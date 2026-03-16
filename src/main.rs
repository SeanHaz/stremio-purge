use chrono::Utc;
use log::{debug, info, warn};
use reqwest::Client;
use serde_json::{json, Value};
use std::error::Error;
use std::fs;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("Starting Continue Watching cleanup script");

    let state_file = "state.json";
    debug!("State file path: {}", state_file);

    let state_data: Value = if Path::new(state_file).exists() {
        debug!("Loading existing state.json");
        let content = fs::read_to_string(state_file)?;
        serde_json::from_str(&content).unwrap_or_else(|_| {
            warn!("Failed to parse state.json, starting with empty state");
            json!({})
        })
    } else {
        debug!("No state file found - first run");
        json!({})
    };

    let mut auth_key = state_data["auth_key"].as_str().unwrap_or("").to_string();
    let stored_val = state_data["last_timestamp"].as_u64().unwrap_or(0);
    let mut current_max_timestamp = stored_val;

    debug!("Auth key present: {}", !auth_key.is_empty());
    debug!("Last processed timestamp from state: {}", stored_val);

    let http = Client::builder().build()?;

    async fn login_flow(http: &Client) -> Result<String, Box<dyn Error>> {
        info!("Token missing or expired. Logging in...");
        let email = std::env::var("STREMIO_EMAIL").expect("STREMIO_EMAIL not set");
        let password = std::env::var("STREMIO_PASSWORD").expect("STREMIO_PASSWORD not set");

        let resp = http.post("https://api.strem.io/api/login")
            .json(&json!({"email": email, "password": password}))
            .send().await?
            .json::<Value>().await?;

        let key = resp["result"]["authKey"].as_str().unwrap_or("").to_string();
        debug!("Login successful - new auth key obtained");
        Ok(key)
    }

    if auth_key.is_empty() {
        auth_key = login_flow(&http).await?;
    }

    debug!("Fetching datastore metadata...");
    let mut resp = http.post("https://api.strem.io/api/datastoreMeta")
        .json(&json!({"authKey": auth_key, "collection": "libraryItem"}))
        .send().await?;

    if resp.status().as_u16() == 401 || resp.status().as_u16() == 403 {
        warn!("Authentication failed (status {}). Re-logging in...", resp.status());
        auth_key = login_flow(&http).await?;
        resp = http.post("https://api.strem.io/api/datastoreMeta")
            .json(&json!({"authKey": auth_key, "collection": "libraryItem"}))
            .send().await?;
    }

    let data: Value = resp.json().await?;
    let items = data["result"].as_array().cloned().unwrap_or_default();

    info!("Retrieved {} library items from datastore metadata", items.len());

    for pair in items {
        let id = pair[0].as_str().unwrap_or("");
        let timestamp = pair[1].as_u64().unwrap_or(0);

        debug!("Checking item ID: {} (timestamp: {})", id, timestamp);

        if timestamp > current_max_timestamp {
            current_max_timestamp = timestamp;
        }

        if timestamp > stored_val {
            debug!("Item {} has newer timestamp → fetching full details", id);

            let item_payload = json!({
                "authKey": auth_key,
                "collection": "libraryItem",
                "ids": [id]
            });

            let item_resp = http.post("https://api.strem.io/api/datastoreGet")
                .json(&item_payload)
                .send()
                .await?;

            let item_data: Value = item_resp.json().await?;
            let results = item_data["result"].as_array();

            if let Some(res_array) = results {
                if !res_array.is_empty() {
                    let item = &res_array[0];
                    let removed = item["removed"].as_bool().unwrap_or(false);
                    let temp = item["temp"].as_bool().unwrap_or(false);

                    debug!("Item {} status → removed: {}, temp: {}", id, removed, temp);

                    if removed != true || temp != false {
                        info!("Updating item {} (setting removed=true, temp=false)", id);

                        let mut updated_item = item.clone();
                        updated_item["removed"] = json!(true);
                        updated_item["temp"] = json!(false);
                        updated_item["_mtime"] = json!(Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));

                        let output = json!({
                            "authKey": auth_key,
                            "collection": "libraryItem",
                            "changes": [updated_item]
                        });

                        info!("{}", serde_json::to_string_pretty(&output)?);
                        info!("############################################################");

                        let put_resp = http.post("https://api.strem.io/api/datastorePut")
                            .json(&output)
                            .send()
                            .await?;

                        let put_json: Value = put_resp.json().await?;

                        info!("Response from server: {}", serde_json::to_string_pretty(&put_json)?);
                    }
                }
            }
        }
    }

    let final_state = json!({
        "auth_key": auth_key,
        "last_timestamp": current_max_timestamp
    });

    debug!("Saving updated state (new max timestamp: {})", current_max_timestamp);
    fs::write(state_file, serde_json::to_string_pretty(&final_state)?)?;

    info!("Finished successfully. Last processed timestamp updated to {}", current_max_timestamp);

    Ok(())
}
