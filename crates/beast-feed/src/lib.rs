//! Pool refresh feed.
//!
//! The feed is intentionally pull-based at this stage. It can refresh a
//! watchlist of pool accounts, decode the on-chain state with `beast-dex`, and
//! expose immutable snapshots to the Radar. A websocket/gRPC feed can be added
//! behind the same interface once provider credentials are available.
//!
//! Safety rules:
//! * a failed refresh never fabricates reserves;
//! * the last good snapshot is retained but marked stale;
//! * only locally modelled pools ever reach the router.

use beast_core::config::PoolConfig;
use beast_dex::catalog::{self, DecoderKind};
use beast_dex::decode::{decode_venue, DecodeAux, DecodeContext};
use beast_dex::model::PoolSnapshot;
use beast_dex::Address;
use beast_rpc::{mint_decimals, token_account_amount, RpcClient};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

pub type Result<T> = std::result::Result<T, FeedError>;

#[derive(Debug, Error)]
pub enum FeedError {
    #[error("rpc: {0}")]
    Rpc(#[from] beast_rpc::RpcError),
    #[error("decode: {0}")]
    Decode(#[from] beast_dex::DexError),
    #[error("unknown venue: {0}")]
    UnknownVenue(String),
    #[error("pool account not found: {0}")]
    NotFound(Address),
    #[error("invalid pool config: {0}")]
    Config(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchedPool {
    pub address: Address,
    pub venue: String,
    pub decimals: Option<(u8, u8)>,
    pub fee_override_bps: Option<u32>,
    pub amm_config: Option<Address>,
}

impl WatchedPool {
    pub fn from_config(pool: &PoolConfig) -> Result<Self> {
        let address = pool.address.parse::<Address>().map_err(|error| {
            FeedError::Config(format!("pool address {}: {error}", pool.address))
        })?;
        let venue = catalog::venue(&pool.venue)
            .ok_or_else(|| FeedError::UnknownVenue(pool.venue.clone()))?;
        if venue.decoder.is_none() {
            return Err(FeedError::Config(format!(
                "venue {} is quote-only and has no local account decoder",
                pool.venue
            )));
        }
        let decimals = match (pool.decimals_a, pool.decimals_b) {
            (Some(a), Some(b)) => Some((a, b)),
            _ => None,
        };
        Ok(Self {
            address,
            venue: pool.venue.clone(),
            decimals,
            fee_override_bps: pool.fee_bps.map(|value| value.min(u32::MAX as u64) as u32),
            amm_config: None,
        })
    }

    pub fn from_parts(address: Address, venue: impl Into<String>) -> Self {
        Self {
            address,
            venue: venue.into(),
            decimals: None,
            fee_override_bps: None,
            amm_config: None,
        }
    }

    pub fn with_decimals(mut self, a: u8, b: u8) -> Self {
        self.decimals = Some((a, b));
        self
    }

    pub fn with_fee_override(mut self, bps: u32) -> Self {
        self.fee_override_bps = Some(bps);
        self
    }

    pub fn with_amm_config(mut self, config: Address) -> Self {
        self.amm_config = Some(config);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedConfig {
    pub interval_ms: u64,
    pub max_concurrent: usize,
    pub stale_after_ms: u64,
    /// Refresh token-account reserves for pools that do not store them.
    pub refresh_reserves: bool,
}

impl Default for FeedConfig {
    fn default() -> Self {
        Self {
            interval_ms: 1_500,
            max_concurrent: 8,
            stale_after_ms: 6_000,
            refresh_reserves: true,
        }
    }
}

impl FeedConfig {
    pub fn validate(&self) -> Result<()> {
        if self.interval_ms < 250 {
            return Err(FeedError::Config("feed.interval_ms must be >= 250".into()));
        }
        if self.max_concurrent == 0 || self.max_concurrent > 64 {
            return Err(FeedError::Config(
                "feed.max_concurrent must be 1..=64".into(),
            ));
        }
        if self.stale_after_ms < self.interval_ms {
            return Err(FeedError::Config(
                "feed.stale_after_ms must be >= feed.interval_ms".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedHealth {
    pub pools: usize,
    pub quotable: usize,
    pub stale: usize,
    pub last_refresh_ms: Option<i64>,
    pub age_ms: Option<u64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeedReport {
    pub attempted: usize,
    pub updated: usize,
    pub failed: usize,
    pub duration_ms: u64,
}

#[derive(Debug, Clone)]
struct PoolState {
    snapshot: Option<PoolSnapshot>,
    updated_at_ms: i64,
    last_error: Option<String>,
    consecutive_failures: u32,
}

pub struct PoolFeed {
    rpc: Arc<RpcClient>,
    watchlist: RwLock<Vec<WatchedPool>>,
    states: RwLock<HashMap<Address, PoolState>>,
    config: FeedConfig,
    last_refresh_ms: AtomicI64,
    last_error: RwLock<Option<String>>,
}

impl PoolFeed {
    pub fn new(
        rpc: Arc<RpcClient>,
        watchlist: Vec<WatchedPool>,
        config: FeedConfig,
    ) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            rpc,
            watchlist: RwLock::new(watchlist),
            states: RwLock::new(HashMap::new()),
            config,
            last_refresh_ms: AtomicI64::new(0),
            last_error: RwLock::new(None),
        })
    }

    pub fn config(&self) -> &FeedConfig {
        &self.config
    }

    pub async fn watchlist(&self) -> Vec<WatchedPool> {
        self.watchlist.read().await.clone()
    }

    pub async fn add(&self, pool: WatchedPool) {
        let mut watchlist = self.watchlist.write().await;
        if !watchlist.iter().any(|item| item.address == pool.address) {
            watchlist.push(pool);
        }
    }

    pub async fn remove(&self, address: Address) -> bool {
        let mut watchlist = self.watchlist.write().await;
        let before = watchlist.len();
        watchlist.retain(|item| item.address != address);
        self.states.write().await.remove(&address);
        watchlist.len() != before
    }

    pub async fn refresh_all(&self) -> FeedReport {
        let report = self.refresh_all_batched().await;
        tracing::debug!(?report, "pool feed refresh");
        report
    }

    /// Refresh the whole watchlist in three batched RPC calls.
    ///
    /// The unbatched path costs 2-3 calls *per pool* per tick. A 10-pool
    /// watchlist therefore burned 20-30 calls every interval, which exhausts
    /// a free-tier monthly quota in about a week. Batching collapses one tick
    /// to three `getMultipleAccounts` calls regardless of watchlist size.
    ///
    /// Pools that fail to decode are reported individually, so a single bad
    /// account never hides the rest of the feed.
    pub async fn refresh_all_batched(&self) -> FeedReport {
        let started = Instant::now();
        let watchlist = self.watchlist().await;
        let attempted = watchlist.len();
        if attempted == 0 {
            self.last_refresh_ms
                .store(chrono::Utc::now().timestamp_millis(), Ordering::Relaxed);
            return FeedReport::default();
        }

        // Stage 1: one RPC call for every pool account in the watchlist.
        let pool_addresses: Vec<Address> = watchlist.iter().map(|p| p.address).collect();
        let pool_accounts = match self.rpc.get_multiple_accounts(&pool_addresses).await {
            Ok(accounts) => accounts,
            Err(error) => {
                tracing::warn!(error = %error, "batched pool fetch failed");
                for watched in &watchlist {
                    self.commit_err(watched, &FeedError::Rpc(error.clone()))
                        .await;
                }
                *self.last_error.write().await = Some(error.to_string());
                return FeedReport {
                    attempted,
                    updated: 0,
                    failed: attempted,
                    duration_ms: started.elapsed().as_millis() as u64,
                };
            }
        };
        let mut first_pass: Vec<(&WatchedPool, Result<PendingPool>)> =
            Vec::with_capacity(attempted);
        for (watched, account) in watchlist.iter().zip(pool_accounts) {
            let venue = match pool_venue(watched) {
                Ok(venue) => venue,
                Err(error) => {
                    first_pass.push((watched, Err(error)));
                    continue;
                }
            };
            let result = account
                .map(|account| beast_rpc::AccountInfo { slot: 0, account })
                .map(|info| begin_decode(watched, venue, info, self.config.refresh_reserves))
                .unwrap_or(Err(FeedError::NotFound(watched.address)));
            first_pass.push((watched, result));
        }

        // Stage 2: one RPC call for every auxiliary account still needed.
        let mut aux_addresses: Vec<Address> = Vec::new();
        for (_, pending) in &first_pass {
            if let Ok(pending) = pending {
                for address in pending.aux_addresses() {
                    if !aux_addresses.contains(&address) {
                        aux_addresses.push(address);
                    }
                }
            }
        }
        let aux_accounts = if aux_addresses.is_empty() {
            Vec::new()
        } else {
            match self.rpc.get_multiple_accounts(&aux_addresses).await {
                Ok(accounts) => accounts,
                Err(error) => {
                    tracing::warn!(error = %error, "batched aux fetch failed");
                    for watched in &watchlist {
                        self.commit_err(watched, &FeedError::Rpc(error.clone()))
                            .await;
                    }
                    *self.last_error.write().await = Some(error.to_string());
                    return FeedReport {
                        attempted,
                        updated: 0,
                        failed: attempted,
                        duration_ms: started.elapsed().as_millis() as u64,
                    };
                }
            }
        };
        let aux_map: HashMap<Address, &beast_rpc::Account> = aux_addresses
            .iter()
            .copied()
            .zip(aux_accounts.iter())
            .filter_map(|(address, account)| account.as_ref().map(|account| (address, account)))
            .collect();

        let mut updated = 0usize;
        let mut failed = 0usize;
        for (watched, pending) in first_pass {
            let outcome = pending.and_then(|pending| {
                let venue = pool_venue(&pending.watched)?;
                finish_decode(venue, pending, &aux_map)
            });
            match outcome {
                Ok(snapshot) => {
                    self.commit_ok(watched, snapshot).await;
                    updated += 1;
                }
                Err(error) => {
                    self.commit_err(watched, &error).await;
                    failed += 1;
                }
            }
        }

        if failed > 0 {
            *self.last_error.write().await =
                Some(format!("{failed}/{attempted} pool refreshes failed"));
        } else {
            *self.last_error.write().await = None;
        }
        self.last_refresh_ms
            .store(chrono::Utc::now().timestamp_millis(), Ordering::Relaxed);
        FeedReport {
            attempted,
            updated,
            failed,
            duration_ms: started.elapsed().as_millis() as u64,
        }
    }

    async fn commit_ok(&self, watched: &WatchedPool, snapshot: PoolSnapshot) {
        let mut states = self.states.write().await;
        states.insert(
            watched.address,
            PoolState {
                snapshot: Some(snapshot),
                updated_at_ms: chrono::Utc::now().timestamp_millis(),
                last_error: None,
                consecutive_failures: 0,
            },
        );
    }

    async fn commit_err(&self, watched: &WatchedPool, error: &FeedError) {
        tracing::warn!(pool = %watched.address, venue = %watched.venue, error = %error, "pool refresh failed");
        let mut states = self.states.write().await;
        let entry = states.entry(watched.address).or_insert_with(|| PoolState {
            snapshot: None,
            updated_at_ms: 0,
            last_error: None,
            consecutive_failures: 0,
        });
        entry.last_error = Some(error.to_string());
        entry.consecutive_failures = entry.consecutive_failures.saturating_add(1);
    }

    pub async fn snapshots(&self) -> Vec<PoolSnapshot> {
        let states = self.states.read().await;
        states
            .values()
            .filter_map(|state| state.snapshot.clone())
            .collect()
    }

    pub async fn fresh_snapshots(&self, max_age_ms: u64) -> Vec<PoolSnapshot> {
        let now = chrono::Utc::now().timestamp_millis();
        let states = self.states.read().await;
        states
            .values()
            .filter(|state| now.saturating_sub(state.updated_at_ms) <= max_age_ms as i64)
            .filter_map(|state| state.snapshot.clone())
            .collect()
    }

    pub async fn snapshot(&self, address: Address) -> Option<PoolSnapshot> {
        self.states
            .read()
            .await
            .get(&address)
            .and_then(|state| state.snapshot.clone())
    }

    pub async fn health(&self) -> FeedHealth {
        let now = chrono::Utc::now().timestamp_millis();
        let states = self.states.read().await;
        let pools = states.len();
        let quotable = states
            .values()
            .filter(|state| {
                state
                    .snapshot
                    .as_ref()
                    .map(PoolSnapshot::is_locally_quotable)
                    .unwrap_or(false)
            })
            .count();
        let stale = states
            .values()
            .filter(|state| {
                now.saturating_sub(state.updated_at_ms) > self.config.stale_after_ms as i64
            })
            .count();
        let last = self.last_refresh_ms.load(Ordering::Relaxed);
        FeedHealth {
            pools,
            quotable,
            stale,
            last_refresh_ms: (last > 0).then_some(last),
            age_ms: (last > 0).then_some(now.saturating_sub(last).max(0) as u64),
            last_error: self.last_error.read().await.clone(),
        }
    }

    /// Refresh every `interval_ms` until cancelled.
    pub async fn run(&self, shutdown: CancellationToken) {
        let mut interval = tokio::time::interval(Duration::from_millis(self.config.interval_ms));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    tracing::info!("pool feed cancelled");
                    break;
                }
                _ = interval.tick() => {
                    let report = self.refresh_all().await;
                    if report.attempted > 0 {
                        tracing::debug!(?report, "pool feed refresh");
                    }
                }
            }
        }
    }
}

/// Refresh a single pool account and decode it.
/// Stage-1 result: the pool decoded without auxiliary accounts, plus a
/// description of the extra accounts needed to finish the decode.
struct PendingPool {
    watched: WatchedPool,
    data: Vec<u8>,
    slot: u64,
    decimals: Option<(u8, u8)>,
    fee_bps: Option<u32>,
    /// `amm_config` account address (Raydium family decoders only).
    config: Option<Address>,
    /// Vault accounts, needed only by decoders that store reserves elsewhere.
    vault_a: Option<Address>,
    vault_b: Option<Address>,
    /// Mint accounts, needed only when decimals are unknown.
    mint_a: Option<Address>,
    mint_b: Option<Address>,
}

impl PendingPool {
    fn aux_addresses(&self) -> Vec<Address> {
        let mut addresses = Vec::with_capacity(5);
        for address in [
            self.config,
            self.vault_a,
            self.vault_b,
            self.mint_a,
            self.mint_b,
        ]
        .into_iter()
        .flatten()
        {
            if !addresses.contains(&address) {
                addresses.push(address);
            }
        }
        addresses
    }
}

pub async fn refresh_pool(
    rpc: &RpcClient,
    watched: &WatchedPool,
    refresh_reserves: bool,
) -> Result<PoolSnapshot> {
    let venue = pool_venue(watched)?;
    let info = rpc
        .get_account_info(watched.address)
        .await?
        .ok_or(FeedError::NotFound(watched.address))?;
    let pending = begin_decode(&watched, venue, info, refresh_reserves)?;
    let aux_addresses = pending.aux_addresses();
    let accounts = if aux_addresses.is_empty() {
        Vec::new()
    } else {
        rpc.get_multiple_accounts(&aux_addresses).await?
    };
    let aux_map: HashMap<Address, &beast_rpc::Account> = aux_addresses
        .iter()
        .copied()
        .zip(accounts.iter())
        .filter_map(|(address, account)| account.as_ref().map(|account| (address, account)))
        .collect();
    finish_decode(venue, pending, &aux_map)
}

fn pool_venue(watched: &WatchedPool) -> Result<&'static beast_dex::catalog::Venue> {
    let venue = catalog::venue(&watched.venue)
        .ok_or_else(|| FeedError::UnknownVenue(watched.venue.clone()))?;
    if venue.decoder.is_none() {
        return Err(FeedError::Config(format!(
            "venue {} is quote-only and has no local account decoder",
            watched.venue
        )));
    }
    Ok(venue)
}

/// Which decoders keep their vault balances in separate accounts rather than
/// in the pool state itself.
fn decoder_needs_reserves(decoder: Option<DecoderKind>) -> bool {
    matches!(
        decoder,
        Some(
            DecoderKind::RaydiumAmmV4
                | DecoderKind::PumpSwap
                | DecoderKind::SplTokenSwap
                | DecoderKind::MeteoraDammV1
                | DecoderKind::MeteoraDammV2
                | DecoderKind::MeteoraDlmm
        )
    )
}

/// Which decoders cannot derive token decimals from the pool state alone.
fn decoder_needs_decimals(decoder: Option<DecoderKind>) -> bool {
    matches!(
        decoder,
        Some(
            DecoderKind::PumpSwap
                | DecoderKind::OrcaWhirlpool
                | DecoderKind::SplTokenSwap
                | DecoderKind::SaberStable
                | DecoderKind::MeteoraDammV1
                | DecoderKind::MeteoraDammV2
                | DecoderKind::MeteoraDlmm
        )
    )
}

/// Stage 1: validate the pool account and learn which aux accounts exist.
///
/// Decoding is deliberate: `decode_venue` is called with no aux data so the
/// pool's own mint/vault addresses can be read out of the state. Those
/// addresses are then fetched in one batched call for the whole watchlist.
fn begin_decode(
    watched: &WatchedPool,
    venue: &beast_dex::catalog::Venue,
    info: beast_rpc::AccountInfo,
    refresh_reserves: bool,
) -> Result<PendingPool> {
    if info.account.owner.to_string() != venue.program_id {
        return Err(FeedError::Config(format!(
            "pool {} owner {} does not match catalogue program {}",
            watched.address, info.account.owner, venue.program_id
        )));
    }
    let data = info.account.data;
    let probe_aux = DecodeAux {
        amm_config: None,
        decimals: watched.decimals,
        fee_override_bps: watched.fee_override_bps,
    };
    let probed = decode_venue(
        venue,
        &data,
        &DecodeContext::new(watched.address, info.slot),
        &probe_aux,
    )?;

    let needs_reserves = refresh_reserves && decoder_needs_reserves(venue.decoder);
    let needs_decimals = watched.decimals.is_none() && decoder_needs_decimals(venue.decoder);

    let config = watched
        .amm_config
        .or_else(|| auxiliary_config_address(venue.decoder, &data));
    Ok(PendingPool {
        watched: watched.clone(),
        data,
        slot: info.slot,
        decimals: watched.decimals,
        fee_bps: watched.fee_override_bps,
        config,
        vault_a: needs_reserves.then_some(probed.vault_a),
        vault_b: needs_reserves.then_some(probed.vault_b),
        mint_a: needs_decimals.then_some(probed.mint_a),
        mint_b: needs_decimals.then_some(probed.mint_b),
    })
}

/// Stage 2: decode again, now with the batched aux accounts resolved.
fn finish_decode(
    venue: &beast_dex::catalog::Venue,
    pending: PendingPool,
    aux_map: &HashMap<Address, &beast_rpc::Account>,
) -> Result<PoolSnapshot> {
    let config = pending
        .config
        .and_then(|address| aux_map.get(&address))
        .map(|account| account.data.as_slice());

    let reserves = match (pending.vault_a, pending.vault_b) {
        (Some(a), Some(b)) => {
            let reserve_a = aux_map
                .get(&a)
                .and_then(|account| token_account_amount(&account.data));
            let reserve_b = aux_map
                .get(&b)
                .and_then(|account| token_account_amount(&account.data));
            match (reserve_a, reserve_b) {
                (Some(a), Some(b)) => Some((a, b)),
                _ => None,
            }
        }
        _ => None,
    };
    let decimals = match (pending.mint_a, pending.mint_b) {
        (Some(a), Some(b)) => {
            let dec_a = aux_map
                .get(&a)
                .and_then(|account| mint_decimals(&account.data));
            let dec_b = aux_map
                .get(&b)
                .and_then(|account| mint_decimals(&account.data));
            match (dec_a, dec_b) {
                (Some(a), Some(b)) => Some((a, b)),
                _ => None,
            }
        }
        _ => pending.decimals,
    };

    let aux = DecodeAux {
        amm_config: config,
        decimals,
        fee_override_bps: pending.fee_bps,
    };
    let mut context = DecodeContext::new(pending.watched.address, pending.slot);
    context.reserves = reserves;
    context.decimals = decimals;
    decode_venue(venue, &pending.data, &context, &aux).map_err(FeedError::from)
}

fn auxiliary_config_address(decoder: Option<DecoderKind>, data: &[u8]) -> Option<Address> {
    let offset = match decoder {
        Some(DecoderKind::RaydiumCpmm | DecoderKind::RaydiumClmm) => 8,
        _ => return None,
    };
    let bytes = data.get(offset..offset + 32)?;
    let address = Address::from_slice(bytes).ok()?;
    (address != Address::ZERO).then_some(address)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_watchlist_parses() {
        let config = PoolConfig {
            address: "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C".into(),
            venue: "raydium_cpmm".into(),
            mint_a: "So11111111111111111111111111111111111111112".into(),
            mint_b: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".into(),
            vault_a: None,
            vault_b: None,
            fee_bps: Some(25),
            decimals_a: Some(9),
            decimals_b: Some(6),
            enabled: true,
        };
        let watched = WatchedPool::from_config(&config).unwrap();
        assert_eq!(watched.venue, "raydium_cpmm");
        assert_eq!(watched.decimals, Some((9, 6)));
        assert_eq!(watched.fee_override_bps, Some(25));
    }

    #[test]
    fn unknown_venue_is_rejected() {
        let config = PoolConfig {
            address: "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C".into(),
            venue: "not_a_venue".into(),
            mint_a: String::new(),
            mint_b: String::new(),
            vault_a: None,
            vault_b: None,
            fee_bps: None,
            decimals_a: None,
            decimals_b: None,
            enabled: true,
        };
        assert!(matches!(
            WatchedPool::from_config(&config),
            Err(FeedError::UnknownVenue(_))
        ));
    }

    #[test]
    fn feed_config_rejects_dangerous_values() {
        let config = FeedConfig {
            interval_ms: 10,
            ..FeedConfig::default()
        };
        assert!(config.validate().is_err());
        let config = FeedConfig {
            max_concurrent: 0,
            ..FeedConfig::default()
        };
        assert!(config.validate().is_err());
        let config = FeedConfig {
            stale_after_ms: 10,
            ..FeedConfig::default()
        };
        assert!(config.validate().is_err());
    }
}
