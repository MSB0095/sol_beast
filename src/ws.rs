use crate::{
    models::BondingCurveState,
    settings::Settings,
    Holding,
    PriceCache,
};
use base64::{engine::general_purpose::STANDARD as Base64Engine, Engine};
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
                    let (curve_pda, _) = Pubkey::find_program_address(
                        &[
                            b"bonding_curve",
                            pump_fun_program_pk.as_ref(),
                            mint_pk.as_ref(),
                        ],
                        &pump_fun_program_pk,
                    );
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
                        if let Ok(state) = BondingCurveState::try_from_slice(&decoded) {
                            if !state.complete {
                                let _price = (state.virtual_sol_reserves as f64
                                    / 1_000_000_000.0)
                                    / state.virtual_token_reserves as f64;
                                // This part is tricky as sub ID doesn't directly give mint.
                                // A better approach is needed, maybe a map from sub ID to mint.
                                // For now, this update path is lossy.
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
