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
use log::error;
use chrono::Utc;
use std::str::FromStr;
use solana_sdk::pubkey::Pubkey;

pub async fn monitor_holdings(
    holdings: Arc<Mutex<HashMap<String, Holding>>>,
    price_cache: Arc<Mutex<PriceCache>>,
    rpc_client: Arc<RpcClient>,
    is_real: bool,
    keypair: Option<Arc<Keypair>>,
    simulate_keypair: Option<Arc<Keypair>>,
    settings: Arc<Settings>,
    trades_map: Arc<Mutex<HashMap<String, BuyRecord>>>,
    ws_control_senders: Arc<Vec<mpsc::Sender<WsRequest>>>,
    sub_map: Arc<Mutex<HashMap<String, (usize, u64)>>>,
    _next_wss_sender: Arc<AtomicUsize>,
    trades_list: Arc<tokio::sync::Mutex<Vec<TradeRecord>>>,
    bot_control: Arc<BotControl>,
    ws_tx: tokio::sync::broadcast::Sender<String>,
) {
    static SUBSCRIBE_ATTEMPT_TIMES: Lazy<tokio::sync::Mutex<HashMap<String, Instant>>> = Lazy::new(|| tokio::sync::Mutex::new(HashMap::new()));
    const SUBSCRIBE_ATTEMPT_DEBOUNCE_SECS: u64 = 30;
    
    let (remove_tx, mut remove_rx) = tokio::sync::mpsc::channel::<String>(100);
    let mut processing = std::collections::HashSet::new();

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        
        while let Ok(msg) = remove_rx.try_recv() {
            let (is_done_only, mint_to_rem) = if msg.starts_with("DONE:") {
                (true, msg.replace("DONE:", ""))
            } else {
                (false, msg)
            };
            
            processing.remove(&mint_to_rem);
            
            if !is_done_only {
                {
                    let mut submap = sub_map.lock().await;
                    if let Some((idx, sub_id)) = submap.remove(&mint_to_rem) {
                        if idx < ws_control_senders.len() {
                            let sender = &ws_control_senders[idx];
                            let (u_tx, u_rx) = tokio::sync::oneshot::channel();
                            let _ = sender.send(WsRequest::Unsubscribe { sub_id, resp: u_tx }).await;
                            let _ = tokio::time::timeout(std::time::Duration::from_secs(3), u_rx).await;
                        }
                    }
                }
                {
                    let mut guard = holdings.lock().await;
                    if guard.remove(&mint_to_rem).is_some() {
                        let _ = bot_control.add_log("info", format!("Removed {} from monitor", mint_to_rem), None).await;
                    }
                }
            }
        }

        let running_state = bot_control.running_state.lock().await;
        if !matches!(*running_state, crate::api::BotRunningState::Running) { 
            drop(running_state);
            continue; 
        }
        drop(running_state);
        
        let holdings_snapshot = { holdings.lock().await.clone() };

        for (mint, holding) in holdings_snapshot {
            if holding.amount == 0 { 
                let _ = remove_tx.send(mint).await;
                continue; 
            }
            if processing.contains(&mint) { continue; }
            processing.insert(mint.clone());

            let rpc_client = Arc::clone(&rpc_client);
            let price_cache = Arc::clone(&price_cache);
            let settings = Arc::clone(&settings);
            let ws_control_senders = Arc::clone(&ws_control_senders);
            let trades_list = Arc::clone(&trades_list);
            let trades_map = Arc::clone(&trades_map);
            let ws_tx = ws_tx.clone();
            let sub_map = Arc::clone(&sub_map);
            let next_wss_sender = Arc::clone(&_next_wss_sender);
            let remove_tx = remove_tx.clone();
            let bot_control = Arc::clone(&bot_control);
            let holdings = Arc::clone(&holdings);
            let kp = keypair.clone();
            let sim_kp = simulate_keypair.clone();
            let mint_c = mint.clone();

            tokio::spawn(async move {
                // Calculate elapsed FIRST — timeout must be checked before the
                // potentially slow price fetch to avoid coins stuck past timeout.
                let elapsed = Utc::now().signed_duration_since(holding.buy_time).num_seconds();
                let is_timed_out = elapsed >= settings.timeout_secs;

                let current_price: f64 = if is_timed_out {
                    // Timeout: use any available cached price (even stale) or buy_price.
                    // Don't block on slow RPC — we already know we want to sell.
                    let cached = {
                        let mut cache_guard = price_cache.lock().await;
                        cache_guard.get(&mint_c).map(|(_, p)| *p)
                    };
                    let p = cached.unwrap_or(holding.buy_price);
                    log::info!("Timeout for {} ({}s >= {}s), using price {:.18}",
                        mint_c, elapsed, settings.timeout_secs, p);
                    p
                } else {
                    // Normal TP/SL evaluation — need fresh price
                    let price_result = if settings.price_source == "wss" {
                        let mut cache_guard = price_cache.lock().await;
                        if let Some((ts, price)) = cache_guard.get(&mint_c) {
                            if Instant::now().duration_since(*ts) < std::time::Duration::from_secs(settings.price_cache_ttl_secs) {
                                Ok(*price)
                            } else {
                                Err("expired")
                            }
                        } else {
                            Err("miss")
                        }
                    } else {
                        rpc::fetch_current_price(&mint_c, &price_cache, &rpc_client, &settings).await.map_err(|_| "rpc_err")
                    };

                    match price_result {
                        Ok(p) => p,
                        Err(_) => {
                            if settings.price_source == "wss" && !ws_control_senders.is_empty() {
                                let has_sub = { sub_map.lock().await.get(&mint_c).is_some() };
                                if !has_sub {
                                    let mut attempts = SUBSCRIBE_ATTEMPT_TIMES.lock().await;
                                    if !matches!(attempts.get(&mint_c), Some(last) if Instant::now().duration_since(*last).as_secs() < SUBSCRIBE_ATTEMPT_DEBOUNCE_SECS) {
                                        attempts.insert(mint_c.clone(), Instant::now());
                                        drop(attempts);
                                        if let Ok(mint_pk) = Pubkey::from_str(&mint_c) {
                                            let pump_prog = Pubkey::from_str(&settings.pump_fun_program).unwrap_or_default();
                                            let (curve_pda, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_prog);
                                            let idx = next_wss_sender.fetch_add(1, Ordering::Relaxed) % ws_control_senders.len();
                                            let (otx, _) = tokio::sync::oneshot::channel();
                                            let _ = ws_control_senders[idx].send(WsRequest::Subscribe { account: curve_pda.to_string(), mint: mint_c.clone(), resp: otx }).await;
                                        }
                                    }
                                }
                            }
                            // Cap RPC fallback at 15s to prevent blocking the monitor task
                            match tokio::time::timeout(
                                std::time::Duration::from_secs(15),
                                rpc::fetch_current_price(&mint_c, &price_cache, &rpc_client, &settings)
                            ).await {
                                Ok(Ok(p)) => p,
                                Ok(Err(e2)) => {
                                    let err_msg = e2.to_string();
                                    if err_msg.contains("migrated") { 
                                        let _ = remove_tx.send(mint_c).await; 
                                    } else {
                                        log::warn!("Price fetch failed for {} (will retry): {}", mint_c, err_msg);
                                        let _ = remove_tx.send(format!("DONE:{}", mint_c)).await;
                                    }
                                    return;
                                }
                                Err(_timeout) => {
                                    log::warn!("Price fetch timed out (15s) for {} (will retry)", mint_c);
                                    let _ = remove_tx.send(format!("DONE:{}", mint_c)).await;
                                    return;
                                }
                            }
                        }
                    }
                };

                let token_divisor = 10f64.powi(holding.decimals as i32);
                let profit_percent = if holding.buy_price != 0.0 { ((current_price - holding.buy_price) / holding.buy_price) * 100.0 } else { 0.0 };
                let tokens = holding.amount as f64 / token_divisor;
                let pnl_sol = (current_price - holding.buy_price) * tokens;

                let _ = ws_tx.send(serde_json::json!({
                    "type": "price-update",
                    "mint": mint_c,
                    "price": current_price,
                    "profit_percent": profit_percent,
                    "pnl_sol": pnl_sol,
                    "buy_price": holding.buy_price,
                    "amount": holding.amount,
                    "decimals": holding.decimals,
                    "triggered_tp": holding.triggered_tp_levels,
                    "triggered_sl": holding.triggered_sl_levels
                }).to_string());

                // --- Multi-level TP/SL evaluation ---
                // Check which TP/SL levels should trigger (that haven't already)
                let mut sell_amount: u64 = 0;
                let mut reason_str = String::new();
                let mut newly_triggered_tp: Vec<usize> = Vec::new();
                let mut newly_triggered_sl: Vec<usize> = Vec::new();

                if is_timed_out {
                    // Timeout: sell ALL remaining tokens
                    sell_amount = holding.amount;
                    reason_str = "TIMEOUT".to_string();
                } else {
                    // Check TP levels (sorted ascending by trigger_percent)
                    let mut tp_levels: Vec<(usize, &crate::settings::TpLevel)> = settings.tp_levels.iter().enumerate().collect();
                    tp_levels.sort_by(|a, b| a.1.trigger_percent.partial_cmp(&b.1.trigger_percent).unwrap_or(std::cmp::Ordering::Equal));
                    for (idx, level) in &tp_levels {
                        if holding.triggered_tp_levels.contains(idx) { continue; }
                        if profit_percent >= level.trigger_percent {
                            let partial = ((level.sell_percent / 100.0) * holding.original_amount as f64).round() as u64;
                            sell_amount += partial;
                            newly_triggered_tp.push(*idx);
                            if reason_str.is_empty() {
                                reason_str = format!("TP{} ({:.0}% @ +{:.1}%)", idx + 1, level.sell_percent, level.trigger_percent);
                            } else {
                                reason_str.push_str(&format!(" + TP{}", idx + 1));
                            }
                        }
                    }

                    // Check SL levels (sorted descending by trigger_percent, i.e. -10% before -20%)
                    let mut sl_levels: Vec<(usize, &crate::settings::SlLevel)> = settings.sl_levels.iter().enumerate().collect();
                    sl_levels.sort_by(|a, b| b.1.trigger_percent.partial_cmp(&a.1.trigger_percent).unwrap_or(std::cmp::Ordering::Equal));
                    for (idx, level) in &sl_levels {
                        if holding.triggered_sl_levels.contains(idx) { continue; }
                        if profit_percent <= level.trigger_percent {
                            let partial = ((level.sell_percent / 100.0) * holding.original_amount as f64).round() as u64;
                            sell_amount += partial;
                            newly_triggered_sl.push(*idx);
                            if reason_str.is_empty() {
                                reason_str = format!("SL{} ({:.0}% @ {:.1}%)", idx + 1, level.sell_percent, level.trigger_percent);
                            } else {
                                reason_str.push_str(&format!(" + SL{}", idx + 1));
                            }
                        }
                    }

                    // Clamp sell_amount to remaining tokens
                    if sell_amount > holding.amount {
                        sell_amount = holding.amount;
                    }
                }

                if sell_amount > 0 {
                    let is_final_sell = sell_amount >= holding.amount;
                    let kp_ref = kp.as_ref().map(|k| k.as_ref());
                    let sim_kp_ref = sim_kp.as_ref().map(|k| k.as_ref());
                    
                    match rpc::sell_token(&mint_c, sell_amount, current_price, holding.decimals, is_real, kp_ref, sim_kp_ref, &rpc_client, &settings, is_final_sell).await {
                        Ok(sell_result) => {
                            let sell_sol = (sell_amount as f64 / token_divisor) * current_price;
                            let buy_sol = holding.buy_price * (sell_amount as f64 / token_divisor);
                            let mut trades = trades_list.lock().await;
                            trades.insert(0, TradeRecord {
                                mint: mint_c.clone(),
                                symbol: holding.metadata.as_ref().and_then(|m| m.symbol.clone()),
                                name: holding.metadata.as_ref().and_then(|m| m.name.clone()),
                                image: holding.metadata.as_ref().and_then(|m| m.image.clone()),
                                trade_type: "sell".to_string(),
                                timestamp: Utc::now().to_rfc3339(),
                                tx_signature: None,
                                amount_sol: sell_sol,
                                amount_tokens: sell_amount as f64 / token_divisor,
                                price_per_token: current_price,
                                profit_loss: Some(sell_sol - buy_sol),
                                profit_loss_percent: Some(profit_percent),
                                reason: Some(reason_str.clone()),
                                decimals: holding.decimals,
                                actual_sol_change: sell_result.sol_balance_change,
                                tx_fee_sol: sell_result.tx_fee_sol,
                            });
                            if trades.len() > 200 { trades.truncate(200); }
                            drop(trades);

                            if is_final_sell {
                                // Full exit: remove holding entirely
                                let _ = remove_tx.send(mint_c.clone()).await;
                                trades_map.lock().await.remove(&mint_c);
                                let _ = bot_control.add_log("info", format!("Sold 100% of {} ({}) at {:.18} (profit: {:.2}%)", mint_c, reason_str, current_price, profit_percent), None).await;
                            } else {
                                // Partial sell: update holding in-place
                                {
                                    let mut guard = holdings.lock().await;
                                    if let Some(h) = guard.get_mut(&mint_c) {
                                        h.amount = h.amount.saturating_sub(sell_amount);
                                        for idx in &newly_triggered_tp { h.triggered_tp_levels.push(*idx); }
                                        for idx in &newly_triggered_sl { h.triggered_sl_levels.push(*idx); }
                                    }
                                }
                                let _ = remove_tx.send(format!("DONE:{}", mint_c)).await;
                                let pct_sold = (sell_amount as f64 / holding.original_amount as f64) * 100.0;
                                let _ = bot_control.add_log("info", format!("Partial sell {:.0}% of {} ({}) at {:.18} (profit: {:.2}%)", pct_sold, mint_c, reason_str, current_price, profit_percent), None).await;
                            }
                        }
                        Err(e) => { 
                            error!("Sell failed for {} ({}): {}", mint_c, reason_str, e);
                            let _ = bot_control.add_log("error", format!("Sell failed for {} ({}): {}", mint_c, reason_str, e), None).await;
                            if is_timed_out {
                                // Force-remove timed-out coins after sell failure to prevent
                                // infinite retry loops. Record as forced timeout sell.
                                log::warn!("Force-removing timed-out {} after sell failure", mint_c);
                                let _ = remove_tx.send(mint_c.clone()).await;
                                let mut trades = trades_list.lock().await;
                                trades.insert(0, TradeRecord {
                                    mint: mint_c.clone(),
                                    symbol: holding.metadata.as_ref().and_then(|m| m.symbol.clone()),
                                    name: holding.metadata.as_ref().and_then(|m| m.name.clone()),
                                    image: holding.metadata.as_ref().and_then(|m| m.image.clone()),
                                    trade_type: "sell".to_string(),
                                    timestamp: Utc::now().to_rfc3339(),
                                    tx_signature: None,
                                    amount_sol: (holding.amount as f64 / token_divisor) * current_price,
                                    amount_tokens: holding.amount as f64 / token_divisor,
                                    price_per_token: current_price,
                                    profit_loss: Some(((holding.amount as f64 / token_divisor) * current_price) - (holding.buy_price * (holding.amount as f64 / token_divisor))),
                                    profit_loss_percent: Some(profit_percent),
                                    reason: Some("TIMEOUT_FORCED".to_string()),
                                    decimals: holding.decimals,
                                    actual_sol_change: None,
                                    tx_fee_sol: None,
                                });
                                if trades.len() > 200 { trades.truncate(200); }
                                trades_map.lock().await.remove(&mint_c);
                            } else {
                                let _ = remove_tx.send(format!("DONE:{}", mint_c)).await;
                            }
                        }
                    }
                } else {
                    let _ = remove_tx.send(format!("DONE:{}", mint_c)).await;
                }
            });
        }
    }
}
