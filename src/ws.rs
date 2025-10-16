use crate::rpc;
use crate::settings::Settings;
use futures_util::{stream::StreamExt, SinkExt};
use log::{error, info};
use lru::LruCache;
use serde_json::{json, Value};
use solana_sdk::signature::Keypair;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use rand::Rng;

use crate::{Holding, PriceCache};

const PUMP_FUN_PROGRAM: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";

pub async fn monitor_pump_fun_tokens(
    wss_urls: Vec<String>,
    seen: Arc<Mutex<LruCache<String, ()>>>,
    holdings: Arc<Mutex<HashMap<String, Holding>>>,
    is_real: bool,
    keypair: Option<Keypair>,
    price_cache: Arc<Mutex<PriceCache>>,
    settings: Arc<Settings>,
) {
    let (tx, mut rx) = mpsc::channel(1000);

    for wss_url in wss_urls {
        let tx = tx.clone();
        let seen = seen.clone();
        tokio::spawn(async move {
            loop {
                let _ = run_ws(&wss_url, tx.clone(), seen.clone()).await;
                let jitter = rand::thread_rng().gen_range(0..5000);
                sleep(Duration::from_millis(5000 + jitter)).await;
            }
        });
    }

    while let Some(msg) = rx.recv().await {
        let holdings_clone = holdings.clone();
        let price_cache_clone = price_cache.clone();
        let keypair_clone = keypair.as_ref().map(|kp| Keypair::try_from(kp.to_bytes().as_slice()).unwrap());
        let seen_clone = seen.clone();
        let settings_clone = settings.clone();
        tokio::spawn(async move {
            if let Err(e) = process_message(&msg, &seen_clone, &holdings_clone, is_real, keypair_clone.as_ref(), &price_cache_clone, settings_clone).await {
                error!("Error processing message: {}", e);
            }
        });
    }
}

async fn run_ws(wss_url: &str, tx: mpsc::Sender<String>, seen: Arc<Mutex<LruCache<String, ()>>>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("Connecting to WSS: {}", wss_url);
    let (ws_stream, _) = connect_async(wss_url).await?;
    let (mut write, mut read) = ws_stream.split();
    write.send(Message::Text(json!({
        "jsonrpc": "2.0", "id": 1, "method": "logsSubscribe",
        "params": [ { "mentions": [PUMP_FUN_PROGRAM] }, { "commitment": "confirmed" } ]
    }).to_string())).await?;

    while let Some(msg) = read.next().await {
        let msg = msg?;
        if let Message::Text(text) = msg {
            let value: Value = serde_json::from_str(&text)?;
            if let Some(signature) = value.get("params").and_then(|p| p.get("result")).and_then(|r| r.get("value")).and_then(|v| v.get("signature")).and_then(|s| s.as_str()) {
                if seen.lock().await.contains(signature) { continue; }
                tx.send(text).await?;
            }
        }
    }
    Ok(())
}

async fn process_message(
    text: &str,
    seen: &Arc<Mutex<LruCache<String, ()>>>,
    holdings: &Arc<Mutex<HashMap<String, Holding>>>,
    is_real: bool,
    keypair: Option<&Keypair>,
    price_cache: &Arc<Mutex<PriceCache>>,
    settings: Arc<Settings>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let value: Value = serde_json::from_str(text)?;
    if let Some(params) = value.get("params").and_then(|p| p.get("result")).and_then(|r| r.get("value")) {
        if let (Some(logs), Some(signature)) = (params.get("logs").and_then(|l| l.as_array()), params.get("signature").and_then(|s| s.as_str())) {
            if logs.iter().any(|log| log.as_str() == Some("Program log: Instruction: InitializeMint2")) {
                let mut cache = seen.lock().await;
                if cache.put(signature.to_string(), ()).is_some() { return Ok(()); }
                info!("New pump.fun token creation: {}", signature);
                let signature_owned = signature.to_string();
                let holdings_clone = holdings.clone();
                let price_cache_clone = price_cache.clone();
                let keypair_clone = keypair.map(|k| Keypair::try_from(k.to_bytes().as_slice()).unwrap());
                let settings_clone = settings.clone();

                tokio::spawn(async move {
                    if let Err(e) = rpc::handle_new_token(&signature_owned, holdings_clone, is_real, keypair_clone.as_ref(), &price_cache_clone, settings_clone).await {
                        error!("Error handling token {}: {}", signature_owned, e);
                    }
                });
            }
        }
    }
    Ok(())
}
