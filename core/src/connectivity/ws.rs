use crate::{models::Holding, models::PriceCache};
use crate::Settings;
use log::{debug, error, info};
use lru::LruCache;
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{mpsc, Mutex, oneshot};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{stream::StreamExt, SinkExt};

#[derive(Debug)]
pub enum WsRequest {
    Subscribe {
        account: String,
        mint: String,
        resp: oneshot::Sender<Result<u64, String>>,
    },
    Unsubscribe {
        sub_id: u64,
        resp: oneshot::Sender<Result<(), String>>,
    },
    GetHealth {
        resp: oneshot::Sender<WsHealth>,
    },
}

#[derive(Debug, Clone)]
pub struct WsHealth {
    pub active_subs: usize,
    pub pending_subs: usize,
    pub recent_timeouts: usize,
    pub is_healthy: bool,
}

pub async fn run_ws(
    wss_url: &str,
    tx: mpsc::Sender<String>,
    seen: Arc<Mutex<LruCache<String, ()>>>,
    holdings: Arc<Mutex<HashMap<String, Holding>>>,
    price_cache: Arc<Mutex<PriceCache>>,
    mut control_rx: mpsc::Receiver<WsRequest>,
    _settings: Arc<Settings>,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting WebSocket connection to {}", wss_url);
    
    // Connect to WebSocket
    let (ws_stream, _) = connect_async(wss_url).await.map_err(|e| {
        error!("Failed to connect to WebSocket: {}", e);
        e
    })?;
    
    let (mut write, mut read) = ws_stream.split();
    let mut ping_interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
    
    let tx_clone1 = tx.clone();
    let tx_clone2 = tx.clone();
    let holdings_clone = holdings.clone();
    let price_cache_clone = price_cache.clone();
    let seen_clone = seen.clone();
    
    // Spawn a task to handle ping messages
    let ping_task = tokio::spawn(async move {
        loop {
            ping_interval.tick().await;
            let ping_msg = Message::Ping(vec![].into());
            if let Err(e) = write.send(ping_msg).await {
                error!("Failed to send ping: {}", e);
                break;
            }
        }
    });
    
    // Handle incoming WebSocket messages
    let message_handler = tokio::spawn(async move {
        while let Some(message) = read.next().await {
            match message {
                Ok(Message::Text(text)) => {
                    debug!("Received WebSocket message: {}", text);
                    
                    // Parse the message and handle it
                    if let Ok(value) = serde_json::from_str::<Value>(&text) {
                        handle_websocket_message(
                            &value,
                            &holdings_clone,
                            &price_cache_clone,
                            &seen_clone,
                        ).await;
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("WebSocket connection closed");
                    break;
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }
    });
    
    // Handle control messages
    let control_handler = tokio::spawn(async move {
        while let Some(request) = control_rx.recv().await {
            match request {
                WsRequest::Subscribe { account, mint, resp } => {
                            info!("Subscribing to account: {}, mint: {}", account, mint);
                            let subscribe_msg = format!(r#"{{"method":"subscribeNewToken","keys":["{}"]}}"#, account);
                            
                            if let Ok(_) = tx_clone1.send(subscribe_msg).await {
                                let _ = resp.send(Ok(1)); // Mock subscription ID
                            } else {
                                let _ = resp.send(Err("Failed to send subscription".to_string()));
                            }
                        }
                        WsRequest::Unsubscribe { sub_id, resp } => {
                            info!("Unsubscribing from: {}", sub_id);
                            let unsubscribe_msg = format!(r#"{{"method":"unsubscribe","id":{}}}"#, sub_id);
                            
                            if let Ok(_) = tx_clone2.send(unsubscribe_msg).await {
                                let _ = resp.send(Ok(()));
                            } else {
                                let _ = resp.send(Err("Failed to send unsubscribe".to_string()));
                            }
                        }
                        WsRequest::GetHealth { resp } => {
                            let health = WsHealth {
                                active_subs: 0, // Mock values
                                pending_subs: 0,
                                recent_timeouts: 0,
                                is_healthy: true,
                            };
                            let _ = resp.send(health);
                        }
                    }
                }
            });
    
    // Wait for any task to complete
    tokio::select! {
        _ = ping_task => {
            info!("Ping task completed");
        }
        _ = message_handler => {
            info!("Message handler completed");
        }
        _ = control_handler => {
            info!("Control handler completed");
        }
    }
    
    Ok(())
}

async fn handle_websocket_message(
    value: &Value,
    _holdings: &Arc<Mutex<HashMap<String, Holding>>>,
    price_cache: &Arc<Mutex<PriceCache>>,
    seen: &Arc<Mutex<LruCache<String, ()>>>,
) {
    if let Some(method) = value.get("method") {
        match method.as_str() {
            Some("newToken") => {
                if let Some(data) = value.get("params").or(value.get("data")) {
                    if let Some(mint) = data.get("mint").and_then(|m| m.as_str()) {
                        debug!("New token detected: {}", mint);
                        
                        // Check if we've seen this mint before
                        {
                            let mut seen_guard = seen.lock().await;
                            if seen_guard.contains(&mint.to_string()) {
                                return;
                            }
                            seen_guard.put(mint.to_string(), ());
                        }
                        
                        // Add to price cache with a placeholder price
                        {
                            let mut cache_guard = price_cache.lock().await;
                            cache_guard.put(mint.to_string(), (std::time::Instant::now(), 0.0));
                        }
                        
                        info!("Processed new token: {}", mint);
                    }
                }
            }
            _ => {
                debug!("Unhandled WebSocket method: {}", method);
            }
        }
    }
}
