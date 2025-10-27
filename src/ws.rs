use crate::{
    models::BondingCurveState,
    settings::Settings,
    Holding,
    PriceCache,
};
use base64::{engine::general_purpose::STANDARD as Base64Engine, Engine};
use log::{debug, error};
use borsh::BorshDeserialize;
use futures_util::{stream::StreamExt, SinkExt};
use lru::LruCache;
use serde_json::{json, Value};
use solana_program::pubkey::Pubkey;
use std::{collections::HashMap, str::FromStr, sync::Arc};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::{connect_async, tungstenite::Message};

pub async fn run_ws(
    wss_url: &str,
    tx: mpsc::Sender<String>,
    seen: Arc<Mutex<LruCache<String, ()>>>,
    holdings: Arc<Mutex<HashMap<String, Holding>>>,
    _price_cache: Arc<Mutex<PriceCache>>,
    settings: Arc<Settings>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (ws_stream, _) = connect_async(wss_url).await?;
    let (mut write, mut read) = ws_stream.split();

    // Subscribe to pump.fun logs
    write
        .send(Message::Text(
            json!({
                "jsonrpc": "2.0", "id": 1, "method": "logsSubscribe",
                "params": [ { "mentions": [&settings.pump_fun_program] }, { "commitment": "confirmed" } ]
            })
            .to_string(),
        ))
        .await?;

    // Subscribe to bonding curve accounts for holdings
    let holdings_accounts = {
        let holdings = holdings.lock().await;
        let pump_fun_program_pk = Pubkey::from_str(&settings.pump_fun_program)?;
        holdings
            .keys()
            .filter_map(|mint_str| {
                Pubkey::from_str(mint_str).ok().map(|mint_pk| {
                    // PDA seeds per pump.fun IDL: ["bonding-curve", mint]
                    let (curve_pda, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_fun_program_pk);
                    curve_pda.to_string()
                })
            })
            .collect::<Vec<_>>()
    };

    for account in holdings_accounts {
        write
            .send(Message::Text(
                json!({
                    "jsonrpc": "2.0", "id": 2, "method": "accountSubscribe",
                    "params": [ account, { "commitment": "confirmed", "encoding": "base64" } ]
                })
                .to_string(),
            ))
            .await?;
    }

    while let Some(msg) = read.next().await {
        let msg = msg?;
        if let Message::Text(text) = msg {
            let value: Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(_) => continue, // Ignore parse errors
            };

            if let Some(signature) = value["params"]["result"]["value"]["signature"].as_str() {
                if !seen.lock().await.contains(signature) {
                    tx.send(text).await?;
                }
            } else if let Some(data) = value["params"]["result"]["value"]["data"].as_array() {
                if let (Some(_account), Some(encoded_data)) = (
                    value["params"]["subscription"].as_u64(), // Assuming subscription ID maps to mint
                    data.get(0).and_then(|v| v.as_str()),
                ) {
                    if let Ok(decoded) = Base64Engine.decode(encoded_data) {
                        // Skip Anchor discriminator if present
                        let slice = if decoded.len() > 8 { &decoded[8..] } else { &decoded[..] };
                        // Debug: print lengths and a short hex prefix to help diagnose layout issues
                        let disc_bytes = &decoded[..std::cmp::min(8, decoded.len())];
                        let prefix_len = std::cmp::min(64, slice.len());
                        let prefix_hex: String = slice[..prefix_len].iter().map(|b| format!("{:02x}", b)).collect();
                        debug!(
                            "WS bonding curve raw bytes len={} slice_len={} discriminator={:?} first{}={}",
                            decoded.len(),
                            slice.len(),
                            disc_bytes,
                            prefix_len,
                            prefix_hex
                        );
                        match BondingCurveState::try_from_slice(slice) {
                            Ok(state) => {
                                if !state.complete {
                                    let _price = (state.virtual_sol_reserves as f64 / 1_000_000_000.0)
                                        / state.virtual_token_reserves as f64;
                                    // This part is tricky as sub ID doesn't directly give mint.
                                    // A better approach is needed, maybe a map from sub ID to mint.
                                    // For now, this update path is lossy.
                                }
                            }
                            Err(e) => {
                                error!("WS deserialize bonding curve failed: {}", e);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
