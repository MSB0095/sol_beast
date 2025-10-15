// Rust
use futures_util::{stream::StreamExt, SinkExt};
use log::{error, info};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

// Placeholder for trading logic
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
    let mut attempt = 1;
    loop {
        match connect_async(url).await {
            Ok((ws_stream, _)) => {
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

                                    match crate::rpc::fetch_transaction_details(signature, &settings.https_url).await {
                                        Ok((creator, mint, metadata)) => {
                                            info!("Creator Address: {}", creator);
                                            info!("New Token Mint Address: {}", mint);
                                            if let Some(meta) = &metadata {
                                                info!("On-Chain Metadata for {}:", mint);
                                                info!("  Name: {}", meta.name.trim());
                                                info!("  Symbol: {}", meta.symbol.trim());
                                                info!("  Image URI: {}", if meta.uri.trim().is_empty() { "None" } else { meta.uri.trim() });
                                                info!("  Seller Fee Basis Points: {}", meta.seller_fee_basis_points);
                                                if let Some(creators) = &meta.creators {
                                                    if !creators.is_empty() {
                                                        info!("  Primary Creator: {} (Verified: {}, Share: {})", creators[0].address, creators[0].verified, creators[0].share);
                                                    }
                                                }
                                                info!("  Update Authority: {}", meta.update_authority);
                                            } else {
                                                info!("No metadata found for mint: {}", mint);
                                            }
                                            info!("---");
                                            // Example trading logic: Buy if uri is non-empty and royalties < 5%
                                            if let Some(meta) = &metadata {
                                                if !meta.uri.trim().is_empty() && meta.seller_fee_basis_points < 500 {
                                                    info!("Token {} meets criteria (non-empty URI, low royalties). Initiating buy.", mint);
                                                    // buy_token(&mint, 1000).await?;
                                                } else {
                                                    info!("Token {} skipped: URI empty or high royalties.", mint);
                                                }
                                            } else {
                                                info!("Token {} skipped: No metadata.", mint);
                                            }
                                        }
                                        Err(e) => {
                                            error!("Failed to fetch details for {}: {}", signature, e);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                return Ok(());
            }
            Err(e) => {
                error!("WebSocket error: {}. Reconnecting in {}ms...", e, 5000);
                tokio::time::sleep(std::time::Duration::from_millis(5000)).await;
                attempt += 1;
                if attempt > 5 {
                    error!("Max WebSocket reconnect attempts reached.");
                    return Ok(());
                }
            }
        }
    }

}