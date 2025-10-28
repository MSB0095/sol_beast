use crate::{settings::Settings, Holding, PriceCache};
use base64::{engine::general_purpose::STANDARD as Base64Engine, Engine};
use futures_util::{stream::StreamExt, SinkExt};
use log::{debug, error, info};
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
}

pub async fn run_ws(
    wss_url: &str,
    tx: mpsc::Sender<String>,
    seen: Arc<Mutex<LruCache<String, ()>>>,
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
            "WSS {} connected; seen cache size {} (max_detect_to_buy_secs={})",
            wss_url,
            seen.lock().await.len(),
            settings.max_detect_to_buy_secs
        );

        // pump.fun program logs
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
        let mut pending_sub: HashMap<i64, oneshot::Sender<Result<u64, String>>> = HashMap::new();

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
                        if let Some(responder) = pending_sub.remove(&id) {
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
                                }
                                let _ = responder.send(Ok(sub_id));
                            } else {
                                let _ = responder.send(Err(format!(
                                    "subscribe result missing subscription id: {}",
                                    text
                                )));
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
                        let (mint, last, _account_pubkey) = match subid_to_mint.get_mut(&sub_id) {
                            Some(v) => v,
                            None => continue,
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
                        let encoded = match data_arr.get(0).and_then(|v| v.as_str()) {
                            Some(e) => e,
                            None => continue,
                        };
                        let decoded = match Base64Engine.decode(encoded) {
                            Ok(d) => d,
                            Err(e) => { debug!("base64 decode error: {}", e); continue; }
                        };

                        // ---- bonding-curve account ----
                        if decoded.len() >= 8 && &decoded[..8] == &CURVE_DISCRIM[..] {
                            if decoded.len() < 8 + 41 {
                                error!("curve account too short for sub {}", sub_id);
                                continue;
                            }
                            let slice2 = &decoded[8..];
                            let vtok = u64::from_le_bytes(slice2[0..8].try_into().unwrap());
                            let vsol = u64::from_le_bytes(slice2[8..16].try_into().unwrap());
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
                            if let Some((_, prev)) = cache.get(mint).map(|e| (e.0.clone(), e.1)) {
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
                        WsRequest::Subscribe { account, mint, resp } => {
                            if active_sub_count >= settings.max_subs_per_wss {
                                let _ = resp.send(Err(format!(
                                    "max subscriptions reached on this WSS ({}).",
                                    settings.max_subs_per_wss
                                )));
                            } else {
                                req_id_counter += 1;
                                let id = req_id_counter;
                                pending_sub.insert(id, resp);
                                let req_json = json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "method": "accountSubscribe",
                                    "params": [ account, { "commitment": "confirmed", "encoding": "base64" } ]
                                })
                                .to_string();
                                if let Err(e) = write.send(Message::Text(req_json)).await {
                                    debug!("subscribe send error: {}", e);
                                    pending_sub.remove(&id);
                                } else {
                                    subid_to_mint.insert(
                                        id as u64, // placeholder – will be overwritten when RPC answers
                                        (mint, Instant::now(), account),
                                    );
                                    active_sub_count += 1;
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
                            let reply = if write.send(Message::Text(req_json)).await.is_err() {
                                Err("failed to send unsubscribe".into())
                            } else {
                                subid_to_mint.remove(&sub_id);
                                if active_sub_count > 0 { active_sub_count -= 1; }
                                Ok(())
                            };
                            let _ = resp.send(reply);
                        }
                    }
                }

                // ---------- periodic TTL clean-up ----------
                _ = tokio::time::sleep(std::time::Duration::from_secs(settings.sub_ttl_secs)) => {
                    let now = Instant::now();
                    let to_remove: Vec<u64> = subid_to_mint
                        .iter()
                        .filter_map(|(sid, (_, last, _))| {
                            (now.duration_since(*last).as_secs() > settings.sub_ttl_secs)
                                .then_some(*sid)
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
                            debug!("failed to send unsubscribe for {}: {}", sid, e);
                        } else {
                            subid_to_mint.remove(&sid);
                            if active_sub_count > 0 { active_sub_count -= 1; }
                            debug!("unsubscribed stale sub {}", sid);
                        }
                    }
                }
            }
        }

        info!("WS connection closed; reconnecting in 2 s …");
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}
