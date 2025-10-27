mod models;
mod rpc;
mod settings;
mod ws;

use crate::{
    models::{Holding, PriceCache},
    settings::Settings,
};
use chrono::Utc;
use log::{error, info, debug};
use lru::LruCache;
use solana_sdk::signature::Keypair;
use mpl_token_metadata::accounts::Metadata as OnchainMetadataRaw;
use std::{collections::HashMap, fs, sync::Arc};
use tokio::sync::{mpsc, Mutex};
use tokio::time::{sleep, Duration};

#[tokio::main(worker_threads = 4)]
async fn main() {
    env_logger::init();
    // Print an unconditional startup line so users see the binary started
    // even when RUST_LOG is not set (typo like RUST_LOGS will otherwise be silent).
    println!(
        "sol_beast starting (pid {}), RUST_LOG={:?}",
        std::process::id(),
        std::env::var("RUST_LOG").ok()
    );
    let settings = Arc::new(Settings::from_file("config.toml"));
    let seen = Arc::new(Mutex::new(LruCache::new(
        settings.cache_capacity.try_into().unwrap(),
    )));
    let holdings = Arc::new(Mutex::new(HashMap::new()));
    let price_cache = Arc::new(Mutex::new(LruCache::new(
        settings.cache_capacity.try_into().unwrap(),
    )));
    let (tx, mut rx) = mpsc::channel(1000);
    let is_real = std::env::args().any(|arg| arg == "--real");
    let keypair = if is_real {
        let bytes = fs::read(settings.wallet_keypair_path.clone()).expect("Keypair file missing");
        Some(Keypair::try_from(bytes.as_slice()).expect("Invalid keypair"))
    } else {
        None
    };

    // Spawn price monitoring
    let holdings_clone = holdings.clone();
    let price_cache_clone = price_cache.clone();
    let settings_clone = settings.clone();
    let keypair_clone = keypair
        .as_ref()
        .map(|kp| Keypair::try_from(kp.to_bytes().as_ref()).unwrap());
    tokio::spawn(async move {
        monitor_holdings(
            holdings_clone,
            price_cache_clone,
            is_real,
            keypair_clone.as_ref(),
            settings_clone,
        )
        .await
    });

    // Spawn WSS tasks
    for wss_url in settings.solana_ws_urls.iter() {
        let tx = tx.clone();
        let seen = seen.clone();
        let holdings_clone = holdings.clone();
        let price_cache_clone = price_cache.clone();
        let settings_clone = settings.clone();
        let wss_url = wss_url.clone();
        tokio::spawn(async move {
            loop {
                if let Err(e) = ws::run_ws(
                    &wss_url,
                    tx.clone(),
                    seen.clone(),
                    holdings_clone.clone(),
                    price_cache_clone.clone(),
                    settings_clone.clone(),
                )
                .await
                {
                    error!("WSS connection to {} failed: {}. Reconnecting...", wss_url, e);
                }
                sleep(Duration::from_millis(5000 + rand::random::<u64>() % 5000)).await;
            }
        });
    }

    // Process messages
    while let Some(msg) = rx.recv().await {
        if let Err(e) = process_message(
            &msg,
            &seen,
            &holdings,
            is_real,
            keypair.as_ref(),
            &price_cache,
            &settings,
        )
        .await
        {
            // Log the error and a truncated preview of the incoming message for debugging.
            let preview: String = msg.chars().take(200).collect();
            error!("process_message failed for incoming message (truncated): {}... error: {}", preview, e);
        }
    }
}

async fn process_message(
    text: &str,
    seen: &Arc<Mutex<LruCache<String, ()>>>,
    holdings: &Arc<Mutex<HashMap<String, Holding>>>,
    is_real: bool,
    keypair: Option<&Keypair>,
    price_cache: &Arc<Mutex<PriceCache>>,
    settings: &Arc<Settings>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let value: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to parse incoming websocket message as JSON: {}. message (truncated)={}", e, text.chars().take(200).collect::<String>());
            return Err(Box::new(e));
        }
    };
    if let Some(params) = value.get("params").and_then(|p| p.get("result")).and_then(|r| r.get("value")) {
        let logs_opt = params.get("logs").and_then(|l| l.as_array());
        let sig_opt = params.get("signature").and_then(|s| s.as_str());
        if logs_opt.is_none() {
            debug!("Incoming message missing logs field: {:?}", params);
        }
        if sig_opt.is_none() {
            debug!("Incoming message missing signature field: {:?}", params);
        }

        if let (Some(logs), Some(signature)) = (logs_opt, sig_opt) {
            if logs.iter().any(|log| log.as_str() == Some("Program log: Instruction: InitializeMint2")) {
                if seen.lock().await.put(signature.to_string(), ()).is_none() {
                    info!("New pump.fun token: {}", signature);
                    if let Err(e) = handle_new_token(signature, holdings, is_real, keypair, price_cache, settings).await {
                        error!("handle_new_token failed for {}: {}", signature, e);
                        return Err(e);
                    }
                }
            }
        }
    } else {
        debug!("Websocket message missing params/result/value: {:?}", value);
    }
    Ok(())
}

async fn handle_new_token(
    signature: &str,
    holdings: &Arc<Mutex<HashMap<String, Holding>>>,
    is_real: bool,
    keypair: Option<&Keypair>,
    price_cache: &Arc<Mutex<PriceCache>>,
    settings: &Arc<Settings>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (creator, mint, curve_pda, holder_addr) = rpc::fetch_transaction_details(signature, settings).await?;
    let (onchain_meta, offchain_meta, onchain_raw) = rpc::fetch_token_metadata(&mint, settings).await?;
    if let Some(m) = &onchain_meta {
        info!("Token {}: Creator={}, Curve={}, Holder={}, URI={}", mint, creator, curve_pda, holder_addr, m.uri.trim_end_matches('\u{0}'));
        if let Some(off) = &offchain_meta {
            info!("Off-chain metadata for {}: name={:?}, symbol={:?}, image={:?}", mint, off.name, off.symbol, off.image);
        }
        if !m.uri.trim_end_matches('\u{0}').is_empty() && m.seller_fee_basis_points < 500 {
            // Ensure the bonding-curve account is readable (able to derive a price) before attempting a buy.
            match rpc::fetch_current_price(&mint, &price_cache, settings).await {
                Ok(_price) => {
                    match rpc::buy_token(&mint, 0.1, is_real, keypair, price_cache.clone(), settings).await {
                        Ok(mut holding) => {
                            // Persist off-chain and raw on-chain metadata with the holding if available
                            holding.metadata = offchain_meta.clone();
                            holding.onchain_raw = onchain_raw.clone();

                            // Build a compact structured on-chain metadata object for quick access.
                            let mut onchain_struct: Option<crate::models::OnchainFullMetadata> = None;
                            if let Some(meta) = onchain_meta.as_ref() {
                                // Prefer the already-deserialized Metadata object returned by fetch_token_metadata
                                let name = meta.name.trim_end_matches('\u{0}').to_string();
                                let symbol = meta.symbol.trim_end_matches('\u{0}').to_string();
                                let uri = meta.uri.trim_end_matches('\u{0}').to_string();
                                onchain_struct = Some(crate::models::OnchainFullMetadata {
                                    name: if name.is_empty() { None } else { Some(name) },
                                    symbol: if symbol.is_empty() { None } else { Some(symbol) },
                                    uri: if uri.is_empty() { None } else { Some(uri) },
                                    seller_fee_basis_points: Some(meta.seller_fee_basis_points),
                                    raw: onchain_raw.clone(),
                                });
                            } else if let Some(raw_bytes) = onchain_raw.as_ref() {
                                // Try to deserialize from raw bytes as a fallback
                                if let Ok(parsed) = OnchainMetadataRaw::safe_deserialize(raw_bytes) {
                                    let name = parsed.name.trim_end_matches('\u{0}').to_string();
                                    let symbol = parsed.symbol.trim_end_matches('\u{0}').to_string();
                                    let uri = parsed.uri.trim_end_matches('\u{0}').to_string();
                                    onchain_struct = Some(crate::models::OnchainFullMetadata {
                                        name: if name.is_empty() { None } else { Some(name) },
                                        symbol: if symbol.is_empty() { None } else { Some(symbol) },
                                        uri: if uri.is_empty() { None } else { Some(uri) },
                                        seller_fee_basis_points: Some(parsed.seller_fee_basis_points),
                                        raw: Some(raw_bytes.clone()),
                                    });
                                }
                            }

                            holding.onchain = onchain_struct.clone();

                            if let Some(off) = &holding.metadata {
                                info!("Persisting off-chain metadata for {} into holdings: name={:?}, symbol={:?}, image={:?}", mint, off.name, off.symbol, off.image);
                            }
                            if let Some(raw) = &holding.onchain_raw {
                                info!("Persisting on-chain raw metadata for {} into holdings ({} bytes)", mint, raw.len());
                            }
                            if let Some(onchain) = &holding.onchain {
                                info!("Persisting parsed on-chain metadata for {} into holdings: name={:?}, symbol={:?}, uri={:?}, seller_fee_basis_points={:?}", mint, onchain.name, onchain.symbol, onchain.uri, onchain.seller_fee_basis_points);
                            }
                            holdings.lock().await.insert(mint.clone(), holding);
                        }
                        Err(e) => log::warn!("Failed to buy {}: {}", mint, e),
                    }
                }
                Err(e) => {
                    // Unable to derive a price; likely the bonding-curve account is missing/empty.
                    log::warn!("Skipping buy for {}: unable to fetch price (bonding curve missing or empty): {}", mint, e);
                }
            }
        }
    }
    Ok(())
}

async fn monitor_holdings(
    holdings: Arc<Mutex<HashMap<String, Holding>>>,
    price_cache: Arc<Mutex<PriceCache>>,
    is_real: bool,
    keypair: Option<&Keypair>,
    settings: Arc<Settings>,
) {
    loop {
        sleep(Duration::from_secs(5)).await;
        let mut to_remove = Vec::new();
        let holdings_snapshot = holdings.lock().await.clone();

        for (mint, holding) in &holdings_snapshot {
            let current_price = match rpc::fetch_current_price(mint, &price_cache, &settings).await {
                Ok(price) => price,
                Err(e) => {
                    log::warn!("Price fetch failed for {}: {}", mint, e);
                    if e.to_string().contains("migrated") {
                        to_remove.push(mint.clone());
                    }
                    continue;
                }
            };
            let profit_percent = ((current_price - holding.buy_price) / holding.buy_price) * 100.0;
            let elapsed = Utc::now().signed_duration_since(holding.buy_time).num_seconds();

            let should_sell = if profit_percent >= settings.tp_percent {
                info!("TP hit for {}: +{:.2}% ({} SOL/token)", mint, profit_percent, current_price);
                true
            } else if profit_percent <= settings.sl_percent {
                info!("SL hit for {}: {:.2}% ({} SOL/token)", mint, profit_percent, current_price);
                true
            } else if elapsed >= settings.timeout_secs {
                info!("Timeout for {}: {}s ({} SOL/token)", mint, elapsed, current_price);
                true
            } else {
                false
            };

            if should_sell {
                if let Err(e) = rpc::sell_token(mint, holding.amount, current_price, is_real, keypair, &settings).await {
                    error!("Sell error for {}: {}", mint, e);
                }
                to_remove.push(mint.clone());
            }
        }

        if !to_remove.is_empty() {
            let mut holdings_lock = holdings.lock().await;
            for mint in to_remove {
                holdings_lock.remove(&mint);
            }
        }
    }
}
