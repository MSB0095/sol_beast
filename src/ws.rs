// Rust
use futures_util::{stream::StreamExt, SinkExt};
use log::{error, info};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

use crate::rpc::fetch_transaction_details;
use crate::Settings;

#[derive(Serialize)]
pub struct SubscribeRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: Vec<serde_json::Value>,
}

pub async fn run_ws(
    settings: &Settings,
    seen_signatures: &mut HashSet<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let url = &settings.wss_url;
    let (ws_stream, _) = connect_async(url).await?;
    info!("WebSocket connected");

    let (mut write, mut read) = ws_stream.split();

    let subscribe_request = SubscribeRequest {
        jsonrpc: "2.0".to_string(),
        id: 421,
        method: "logsSubscribe".to_string(),
        params: vec![
            serde_json::json!({ "mentions": [settings.pump_fun_program.clone()] }),
            serde_json::json!({ "commitment": "confirmed" }),
        ],
    };
    write
        .send(Message::Text(serde_json::to_string(&subscribe_request)?))
        .await?;

    while let Some(message) = read.next().await {
        let message = message?;
        if let Message::Text(text) = message {
            let value: Value = serde_json::from_str(&text)?;
            if let Some(params) = value.get("params").and_then(|p| p.get("result")).and_then(|r| r.get("value")) {
                if let (Some(logs), Some(signature)) = (
                    params.get("logs").and_then(|l| l.as_array()),
                    params.get("signature").and_then(|s| s.as_str()),
                ) {
                    if logs.iter().any(|log| log.as_str() == Some("Program log: Instruction: InitializeMint2")) {
                        if seen_signatures.contains(signature) {
                            continue;
                        }
                        seen_signatures.insert(signature.to_string());
                        info!("New pump.fun token detected! Signature: {}", signature);

                        if let Err(e) = fetch_transaction_details(signature, &settings.https_url).await {
                            error!("Failed to fetch details for {}: {}", signature, e);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}