use crate::{
    models::{Holding, PriceCache},
    settings::Settings,
    rpc,
    api::{TradeRecord, BotControl},
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



use crate::shyft_monitor::ShyftControlMessage;

pub async fn monitor_holdings(
    holdings: Arc<Mutex<HashMap<String, Holding>>>,
    price_cache: Arc<Mutex<PriceCache>>,
    rpc_client: Arc<RpcClient>,
    is_real: bool,
    keypair: Option<&Keypair>,
    simulate_keypair: Option<&Keypair>,
    settings: Arc<Settings>,
    trades_map: Arc<Mutex<HashMap<String, BuyRecord>>>,
    shyft_control_tx: mpsc::Sender<ShyftControlMessage>,
    trades_list: Arc<tokio::sync::Mutex<Vec<TradeRecord>>>,
    bot_control: Arc<BotControl>,
) {
    // Debounce maps to avoid repeated subscribe/prime attempts and noisy warnings
    static PRICE_MISS_WARN_TIMES: Lazy<tokio::sync::Mutex<HashMap<String, Instant>>> = Lazy::new(|| tokio::sync::Mutex::new(HashMap::new()));
    const PRICE_MISS_WARN_DEBOUNCE_SECS: u64 = 60;
    
    let mut subscribed_mints = std::collections::HashSet::new();

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        
        // Check if bot is still running before processing trades
        let running_state = bot_control.running_state.lock().await;
        if format!("{:?}", *running_state).to_lowercase() != "running" {
            debug!("Monitor exiting: bot is not in running state");
            drop(running_state);
            break;
        }
        drop(running_state);
        
        let mut to_remove = Vec::new();
        let holdings_snapshot = holdings.lock().await.clone();

        for (mint, holding) in &holdings_snapshot {
            // If amount appears to be zero (e.g., token transferred/sold externally)
            // schedule removal to keep in-memory holdings consistent with on-chain state.
            if holding.amount == 0 {
                debug!("Detected zero balance for {} - scheduling removal", mint);
                to_remove.push(mint.clone());
                continue;
            }
            
            // Ensure subscription if using WSS
            if settings.price_source == "wss" {
                if !subscribed_mints.contains(mint) {
                    let _ = shyft_control_tx.send(ShyftControlMessage::SubscribePrice(mint.clone())).await;
                    subscribed_mints.insert(mint.clone());
                }
            }

            // Prefer WSS-provided cached prices when configured to use WSS only.
            let current_price_result: Result<f64, Box<dyn std::error::Error + Send + Sync>> = if settings.price_source == "wss" {
                let mut cache_guard = price_cache.lock().await;
                if let Some((ts, price)) = cache_guard.get(mint) {
                    // honor the cache TTL
                    if Instant::now().duration_since(*ts) < std::time::Duration::from_secs(settings.price_cache_ttl_secs) {
                        Ok(*price)
                    } else {
                        Err(Box::new(std::io::Error::other(format!("WSS cached price for {} expired", mint))))
                    }
                } else {
                    Err(Box::new(std::io::Error::other(format!("No WSS cached price for {}", mint))))
                }
            } else {
                rpc::fetch_current_price(mint, &price_cache, &rpc_client, &settings).await
            };

            let current_price = match current_price_result {
                Ok(price) => price,
                Err(e) => {
                    // If WSS failed (expired or missing), try one RPC prime
                    if settings.price_source == "wss" {
                         match rpc::fetch_current_price(mint, &price_cache, &rpc_client, &settings).await {
                            Ok(p) => {
                                debug!("Monitor primed price via RPC for {}: {:.18}", mint, p);
                                p
                            }
                            Err(e2) => {
                                let mut warns = PRICE_MISS_WARN_TIMES.lock().await;
                                let now = Instant::now();
                                let should_log = !matches!(warns.get(mint), Some(last) if now.duration_since(*last).as_secs() < PRICE_MISS_WARN_DEBOUNCE_SECS);
                                if should_log {
                                    warns.insert(mint.clone(), now);
                                    log::warn!("Price fetch failed for {}: {}", mint, e2);
                                }
                                if e2.to_string().contains("migrated") {
                                    to_remove.push(mint.clone());
                                }
                                continue;
                            }
                        }
                    } else {
                        log::warn!("Price fetch failed for {}: {}", mint, e);
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
                info!("TP hit for {}: +{:.6}% ({:.18} SOL/token)", mint, profit_percent, current_price);
                true
            } else if profit_percent <= settings.sl_percent {
                info!("SL hit for {}: {:.6}% ({:.18} SOL/token)", mint, profit_percent, current_price);
                true
            } else if elapsed >= settings.timeout_secs {
                info!("Timeout for {}: {}s ({:.18} SOL/token)", mint, elapsed, current_price);
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
                    simulate_keypair,
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
                // amount is in microtokens (10^6), so convert to tokens
                let sell_tokens_amount = sell_tokens as f64 / 1_000_000.0;
                // current_price is SOL per token; compute totals in SOL
                let sell_sol = sell_tokens_amount * current_price;
                let profit_percent = if holding.buy_price != 0.0 { ((current_price - holding.buy_price) / holding.buy_price) * 100.0 } else { 0.0 };
                // compute profit in SOL
                let profit_sol = sell_sol - (holding.buy_price * sell_tokens_amount);
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
                        amount_tokens: sell_tokens_amount,
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
                            sell_tokens = format!("{:.6}", sell_tokens_amount),
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
            for mint in &to_remove {
                if subscribed_mints.remove(mint) {
                    let _ = shyft_control_tx.send(ShyftControlMessage::UnsubscribePrice(mint.clone())).await;
                    debug!("Unsubscribed {} after sell", mint);
                }
            }

            let mut holdings_lock = holdings.lock().await;
            for mint in to_remove {
                // Log removal to API for better observability
                let _ = bot_control
                    .add_log(
                        "info",
                        format!("Removing holding {} from in-memory map", mint),
                        None,
                    )
                    .await;
                holdings_lock.remove(&mint);
            }
        }

        // Clean up old entries from debounce maps (every 10 minutes) to prevent unbounded growth
        static LAST_CLEANUP: Lazy<tokio::sync::Mutex<Option<Instant>>> = 
            Lazy::new(|| tokio::sync::Mutex::new(None));
        let mut last_cleanup = LAST_CLEANUP.lock().await;
        let now = Instant::now();
        if last_cleanup.is_none() || now.duration_since(last_cleanup.unwrap()) > std::time::Duration::from_secs(600) {
            *last_cleanup = Some(now);
            
            // Clean up SUBSCRIBE_ATTEMPT_TIMES
            let mut attempts = SUBSCRIBE_ATTEMPT_TIMES.lock().await;
            let cutoff = now - std::time::Duration::from_secs(SUBSCRIBE_ATTEMPT_DEBOUNCE_SECS * 2);
            attempts.retain(|_, last_time| *last_time > cutoff);
            
            // Clean up PRICE_MISS_WARN_TIMES  
            let mut warns = PRICE_MISS_WARN_TIMES.lock().await;
            let cutoff = now - std::time::Duration::from_secs(PRICE_MISS_WARN_DEBOUNCE_SECS * 2);
            warns.retain(|_, last_time| *last_time > cutoff);
        }
    }
}