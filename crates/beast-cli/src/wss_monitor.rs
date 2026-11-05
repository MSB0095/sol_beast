//! WebSocket price monitor for USDT/USDC against Jupiter reference.
//! All backend Rust — no JavaScript.
//!
//! Subscribes to CPMM/AMM v4 vault accounts across two WSS feeds (Helius +
//! PublicNode), decodes live vault balances, subtracts pending CPMM fees, and
//! compares pool prices against the Jupiter reference price. Price deviations
//! above a threshold are reported so the flash-loan arb scanner can price and
//! execute them immediately.

use anyhow::{bail, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use beast_dex::Address;
use beast_exec::WSOL_MINT;
use beast_rpc::RpcClient;
use futures_util::SinkExt as _;
use futures_util::StreamExt as _;
use serde_json::Value;
use solana_sdk::pubkey::Pubkey;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;

/// Raydium CPMM program id.
const CPMM_PROGRAM: &str = "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C";
/// Raydium AMM v4 program id.
const AMM_V4_PROGRAM: &str = "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8";

/// USDT and USDC mints.
const USDT: &str = "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB";
const USDC: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

/// Jupiter quote base URL.
const JUPITER_BASE: &str = "https://lite-api.jup.ag/swap/v1";

/// Deviation threshold (bps) above which a divergence is reported.
const SPREAD_THRESHOLD_BPS: f64 = 10.0;

/// How often Jupiter reference prices are refreshed (ms).
const JUPITER_REFRESH_MS: u64 = 30_000;

/// Status report interval (ms).
const STATUS_INTERVAL_MS: u64 = 5_000;

/// WSOL mint.
const WSOL: &str = WSOL_MINT;

/// One decoded pool with its live state.
#[derive(Debug, Clone)]
struct PoolState {
    address: String,
    venue: String,
    mint_a: String,
    mint_b: String,
    wsol_side: usize,
    vault_a: String,
    vault_b: String,
    mint_other: String,
    pending_fees_a: u64,
    pending_fees_b: u64,
}

/// One live price observation.
#[derive(Debug, Clone, serde::Serialize)]
struct PriceObservation {
    pool: String,
    venue: String,
    token: String,
    price_per_wsol: f64,
    deviation_bps: f64,
    jupiter_ref: f64,
}

/// Shared monitor state.
#[derive(Debug, Default)]
struct MonitorState {
    pools: Mutex<HashMap<String, PoolState>>,
    vault_balances: Mutex<HashMap<String, u64>>,
    jupiter_usdc_per_wsol: Mutex<Option<f64>>,
    jupiter_usdt_per_usdc: Mutex<Option<f64>>,
    update_count: AtomicU64,
    subscription_acks: AtomicU64,
    subscription_errors: AtomicU64,
    divergences: Mutex<Vec<PriceObservation>>,
}

impl MonitorState {
    async fn record_balance(&self, vault: String, balance: u64) {
        self.vault_balances.lock().await.insert(vault, balance);
    }

    async fn insert_pool(&self, pool: PoolState) {
        self.pools.lock().await.insert(pool.address.clone(), pool);
    }

    async fn set_jupiter_prices(&self, usdc_per_wsol: Option<f64>, usdt_per_usdc: Option<f64>) {
        *self.jupiter_usdc_per_wsol.lock().await = usdc_per_wsol;
        *self.jupiter_usdt_per_usdc.lock().await = usdt_per_usdc;
    }

    async fn push_divergence(&self, obs: PriceObservation) {
        let mut div = self.divergences.lock().await;
        div.push(obs);
        div.truncate(200);
    }

    async fn snapshot(&self) -> (usize, usize, Option<f64>, Option<f64>, u64, u64, u64, Vec<PriceObservation>) {
        let pools = self.pools.lock().await.len();
        let vaults = self.vault_balances.lock().await.len();
        let usdc = *self.jupiter_usdc_per_wsol.lock().await;
        let usdt = *self.jupiter_usdt_per_usdc.lock().await;
        let count = self.update_count.load(Ordering::Relaxed);
        let subscription_acks = self.subscription_acks.load(Ordering::Relaxed);
        let subscription_errors = self.subscription_errors.load(Ordering::Relaxed);
        let div = self.divergences.lock().await.clone();
        (pools, vaults, usdc, usdt, count, subscription_acks, subscription_errors, div)
    }
}

/// CPMM pending fees for one side: protocol_fees[341,349], fund_fees[357,365],
/// creator_fees[397,405].
fn cpmm_pending_fees(data: &[u8], side: usize) -> u64 {
    let base = if side == 0 { 341 } else { 349 };
    let p = u64::from_le_bytes(
        data.get(base..base + 8)
            .and_then(|b| b.try_into().ok())
            .unwrap_or([0; 8]),
    );
    let f = u64::from_le_bytes(
        data.get(base + 16..base + 24)
            .and_then(|b| b.try_into().ok())
            .unwrap_or([0; 8]),
    );
    let c = u64::from_le_bytes(
        data.get(base + 56..base + 64)
            .and_then(|b| b.try_into().ok())
            .unwrap_or([0; 8]),
    );
    p.saturating_add(f).saturating_add(c)
}

/// Read a base58-encoded pubkey from account data at offset.
fn read_pubkey(data: &[u8], offset: usize) -> Option<String> {
    let bytes = data.get(offset..offset + 32)?;
    Some(bs58::encode(bytes).into_string())
}

/// Read u64 little-endian from account data at offset.
fn read_u64(data: &[u8], offset: usize) -> Option<u64> {
    let bytes = data.get(offset..offset + 8)?;
    Some(u64::from_le_bytes(bytes.try_into().ok()?))
}

/// CPMM vault offsets: vaults at 72/104, mints at 168/200.
const CPMM_VAULT_A: usize = 72;
const CPMM_VAULT_B: usize = 104;
const CPMM_MINT_A: usize = 168;
const CPMM_MINT_B: usize = 200;

/// AMM v4 vault offsets: vaults at 104/136, mints at 400/432.
const AMM_V4_VAULT_A: usize = 104;
const AMM_V4_VAULT_B: usize = 136;
const AMM_V4_MINT_A: usize = 400;
const AMM_V4_MINT_B: usize = 432;

fn decode_cpmm_pool(data: &[u8]) -> Option<PoolState> {
    let mint_a = read_pubkey(data, CPMM_MINT_A)?;
    let mint_b = read_pubkey(data, CPMM_MINT_B)?;
    let wsol_side = if mint_a == WSOL {
        0
    } else if mint_b == WSOL {
        1
    } else {
        return None;
    };
    let mint_other = if wsol_side == 0 { mint_b.clone() } else { mint_a.clone() };
    if mint_other != USDT && mint_other != USDC {
        return None;
    }
    let vault_a = read_pubkey(data, CPMM_VAULT_A)?;
    let vault_b = read_pubkey(data, CPMM_VAULT_B)?;
    Some(PoolState {
        address: String::new(),
        venue: "raydium_cpmm".to_string(),
        mint_a,
        mint_b,
        wsol_side,
        vault_a,
        vault_b,
        mint_other,
        pending_fees_a: cpmm_pending_fees(data, 0),
        pending_fees_b: cpmm_pending_fees(data, 1),
    })
}

fn decode_amm_v4_pool(data: &[u8]) -> Option<PoolState> {
    let mint_a = read_pubkey(data, AMM_V4_MINT_A)?;
    let mint_b = read_pubkey(data, AMM_V4_MINT_B)?;
    let wsol_side = if mint_a == WSOL {
        0
    } else if mint_b == WSOL {
        1
    } else {
        return None;
    };
    let mint_other = if wsol_side == 0 { mint_b.clone() } else { mint_a.clone() };
    if mint_other != USDT && mint_other != USDC {
        return None;
    }
    let vault_a = read_pubkey(data, AMM_V4_VAULT_A)?;
    let vault_b = read_pubkey(data, AMM_V4_VAULT_B)?;
    Some(PoolState {
        address: String::new(),
        venue: "raydium_amm_v4".to_string(),
        mint_a,
        mint_b,
        wsol_side,
        vault_a,
        vault_b,
        mint_other,
        pending_fees_a: 0,
        pending_fees_b: 0,
    })
}

/// Compute CPMM price: token_per_wsol * 1000 (token has 6 decimals, WSOL has 9).
fn cpmm_price(pool: &PoolState, bal_a: u64, bal_b: u64) -> Option<f64> {
    let eff_a = bal_a.saturating_sub(pool.pending_fees_a);
    let eff_b = bal_b.saturating_sub(pool.pending_fees_b);
    if eff_a == 0 || eff_b == 0 {
        return None;
    }
    let (wsol_res, tok_res) = if pool.wsol_side == 0 { (eff_a, eff_b) } else { (eff_b, eff_a) };
    Some(tok_res as f64 / wsol_res as f64 * 1000.0)
}

/// Jupiter reference price fetch via HTTP.
async fn fetch_jupiter_prices(http: &reqwest::Client) -> (Option<f64>, Option<f64>) {
    let usdc_per_wsol = fetch_one(http, WSOL, USDC, 1_000_000_000)
        .await
        .ok()
        .map(|out| out / 1e6);
    let usdt_per_usdc = fetch_one(http, USDC, USDT, 1_000_000)
        .await
        .ok()
        .map(|out| out / 1e6);
    (usdc_per_wsol, usdt_per_usdc)
}

async fn fetch_one(
    http: &reqwest::Client,
    input_mint: &str,
    output_mint: &str,
    amount: u64,
) -> Result<f64> {
    let url = format!(
        "{JUPITER_BASE}/quote?inputMint={input_mint}&outputMint={output_mint}&amount={amount}&slippageBps=50"
    );
    let resp: Value = http
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let out = resp
        .get("outAmount")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .ok_or_else(|| anyhow::anyhow!("no outAmount"))?;
    Ok(out)
}

/// One WSS feed connection.
struct WsFeed {
    name: String,
    url: String,
    ws: Mutex<Option<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>>>,
    subscribed: Mutex<HashSet<String>>,
    log_subscribed: Mutex<HashSet<String>>,
    next_request_id: AtomicU64,
    state: Arc<MonitorState>,
}

impl WsFeed {
    fn new(name: &str, url: String, state: Arc<MonitorState>) -> Self {
        Self {
            name: name.to_string(),
            url,
            ws: Mutex::new(None),
            subscribed: Mutex::new(HashSet::new()),
            log_subscribed: Mutex::new(HashSet::new()),
            next_request_id: AtomicU64::new(1),
            state,
        }
    }

    async fn connect(&self) -> Result<()> {
        let (ws_stream, _) = connect_async(&self.url).await?;
        *self.ws.lock().await = Some(ws_stream);
        tracing::info!(feed = %self.name, "WSS connected");
        self.resubscribe().await;
        Ok(())
    }

    fn next_request_id(&self) -> u64 {
        self.next_request_id.fetch_add(1, Ordering::Relaxed)
    }

    async fn send_subscription(&self, address: &str, kind: &'static str) -> Result<u64> {
        let request_id = self.next_request_id();
        let method = if kind == "account" {
            "accountSubscribe"
        } else {
            "logsSubscribe"
        };
        let params = if kind == "account" {
            serde_json::json!([address, {"encoding": "base64", "commitment": "confirmed"}])
        } else {
            serde_json::json!([{"mentions": [address]}, {"commitment": "confirmed"}])
        };
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method,
            "params": params,
        });
        let mut ws_guard = self.ws.lock().await;
        let Some(ws) = ws_guard.as_mut() else {
            anyhow::bail!("websocket is not connected");
        };
        ws.send(Message::Text(msg.to_string())).await?;
        Ok(request_id)
    }

    async fn resubscribe(&self) {
        let subscribed = self.subscribed.lock().await.clone();
        let logged = self.log_subscribed.lock().await.clone();
        for address in subscribed {
            if let Err(error) = self.send_subscription(&address, "account").await {
                tracing::error!(feed = %self.name, %address, %error, "account resubscribe failed");
            }
        }
        for address in logged {
            if let Err(error) = self.send_subscription(&address, "logs").await {
                tracing::error!(feed = %self.name, %address, %error, "logs resubscribe failed");
            }
        }
    }

    async fn subscribe(&self, addresses: &[String]) {
        let mut set = self.subscribed.lock().await;
        let mut new_addresses = Vec::new();
        for address in addresses {
            if set.insert(address.clone()) {
                new_addresses.push(address.clone());
            }
        }
        drop(set);
        for address in &new_addresses {
            if let Err(error) = self.send_subscription(address, "account").await {
                tracing::error!(feed = %self.name, %address, %error, "account subscription failed");
            }
        }
        tracing::info!(feed = %self.name, count = new_addresses.len(), "accountSubscribe requests sent");
    }

    async fn subscribe_logs(&self, addresses: &[String]) {
        let mut set = self.log_subscribed.lock().await;
        let mut new_addresses = Vec::new();
        for address in addresses {
            if set.insert(address.clone()) {
                new_addresses.push(address.clone());
            }
        }
        drop(set);
        for address in &new_addresses {
            if let Err(error) = self.send_subscription(address, "logs").await {
                tracing::error!(feed = %self.name, %address, %error, "logs subscription failed");
            }
        }
        tracing::info!(feed = %self.name, count = new_addresses.len(), "logsSubscribe requests sent");
    }

    async fn run(self) -> Result<()> {
        loop {
            if let Err(e) = self.connect().await {
                tracing::error!(feed = %self.name, error = %e, "connect failed, retrying in 2s");
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
            {
                let mut ws_guard = self.ws.lock().await;
                let Some(ws) = ws_guard.as_mut() else {
                    continue;
                };
                while let Some(msg_result) = ws.next().await {
                    match msg_result {
                        Ok(Message::Text(text)) => {
                            if let Err(e) = self.handle_message(&text).await {
                                tracing::error!(feed = %self.name, error = %e, "message handling error");
                            }
                        }
                        Ok(Message::Close(..)) => {
                            tracing::warn!(feed = %self.name, "WSS closed");
                            break;
                        }
                        Ok(_) => {}
                        Err(e) => {
                            tracing::error!(feed = %self.name, error = %e, "WSS error");
                            break;
                        }
                    }
                }
            }
            *self.ws.lock().await = None;
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    async fn handle_message(&self, text: &str) -> Result<()> {
        let msg: Value = serde_json::from_str(text)?;
        if msg.get("method").and_then(|v| v.as_str()).is_none() {
            if let Some(error) = msg.get("error") {
                self.state.subscription_errors.fetch_add(1, Ordering::Relaxed);
                tracing::error!(feed = %self.name, id = %msg.get("id").map(|v| v.to_string()).unwrap_or_else(|| "unknown".into()), error = %error, "subscription rejected");
            } else if msg.get("result").is_some() {
                self.state.subscription_acks.fetch_add(1, Ordering::Relaxed);
                tracing::debug!(feed = %self.name, id = %msg.get("id").map(|v| v.to_string()).unwrap_or_else(|| "unknown".into()), "subscription acknowledged");
            } else {
                tracing::debug!(feed = %self.name, "unrecognized websocket message");
            }
            return Ok(());
        }
        if let Some(method) = msg.get("method").and_then(|v| v.as_str()) {
            match method {
                "accountNotification" => {
                    let params = msg.get("params").and_then(|v| v.get("result")).and_then(|v| v.get("value"));
                    if let Some(value) = params {
                        let address = value.get("address").and_then(|v| v.as_str()).unwrap_or("");
                        let data = value.get("data").and_then(|v| v.get(0)).and_then(|v| v.as_str()).unwrap_or("");
                        let raw = STANDARD.decode(data).unwrap_or_default();
                        let balance = read_u64(&raw, 64).unwrap_or(0);
                        self.state.record_balance(address.to_string(), balance).await;
                        self.state.update_count.fetch_add(1, Ordering::Relaxed);
                        self.check_divergences();
                    }
                }
                "logsNotification" => {
                    self.check_divergences();
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn check_divergences(&self) {
        let state = self.state.clone();
        tokio::spawn(async move {
            if let Err(e) = compute_divergences(&state).await {
                tracing::debug!(error = %e, "divergence compute skipped");
            }
        });
    }
}

/// Compute price deviations vs Jupiter and record them.
async fn compute_divergences(state: &MonitorState) -> Result<()> {
    let (pools, _vaults, usdc_ref, usdt_ref, _updates, _acks, _errors, _div) = state.snapshot().await;
    let usdc_ref = match usdc_ref {
        Some(v) if v > 0.0 => v,
        _ => return Ok(()),
    };
    let usdt_ref = match usdt_ref {
        Some(v) if v > 0.0 => v,
        _ => return Ok(()),
    };
    let usdt_ref_per_wsol = usdc_ref * usdt_ref;

    let vault_balances = state.vault_balances.lock().await;
    let pools = state.pools.lock().await;

    for pool in pools.values() {
        let bal_a = vault_balances.get(&pool.vault_a).copied().unwrap_or(0);
        let bal_b = vault_balances.get(&pool.vault_b).copied().unwrap_or(0);
        let price = match pool.venue.as_str() {
            "raydium_cpmm" => cpmm_price(pool, bal_a, bal_b),
            "raydium_amm_v4" => {
                if bal_a == 0 || bal_b == 0 {
                    None
                } else {
                    let (wsol_res, tok_res) = if pool.wsol_side == 0 { (bal_a, bal_b) } else { (bal_b, bal_a) };
                    Some(tok_res as f64 / wsol_res as f64 * 1000.0)
                }
            }
            _ => None,
        };
        let Some(price) = price else { continue };
        let jupiter_ref = if pool.mint_other == USDT { usdt_ref_per_wsol } else { usdc_ref };
        let deviation_bps = (price - jupiter_ref).abs() / jupiter_ref * 10_000.0;
        if deviation_bps >= SPREAD_THRESHOLD_BPS {
            let obs = PriceObservation {
                pool: pool.address.clone(),
                venue: pool.venue.clone(),
                token: pool.mint_other.clone(),
                price_per_wsol: price,
                deviation_bps,
                jupiter_ref,
            };
            state.push_divergence(obs).await;
            tracing::warn!(
                token = %pool.mint_other,
                pool = %pool.address,
                venue = %pool.venue,
                price = %price,
                jupiter_ref = %jupiter_ref,
                deviation_bps = %deviation_bps,
                "PRICE DIVERGENCE"
            );
        }
    }
    Ok(())
}

/// Discover USDT/USDC CPMM/AMM v4 pool addresses via HTTP, then seed the monitor.
async fn seed_pools(rpc: &RpcClient, http: &reqwest::Client, state: Arc<MonitorState>) -> Result<u64> {
    let mut pool_addrs: Vec<String> = Vec::new();

    // Try loading from the previously discovered pairs file.
    if let Ok(text) = std::fs::read_to_string("/tmp/opencode/wsol_pairs.json") {
        if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&text) {
            for entry in arr {
                let mint = entry.get("mint").and_then(|v| v.as_str()).unwrap_or("");
                if mint != USDT && mint != USDC {
                    continue;
                }
                let pools = entry
                    .get("pools")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                for p in pools {
                    let addr = p.get("pool").and_then(|v| v.as_str()).unwrap_or("");
                    let venue = p.get("venue").and_then(|v| v.as_str()).unwrap_or("");
                    if (venue == "raydium_cpmm" || venue == "raydium_amm_v4") && !addr.is_empty() {
                        pool_addrs.push(addr.to_string());
                    }
                }
            }
        }
    }

    // Fall back to on-chain discovery if file not available.
    if pool_addrs.is_empty() {
        tracing::info!("wsol_pairs.json not available; discovering pools on-chain");
        for program in [CPMM_PROGRAM, AMM_V4_PROGRAM] {
            for mint in [USDT, USDC] {
                let program_address = Address::from_str(program)?;
                let mint_key = Pubkey::from_str(mint)?;
                let wsol_key = Pubkey::from_str(WSOL)?;
                for (first, second) in [(wsol_key, mint_key), (mint_key, wsol_key)] {
                    let filters = vec![
                        serde_json::json!({"memcmp": {"offset": if program == CPMM_PROGRAM {168} else {400}, "bytes": first.to_string()}}),
                        serde_json::json!({"memcmp": {"offset": if program == CPMM_PROGRAM {200} else {432}, "bytes": second.to_string()}}),
                    ];
                    let accounts = rpc.get_program_accounts(program_address, filters, None).await?;
                    for (addr, _account) in accounts {
                        pool_addrs.push(addr.to_string());
                    }
                }
            }
        }
    }

    // De-duplicate.
    let mut seen = HashSet::new();
    pool_addrs.retain(|a| seen.insert(a.clone()));

    tracing::info!(count = pool_addrs.len(), "discovered USDT/USDC CPMM/AMM v4 pools");

    // Fetch pool accounts to decode vaults and mints.
    let chunk_size = 100;
    for chunk in pool_addrs.chunks(chunk_size) {
        let keys: Vec<Address> = chunk
            .iter()
            .filter_map(|a| Address::from_str(a).ok())
            .collect();
        if keys.is_empty() {
            continue;
        }
        let accounts = rpc.get_multiple_accounts(&keys).await?;
        for (addr, account_opt) in chunk.iter().zip(accounts.iter()) {
            let Some(account) = account_opt else { continue };
            let data = &account.data;
            let owner = &account.owner.to_string();
            let decoded = if owner == CPMM_PROGRAM {
                decode_cpmm_pool(data)
            } else if owner == AMM_V4_PROGRAM {
                decode_amm_v4_pool(data)
            } else {
                None
            };
            let mut pool = match decoded {
                Some(p) => p,
                None => continue,
            };
            pool.address = addr.clone();
            state.insert_pool(pool).await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }

    let pool_count = state.pools.lock().await.len() as u64;
    tracing::info!(count = pool_count, "pools decoded and seeded");

    // Read initial vault balances.
    let vaults: Vec<String> = state
        .pools
        .lock()
        .await
        .values()
        .flat_map(|p| [p.vault_a.clone(), p.vault_b.clone()])
        .collect();
    for chunk in vaults.chunks(chunk_size) {
        let keys: Vec<Address> = chunk
            .iter()
            .filter_map(|a| Address::from_str(a).ok())
            .collect();
        if keys.is_empty() {
            continue;
        }
        let accounts = rpc.get_multiple_accounts(&keys).await?;
        for (addr, account_opt) in chunk.iter().zip(accounts.iter()) {
            if let Some(account) = account_opt {
                let data = &account.data;
                if let Some(balance) = read_u64(data, 64) {
                    state.record_balance(addr.clone(), balance).await;
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }

    let balance_count = state.vault_balances.lock().await.len();
    tracing::info!(count = balance_count, "initial vault balances read");

    Ok(pool_count)
}

/// Split vault addresses across the two WSS feeds while honoring their
/// documented subscription limits. PublicNode allows only one concurrent
/// subscription, so it is pinned to the first vault; Helius carries the rest.
const PUBLICNODE_MAX_SUBSCRIPTIONS: usize = 1;

async fn subscribe_vaults(feed1: &WsFeed, feed2: &WsFeed, state: &MonitorState) {
    let vaults: Vec<String> = state
        .pools
        .lock()
        .await
        .values()
        .flat_map(|p| [p.vault_a.clone(), p.vault_b.clone()])
        .collect();
    let feed2_count = vaults.len().min(PUBLICNODE_MAX_SUBSCRIPTIONS);
    let feed1_count = vaults.len().saturating_sub(feed2_count);
    feed1.subscribe(&vaults[..feed1_count]).await;
    feed2.subscribe(&vaults[feed1_count..]).await;

    let active: Vec<String> = state
        .pools
        .lock()
        .await
        .values()
        .take(15)
        .map(|p| p.address.clone())
        .collect();
    feed1.subscribe_logs(&active).await;

    tracing::info!(
        feed1_count,
        feed2_count,
        log_count = active.len(),
        publicnode_limit = PUBLICNODE_MAX_SUBSCRIPTIONS,
        "vault subscriptions split across feeds"
    );
}

/// Refresh Jupiter prices periodically and on trade activity.
async fn jupiter_refresh_loop(state: Arc<MonitorState>, http: reqwest::Client) {
    loop {
        let (usdc_per_wsol, usdt_per_usdc) = fetch_jupiter_prices(&http).await;
        state.set_jupiter_prices(usdc_per_wsol, usdt_per_usdc).await;
        tracing::info!(
            usdc_per_wsol = usdc_per_wsol.map(|v| format!("{v:.6}")).unwrap_or_default(),
            usdt_per_usdc = usdt_per_usdc.map(|v| format!("{v:.6}")).unwrap_or_default(),
            "Jupiter reference prices refreshed"
        );
        tokio::time::sleep(std::time::Duration::from_millis(JUPITER_REFRESH_MS)).await;
    }
}

/// Periodic status report.
async fn status_loop(state: Arc<MonitorState>) {
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(STATUS_INTERVAL_MS)).await;
        let (pools, vaults, usdc, usdt, updates, subscription_acks, subscription_errors, div) = state.snapshot().await;
        let usdc_s = usdc.map(|v| format!("{v:.6}")).unwrap_or("-".into());
        let usdt_s = usdt.map(|v| format!("{v:.6}")).unwrap_or("-".into());
        tracing::info!(
            pools, vaults, updates, subscription_acks, subscription_errors,
            usdc_ref = %usdc_s, usdt_ref = %usdt_s, recent_divergences = div.len(),
            "WSS monitor status"
        );
    }
}

/// Run the WSS price monitor continuously.
pub async fn run_wss_monitor(
    helius_ws: String,
    publicnode_ws: String,
    _helius_http: String,
    rpc: Arc<RpcClient>,
) -> Result<()> {
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let state = Arc::new(MonitorState::default());
    let pool_count = seed_pools(rpc.as_ref(), &http, state.clone()).await?;
    if pool_count == 0 {
        bail!("no USDT/USDC CPMM/AMM v4 pools discovered; monitor cannot run");
    }

    let (usdc_per_wsol, usdt_per_usdc) = fetch_jupiter_prices(&http).await;
    state.set_jupiter_prices(usdc_per_wsol, usdt_per_usdc).await;

    let state_clone = state.clone();
    let http_clone = http.clone();
    tokio::spawn(async move {
        jupiter_refresh_loop(state_clone, http_clone).await;
    });

    let state_clone = state.clone();
    tokio::spawn(async move {
        status_loop(state_clone).await;
    });

    let feed1 = WsFeed::new("helius", helius_ws, state.clone());
    let feed2 = WsFeed::new("publicnode", publicnode_ws, state.clone());

    subscribe_vaults(&feed1, &feed2, &state).await;

    let f1 = feed1.run();
    let f2 = feed2.run();
    tokio::select! {
        res = f1 => { res?; }
        res = f2 => { res?; }
    }
    Ok(())
}
