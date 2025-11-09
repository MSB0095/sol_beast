use crate::{
    models::{Holding, PriceCache},
    settings::Settings,
    rpc,
    api::TradeRecord,
    state::BuyRecord,
};
use solana_client::rpc_client::RpcClient;
use std::{collections::HashMap, sync::Arc, time::Instant};
use tokio::sync::{Mutex, mpsc};
use solana_sdk::{
    signature::{Keypair},
};
use crate::ws::WsRequest;
use std::sync::atomic::{AtomicUsize, Ordering};
use once_cell::sync::Lazy;
use log::{info, debug, error};
use chrono::Utc;
use std::str::FromStr;
use std::io::Write;



pub async fn monitor_holdings(
    holdings: Arc<Mutex<HashMap<String, Holding>>>,
    price_cache: Arc<Mutex<PriceCache>>,
    rpc_client: Arc<RpcClient>,
    is_real: bool,
    keypair: Option<&Keypair>,
    simulate_keypair: Option<&Keypair>,
    settings: Arc<Settings>,
    trades_map: Arc<Mutex<HashMap<String, BuyRecord>>>,
    ws_control_senders: Arc<Vec<mpsc::Sender<WsRequest>>>,
    sub_map: Arc<Mutex<HashMap<String, (usize, u64)>>>,
    _next_wss_sender: Arc<AtomicUsize>,
    trades_list: Arc<tokio::sync::Mutex<Vec<TradeRecord>>>,
) {
    // Debounce maps to avoid repeated subscribe/prime attempts and noisy warnings
    static SUBSCRIBE_ATTEMPT_TIMES: Lazy<tokio::sync::Mutex<HashMap<String, Instant>>> = Lazy::new(|| tokio::sync::Mutex::new(HashMap::new()));
    const SUBSCRIBE_ATTEMPT_DEBOUNCE_SECS: u64 = 30;
    static PRICE_MISS_WARN_TIMES: Lazy<tokio::sync::Mutex<HashMap<String, Instant>>> = Lazy::new(|| tokio::sync::Mutex::new(HashMap::new()));
    const PRICE_MISS_WARN_DEBOUNCE_SECS: u64 = 60;
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        let mut to_remove = Vec::new();
        let holdings_snapshot = holdings.lock().await.clone();

        for (mint, holding) in &holdings_snapshot {
            // Prefer WSS-provided cached prices when configured to use WSS only.
            // This avoids RPC polling and keeps the monitor reacting to real-time
            // websocket updates. If `price_source` is not strict "wss", fall
            // back to the RPC fetcher which itself will consult the same cache
            // and only call RPC when needed.
            let current_price_result: Result<f64, Box<dyn std::error::Error + Send + Sync>> = if settings.price_source == "wss" {
                let mut cache_guard = price_cache.lock().await;
                if let Some((ts, price)) = cache_guard.get(mint) {
                    // honor the cache TTL
                    if Instant::now().duration_since(*ts) < std::time::Duration::from_secs(settings.price_cache_ttl_secs) {
                        Ok(*price)
                    } else {
                        Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, format!("WSS cached price for {} expired", mint))))
                    }
                } else {
                    Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, format!("No WSS cached price for {}", mint))))
                }
            } else {
                rpc::fetch_current_price(mint, &price_cache, &rpc_client, &settings).await
            };

            let current_price = match current_price_result {
                Ok(price) => price,
                Err(e) => {
                    // If we're using WSS as the source, try to ensure a subscription
                    // exists and attempt a single RPC prime before giving up. Rate-
                    // limit subscribe/prime attempts per-mint to avoid storms.
                    if settings.price_source == "wss" {
                        // Check if a subscription exists for this mint
                        let has_sub = { sub_map.lock().await.get(mint).is_some() };
                        let mut attempted_subscribe = false;
                        if !has_sub {
                            let mut attempts = SUBSCRIBE_ATTEMPT_TIMES.lock().await;
                            let now = Instant::now();
                            let do_try = match attempts.get(mint) {
                                Some(last) if now.duration_since(*last).as_secs() < SUBSCRIBE_ATTEMPT_DEBOUNCE_SECS => false,
                                _ => true,
                            };
                            if do_try {
                                attempts.insert(mint.clone(), now);
                                if !ws_control_senders.is_empty() {
                                    let idx = _next_wss_sender.fetch_add(1, Ordering::Relaxed) % ws_control_senders.len();
                                    let sender = &ws_control_senders[idx];
                                    let (resp_tx, resp_rx) = tokio::sync::oneshot::channel::<Result<u64, String>>();
                                    let pump_prog = match solana_sdk::pubkey::Pubkey::from_str(&settings.pump_fun_program) {
                                        Ok(pk) => pk,
                                        Err(_) => {
                                            debug!("Invalid pump_fun_program pubkey in settings");
                                            solana_sdk::pubkey::Pubkey::default()
                                        }
                                    };
                                    if let Ok(mint_pk) = solana_sdk::pubkey::Pubkey::from_str(mint) {
                                        let (curve_pda, _) = solana_sdk::pubkey::Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_prog);
                                        let _ = sender.send(WsRequest::Subscribe { account: curve_pda.to_string(), mint: mint.clone(), resp: resp_tx }).await;
                                        match tokio::time::timeout(std::time::Duration::from_secs(settings.wss_subscribe_timeout_secs), resp_rx).await {
                                            Ok(Ok(Ok(sub_id))) => {
                                                // persist mapping
                                                sub_map.lock().await.insert(mint.clone(), (idx, sub_id));
                                                debug!("Monitor auto-subscribed {} on sub {} (idx={})", mint, sub_id, idx);
                                                attempted_subscribe = true;
                                            }
                                            _ => {
                                                debug!("Monitor subscribe attempt failed/timed out for {}", mint);
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // If subscription exists or we attempted one, try one RPC prime
                        // to populate the cache (rate-limited implicitly by subscribe debounce).
                        if has_sub || attempted_subscribe {
                            match rpc::fetch_current_price(mint, &price_cache, &rpc_client, &settings).await {
                                Ok(p) => {
                                    debug!("Monitor primed price via RPC for {}: {:.18}", mint, p);
                                    // Continue to compute using the newly-fetched price
                                    p
                                }
                                Err(e2) => {
                                    // Debounced warn
                                    let mut warns = PRICE_MISS_WARN_TIMES.lock().await;
                                    let now = Instant::now();
                                    let should_log = match warns.get(mint) {
                                        Some(last) if now.duration_since(*last).as_secs() < PRICE_MISS_WARN_DEBOUNCE_SECS => false,
                                        _ => true,
                                    };
                                    if should_log {
                                        warns.insert(mint.clone(), now);
                                        log::warn!("Price fetch failed for {}: {}", mint, e2);
                                    } else {
                                        debug!("Suppressed repeated price-miss warn for {}: {}", mint, e2);
                                    }
                                    // If migrated, schedule removal
                                    if e2.to_string().contains("migrated") {
                                        to_remove.push(mint.clone());
                                    }
                                    continue;
                                }
                            }
                        } else {
                            // No subscription and we didn't attempt one — debounced warn and continue
                            let mut warns = PRICE_MISS_WARN_TIMES.lock().await;
                            let now = Instant::now();
                            let should_log = match warns.get(mint) {
                                Some(last) if now.duration_since(*last).as_secs() < PRICE_MISS_WARN_DEBOUNCE_SECS => false,
                                _ => true,
                            };
                            if should_log {
                                warns.insert(mint.clone(), now);
                                log::warn!("Price fetch failed for {}: {}", mint, e);
                            } else {
                                debug!("Suppressed repeated price-miss warn for {}: {}", mint, e);
                            }
                            if e.to_string().contains("migrated") {
                                to_remove.push(mint.clone());
                            }
                            continue;
                        }
                    } else {
                        log::warn!("Price fetch failed for {}: {}", mint, e);
                        // If the curve reports migrated, schedule removal of holding
                        if e.to_string().contains("migrated") {
                            to_remove.push(mint.clone());
                        }
                        continue;
                    }
                }
            };
            // current_price and holding.buy_price are SOL per token
            let profit_percent = if holding.buy_price != 0.0 {
                ((current_price - holding.buy_price) / holding.buy_price) * 100.0
            } else { 0.0 };
            let elapsed = Utc::now().signed_duration_since(holding.buy_time).num_seconds();

            let should_sell = if profit_percent >= settings.tp_percent {
                info!("TP hit for {}: +{:.6}% ({} SOL/token)", mint, profit_percent, format!("{:.18}", current_price));
                true
            } else if profit_percent <= settings.sl_percent {
                info!("SL hit for {}: {:.6}% ({} SOL/token)", mint, profit_percent, format!("{:.18}", current_price));
                true
            } else if elapsed >= settings.timeout_secs {
                info!("Timeout for {}: {}s ({} SOL/token)", mint, elapsed, format!("{:.18}", current_price));
                true
            } else {
                false
            };

            if should_sell {
                // Attempt sell
                match rpc::sell_token(
                    mint,
                    holding.amount,
                    current_price,
                    is_real,
                    keypair,
                    simulate_keypair.as_ref().map(|a| &**a),
                    &rpc_client,
                    &settings,
                )
                .await
                {
                    Ok(_) => {
                        let _reason = if profit_percent >= settings.tp_percent {
                            "Take Profit"
                        } else if profit_percent <= settings.sl_percent {
                            "Stop Loss"
                        } else {
                            "Timeout"
                        };
                        // bot_log!(
                        //     "info",
                        //     format!("Successfully sold token {}", mint),
                        //     format!("Reason: {}, Profit: {:.2}%, Current price: {:.9} SOL", reason, profit_percent, current_price)
                        // );
                    }
                    Err(e) => {
                        error!("Sell error for {}: {}", mint, e);
                        // bot_log!("error", format!("Failed to sell token {}", mint), format!("{}", e));
                    }
                }
                // Prepare trade CSV row using buy record if available
                let sell_time = Utc::now();
                let sell_tokens = holding.amount;
                // current_price is SOL per token; compute totals in SOL
                let sell_sol = sell_tokens as f64 * current_price;
                let profit_percent = if holding.buy_price != 0.0 { ((current_price - holding.buy_price) / holding.buy_price) * 100.0 } else { 0.0 };
                // compute profit in SOL
                let profit_sol = (sell_tokens as f64 * current_price) - (holding.buy_price * holding.amount as f64);
                let _profit_lamports = profit_sol * 1_000_000_000.0;
                let stop_reason = if profit_percent >= settings.tp_percent { "TP".to_string() } else if profit_percent <= settings.sl_percent { "SL".to_string() } else { "TIMEOUT".to_string() };
                
                // Add sell trade record to API
                {
                    let mut trades = trades_list.lock().await;
                    trades.insert(0, TradeRecord {
                        mint: mint.clone(),
                        symbol: holding.metadata.as_ref().and_then(|m| m.symbol.clone())
                            .or_else(|| holding.onchain.as_ref().and_then(|o| o.symbol.clone())),
                        name: holding.metadata.as_ref().and_then(|m| m.name.clone())
                            .or_else(|| holding.onchain.as_ref().and_then(|o| o.name.clone())),
                        image: holding.metadata.as_ref().and_then(|m| m.image.clone()),
                        trade_type: "sell".to_string(),
                        timestamp: sell_time.to_rfc3339(),
                        tx_signature: None,
                        amount_sol: sell_sol,
                        amount_tokens: sell_tokens as f64,
                        price_per_token: current_price,
                        profit_loss: Some(profit_sol),
                        profit_loss_percent: Some(profit_percent),
                        reason: Some(stop_reason.clone()),
                    });
                    // Keep only last 200 trades
                    if trades.len() > 200 {
                        trades.truncate(200);
                    }
                }
                
                // Remove buy record and write CSV
                if let Some(buy_rec) = trades_map.lock().await.remove(mint) {
                    // Append CSV row
                    let file_path = "trades.csv";
                    // New clearer header (human-readable, consistent numeric formatting)
                    let header = "mint,symbol,name,uri,image,creator,detect_time,buy_time,detect_to_buy_secs,buy_sol,buy_price_sol_per_token,buy_tokens,sell_time,stop_reason,sell_tokens,sell_sol,profit_percent,profit_sol\n";
                    let needs_header = !std::path::Path::new(file_path).exists();
                    if needs_header {
                        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(file_path) {
                            let _ = f.write_all(header.as_bytes());
                        }
                    }

                    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(file_path) {
                        let detect_to_buy = (buy_rec.buy_time - buy_rec.detect_time).num_seconds();
                        // buy_rec.buy_price is SOL per token
                        let buy_price_sol = buy_rec.buy_price;
                        // Format numbers for readability: SOL amounts with 9 decimals, percents with 2 decimals
                        let buy_sol_fmt = format!("{:.9}", buy_rec.buy_amount_sol);
                        let buy_price_sol_fmt = format!("{:.9}", buy_price_sol);
                        let sell_sol_fmt = format!("{:.9}", sell_sol);
                        let profit_percent_fmt = format!("{:.2}", profit_percent);
                        let profit_sol_fmt = format!("{:.9}", profit_sol);

                        // CSV-quote text fields to avoid breaking on commas/newlines
                        let q = |s: String| -> String {
                            // Escape double-quotes by doubling them
                            let escaped = s.replace('"', "\"\"");
                            format!("\"{}\"", escaped)
                        };

                        let line = format!(
                            "{mint},{symbol},{name},{uri},{image},{creator},{detect_time},{buy_time},{detect_to_buy_secs},{buy_sol},{buy_price},{buy_tokens},{sell_time},{stop_reason},{sell_tokens},{sell_sol},{profit_percent},{profit_sol}\n",
                            mint = q(buy_rec.mint),
                            symbol = q(buy_rec.symbol.unwrap_or_else(|| "".to_string())),
                            name = q(buy_rec.name.unwrap_or_else(|| "".to_string())),
                            uri = q(buy_rec.uri.unwrap_or_else(|| "".to_string())),
                            image = q(buy_rec.image.unwrap_or_else(|| "".to_string())),
                            creator = q(buy_rec.creator),
                            detect_time = buy_rec.detect_time.format("%+"),
                            buy_time = buy_rec.buy_time.format("%+"),
                            detect_to_buy_secs = detect_to_buy,
                            buy_sol = buy_sol_fmt,
                            buy_price = buy_price_sol_fmt,
                            buy_tokens = buy_rec.buy_amount_tokens,
                            sell_time = sell_time.format("%+"),
                            stop_reason = stop_reason,
                            sell_tokens = sell_tokens,
                            sell_sol = sell_sol_fmt,
                            profit_percent = profit_percent_fmt,
                            profit_sol = profit_sol_fmt
                        );
                        let _ = f.write_all(line.as_bytes());
                    }
                }
                to_remove.push(mint.clone());
            }
        }

        if !to_remove.is_empty() {
            // Unsubscribe from WSS for removed holdings to free subscription slots.
            let mut submap = sub_map.lock().await;
            for mint in &to_remove {
                if let Some((idx, sub_id)) = submap.remove(mint) {
                    if idx < ws_control_senders.len() {
                        let sender = &ws_control_senders[idx];
                        let (u_tx, u_rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
                        let _ = sender.send(WsRequest::Unsubscribe { sub_id, resp: u_tx }).await;
                        let _ = tokio::time::timeout(std::time::Duration::from_secs(3), u_rx).await;
                        debug!("Unsubscribed {} sub {} after sell", mint, sub_id);
                    }
                }
            }

            let mut holdings_lock = holdings.lock().await;
            for mint in to_remove {
                holdings_lock.remove(&mint);
            }
        }
    }
}