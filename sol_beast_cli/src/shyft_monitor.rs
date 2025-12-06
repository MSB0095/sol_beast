use crate::models::{Holding, PriceCache};
use crate::settings::Settings;
use base64::{engine::general_purpose::STANDARD as Base64Engine, Engine};
use futures_util::{SinkExt, StreamExt};
use log::{debug, error, info, warn};
use sol_beast_core::shyft::{
    AccountSubscriptionResponse, GraphQLResponse, NewTokenSubscriptionResponse, ShyftService,
    ShyftTransaction,
};
use solana_program::pubkey::Pubkey;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::{connect_async, tungstenite::Message};

pub enum ShyftMonitorMessage {
    NewToken(ShyftTransaction),
}

pub enum ShyftControlMessage {
    SubscribePrice(String),   // mint
    UnsubscribePrice(String), // mint
}

fn get_bonding_curve_pda(mint: &str, program_id: &str) -> Option<String> {
    let mint_pubkey = Pubkey::from_str(mint).ok()?;
    let program_pubkey = Pubkey::from_str(program_id).ok()?;
    let (pda, _) = Pubkey::find_program_address(
        &[b"bonding-curve", mint_pubkey.as_ref()],
        &program_pubkey,
    );
    Some(pda.to_string())
}

pub async fn start_shyft_monitor(
    settings: Arc<Settings>,
    holdings: Arc<Mutex<HashMap<String, Holding>>>,
    price_cache: Arc<Mutex<PriceCache>>,
    tx: mpsc::Sender<ShyftMonitorMessage>,
    mut control_rx: mpsc::Receiver<ShyftControlMessage>,
) {
    let shyft_service = match ShyftService::new(&settings) {
        Some(s) => s,
        None => {
            error!("Shyft API key not configured. Shyft monitor cannot start.");
            return;
        }
    };

    // Shyft GraphQL WSS URL
    // Ensure we use the wss:// protocol
    let base_url = settings.shyft_graphql_url.replace("https://", "wss://").replace("http://", "ws://");
    let url_str = format!("{}?api_key={}", base_url, shyft_service.api_key);

    info!("Connecting to Shyft GraphQL WebSocket at {}", base_url);

    loop {
        match connect_async(&url_str).await {
            Ok((ws_stream, _)) => {
                info!("Connected to Shyft GraphQL WSS");
                let (mut write, mut read) = ws_stream.split();

                // 1. Send Connection Init (graphql-transport-ws)
                let init_msg = serde_json::json!({ "type": "connection_init", "payload": {} }).to_string();
                if let Err(e) = write.send(Message::Text(init_msg)).await {
                    error!("Failed to send connection_init: {}", e);
                    continue;
                }

                // 2. Subscribe to New Tokens (ID: "new_tokens")
                let new_token_query =
                    shyft_service.get_new_token_subscription_query(&settings.pump_fun_program);
                let start_msg = serde_json::json!({
                    "id": "new_tokens",
                    "type": "start",
                    "payload": serde_json::from_str::<serde_json::Value>(&new_token_query).unwrap()
                })
                .to_string();
                if let Err(e) = write.send(Message::Text(start_msg)).await {
                    error!("Failed to subscribe to new tokens: {}", e);
                }

                // Map to track active price subscriptions: mint -> subscription_id
                let mut price_subs: HashMap<String, String> = HashMap::new();

                loop {
                    tokio::select! {
                        // Handle incoming WebSocket messages
                        msg = read.next() => {
                            match msg {
                                Some(Ok(Message::Text(text))) => {
                                    let v: serde_json::Value = match serde_json::from_str(&text) {
                                        Ok(v) => v,
                                        Err(_) => continue,
                                    };
                                    let msg_type = v["type"].as_str().unwrap_or("");

                                    match msg_type {
                                        "connection_ack" => {
                                            info!("Shyft GraphQL connection acknowledged");
                                        }
                                        "data" => {
                                            let id = v["id"].as_str().unwrap_or("");
                                            let payload = &v["payload"];

                                            if id == "new_tokens" {
                                                if let Ok(resp) = serde_json::from_value::<GraphQLResponse<NewTokenSubscriptionResponse>>(payload.clone()) {
                                                    if let Some(data) = resp.data {
                                                        for tx_data in data.Transaction {
                                                            let _ = tx.send(ShyftMonitorMessage::NewToken(tx_data)).await;
                                                        }
                                                    }
                                                }
                                            } else if id.starts_with("price_") {
                                                // Handle price update
                                                if let Ok(resp) = serde_json::from_value::<GraphQLResponse<AccountSubscriptionResponse>>(payload.clone()) {
                                                    if let Some(data) = resp.data {
                                                        for acc in data.Account {
                                                            let mint = id.strip_prefix("price_").unwrap_or("");
                                                            if !mint.is_empty() {
                                                                if let Ok(bytes) = Base64Engine.decode(&acc.data) {
                                                                    // Parse bonding curve state manually
                                                                    // Skip 8 bytes discriminator, then parse virtual reserves
                                                                    if bytes.len() >= 24 {
                                                                        let slice = &bytes[8..];
                                                                        if slice.len() >= 16 {
                                                                            let virtual_token_reserves = u64::from_le_bytes(slice[0..8].try_into().expect("slice length verified"));
                                                                            let virtual_sol_reserves = u64::from_le_bytes(slice[8..16].try_into().expect("slice length verified"));
                                                                            if virtual_token_reserves > 0 {
                                                                                let price = virtual_sol_reserves as f64 / virtual_token_reserves as f64;
                                                                                let mut cache = price_cache.lock().await;
                                                                                cache.put(mint.to_string(), (std::time::Instant::now(), price));
                                                                                debug!("Shyft price update for {}: {}", mint, price);
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        "error" => {
                                            error!("Shyft GraphQL error: {:?}", v["payload"]);
                                        }
                                        _ => {}
                                    }
                                }
                                Some(Ok(Message::Close(_))) => {
                                    warn!("Shyft WSS closed");
                                    break;
                                }
                                Some(Err(e)) => {
                                    error!("Shyft WSS error: {}", e);
                                    break;
                                }
                                None => {
                                    break;
                                }
                                _ => {}
                            }
                        }

                        // Handle control messages (Subscribe/Unsubscribe prices)
                        Some(ctrl) = control_rx.recv() => {
                            match ctrl {
                                ShyftControlMessage::SubscribePrice(mint) => {
                                    if !price_subs.contains_key(&mint) {
                                        if let Some(pda) = get_bonding_curve_pda(&mint, &settings.pump_fun_program) {
                                            let query = shyft_service.get_account_subscription_query(&[pda]);
                                            let id = format!("price_{}", mint);
                                            let start_msg = serde_json::json!({
                                                "id": id,
                                                "type": "start",
                                                "payload": serde_json::from_str::<serde_json::Value>(&query).unwrap()
                                            }).to_string();
                                            if write.send(Message::Text(start_msg)).await.is_ok() {
                                                price_subs.insert(mint.clone(), id);
                                                debug!("Subscribed to price updates for {}", mint);
                                            }
                                        }
                                    }
                                }
                                ShyftControlMessage::UnsubscribePrice(mint) => {
                                    if let Some(id) = price_subs.remove(&mint) {
                                        let stop_msg = serde_json::json!({
                                            "id": id,
                                            "type": "stop"
                                        }).to_string();
                                        let _ = write.send(Message::Text(stop_msg)).await;
                                        debug!("Unsubscribed from price updates for {}", mint);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to connect to Shyft WSS: {}", e);
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    }
}
