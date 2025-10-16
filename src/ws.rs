// Rust
use futures_util::{stream::StreamExt, SinkExt};
use log::info;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

/// Run a single WSS connection, deduplicate, and forward new events to channel.
pub async fn run_ws(
    wss_url: &str,
    tx: mpsc::Sender<String>,
    seen: Arc<Mutex<lru::LruCache<String, ()>>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (ws_stream, _) = connect_async(wss_url).await?;
    let (mut write, mut read) = ws_stream.split();
    write
        .send(Message::Text(
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "logsSubscribe",
                "params": [
                    { "mentions": ["6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"] },
                    { "commitment": "confirmed" }
                ]
            })
            .to_string(),
        ))
        .await?;

    while let Some(msg) = read.next().await {
        let msg = msg?;
        if let Message::Text(text) = msg {
            let value: Value = serde_json::from_str(&text)?;
            if let Some(signature) = value
                .get("params")
                .and_then(|p| p.get("result"))
                .and_then(|r| r.get("value"))
                .and_then(|v| v.get("signature"))
                .and_then(|s| s.as_str())
            {
                if seen.lock().await.contains(signature) {
                    continue;
                }
                tx.send(text).await?;
            }
        }
    }
    Ok(())
}

/// Process a message: deduplicate and trigger token handling.
pub async fn process_message(
    text: &str,
    seen: &Arc<Mutex<lru::LruCache<String, ()>>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let value: Value = serde_json::from_str(text)?;
    if let Some(params) = value
        .get("params")
        .and_then(|p| p.get("result"))
        .and_then(|r| r.get("value"))
    {
        if let (Some(logs), Some(signature)) = (
            params.get("logs").and_then(|l| l.as_array()),
            params.get("signature").and_then(|s| s.as_str()),
        ) {
            if logs
                .iter()
                .any(|log| log.as_str() == Some("Program log: Instruction: InitializeMint2"))
            {
                let mut cache = seen.lock().await;
                if cache.put(signature.to_string(), ()).is_some() {
                    return Ok(());
                }
                info!("New token: {}", signature);
                let signature_owned = signature.to_string();
                tokio::spawn(async move {
                    if let Err(e) = crate::rpc::handle_new_token(&signature_owned).await {
                        log::error!("Token handle error: {}", e);
                    }
                });
            }
        }
    }
    Ok(())
}