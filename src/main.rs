mod models;
mod rpc;
mod tx_builder;
mod idl;
mod settings;
mod ws;
mod helius_sender;
use ws::WsRequest;

use std::str::FromStr;
use once_cell::sync::Lazy;

// Debounce for repetitive "max held coins" logs so we don't spam the logs.
static LAST_MAX_HELD_LOG: Lazy<tokio::sync::Mutex<Option<Instant>>> = Lazy::new(|| tokio::sync::Mutex::new(None));
const MAX_HELD_LOG_DEBOUNCE_SECS: u64 = 60;
use crate::{
    models::{Holding, PriceCache},
    settings::Settings,
};
use chrono::Utc;
use log::{error, info, debug};
use solana_sdk::signature::Signer;
use lru::LruCache;
use solana_sdk::signature::Keypair;
use mpl_token_metadata::accounts::Metadata as OnchainMetadataRaw;
use std::{collections::HashMap, fs, sync::Arc, time::Instant};
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{mpsc, Mutex};
use tokio::time::{sleep, Duration};

// Buy record used to track a pending trade until it's closed (sold).
#[derive(Clone, Debug)]
struct BuyRecord {
    mint: String,
    symbol: Option<String>,
    name: Option<String>,
    uri: Option<String>,
    image: Option<String>,
    creator: String,
    detect_time: chrono::DateTime<Utc>,
    buy_time: chrono::DateTime<Utc>,
    buy_amount_sol: f64,
    buy_amount_tokens: u64,
    buy_price: f64,
}

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
    // Touch these settings here so they are used by the binary (avoid warnings)
    let price_source_cfg = settings.price_source.clone();
    let rpc_rotate_secs_cfg = settings.rpc_rotate_interval_secs;
    info!("Configured price_source={} rpc_rotate_interval_secs={}", price_source_cfg, rpc_rotate_secs_cfg);
    let seen = Arc::new(Mutex::new(LruCache::new(
        settings.cache_capacity.try_into().unwrap(),
    )));
    let holdings = Arc::new(Mutex::new(HashMap::new()));
    // Map to track buy metadata so we can write completed trades to CSV on sell
    let trades_map: Arc<Mutex<HashMap<String, BuyRecord>>> = Arc::new(Mutex::new(HashMap::new()));
    let price_cache = Arc::new(Mutex::new(LruCache::new(
        settings.cache_capacity.try_into().unwrap(),
    )));
    // Shared map of active subscriptions for mints we care about. Value is (wss_sender_index, sub_id)
    let sub_map: Arc<Mutex<HashMap<String, (usize, u64)>>> = Arc::new(Mutex::new(HashMap::new()));
    let (tx, mut rx) = mpsc::channel(1000);
    let is_real = std::env::args().any(|arg| arg == "--real");
    // Load real keypair either from path or from JSON in config (optional)
    // Prefer base64 env var for keypairs to avoid storing keys on disk.
    let keypair: Option<std::sync::Arc<Keypair>> = if is_real {
        if let Some(bytes) = settings::load_keypair_from_env_var("SOL_BEAST_KEYPAIR_B64") {
            Some(std::sync::Arc::new(Keypair::try_from(bytes.as_slice()).expect("Invalid SOL_BEAST_KEYPAIR_B64")))
        } else if let Some(pk_string) = settings.wallet_private_key_string.clone() {
            let bytes = settings::parse_private_key_string(&pk_string).expect("Invalid wallet_private_key_string");
            Some(std::sync::Arc::new(Keypair::try_from(bytes.as_slice()).expect("Invalid private key")))
        } else if let Some(j) = settings.wallet_keypair_json.clone() {
            let bytes: Vec<u8> = serde_json::from_str(&j).expect("Invalid wallet_keypair_json");
            Some(std::sync::Arc::new(Keypair::try_from(bytes.as_slice()).expect("Invalid keypair")))
        } else if let Some(path) = settings.wallet_keypair_path.clone() {
            let bytes = fs::read(path).expect("Keypair file missing");
            Some(std::sync::Arc::new(Keypair::try_from(bytes.as_slice()).expect("Invalid keypair")))
        } else {
            panic!("No wallet keypair configured! Set wallet_keypair_path, wallet_private_key_string, wallet_keypair_json, or SOL_BEAST_KEYPAIR_B64 env var");
        }
    } else {
        None
    };

    // Optional simulation keypair (used for dry-run signing). If not provided,
    // the code will fall back to generating an ephemeral Keypair at runtime.
    let simulate_keypair: Option<std::sync::Arc<Keypair>> = if let Some(bytes) = settings::load_keypair_from_env_var("SOL_BEAST_SIMULATE_KEYPAIR_B64") {
        Some(std::sync::Arc::new(Keypair::try_from(bytes.as_slice()).expect("Invalid SOL_BEAST_SIMULATE_KEYPAIR_B64")))
    } else if let Some(pk_string) = settings.simulate_wallet_private_key_string.clone() {
        let bytes = settings::parse_private_key_string(&pk_string).expect("Invalid simulate_wallet_private_key_string");
        let kp = Keypair::try_from(bytes.as_slice()).expect("Invalid simulate private key");
        info!("Loaded simulate keypair, pubkey: {}", kp.pubkey());
        Some(std::sync::Arc::new(kp))
    } else if let Some(j) = settings.simulate_wallet_keypair_json.clone() {
        let bytes: Vec<u8> = serde_json::from_str(&j).expect("Invalid simulate_wallet_keypair_json");
        Some(std::sync::Arc::new(Keypair::try_from(bytes.as_slice()).expect("Invalid simulate keypair")))
    } else {
        None
    };

    // Spawn price monitoring
    let _holdings_clone = holdings.clone();
    let _price_cache_clone = price_cache.clone();
    let _settings_clone = settings.clone();
    let _keypair_clone = keypair.clone();
    let simulate_keypair_clone = simulate_keypair.clone();
    let _trades_map_clone = trades_map.clone();

    // Spawn WSS tasks and keep control senders so we can request subscriptions
    let mut ws_control_senders: Vec<mpsc::Sender<WsRequest>> = Vec::new();
    for wss_url in settings.solana_ws_urls.iter() {
        let tx = tx.clone();
        let seen = seen.clone();
        let holdings_clone = holdings.clone();
        let price_cache_clone = price_cache.clone();
        let settings_clone = settings.clone();
        let wss_url = wss_url.clone();
        let (ctrl_tx, ctrl_rx) = mpsc::channel(256);
        ws_control_senders.push(ctrl_tx.clone());
        // Spawn a single task that owns the control receiver. `ws::run_ws` will
        // manage its own internal state and reconnect logic where appropriate.
        tokio::spawn(async move {
            if let Err(e) = ws::run_ws(
                &wss_url,
                tx.clone(),
                seen.clone(),
                holdings_clone.clone(),
                price_cache_clone.clone(),
                ctrl_rx,
                settings_clone.clone(),
            )
            .await
            {
                error!("WSS connection to {} failed: {}.", wss_url, e);
            }
        });
    }
    let ws_control_senders = Arc::new(ws_control_senders);
    // Round-robin index for WSS sender selection (true round-robin)
    let next_wss_sender = Arc::new(AtomicUsize::new(0usize));

    // Now spawn price monitoring (after ws_control_senders exists so monitor
    // can unsubscribe subscriptions on sell).
    let holdings_clone = holdings.clone();
    let price_cache_clone = price_cache.clone();
    let settings_clone = settings.clone();
    let keypair_clone = keypair
        .as_ref()
        .map(|kp| Keypair::try_from(kp.to_bytes().as_ref()).unwrap());
    let trades_map_clone = trades_map.clone();
    let ws_control_senders_clone_for_monitor = ws_control_senders.clone();
    let sub_map_clone_for_monitor = sub_map.clone();
    let next_wss_sender_clone_for_monitor = next_wss_sender.clone();
    let simulate_keypair_clone_for_monitor = simulate_keypair_clone.clone();
    tokio::spawn(async move {
        monitor_holdings(
            holdings_clone,
            price_cache_clone,
            is_real,
            keypair_clone.as_ref(),
            simulate_keypair_clone_for_monitor.as_ref().map(|a| &**a),
            settings_clone,
            trades_map_clone,
            ws_control_senders_clone_for_monitor,
            sub_map_clone_for_monitor,
            next_wss_sender_clone_for_monitor,
        )
        .await
    });

    // Process messages
    while let Some(msg) = rx.recv().await {
        if let Err(e) = process_message(
            &msg,
            &seen,
            &holdings,
            is_real,
            keypair.as_ref().map(|v| &**v),
            simulate_keypair.as_ref().map(|v| &**v),
            &price_cache,
            &settings,
            ws_control_senders.clone(),
            next_wss_sender.clone(),
            trades_map.clone(),
            sub_map.clone(),
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
    simulate_keypair: Option<&Keypair>,
    price_cache: &Arc<Mutex<PriceCache>>,
    settings: &Arc<Settings>,
    ws_control_senders: Arc<Vec<mpsc::Sender<WsRequest>>>,
    next_wss_sender: Arc<AtomicUsize>,
    trades_map: Arc<Mutex<HashMap<String, BuyRecord>>>,
    sub_map: Arc<Mutex<HashMap<String, (usize, u64)>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // NOTE: don't short-circuit all incoming messages when max held coins is
    // reached — that would also skip websocket account notifications which we
    // rely on to update cached prices. Only skip handling of new token
    // detection (InitializeMint2) when at capacity. We'll perform a debounced
    // debug log where appropriate.
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
                        // If we're already at max holdings, skip detection work
                        // but do not block processing of other websocket messages
                        // (like account notifications). Debounce the debug log
                        // so it doesn't spam the logs.
                        if holdings.lock().await.len() >= settings.max_holded_coins {
                            let mut last_lock = LAST_MAX_HELD_LOG.lock().await;
                            let now = Instant::now();
                            let should_log = match *last_lock {
                                Some(ts) => now.duration_since(ts).as_secs() > MAX_HELD_LOG_DEBOUNCE_SECS,
                                None => true,
                            };
                            if should_log {
                                *last_lock = Some(now);
                                debug!("Max held coins reached ({}); skipping incoming message processing", settings.max_holded_coins);
                            }
                            // Do not attempt handle_new_token when at capacity.
                            return Ok(());
                        }

                        let detect_time = Utc::now();
                        info!("New pump.fun token: {}", signature);
                        if let Err(e) = handle_new_token(
                            signature,
                            holdings,
                            is_real,
                            keypair,
                            simulate_keypair,
                            price_cache,
                            settings,
                            ws_control_senders.clone(),
                            next_wss_sender.clone(),
                            detect_time,
                            trades_map.clone(),
                            sub_map.clone(),
                        )
                        .await
                        {
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
    simulate_keypair: Option<&Keypair>,
    price_cache: &Arc<Mutex<PriceCache>>,
    settings: &Arc<Settings>,
    ws_control_senders: Arc<Vec<mpsc::Sender<WsRequest>>>,
    next_wss_sender: Arc<AtomicUsize>,
    detect_time: chrono::DateTime<Utc>,
    trades_map: Arc<Mutex<HashMap<String, BuyRecord>>>,
    sub_map: Arc<Mutex<HashMap<String, (usize, u64)>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio::sync::oneshot;
    let (creator, mint, curve_pda, holder_addr) = rpc::fetch_transaction_details(signature, settings).await?;
    let (onchain_meta, offchain_meta, onchain_raw) = rpc::fetch_token_metadata(&mint, settings).await?;
    if let Some(m) = &onchain_meta {
        info!("Token {}: Creator={}, Curve={}, Holder={}, URI={}", mint, creator, curve_pda, holder_addr, m.uri.trim_end_matches('\u{0}'));
        if let Some(off) = &offchain_meta {
            info!("Off-chain metadata for {}: name={:?}, symbol={:?}, image={:?}", mint, off.name, off.symbol, off.image);
        }
        if !m.uri.trim_end_matches('\u{0}').is_empty() && m.seller_fee_basis_points < 500 {
            // Enforce max_holded_coins
            if holdings.lock().await.len() >= settings.max_holded_coins {
                info!("Max held coins reached ({}); skipping buy for {}", settings.max_holded_coins, mint);
                return Ok(());
            }

            // Try to get a fast WSS-provided price first depending on price_source.
            // If `price_source` == "rpc" we skip WSS and use RPC only.
            // If `price_source` == "wss" we attempt WSS and do NOT fall back to RPC.
            let price_source = settings.price_source.clone();
            let mut _used_wss = false;
            if price_source != "rpc" && !ws_control_senders.is_empty() {
                // true round-robin selection across WSS senders
                let idx = next_wss_sender.fetch_add(1, Ordering::Relaxed) % ws_control_senders.len();
                let sender = &ws_control_senders[idx];
                let (resp_tx, resp_rx) = oneshot::channel::<Result<u64, String>>();
                // Subscribe to the bonding_curve PDA (streamed state includes virtual reserves)
                let pump_prog = solana_sdk::pubkey::Pubkey::from_str(&settings.pump_fun_program)?;
                let mint_pk = solana_sdk::pubkey::Pubkey::from_str(&mint)?;
                let (curve_pda, _) = solana_sdk::pubkey::Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_prog);
                let subscribe_req = WsRequest::Subscribe { account: curve_pda.to_string(), mint: mint.clone(), resp: resp_tx };
                if let Err(e) = sender.send(subscribe_req).await {
                    log::warn!("Failed to send subscribe request for {}: {}", mint, e);
                } else {
                    // Wait for a subscription id or timeout
                    match tokio::time::timeout(std::time::Duration::from_secs(settings.wss_subscribe_timeout_secs), resp_rx).await {
                        Ok(Ok(Ok(sub_id))) => {
                            _used_wss = true;
                            debug!("Subscribed to {} on sub {}", mint, sub_id);
                            // Attempt an immediate RPC fetch to prime the price cache.
                            // Some WSS providers delay the initial account notification;
                            // fetching the curve once via RPC gives us an initial price
                            // we can act on while the WSS subscription delivers updates.
                            let mut price_opt: Option<f64> = None;
                            match rpc::fetch_current_price(&mint, &price_cache, settings).await {
                                Ok(p) => {
                                    debug!("Primed price cache for {} via RPC: {:.18} SOL/token", mint, p);
                                    price_opt = Some(p);
                                }
                                Err(e) => {
                                    debug!("RPC prime failed for {}: {}. Will wait for WSS initial notification ({}s)", mint, e, settings.wss_subscribe_timeout_secs);
                                }
                            }

                            // If RPC prime didn't succeed, wait briefly for the price to appear in the shared cache
                                            // Track whether we want to keep the WSS subscription
                                            // alive after the function returns. If we buy the token
                                            // we keep the subscription active so the monitor gets
                                            // live updates; otherwise we unsubscribe to free slots.
                                            let mut keep_sub = false;
                                            if price_opt.is_none() {
                                let start = Instant::now();
                                while start.elapsed().as_secs() < settings.wss_subscribe_timeout_secs {
                                    if let Some((_, price)) = price_cache.lock().await.get(&mint).cloned() {
                                        price_opt = Some(price);
                                        break;
                                    }
                                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                                }
                            }
                            if let Some(_price) = price_opt {
                                match rpc::buy_token(
                                    &mint,
                                    settings.buy_amount,
                                    is_real,
                                    keypair,
                                    simulate_keypair.as_ref().map(|a| &**a),
                                    price_cache.clone(),
                                    settings,
                                )
                                .await {
                                    Ok(mut holding) => {
                                        holding.metadata = offchain_meta.clone();
                                        holding.onchain_raw = onchain_raw.clone();
                                        // Build compact onchain_struct as before
                                        let mut onchain_struct: Option<crate::models::OnchainFullMetadata> = None;
                                        if let Some(meta) = onchain_meta.as_ref() {
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
                                        let buy_record = BuyRecord {
                                            mint: mint.clone(),
                                            symbol: offchain_meta.as_ref().and_then(|o| o.symbol.clone()),
                                            name: offchain_meta.as_ref().and_then(|o| o.name.clone()),
                                            uri: offchain_meta.as_ref().and_then(|o| o.image.clone()).or_else(|| onchain_struct.as_ref().and_then(|on| on.uri.clone())),
                                            image: offchain_meta.as_ref().and_then(|o| o.image.clone()),
                                            creator: creator.clone(),
                                            detect_time: detect_time,
                                            buy_time: holding.buy_time,
                                            buy_amount_sol: settings.buy_amount,
                                            buy_amount_tokens: holding.amount,
                                            buy_price: holding.buy_price,
                                        };
                                        trades_map.lock().await.insert(mint.clone(), buy_record);
                                        holdings.lock().await.insert(mint.clone(), holding);
                                        // Keep the subscription active for this mint
                                        keep_sub = true;
                                        // Persist the mapping of mint -> (wss_idx, sub_id)
                                        let mut map = sub_map.lock().await;
                                        map.insert(mint.clone(), (idx, sub_id));
                                    }
                                    Err(e) => log::warn!("Failed to buy {}: {}", mint, e),
                                }
                            } else {
                                // Collect some diagnostic info so operator can manually
                                // inspect the bonding-curve PDA in a Solana explorer.
                                let cached_price = price_cache
                                    .lock()
                                    .await
                                    .get(&mint)
                                    .map(|(_, p)| format!("{:.18}", p))
                                    .unwrap_or_else(|| "none".to_string());

                                log::warn!(
                                    "No WSS price update received for {} within {}s; skipping buy. details: mint={} curve_pda={} holder_addr={} sub_id={} pump_fun_program={} cached_price_sol={}. Paste the curve_pda into a Solana explorer to inspect account data and reserves.",
                                    mint,
                                    settings.wss_subscribe_timeout_secs,
                                    mint,
                                    curve_pda,
                                    holder_addr,
                                    sub_id,
                                    settings.pump_fun_program,
                                    cached_price
                                );
                            }
                            // Unsubscribe to minimize active subscriptions if we did
                            // not keep the subscription (e.g., we skipped buy or buy failed).
                            if !keep_sub {
                                let (u_tx, u_rx) = oneshot::channel::<Result<(), String>>();
                                let _ = sender.send(WsRequest::Unsubscribe { sub_id, resp: u_tx }).await;
                                let _ = tokio::time::timeout(std::time::Duration::from_secs(3), u_rx).await;
                            }
                        }
                        Ok(Ok(Err(err_msg))) => log::warn!("Subscribe request rejected for {}: {}", mint, err_msg),
                        Ok(Err(_)) | Err(_) => log::warn!("Subscribe request timed out or failed for {}", mint),
                    }
                }
            }
            // We do not fall back to RPC here; WSS-only mode requires a streamed
            // price update from the bonding_curve PDA. If no WSS price was used,
            // skip buying.
        }
    }
    Ok(())
}

async fn monitor_holdings(
    holdings: Arc<Mutex<HashMap<String, Holding>>>,
    price_cache: Arc<Mutex<PriceCache>>,
    is_real: bool,
    keypair: Option<&Keypair>,
    simulate_keypair: Option<&Keypair>,
    settings: Arc<Settings>,
    trades_map: Arc<Mutex<HashMap<String, BuyRecord>>>,
    ws_control_senders: Arc<Vec<mpsc::Sender<WsRequest>>>,
    sub_map: Arc<Mutex<HashMap<String, (usize, u64)>>>,
    _next_wss_sender: Arc<AtomicUsize>,
) {
    // Debounce maps to avoid repeated subscribe/prime attempts and noisy warnings
    static SUBSCRIBE_ATTEMPT_TIMES: Lazy<tokio::sync::Mutex<HashMap<String, Instant>>> = Lazy::new(|| tokio::sync::Mutex::new(HashMap::new()));
    const SUBSCRIBE_ATTEMPT_DEBOUNCE_SECS: u64 = 30;
    static PRICE_MISS_WARN_TIMES: Lazy<tokio::sync::Mutex<HashMap<String, Instant>>> = Lazy::new(|| tokio::sync::Mutex::new(HashMap::new()));
    const PRICE_MISS_WARN_DEBOUNCE_SECS: u64 = 60;
    loop {
        sleep(Duration::from_secs(5)).await;
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
                rpc::fetch_current_price(mint, &price_cache, &settings).await
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
                            match rpc::fetch_current_price(mint, &price_cache, &settings).await {
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
                if let Err(e) = rpc::sell_token(
                    mint,
                    holding.amount,
                    current_price,
                    is_real,
                    keypair,
                    simulate_keypair.as_ref().map(|a| &**a),
                    &settings,
                )
                .await
                {
                    error!("Sell error for {}: {}", mint, e);
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
