use crate::{settings::Settings, Holding, PriceCache};
use base64::{engine::general_purpose::STANDARD as Base64Engine, Engine};
use futures_util::{stream::StreamExt, SinkExt};
use log::{debug, error, info, warn};
use lru::LruCache;
use serde_json::{json, Value};
use solana_program::pubkey::Pubkey;
use std::{
    collections::HashMap,
    str::FromStr,
    sync::Arc,
    time::Instant,
};
use tokio::sync::{mpsc, Mutex, oneshot};
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[allow(dead_code)]
#[derive(Debug)]
pub enum WsRequest {
    Subscribe {
        account: String,
        mint: String,
        resp: oneshot::Sender<Result<u64, String>>,
    },
    Unsubscribe {
        sub_id: u64,
        resp: oneshot::Sender<Result<(), String>>,
    },
    GetHealth {
        resp: oneshot::Sender<WsHealth>,
    },
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct WsHealth {
    pub active_subs: usize,
    pub pending_subs: usize,
    pub recent_timeouts: usize,
    pub is_healthy: bool,
}

#[allow(dead_code)]
pub async fn run_ws(
    wss_url: &str,
    tx: mpsc::Sender<String>,
    _seen: Arc<Mutex<LruCache<String, ()>>>,
    holdings: Arc<Mutex<HashMap<String, Holding>>>,
    price_cache: Arc<Mutex<PriceCache>>,
    mut control_rx: mpsc::Receiver<WsRequest>,
    settings: Arc<Settings>,
) -> Result<(), Box<dyn std::error::Error>> {
    // ---------- outer re-connect loop ----------
    loop {
        let (ws_stream, _) = connect_async(wss_url).await?;
        let (mut write, mut read) = ws_stream.split();

        debug!(
            "WSS {} connected (max_create_to_buy_secs={})",
            wss_url,
            settings.max_create_to_buy_secs
        );

        // Subscribe to pump.fun program logs with optimized filtering
        // Use logsSubscribe which provides transaction signatures directly
        // This is the most reliable method that all professional snipers use
        info!(
            "Subscribing to pump.fun program logs: {} for new token detection",
            &settings.pump_fun_program
        );
        write
            .send(Message::Text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "logsSubscribe",
                    "params": [
                        { "mentions": [ &settings.pump_fun_program ] },
                        { "commitment": "confirmed" }
                    ]
                })
                .to_string(),
            ))
            .await?;

        // bonding-curve accounts for everything we already hold — subscribe to
        // the bonding_curve PDA (not the token vault). The BondingCurveState is
        // dynamic and contains both virtual reserves needed for price calculation.
        let holdings_accounts = {
            let holdings = holdings.lock().await;
            let pump_prog = Pubkey::from_str(&settings.pump_fun_program)?;
            holdings
                .keys()
                .filter_map(|mint_str| {
                    Pubkey::from_str(mint_str).ok().map(|mint_pk| {
                        let (pda, _) = Pubkey::find_program_address(
                            &[b"bonding-curve", mint_pk.as_ref()],
                            &pump_prog,
                        );
                        pda.to_string()
                    })
                })
                .collect::<Vec<_>>()
        };

        for acc in holdings_accounts {
            write
                .send(Message::Text(
                    json!({
                        "jsonrpc": "2.0",
                        "id": 2,
                        "method": "accountSubscribe",
                        "params": [ acc, { "commitment": "confirmed", "encoding": "base64" } ]
                    })
                    .to_string(),
                ))
                .await?;
        }

        // ---------- runtime state ----------
        let mut req_id_counter: i64 = 1000;
        let mut active_sub_count: usize = 0;
        let mut subid_to_mint: HashMap<u64, (String, Instant, String)> = HashMap::new();
        let mut pending_sub: HashMap<i64, (oneshot::Sender<Result<u64, String>>, Instant)> = HashMap::new();
        let mut recent_timeouts: usize = 0;
        let mut last_successful_sub: Option<Instant> = None;

        const CURVE_DISCRIM: [u8; 8] = [0x17, 0xb7, 0xf8, 0x37, 0x60, 0xd8, 0xac, 0x60];

        // ---------- inner event loop ----------
        loop {
            tokio::select! {
                // ---------- websocket incoming ----------
                msg = read.next() => {
                    let msg = match msg {
                        Some(Ok(m)) => m,
                        Some(Err(e)) => { error!("WS read error: {}", e); break; }
                        None => { error!("WS stream ended"); break; }
                    };

                    let text = match msg {
                        Message::Text(t) => t,
                        Message::Close(_) => { error!("WS close frame"); break; }
                        _ => continue,
                    };

                    let value: Value = match serde_json::from_str(&text) {
                        Ok(v) => v,
                        Err(e) => { debug!("JSON parse error: {}", e); continue; }
                    };

                    // Only forward notifications (which contain `params`) into the
                    // main processing channel. RPC responses that only contain
                    // `id`/`result` are handled locally here and shouldn't be
                    // forwarded (they previously triggered "missing params" logs).
                    if value.get("params").is_some() {
                        let _ = tx.send(text.clone()).await;
                    }

                    // ---- subscription response ----
                    if let (Some(id), Some(result)) =
                        (value.get("id").and_then(|v| v.as_i64()), value.get("result"))
                    {
                        if let Some((responder, _timestamp)) = pending_sub.remove(&id) {
                            if let Some(sub_id) = result.as_u64() {
                                // Move any placeholder mapping keyed by request-id to the
                                // actual subscription id returned by the RPC. Earlier we
                                // insert a placeholder entry using the request id so
                                // that we can preserve mint/account info until the
                                // RPC responds with the real sub id. If we don't
                                // remap, account notifications (which carry the real
                                // subscription id) will not be associated with the
                                // intended mint.
                                if let Some(entry) = subid_to_mint.remove(&(id as u64)) {
                                    subid_to_mint.insert(sub_id, entry);
                                    debug!("Subscription confirmed: req_id={} -> sub_id={}", id, sub_id);
                                }
                                last_successful_sub = Some(Instant::now());
                                // Reset timeout counter on success
                                if recent_timeouts > 0 {
                                    recent_timeouts = recent_timeouts.saturating_sub(1);
                                }
                                let _ = responder.send(Ok(sub_id));
                            } else {
                                // Subscription failed - decrement counter
                                active_sub_count = active_sub_count.saturating_sub(1);
                                let _ = responder.send(Err(format!(
                                    "subscribe result missing subscription id: {}",
                                    text
                                )));
                            }
                            continue;
                        }
                    }

                    // ---- unsubscribe response ----
                    if let Some(id) = value.get("id").and_then(|v| v.as_i64()) {
                        if id == -1 {
                            // This is an unsubscribe response (we use id=-1 for unsubscribes)
                            // The result should be a boolean indicating success
                            if let Some(result) = value.get("result") {
                                if result.as_bool() == Some(true) {
                                    debug!("Unsubscribe confirmed");
                                } else {
                                    debug!("Unsubscribe response: {:?}", result);
                                }
                            }
                            continue;
                        }
                    }

                    // ---- account notification ----
                    if let Some(params) = value.get("params") {
                        let sub_id = match params.get("subscription").and_then(|v| v.as_u64()) {
                            Some(s) => s,
                            None => continue,
                        };
                        
                        // Check if this is a bonding curve account we're tracking
                        let (mint, last, _account_pubkey) = match subid_to_mint.get_mut(&sub_id) {
                            Some(v) => v,
                            None => continue, // Not a subscription we're tracking
                        };
                        *last = Instant::now();
                        
                        let data_arr = match params
                            .get("result")
                            .and_then(|v| v.get("value"))
                            .and_then(|v| v.get("data"))
                            .and_then(|v| v.as_array())
                        {
                            Some(a) => a,
                            None => continue,
                        };
                        let encoded = match data_arr.first().and_then(|v| v.as_str()) {
                            Some(e) => e,
                            None => continue,
                        };
                        let decoded = match Base64Engine.decode(encoded) {
                            Ok(d) => d,
                            Err(e) => { debug!("base64 decode error: {}", e); continue; }
                        };

                        // ---- bonding-curve account ----
                        if decoded.len() >= 8 && decoded[..8] == CURVE_DISCRIM[..] {
                            if decoded.len() < 8 + 41 {
                                error!("curve account too short for sub {}", sub_id);
                                continue;
                            }
                            let slice2 = &decoded[8..];
                            let vtok = if let Ok(bytes) = slice2[0..8].try_into() {
                                u64::from_le_bytes(bytes)
                            } else {
                                error!("Failed to convert slice to u64 for vtok");
                                continue;
                            };
                            let vsol = if let Ok(bytes) = slice2[8..16].try_into() {
                                u64::from_le_bytes(bytes)
                            } else {
                                error!("Failed to convert slice to u64 for vsol");
                                continue;
                            };
                            let complete = slice2[40] != 0;

                            if complete {
                                error!("Bonding curve state reports migrated for sub {} mint {}", sub_id, mint);
                                continue;
                            }

                            if vtok == 0 {
                                error!("Error: virtual_token_reserves is zero for mint {} (sub {})", mint, sub_id);
                                continue;
                            }

                            // Compute price in SOL per token using the virtual reserves
                            let virtual_sol_lamports = vsol as f64;
                            let virtual_token_base_units = vtok as f64;
                            // SOL = lamports / 1e9; tokens = base_units / 1e6
                            let price_in_sol_per_token = (virtual_sol_lamports / 1_000_000_000.0)
                                / (virtual_token_base_units / 1_000_000.0);

                            let mut cache = price_cache.lock().await;
                            if let Some((_, prev)) = cache.get(mint).map(|e| (e.0, e.1)) {
                                // Compute percent change robustly. If the previous price is
                                // extremely small or zero, clamp the denominator to avoid
                                // producing misleading huge percentages.
                                let denom = if prev.abs() < 1e-18 { 1e-18 } else { prev };
                                let pct_last = (price_in_sol_per_token - prev) / denom * 100.0;

                                // If we hold this mint, also compute change relative to the
                                // buy price for operator convenience.
                                let mut pct_from_buy: Option<f64> = None;
                                if let Ok(holdings_guard) = holdings.try_lock() {
                                    if let Some(h) = holdings_guard.get(mint) {
                                        let buy = h.buy_price;
                                        if buy.abs() >= 1e-18 {
                                            pct_from_buy = Some((price_in_sol_per_token - buy) / buy * 100.0);
                                        }
                                    }
                                }

                                if pct_last.abs() > 0.0 {
                                    if let Some(pbuy) = pct_from_buy {
                                        info!(
                                            "WSS price change for {}: {:.18} -> {:.18} SOL (delta_last={:.6}%, delta_from_buy={:+.6}%)",
                                            mint, prev, price_in_sol_per_token, pct_last, pbuy
                                        );
                                    } else {
                                        info!(
                                            "WSS price change for {}: {:.18} -> {:.18} SOL (delta_last={:.6}%)",
                                            mint, prev, price_in_sol_per_token, pct_last
                                        );
                                    }
                                }
                            } else {
                                info!(
                                    "WSS initial price for {}: {:.18} SOL",
                                    mint, price_in_sol_per_token
                                );
                            }
                            // Store price in SOL per token (consistent with monitor expectations)
                            cache.put(mint.clone(), (Instant::now(), price_in_sol_per_token));
                            debug!("WS updated curve price for {} sub {}", mint, sub_id);

                            continue;
                        }

                        // We no longer subscribe to SPL token accounts. All price
                        // information comes from the bonding_curve PDA stream. If the
                        // account data doesn't match a known shape above, ignore it.

                        debug!("unrecognised account data shape for sub {} (len={})", sub_id, decoded.len());
                    }

                }

                // ---------- control channel ----------
                Some(req) = control_rx.recv() => {
                    match req {
                        WsRequest::GetHealth { resp } => {
                            // Check if connection is healthy based on recent activity
                            let is_healthy = recent_timeouts < 3 && (
                                last_successful_sub.is_none() || 
                                last_successful_sub.map(|t| t.elapsed().as_secs() < 300).unwrap_or(false)
                            );
                            let _ = resp.send(WsHealth {
                                active_subs: active_sub_count,
                                pending_subs: pending_sub.len(),
                                recent_timeouts,
                                is_healthy,
                            });
                        }
                        WsRequest::Subscribe { account, mint, resp } => {
                            // Fast-fail if connection appears unhealthy
                            if recent_timeouts >= 5 {
                                warn!("WSS connection unhealthy (recent_timeouts={}), rejecting subscription for {}", recent_timeouts, mint);
                                let _ = resp.send(Err(format!("WSS connection degraded (timeouts={})", recent_timeouts)));
                                continue;
                            }
                            
                            if active_sub_count >= settings.max_subs_per_wss {
                                let _ = resp.send(Err(format!(
                                    "max subscriptions reached on this WSS ({})",
                                    settings.max_subs_per_wss
                                )));
                            } else {
                                req_id_counter += 1;
                                let id = req_id_counter;
                                let req_json = json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "method": "accountSubscribe",
                                    "params": [ account.clone(), { "commitment": "confirmed", "encoding": "base64" } ]
                                })
                                .to_string();
                                if let Err(e) = write.send(Message::Text(req_json)).await {
                                    error!("subscribe send error for {}: {}", mint, e);
                                    let _ = resp.send(Err(format!("failed to send subscribe request: {}", e)));
                                } else {
                                    // Store pending subscription with timestamp for timeout tracking
                                    pending_sub.insert(id, (resp, Instant::now()));
                                    subid_to_mint.insert(
                                        id as u64, // placeholder – will be overwritten when RPC answers
                                        (mint.clone(), Instant::now(), account),
                                    );
                                    // Increment counter optimistically - will decrement if subscription fails
                                    active_sub_count += 1;
                                    debug!("Sent subscribe request for {} (req_id={}, active={}/{})", mint, id, active_sub_count, settings.max_subs_per_wss);
                                }
                            }
                        }
                        WsRequest::Unsubscribe { sub_id, resp } => {
                            let req_json = json!({
                                "jsonrpc": "2.0",
                                "id": -1,
                                "method": "accountUnsubscribe",
                                "params": [ sub_id ]
                            })
                            .to_string();
                            if let Err(e) = write.send(Message::Text(req_json)).await {
                                error!("failed to send unsubscribe for sub {}: {}", sub_id, e);
                                let _ = resp.send(Err(format!("failed to send unsubscribe: {}", e)));
                            } else {
                                // Remove from tracking and decrement counter
                                if let Some((mint, _, _)) = subid_to_mint.remove(&sub_id) {
                                    active_sub_count = active_sub_count.saturating_sub(1);
                                    debug!("Sent unsubscribe for {} sub {} (active={}/{})", mint, sub_id, active_sub_count, settings.max_subs_per_wss);
                                } else {
                                    debug!("Sent unsubscribe for unknown sub {} (active={}/{})", sub_id, active_sub_count, settings.max_subs_per_wss);
                                }
                                let _ = resp.send(Ok(()));
                            }
                        }
                    }
                }

                // ---------- periodic TTL clean-up ----------
                _ = tokio::time::sleep(std::time::Duration::from_secs(settings.sub_ttl_secs.min(30))) => {
                    let now = Instant::now();
                    
                    // Clean up timed-out pending subscription requests
                    let timed_out_pending: Vec<i64> = pending_sub
                        .iter()
                        .filter_map(|(req_id, (_sender, timestamp))| {
                            if now.duration_since(*timestamp).as_secs() > settings.wss_subscribe_timeout_secs {
                                Some(*req_id)
                            } else {
                                None
                            }
                        })
                        .collect();
                    
                    if !timed_out_pending.is_empty() {
                        recent_timeouts = recent_timeouts.saturating_add(timed_out_pending.len());
                        warn!("WSS {} timing out {} subscriptions (recent_timeouts={})", wss_url, timed_out_pending.len(), recent_timeouts);
                    }
                    
                    for req_id in timed_out_pending {
                        if let Some((sender, _)) = pending_sub.remove(&req_id) {
                            // Decrement active count since this subscription never completed
                            active_sub_count = active_sub_count.saturating_sub(1);
                            // Remove placeholder mapping
                            subid_to_mint.remove(&(req_id as u64));
                            let _ = sender.send(Err(format!("subscription request timed out after {}s", settings.wss_subscribe_timeout_secs)));
                            debug!("Cleaned up timed-out pending subscription req_id={} (active={}/{})", req_id, active_sub_count, settings.max_subs_per_wss);
                        }
                    }
                    
                    // Clean up stale active subscriptions based on TTL
                    let to_remove: Vec<u64> = subid_to_mint
                        .iter()
                        .filter_map(|(sid, (mint, last, _))| {
                            // Skip placeholder entries (these are tracked in pending_sub)
                            if pending_sub.contains_key(&(*sid as i64)) {
                                return None;
                            }
                            if now.duration_since(*last).as_secs() > settings.sub_ttl_secs {
                                debug!("Subscription {} for {} is stale ({}s since last update, TTL={}s)", sid, mint, now.duration_since(*last).as_secs(), settings.sub_ttl_secs);
                                Some(*sid)
                            } else {
                                None
                            }
                        })
                        .collect();

                    for sid in to_remove {
                        let req_json = json!({
                            "jsonrpc": "2.0",
                            "id": -1,
                            "method": "accountUnsubscribe",
                            "params": [ sid ]
                        })
                        .to_string();
                        if let Err(e) = write.send(Message::Text(req_json)).await {
                            error!("failed to send unsubscribe for stale sub {}: {}", sid, e);
                        } else if let Some((mint, _, _)) = subid_to_mint.remove(&sid) {
                            active_sub_count = active_sub_count.saturating_sub(1);
                            debug!("Unsubscribed stale sub {} for {} (active={}/{})", sid, mint, active_sub_count, settings.max_subs_per_wss);
                        }
                    }
                }
            }
        }

        info!("WS connection closed; reconnecting in 2 s …");
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}
