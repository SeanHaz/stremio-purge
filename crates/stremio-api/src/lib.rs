use std::error::Error;
use chrono::Utc;
use log::{debug, info};
use reqwest::Client;
use serde_json::{json, Value};
use db::models::{SYNC_SERIES, SYNC_MOVIES, SYNC_TV, User};

pub const CHUNK_SIZE: usize = 50;

pub async fn update_library_flow(http: &Client, user: &User) -> Option<i64> {
    let metadata = get_metadata(&http, &user.auth_key).await.unwrap_or_default();
    info!("metadata: {}", metadata);
    let (modified, max_timestamp) = get_modified(metadata, user.get_min_active());
    info!("modified: {}, max_timestamp: {}", modified, max_timestamp);
    let library_data = get_library_data(&http, modified, &user.auth_key).await.unwrap_or_default();
    update_and_push(&http, &user, library_data).await.ok()?;
    Some(max_timestamp)
}

pub async fn update_and_push(http: &Client, user: &User, library_data: Value) -> anyhow::Result<Value>{
    let mut types_to_update = Vec::new();
    if user.is_bit_active(SYNC_TV) { types_to_update.push("tv")}
    if user.is_bit_active(SYNC_SERIES) { types_to_update.push("series")}
    if user.is_bit_active(SYNC_MOVIES) { types_to_update.push("movie")}

    let current_time = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    let changes: Vec<Value> = library_data["result"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|mut item| {
            let item_type = item["type"].as_str().unwrap_or("");
            let item_temp = match item.get("temp") {
                Some(Value::Bool(b)) => *b,
                Some(Value::String(s)) => {
                    match s.trim().to_lowercase().as_str() {
                        "false" => false,
                        _ => true,
                    }
                }
                _ => true,
            };

            let item_removed = match item.get("removed") {
                Some(Value::Bool(b)) => *b,
                Some(Value::String(s)) => {
                    match s.trim().to_lowercase().as_str() {
                        "true" => true,
                        _ => false,
                    }
                }
                _ => false,
            };
            
            info!("type: {}, item_remove: {}, item_temp: {}", item_type, item_removed, item_temp);
            
            if types_to_update.contains(&item_type) && ( !item_removed || item_temp ) {
                item["removed"] = json!(true);
                item["temp"] = json!(false);
                item["_mtime"] = json!(current_time);

                Some(item)
            } else {
                None
            }
            
        })
        .collect();

    for chunk in changes.chunks(CHUNK_SIZE){
        let payload = json!({
            "authKey": user.auth_key,
            "collection": "libraryItem",
            "changes": chunk,
        });
        let resp = http.post("https://api.strem.io/api/datastorePut")
            .json(&payload)
            .send().await?
            .json::<serde_json::Value>()
            .await?;
        
        if let Some(success) = resp["result"]["success"].as_bool() {
            if !success {
                return Err(anyhow::anyhow!("Stremio API error"));
            }
        } else if let Some(err) = resp["error"]["message"].as_str() {
            return Err(anyhow::anyhow!("Stremio API error: {}", err));
        }
    }

    Ok(json!({
        "result": {
            "success": true
        }
    }))
}


pub async fn get_library_data(http: &Client, library: Value, auth_key: &str) -> Result<serde_json::Value, reqwest::Error> {
    let ids_array = match library.as_array() {
        Some(arr) => arr,
        None => return Ok(json!({"result": []})),
    };

    if ids_array.is_empty() {
        return Ok(json!({"result": []}));
    }

    let mut all_items = Vec::new();
    
    for chunk in ids_array.chunks(CHUNK_SIZE){
        let payload = json!({
            "authKey": auth_key,
            "collection": "libraryItem",
            "ids": chunk
        });

        let resp = http.post("https://api.strem.io/api/datastoreGet")
            .json(&payload)
            .send().await?
            .json::<serde_json::Value>()
            .await?;

        if let Some(items) = resp["result"].as_array() {
            all_items.extend_from_slice(items);
        }
    }
    

    Ok(json!({ "result": all_items }))
}


pub fn get_modified(metadata: Value, min_active: i64) -> (Value, i64) {
    let mut max_timestamp = 0;     
    
    let ids: Value = metadata
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|entry| {
            let id = entry.get(0)?.as_str()?;
            let ts = entry.get(1)?.as_i64()?;

            if ts > max_timestamp {
                max_timestamp = ts;
            }

            if ts > min_active {
                Some(id.to_string())
            } else {
                None
            }
        })
        .collect();
    
    (ids, max_timestamp)
}

pub async fn get_metadata(http: &Client, auth_key: &str) -> Result<Value, Box<dyn Error>> {
    let resp = http.post("https://api.strem.io/api/datastoreMeta")
        .json(&json!({
            "authKey": auth_key, 
            "collection": "libraryItem"
        }))
        .send()
        .await?;

    let data: Value = resp.json().await?;

    if let Some(err_msg) = data["error"]["message"].as_str() {
        info!("Invalid Login credentials or API error: {}", err_msg);
        return Err(err_msg.into());
    }

    let items = data["result"].clone();

    Ok(items)
}


pub async fn get_auth(http: &Client, email: &str, password: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
    let resp = http.post("https://api.strem.io/api/login")
        .json(&json!({"email": email, "password": password}))
        .send().await?
        .json::<Value>().await?;

    let key = resp["result"]["authKey"]
        .as_str()
        .ok_or_else(|| {
            resp["error"]["message"]
                .as_str()
                .unwrap_or("Unknown error getting auth_key")
                .to_string()
        })?
        .to_string();
    
    debug!("Login successful - new auth key obtained");
    Ok(key)
}

pub async fn get_user_id(http: &Client, auth_key: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
    info!("Getting user id");
    let resp = http.post("https://api.strem.io/api/getUser")
	.json(&json!({"authKey": auth_key}))
	.send().await?
	.json::<Value>().await?;

    let id = resp["result"]["_id"]
        .as_str()
        .ok_or_else(|| {
            resp["error"]["message"]
                .as_str()
                .unwrap_or("Unknown error getting user_id")
        })?
        .to_string();
    info!("Fectched id: {}", !id.is_empty());
    Ok(id)
}
