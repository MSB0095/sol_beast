use crate::{models::Holding, models::PriceCache};
use crate::Settings;
use log::{debug, error, info, warn};
use lru::LruCache;
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{mpsc, Mutex, oneshot};
#[cfg(not(target_arch = "wasm32"))]
use tokio_tungstenite::connect_async_tls_with_config;
#[cfg(target_arch = "wasm32")]
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::Connector;
#[cfg(not(target_arch = "wasm32"))]
use native_tls::TlsConnector as NativeTlsConnector;
use futures_util::{stream::StreamExt, SinkExt};

/// Outgoing message type for the WebSocket writer task. This keeps all
/// outgoing messages in a single channel allowing a single task to own
/// the `write` sink returned by `split()`.
#[derive(Debug)]
pub enum OutgoingMessage {
    Text(String),
    Ping,
    Raw(Message),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use crate::config::settings::Settings;
    use std::num::NonZeroUsize;
    use lru::LruCache;
    use tokio::sync::mpsc;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_handle_transaction_notification_detects_mint() {
        let settings_toml = format!(r#"
solana_ws_urls = []
solana_rpc_urls = ["http://localhost:8899"]
pump_fun_program = ""
metadata_program = ""
tp_percent = 30.0
sl_percent = -20.0
timeout_secs = 3600
cache_capacity = 16
price_cache_ttl_secs = 60
buy_amount = 0.1
"#);
        let settings = Arc::new(Settings::from_toml_str(&settings_toml).unwrap());

        let seen = Arc::new(Mutex::new(LruCache::<String, ()>::new(NonZeroUsize::new(16).unwrap())));
        let price_cache = Arc::new(Mutex::new(LruCache::<String, (std::time::Instant, f64)>::new(NonZeroUsize::new(16).unwrap())));
        let holdings = Arc::new(Mutex::new(HashMap::new()));
        let (detected_tx, mut detected_rx) = mpsc::channel::<String>(4);

        let msg = json!({"method":"transactionNotification","params":{"result":{"type":"TOKEN_MINT","tokenTransfers":[{"mint":"MINT123"}]}}});
        handle_websocket_message(&msg, &holdings, &price_cache, &seen, &settings, &Some(detected_tx)).await;

        // Received detected token in channel
        if let Some(received) = detected_rx.recv().await {
            assert_eq!(received, "MINT123");
        } else {
            panic!("Expected a detected mint to be sent to the detector channel");
        }
    }
}

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
    tx: mpsc::Sender<OutgoingMessage>,
    mut outgoing_rx: mpsc::Receiver<OutgoingMessage>,
    seen: Arc<Mutex<LruCache<String, ()>>>,
    holdings: Arc<Mutex<HashMap<String, Holding>>>,
    price_cache: Arc<Mutex<PriceCache>>,
    mut control_rx: mpsc::Receiver<WsRequest>,
    _settings: Arc<Settings>,
    detected_tx: Option<mpsc::Sender<String>>,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting WebSocket connection to {}", wss_url);

    // Prefer using the platform-native TLS (native-tls) on native builds. This
    // will use the OS trust store and avoid 'UnknownIssuer' when system CAs
    // are present.
    #[cfg(not(target_arch = "wasm32"))]
    let tls_connector = {
        info!("Using platform-native TLS connector for WebSocket connections (native-tls)");
        let mut builder = NativeTlsConnector::builder();
        if _settings.disable_tls_verification {
            info!("Disabling TLS verification for WebSocket (unsafe)");
            builder.danger_accept_invalid_certs(true);
        }
        match builder.build() {
            Ok(conn) => Connector::NativeTls(conn),
            Err(e) => {
                error!("Failed to build native TLS connector: {}", e);
                // Fall back to no connector and rely on default behavior
                Connector::Plain
            }
        }
    };

    // Connect to WebSocket - pass TLS connector on native platforms to ensure
    // we trust system roots. For non-TLS (ws://) we can use a plain connection.
    #[cfg(not(target_arch = "wasm32"))]
    let (ws_stream, _) = connect_async_tls_with_config(
        wss_url,
        None,
        false,
        Some(tls_connector),
    )
    .await
    .map_err(|e| {
        // Provide guidance for TLS validation failures caused by missing CA bundles
        let err_str = e.to_string();
        if err_str.contains("UnknownIssuer") || err_str.contains("invalid peer certificate") {
            error!("TLS validation error when connecting to {}: {}", wss_url, err_str);
            error!("This often means the system trust store is missing or a proxy is intercepting TLS. On Debian/Ubuntu, ensure the 'ca-certificates' package is installed; on other distros, make sure system roots are present.");
        } else {
            error!("Failed to connect to WebSocket: {}", e);
        }
        e
    })?;

    #[cfg(target_arch = "wasm32")]
    let (ws_stream, _) = connect_async(wss_url).await.map_err(|e| {
        error!("Failed to connect to WebSocket: {}", e);
        e
    })?;
    
    let (mut write, mut read) = ws_stream.split();
    let mut ping_interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
    
    let tx_for_ping = tx.clone();
    let tx_clone1 = tx.clone();
    let tx_clone2 = tx.clone();
    let holdings_clone = holdings.clone();
    let price_cache_clone = price_cache.clone();
    let seen_clone = seen.clone();
    
    // Spawn a task to handle outgoing message writes
    let writer_task = tokio::spawn(async move {
        loop {
            match outgoing_rx.recv().await {
                Some(OutgoingMessage::Text(msg)) => {
                    if let Err(e) = write.send(Message::Text(msg.into())).await {
                        error!("Failed to send outgoing text message: {}", e);
                        break;
                    }
                }
                Some(OutgoingMessage::Ping) => {
                    if let Err(e) = write.send(Message::Ping(vec![].into())).await {
                        error!("Failed to send outgoing ping: {}", e);
                        break;
                    }
                }
                Some(OutgoingMessage::Raw(msg)) => {
                    if let Err(e) = write.send(msg).await {
                        error!("Failed to send outgoing raw message: {}", e);
                        break;
                    }
                }
                None => {
                    info!("Outgoing message channel closed");
                    break;
                }
            }
        }
    });

    // Send initial logs subscription message for pump.fun program so that
    // non-Helius endpoints that support logsSubscribe will show new coin logs.
    // This uses the standard JSON-RPC `logsSubscribe` with `mentions` filter.
    if !_settings.pump_fun_program.is_empty() {
        let subscribe_msg = format!(r#"{{"jsonrpc":"2.0","id":1,"method":"logsSubscribe","params":[{{"mentions":["{}"]}}, {{"commitment":"confirmed"}}]}}"#, _settings.pump_fun_program);
        let _ = tx.clone().send(OutgoingMessage::Text(subscribe_msg)).await;
    }

    // If Helius enhanced transaction subscription is enabled, send transactionSubscribe
    if _settings.helius_ws_enabled {
        let filter = format!(r#"{{\"mentions\":[\"{}\"]}}"#, _settings.pump_fun_program);
        let subscribe_tx = format!(r#"{{"jsonrpc":"2.0","id":2,"method":"transactionSubscribe","params":[{},{{"encoding":"jsonParsed","transactionDetails":"full","commitment":"processed"}}]}}"#, filter);
        let _ = tx.clone().send(OutgoingMessage::Text(subscribe_tx)).await;
    }

    // Spawn a task to handle ping messages
    let ping_task = tokio::spawn(async move {
        loop {
            ping_interval.tick().await;
            // Send a ping by putting a Ping message into the outgoing channel
            if let Err(e) = tx_for_ping.send(OutgoingMessage::Ping).await {
                error!("Failed to send ping via outgoing channel: {}", e);
                break;
            }
        }
    });
    
    // Handle incoming WebSocket messages
    let detected_tx_clone = detected_tx.clone();
    let message_handler = tokio::spawn(async move {
        while let Some(message) = read.next().await {
            match message {
                Ok(Message::Text(text)) => {
                    warn!("Received WebSocket message: {}", text);
                    
                    // Parse the message and handle it
                    if let Ok(value) = serde_json::from_str::<Value>(&text) {
                        handle_websocket_message(
                            &value,
                            &holdings_clone,
                            &price_cache_clone,
                            &seen_clone,
                            &_settings,
                            &detected_tx_clone,
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
                            
                            if let Ok(_) = tx_clone1.send(OutgoingMessage::Text(subscribe_msg)).await {
                                let _ = resp.send(Ok(1)); // Mock subscription ID
                            } else {
                                let _ = resp.send(Err("Failed to send subscription".to_string()));
                            }
                        }
                        WsRequest::Unsubscribe { sub_id, resp } => {
                            info!("Unsubscribing from: {}", sub_id);
                            let unsubscribe_msg = format!(r#"{{"method":"unsubscribe","id":{}}}"#, sub_id);
                            
                            if let Ok(_) = tx_clone2.send(OutgoingMessage::Text(unsubscribe_msg)).await {
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
        _ = writer_task => {
            info!("Writer task completed");
        }
    }
    
    Ok(())
}

async fn handle_websocket_message(
    value: &Value,
    _holdings: &Arc<Mutex<HashMap<String, Holding>>>,
    price_cache: &Arc<Mutex<PriceCache>>,
    seen: &Arc<Mutex<LruCache<String, ()>>>,
    settings: &Arc<Settings>,
    detected_tx: &Option<mpsc::Sender<String>>,
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
            Some("logsNotification") => {
                // Parse the logsNotification results for new pump.fun events.
                if let Some(params) = value.get("params") {
                    if let Some(result) = params.get("result") {
                        if let Some(value_node) = result.get("value") {
                            if let Some(logs_array) = value_node.get("logs") {
                                if let Some(logs_vec) = logs_array.as_array() {
                                    for log_entry in logs_vec {
                                        if let Some(log_str) = log_entry.as_str() {
                                            if log_str.contains(&settings.pump_fun_program) {
                                                warn!("Detected pump.fun log: {}", log_str);
                                                // For now, add to seen cache so we don't re-trigger frequently
                                                let mut seen_guard = seen.lock().await;
                                                let key = format!("log:{}", chrono::Utc::now().timestamp());
                                                seen_guard.put(key, ());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Some("transactionNotification") => {
                if let Some(params) = value.get("params") {
                    if let Some(result) = params.get("result") {
                        if let Some(_type) = result.get("type") {
                            if _type.as_str() == Some("TOKEN_MINT") {
                                // Parse mint from tokenTransfers array
                                if let Some(token_transfers) = result.get("tokenTransfers") {
                                    if let Some(arr) = token_transfers.as_array() {
                                        for t in arr {
                                            if let Some(mint) = t.get("mint").and_then(|m| m.as_str()) {
                                                warn!("Helius TOKEN_MINT detected: {}", mint);
                                                // Add to price cache & seen to prevent duplicates
                                                {
                                                    let mut seen_guard = seen.lock().await;
                                                    if seen_guard.contains(&mint.to_string()) {
                                                        continue;
                                                    }
                                                    seen_guard.put(mint.to_string(), ());
                                                }
                                                {
                                                    let mut cache_guard = price_cache.lock().await;
                                                    cache_guard.put(mint.to_string(), (std::time::Instant::now(), 0.0));
                                                }
                                                // Notify the detector that we saw a new mint
                                                if let Some(sender) = detected_tx {
                                                    if let Err(e) = sender.clone().send(mint.to_string()).await {
                                                        error!("Failed to notify detector for mint {}: {}", mint, e);
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
            }
            _ => {
                debug!("Unhandled WebSocket method: {}", method);
            }
        }
    }
}
