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
use futures_util::stream::{self, StreamExt};
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
        let started = Instant::now();
        let watchlist = self.watchlist().await;
        let attempted = watchlist.len();
        let max_concurrent = self.config.max_concurrent.max(1);
        let results: Vec<bool> = stream::iter(watchlist.into_iter().map(|watched| async move {
            match refresh_pool(&self.rpc, &watched, self.config.refresh_reserves).await {
                Ok(snapshot) => {
                    self.commit_ok(&watched, snapshot).await;
                    true
                }
                Err(error) => {
                    self.commit_err(&watched, &error).await;
                    false
                }
            }
        }))
        .buffer_unordered(max_concurrent)
        .collect()
        .await;
        let updated = results.iter().filter(|ok| **ok).count();
        let failed = attempted.saturating_sub(updated);
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
pub async fn refresh_pool(
    rpc: &RpcClient,
    watched: &WatchedPool,
    refresh_reserves: bool,
) -> Result<PoolSnapshot> {
    let venue = catalog::venue(&watched.venue)
        .ok_or_else(|| FeedError::UnknownVenue(watched.venue.clone()))?;
    if venue.decoder.is_none() {
        return Err(FeedError::Config(format!(
            "venue {} is quote-only and has no account decoder",
            watched.venue
        )));
    }
    let info = rpc
        .get_account_info(watched.address)
        .await?
        .ok_or(FeedError::NotFound(watched.address))?;
    if info.account.owner.to_string() != venue.program_id {
        return Err(FeedError::Config(format!(
            "pool {} owner {} does not match catalogue program {}",
            watched.address, info.account.owner, venue.program_id
        )));
    }
    let data = info.account.data;
    let config_address = watched
        .amm_config
        .or_else(|| auxiliary_config_address(venue.decoder, &data));
    let config_account = match config_address {
        Some(address) => rpc.get_account_info(address).await.ok().flatten(),
        None => None,
    };
    let aux = DecodeAux {
        amm_config: config_account
            .as_ref()
            .map(|account| account.account.data.as_slice()),
        decimals: watched.decimals,
        fee_override_bps: watched.fee_override_bps,
    };
    let base_context = DecodeContext::new(watched.address, info.slot);
    let first = decode_venue(venue, &data, &base_context, &aux)?;

    let needs_reserves = refresh_reserves
        && matches!(
            venue.decoder,
            Some(
                DecoderKind::RaydiumAmmV4
                    | DecoderKind::PumpSwap
                    | DecoderKind::SplTokenSwap
                    | DecoderKind::MeteoraDammV1
                    | DecoderKind::MeteoraDammV2
                    | DecoderKind::MeteoraDlmm
            )
        );
    let needs_decimals = watched.decimals.is_none()
        && matches!(
            venue.decoder,
            Some(
                DecoderKind::PumpSwap
                    | DecoderKind::OrcaWhirlpool
                    | DecoderKind::SplTokenSwap
                    | DecoderKind::SaberStable
                    | DecoderKind::MeteoraDammV1
                    | DecoderKind::MeteoraDammV2
                    | DecoderKind::MeteoraDlmm
            )
        );

    let mut reserves = None;
    let mut decimals = watched.decimals;
    if needs_reserves || needs_decimals {
        let mut addresses = Vec::with_capacity(4);
        if needs_reserves {
            addresses.push(first.vault_a);
            addresses.push(first.vault_b);
        }
        if needs_decimals {
            addresses.push(first.mint_a);
            addresses.push(first.mint_b);
        }
        let mut unique = Vec::with_capacity(addresses.len());
        for address in addresses {
            if !unique.contains(&address) {
                unique.push(address);
            }
        }
        let accounts = rpc.get_multiple_accounts(&unique).await?;
        let by_address: HashMap<Address, &beast_rpc::Account> = unique
            .iter()
            .copied()
            .zip(accounts.iter())
            .filter_map(|(address, account)| account.as_ref().map(|account| (address, account)))
            .collect();
        if needs_reserves {
            let reserve_a = by_address
                .get(&first.vault_a)
                .and_then(|account| token_account_amount(&account.data));
            let reserve_b = by_address
                .get(&first.vault_b)
                .and_then(|account| token_account_amount(&account.data));
            if let (Some(reserve_a), Some(reserve_b)) = (reserve_a, reserve_b) {
                reserves = Some((reserve_a, reserve_b));
            }
        }
        if needs_decimals {
            let decimals_a = by_address
                .get(&first.mint_a)
                .and_then(|account| mint_decimals(&account.data));
            let decimals_b = by_address
                .get(&first.mint_b)
                .and_then(|account| mint_decimals(&account.data));
            if let (Some(decimals_a), Some(decimals_b)) = (decimals_a, decimals_b) {
                decimals = Some((decimals_a, decimals_b));
            }
        }
    }

    let mut context = DecodeContext::new(watched.address, info.slot);
    context.reserves = reserves;
    context.decimals = decimals;
    let snapshot = decode_venue(venue, &data, &context, &aux)?;
    Ok(snapshot)
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
