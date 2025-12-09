use crate::{
    models::Holding,
    rpc,
    api::{TradeRecord, BotControl},
    state::BuyRecord,
    price_subscriber::CliPriceSubscriber,
};
use sol_beast_core::settings::Settings;
use sol_beast_core::sell_service::{SellService, SellConfig};
use sol_beast_core::native::{NativeRpcClient, transaction_signer::NativeTransactionSigner};
use solana_client::rpc_client::RpcClient;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use solana_sdk::signature::Keypair;
use log::{info, debug, error, warn};
use chrono::Utc;
use std::io::Write;

pub async fn monitor_holdings(
    holdings: Arc<Mutex<HashMap<String, Holding>>>,
    rpc_client: Arc<RpcClient>,
    is_real: bool,
    keypair: Option<&Keypair>,
    _simulate_keypair: Option<&Keypair>,
    settings: Arc<Settings>,
    trades_map: Arc<Mutex<HashMap<String, BuyRecord>>>,
    price_subscriber: Arc<Mutex<CliPriceSubscriber>>,
    trades_list: Arc<tokio::sync::Mutex<Vec<TradeRecord>>>,
    bot_control: Arc<BotControl>,
) {
    // Helper to resolve the current price using PriceSubscriber, with RPC fallback
    async fn resolve_price(
        mint: &str,
        price_subscriber: &Arc<Mutex<CliPriceSubscriber>>,
        rpc_client: &Arc<RpcClient>,
        settings: &Arc<Settings>,
    ) -> Option<f64> {
        if settings.price_source == "wss" {
            // Ensure subscription and try cached price first
            {
                let sub = price_subscriber.lock().await;
                let _ = sub.subscribe_mint(mint).await;
                if let Some(p) = sub.cached_price(mint).await {
                    return Some(p);
                }
            }

            // Fallback to RPC and seed the cache
            let cache = price_subscriber.lock().await.price_cache();
            if let Ok(p) = rpc::fetch_current_price(mint, &cache, rpc_client, settings).await {
                price_subscriber.lock().await.prime_price(mint, p).await;
                return Some(p);
            }
        } else {
            let cache = price_subscriber.lock().await.price_cache();
            if let Ok(p) = rpc::fetch_current_price(mint, &cache, rpc_client, settings).await {
                return Some(p);
            }
        }
        None
    }

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

            let Some(current_price) = resolve_price(mint, &price_subscriber, &rpc_client, &settings).await else {
                warn!("Price unavailable for {} - skipping", mint);
                continue;
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
                let sell_result = if is_real {
                    if let Some(payer) = keypair {
                        let owned_kp = Keypair::try_from(&payer.to_bytes()[..]).unwrap();
                        let signer = NativeTransactionSigner::new(owned_kp);
                        let rpc_wrapper = NativeRpcClient::from_arc(rpc_client.clone());
                        
                        let config = SellConfig {
                            mint: mint.clone(),
                            amount: holding.amount,
                            current_price_sol: current_price,
                            close_ata: true,
                        };
                        
                        SellService::execute_sell(config, &rpc_wrapper, &signer, &settings).await
                    } else {
                        Err(sol_beast_core::error::CoreError::Validation("Keypair required for real sell".to_string()))
                    }
                } else {
                    info!("Simulating sell for {} (Dry Run)", mint);
                    Ok(sol_beast_core::sell_service::SellResult {
                        mint: mint.clone(),
                        amount: holding.amount,
                        transaction_signature: "simulated_sell_sig".to_string(),
                        timestamp: Utc::now().timestamp(),
                    })
                };

                match sell_result
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
            let mut holdings_lock = holdings.lock().await;
            for mint in to_remove {
                // Unsubscribe from price stream
                let _ = price_subscriber.lock().await.unsubscribe_mint(&mint).await;
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
    }
}