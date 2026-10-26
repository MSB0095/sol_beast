//! Process-wide counters. Keep this dependency-free so every crate can share it.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
pub struct StrategyMetrics {
    pub opportunities: AtomicU64,
    pub simulated: AtomicU64,
    pub not_landed: AtomicU64,
    pub landed: AtomicU64,
    pub reverted: AtomicU64,
    pub skipped: AtomicU64,
    pub realized_net_lamports: AtomicI64,
    pub attempts: AtomicU64,
    pub dropped: AtomicU64,
}

#[derive(Debug)]
pub struct Metrics {
    started_at: DateTime<Utc>,
    pub feed_updates: AtomicU64,
    pub feed_dropped: AtomicU64,
    pub ws_reconnects: AtomicU64,
    pub rpc_errors: AtomicU64,
    pub opportunities: AtomicU64,
    pub simulated: AtomicU64,
    pub bundles_submitted: AtomicU64,
    pub bundles_landed: AtomicU64,
    pub bundles_dropped: AtomicU64,
    pub bundles_reverted: AtomicU64,
    pub guard_violations: AtomicU64,
    pub rent_closed_accounts: AtomicU64,
    pub rent_recovered_lamports: AtomicU64,
    pub unexpected_balance_delta_lamports: AtomicU64,
    pub realized_net_lamports: AtomicI64,
    pub last_slot: AtomicU64,
    pub last_update_ms: AtomicI64,
    pub feed_lag_ms: AtomicU64,
    per_strategy: Mutex<BTreeMap<String, Arc<StrategyMetrics>>>,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            started_at: Utc::now(),
            feed_updates: AtomicU64::new(0),
            feed_dropped: AtomicU64::new(0),
            ws_reconnects: AtomicU64::new(0),
            rpc_errors: AtomicU64::new(0),
            opportunities: AtomicU64::new(0),
            simulated: AtomicU64::new(0),
            bundles_submitted: AtomicU64::new(0),
            bundles_landed: AtomicU64::new(0),
            bundles_dropped: AtomicU64::new(0),
            bundles_reverted: AtomicU64::new(0),
            guard_violations: AtomicU64::new(0),
            rent_closed_accounts: AtomicU64::new(0),
            rent_recovered_lamports: AtomicU64::new(0),
            unexpected_balance_delta_lamports: AtomicU64::new(0),
            realized_net_lamports: AtomicI64::new(0),
            last_slot: AtomicU64::new(0),
            last_update_ms: AtomicI64::new(0),
            feed_lag_ms: AtomicU64::new(0),
            per_strategy: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn strategy(&self, name: &str) -> Arc<StrategyMetrics> {
        let mut map = self.per_strategy.lock().expect("metrics mutex poisoned");
        map.entry(name.to_string())
            .or_insert_with(|| Arc::new(StrategyMetrics::default()))
            .clone()
    }

    pub fn observe_feed_update(&self, slot: u64, now_ms: i64) {
        self.feed_updates.fetch_add(1, Ordering::Relaxed);
        self.last_slot.store(slot, Ordering::Relaxed);
        self.last_update_ms.store(now_ms, Ordering::Relaxed);
    }

    pub fn set_feed_lag(&self, lag_ms: u64) {
        self.feed_lag_ms.store(lag_ms, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        let now_ms = Utc::now().timestamp_millis();
        let per_strategy = self
            .per_strategy
            .lock()
            .expect("metrics mutex poisoned")
            .iter()
            .map(|(name, metrics)| {
                (
                    name.clone(),
                    StrategyMetricsSnapshot {
                        opportunities: metrics.opportunities.load(Ordering::Relaxed),
                        simulated: metrics.simulated.load(Ordering::Relaxed),
                        not_landed: metrics.not_landed.load(Ordering::Relaxed),
                        landed: metrics.landed.load(Ordering::Relaxed),
                        reverted: metrics.reverted.load(Ordering::Relaxed),
                        skipped: metrics.skipped.load(Ordering::Relaxed),
                        attempts: metrics.attempts.load(Ordering::Relaxed),
                        dropped: metrics.dropped.load(Ordering::Relaxed),
                        realized_net_lamports: metrics
                            .realized_net_lamports
                            .load(Ordering::Relaxed),
                    },
                )
            })
            .collect();
        MetricsSnapshot {
            started_at: self.started_at,
            uptime_seconds: (now_ms - self.started_at.timestamp_millis()).max(0) as u64 / 1_000,
            feed_updates: self.feed_updates.load(Ordering::Relaxed),
            feed_dropped: self.feed_dropped.load(Ordering::Relaxed),
            ws_reconnects: self.ws_reconnects.load(Ordering::Relaxed),
            rpc_errors: self.rpc_errors.load(Ordering::Relaxed),
            opportunities: self.opportunities.load(Ordering::Relaxed),
            simulated: self.simulated.load(Ordering::Relaxed),
            bundles_submitted: self.bundles_submitted.load(Ordering::Relaxed),
            bundles_landed: self.bundles_landed.load(Ordering::Relaxed),
            bundles_dropped: self.bundles_dropped.load(Ordering::Relaxed),
            bundles_reverted: self.bundles_reverted.load(Ordering::Relaxed),
            guard_violations: self.guard_violations.load(Ordering::Relaxed),
            rent_closed_accounts: self.rent_closed_accounts.load(Ordering::Relaxed),
            rent_recovered_lamports: self.rent_recovered_lamports.load(Ordering::Relaxed),
            unexpected_balance_delta_lamports: self
                .unexpected_balance_delta_lamports
                .load(Ordering::Relaxed),
            realized_net_lamports: self.realized_net_lamports.load(Ordering::Relaxed),
            last_slot: self.last_slot.load(Ordering::Relaxed),
            last_update_ms: self.last_update_ms.load(Ordering::Relaxed),
            feed_lag_ms: self.feed_lag_ms.load(Ordering::Relaxed),
            per_strategy,
        }
    }

    pub fn prometheus(&self) -> String {
        let s = self.snapshot();
        let mut out = String::with_capacity(2_048);
        macro_rules! metric {
            ($name:literal, $help:literal, $value:expr) => {
                out.push_str(concat!("# HELP ", $name, " ", $help, "\n"));
                out.push_str(concat!("# TYPE ", $name, " gauge\n"));
                out.push_str(concat!($name, " "));
                out.push_str(&$value.to_string());
                out.push('\n');
            };
        }
        metric!(
            "beast_uptime_seconds",
            "Process uptime in seconds",
            s.uptime_seconds
        );
        metric!(
            "beast_feed_updates_total",
            "Feed updates accepted",
            s.feed_updates
        );
        metric!(
            "beast_feed_dropped_total",
            "Feed updates dropped",
            s.feed_dropped
        );
        metric!(
            "beast_ws_reconnects_total",
            "WebSocket reconnects",
            s.ws_reconnects
        );
        metric!("beast_rpc_errors_total", "RPC errors", s.rpc_errors);
        metric!(
            "beast_opportunities_total",
            "Opportunities observed",
            s.opportunities
        );
        metric!(
            "beast_simulated_total",
            "Opportunities simulated",
            s.simulated
        );
        metric!(
            "beast_bundles_submitted_total",
            "Bundles submitted",
            s.bundles_submitted
        );
        metric!(
            "beast_bundles_landed_total",
            "Bundles landed",
            s.bundles_landed
        );
        metric!(
            "beast_bundles_dropped_total",
            "Bundles dropped (must cost zero)",
            s.bundles_dropped
        );
        metric!(
            "beast_bundles_reverted_total",
            "Bundles landed but reverted",
            s.bundles_reverted
        );
        metric!(
            "beast_guard_violations_total",
            "On-chain guard violations",
            s.guard_violations
        );
        metric!(
            "beast_rent_closed_accounts_total",
            "Rent accounts closed",
            s.rent_closed_accounts
        );
        metric!(
            "beast_rent_recovered_lamports",
            "Lamports recovered from closed token accounts",
            s.rent_recovered_lamports
        );
        metric!(
            "beast_realized_net_lamports",
            "Realized net lamports",
            s.realized_net_lamports
        );
        metric!("beast_last_slot", "Last observed slot", s.last_slot);
        metric!(
            "beast_feed_lag_ms",
            "Feed lag in milliseconds",
            s.feed_lag_ms
        );
        for (name, m) in &s.per_strategy {
            out.push_str(&format!(
                "beast_strategy_landed_total{{strategy=\"{name}\"}} {}\n",
                m.landed
            ));
            out.push_str(&format!(
                "beast_strategy_dropped_total{{strategy=\"{name}\"}} {}\n",
                m.dropped
            ));
            out.push_str(&format!(
                "beast_strategy_realized_net_lamports{{strategy=\"{name}\"}} {}\n",
                m.realized_net_lamports
            ));
        }
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyMetricsSnapshot {
    pub opportunities: u64,
    pub simulated: u64,
    pub not_landed: u64,
    pub landed: u64,
    pub reverted: u64,
    pub skipped: u64,
    pub attempts: u64,
    pub dropped: u64,
    pub realized_net_lamports: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub started_at: DateTime<Utc>,
    pub uptime_seconds: u64,
    pub feed_updates: u64,
    pub feed_dropped: u64,
    pub ws_reconnects: u64,
    pub rpc_errors: u64,
    pub opportunities: u64,
    pub simulated: u64,
    pub bundles_submitted: u64,
    pub bundles_landed: u64,
    pub bundles_dropped: u64,
    pub bundles_reverted: u64,
    pub guard_violations: u64,
    pub rent_closed_accounts: u64,
    pub rent_recovered_lamports: u64,
    pub unexpected_balance_delta_lamports: u64,
    pub realized_net_lamports: i64,
    pub last_slot: u64,
    pub last_update_ms: i64,
    pub feed_lag_ms: u64,
    pub per_strategy: BTreeMap<String, StrategyMetricsSnapshot>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_and_prometheus() {
        let metrics = Metrics::new();
        metrics.observe_feed_update(42, 1_000);
        metrics.bundles_dropped.fetch_add(1, Ordering::Relaxed);
        let strategy = metrics.strategy("cross_dex_arb");
        strategy.landed.fetch_add(2, Ordering::Relaxed);
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.last_slot, 42);
        assert_eq!(snapshot.bundles_dropped, 1);
        assert_eq!(snapshot.per_strategy["cross_dex_arb"].landed, 2);
        let text = metrics.prometheus();
        assert!(text.contains("beast_last_slot 42"));
        assert!(text.contains("strategy=\"cross_dex_arb\""));
    }
}
