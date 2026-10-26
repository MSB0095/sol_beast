//! Fail-closed risk engine: controls from §14 of the roadmap.

use crate::config::RiskConfig;
use crate::types::{Mode, Opportunity, PromotionStats, StrategyKind};
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskControls {
    pub balance_floor_lamports: u64,
    pub max_inflight_bundles: usize,
    pub max_inflight_per_strategy: usize,
    pub max_attempts_per_min: u32,
    pub daily_landed_cost_cap_lamports: u64,
    pub max_model_error_bps: i64,
    pub max_feed_lag_ms: u64,
    pub max_slot_lag: u64,
    pub max_clock_skew_ms: u64,
    pub max_consecutive_failures: u32,
    pub unexpected_balance_delta_lamports: u64,
    pub kill_switch_file: std::path::PathBuf,
}

impl Default for RiskControls {
    fn default() -> Self {
        Self::from(&RiskConfig::default())
    }
}

impl From<&RiskConfig> for RiskControls {
    fn from(cfg: &RiskConfig) -> Self {
        Self {
            balance_floor_lamports: cfg.balance_floor_lamports,
            max_inflight_bundles: cfg.max_inflight_bundles,
            max_inflight_per_strategy: cfg.max_inflight_per_strategy,
            max_attempts_per_min: cfg.max_attempts_per_min,
            daily_landed_cost_cap_lamports: cfg.daily_landed_cost_cap_lamports,
            max_model_error_bps: cfg.max_model_error_bps,
            max_feed_lag_ms: cfg.max_feed_lag_ms,
            max_slot_lag: cfg.max_slot_lag,
            max_clock_skew_ms: cfg.max_clock_skew_ms,
            max_consecutive_failures: cfg.max_consecutive_failures,
            unexpected_balance_delta_lamports: cfg.unexpected_balance_delta_lamports,
            kill_switch_file: cfg.kill_switch_file.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskState {
    Healthy,
    Degraded,
    Halted,
}

impl RiskState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::Halted => "halted",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskReason {
    pub code: String,
    pub message: String,
    pub at_ms: i64,
    pub hard: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny(String),
}

impl Decision {
    pub fn is_allow(&self) -> bool {
        matches!(self, Self::Allow)
    }
}

#[derive(Debug, Clone, Default)]
struct StrategyRuntime {
    attempts: VecDeque<Instant>,
    inflight: usize,
    consecutive_failures: u32,
    landed_cost_today: u64,
    day: Option<NaiveDate>,
}

#[derive(Debug)]
pub struct RiskEngine {
    controls: RiskControls,
    state: RiskState,
    reasons: Vec<RiskReason>,
    attempts: VecDeque<Instant>,
    inflight: usize,
    landed_cost_today: u64,
    day: NaiveDate,
    per_strategy: HashMap<StrategyKind, StrategyRuntime>,
    model_errors_by_pool: HashMap<String, u32>,
}

impl RiskEngine {
    pub fn new(controls: RiskControls) -> Self {
        Self {
            controls,
            state: RiskState::Healthy,
            reasons: vec![],
            attempts: VecDeque::new(),
            inflight: 0,
            landed_cost_today: 0,
            day: Utc::now().date_naive(),
            per_strategy: HashMap::new(),
            model_errors_by_pool: HashMap::new(),
        }
    }

    pub fn controls(&self) -> &RiskControls {
        &self.controls
    }

    pub fn state(&self) -> RiskState {
        self.state
    }

    pub fn reasons(&self) -> &[RiskReason] {
        &self.reasons
    }

    pub fn is_halted(&self) -> bool {
        self.state == RiskState::Halted
    }

    pub fn halt(&mut self, code: &str, message: impl Into<String>) {
        self.state = RiskState::Halted;
        self.reasons.push(RiskReason {
            code: code.into(),
            message: message.into(),
            at_ms: Utc::now().timestamp_millis(),
            hard: true,
        });
    }

    pub fn degrade(&mut self, code: &str, message: impl Into<String>) {
        if self.state != RiskState::Halted {
            self.state = RiskState::Degraded;
        }
        self.reasons.push(RiskReason {
            code: code.into(),
            message: message.into(),
            at_ms: Utc::now().timestamp_millis(),
            hard: false,
        });
    }

    /// Human-only resume after a hard halt. A review should accompany this call.
    pub fn resume(&mut self) {
        self.state = RiskState::Healthy;
        self.reasons.clear();
    }

    /// Auto-recovery is only allowed from soft degradation.
    pub fn auto_recover(&mut self) {
        if self.state == RiskState::Degraded {
            self.state = RiskState::Healthy;
            self.reasons.clear();
        }
    }

    pub fn check_balance(&mut self, balance_lamports: u64) -> Decision {
        if balance_lamports < self.controls.balance_floor_lamports {
            self.degrade(
                "balance_floor",
                format!(
                    "wallet balance {} is below floor {}",
                    balance_lamports, self.controls.balance_floor_lamports
                ),
            );
            return Decision::Deny("balance below safety floor".into());
        }
        Decision::Allow
    }

    pub fn check_health(
        &mut self,
        feed_lag_ms: u64,
        slot_lag: u64,
        clock_skew_ms: u64,
    ) -> Decision {
        if feed_lag_ms > self.controls.max_feed_lag_ms {
            self.degrade(
                "feed_lag",
                format!(
                    "feed lag {feed_lag_ms}ms exceeds {}ms",
                    self.controls.max_feed_lag_ms
                ),
            );
            return Decision::Deny("stale feed".into());
        }
        if slot_lag > self.controls.max_slot_lag {
            self.degrade(
                "slot_lag",
                format!("slot lag {slot_lag} exceeds {}", self.controls.max_slot_lag),
            );
            return Decision::Deny("stale slot".into());
        }
        if clock_skew_ms > self.controls.max_clock_skew_ms {
            self.degrade(
                "clock_skew",
                format!(
                    "clock skew {clock_skew_ms}ms exceeds {}ms",
                    self.controls.max_clock_skew_ms
                ),
            );
            return Decision::Deny("clock skew".into());
        }
        Decision::Allow
    }

    pub fn check_kill_switch(&mut self) -> Decision {
        if self.controls.kill_switch_file.exists() {
            self.halt(
                "kill_switch",
                format!(
                    "kill-switch file {} exists",
                    self.controls.kill_switch_file.display()
                ),
            );
            return Decision::Deny("kill switch active".into());
        }
        Decision::Allow
    }

    pub fn check_kill_switch_path(path: &Path) -> bool {
        path.exists()
    }

    fn roll_day(&mut self) {
        let today = Utc::now().date_naive();
        if today != self.day {
            self.day = today;
            self.landed_cost_today = 0;
            for runtime in self.per_strategy.values_mut() {
                runtime.day = Some(today);
                runtime.landed_cost_today = 0;
            }
        }
    }

    fn prune_attempts(&mut self) {
        let cutoff = Instant::now() - Duration::from_secs(60);
        while self
            .attempts
            .front()
            .is_some_and(|instant| *instant < cutoff)
        {
            self.attempts.pop_front();
        }
    }

    pub fn evaluate(&mut self, opportunity: &Opportunity, strategy_mode: Mode) -> Decision {
        self.roll_day();
        if self.state == RiskState::Halted {
            return Decision::Deny("risk engine halted".into());
        }
        if self.check_kill_switch() == Decision::Allow {
            // continue
        } else {
            return Decision::Deny("kill switch active".into());
        }
        if strategy_mode == Mode::Off {
            return Decision::Deny("strategy is off".into());
        }
        if strategy_mode == Mode::Radar {
            return Decision::Deny("strategy is radar-only".into());
        }
        if self.inflight >= self.controls.max_inflight_bundles {
            return Decision::Deny("global in-flight limit reached".into());
        }
        self.prune_attempts();
        if self.attempts.len() >= self.controls.max_attempts_per_min as usize {
            return Decision::Deny("global attempts/minute limit reached".into());
        }
        let runtime = self.per_strategy.entry(opportunity.strategy).or_default();
        if runtime.inflight >= self.controls.max_inflight_per_strategy {
            return Decision::Deny("strategy in-flight limit reached".into());
        }
        if runtime.landed_cost_today >= self.controls.daily_landed_cost_cap_lamports {
            return Decision::Deny("strategy daily landed-cost cap reached".into());
        }
        if self.landed_cost_today >= self.controls.daily_landed_cost_cap_lamports {
            return Decision::Deny("global daily landed-cost cap reached".into());
        }
        if opportunity.est_net_lamports < 0 {
            return Decision::Deny("negative modeled net".into());
        }
        if opportunity.est_costs.tip_lamports > opportunity.est_gross_lamports.max(0) as u64 {
            return Decision::Deny("tip exceeds gross profit".into());
        }
        if self
            .model_errors_by_pool
            .get(&opportunity.dedupe_key)
            .copied()
            .unwrap_or(0)
            >= 3
        {
            return Decision::Deny("dedupe key blacklisted after repeated model errors".into());
        }
        Decision::Allow
    }

    pub fn begin_attempt(&mut self, strategy: StrategyKind) {
        self.roll_day();
        let now = Instant::now();
        self.attempts.push_back(now);
        self.inflight += 1;
        let runtime = self.per_strategy.entry(strategy).or_default();
        runtime.attempts.push_back(now);
        runtime.inflight += 1;
    }

    pub fn finish_attempt(&mut self, strategy: StrategyKind) {
        self.inflight = self.inflight.saturating_sub(1);
        if let Some(runtime) = self.per_strategy.get_mut(&strategy) {
            runtime.inflight = runtime.inflight.saturating_sub(1);
        }
    }

    pub fn record_build_failure(&mut self, strategy: StrategyKind) {
        let max = self.controls.max_consecutive_failures;
        let failures = {
            let runtime = self.per_strategy.entry(strategy).or_default();
            runtime.consecutive_failures += 1;
            runtime.consecutive_failures
        };
        if failures >= max {
            self.degrade(
                "consecutive_failures",
                format!(
                    "strategy {} hit {} consecutive build/sim failures",
                    strategy.as_str(),
                    failures
                ),
            );
        }
    }

    pub fn record_success(&mut self, strategy: StrategyKind) {
        if let Some(runtime) = self.per_strategy.get_mut(&strategy) {
            runtime.consecutive_failures = 0;
        }
    }

    pub fn record_landed_cost(&mut self, strategy: StrategyKind, cost_lamports: u64) {
        self.roll_day();
        self.landed_cost_today = self.landed_cost_today.saturating_add(cost_lamports);
        let runtime = self.per_strategy.entry(strategy).or_default();
        runtime.landed_cost_today = runtime.landed_cost_today.saturating_add(cost_lamports);
        if runtime.landed_cost_today >= self.controls.daily_landed_cost_cap_lamports {
            self.degrade(
                "daily_cost_cap",
                format!(
                    "strategy {} reached its daily landed-cost cap",
                    strategy.as_str()
                ),
            );
        }
    }

    /// Returns true when the error is within tolerance. Repeats blacklist the
    /// opportunity's dedupe key for the rest of the process.
    pub fn record_model_error(&mut self, key: &str, model_error_bps: i64) -> bool {
        let within = model_error_bps.unsigned_abs() as i64 <= self.controls.max_model_error_bps;
        if !within {
            let count = self
                .model_errors_by_pool
                .entry(key.to_string())
                .or_default();
            *count += 1;
            if *count >= 3 {
                self.degrade(
                    "model_error",
                    format!("pool/route {key} exceeded model-error tolerance 3 times"),
                );
            }
        }
        within
    }

    pub fn record_unexplained_delta(&mut self, expected: i64, actual: i64) {
        let delta = expected.saturating_sub(actual).unsigned_abs();
        if delta > self.controls.unexpected_balance_delta_lamports {
            self.halt(
                "unexpected_balance_delta",
                format!(
                    "unexplained balance delta {delta} lamports exceeds {}",
                    self.controls.unexpected_balance_delta_lamports
                ),
            );
        }
    }

    pub fn promotion_allowed(
        current: Mode,
        target: Mode,
        stats: &PromotionStats,
    ) -> std::result::Result<(), String> {
        if target.rank() <= current.rank() {
            return Err("target mode must be higher than current mode".into());
        }
        match (current, target) {
            (Mode::Off, Mode::Radar) => Ok(()),
            (Mode::Radar, Mode::Canary) => {
                if stats.radar_days < 14.0 {
                    return Err("need at least 14 days of Radar data".into());
                }
                if stats.dropped == 0 {
                    return Err("need at least one simulated/dropped attempt".into());
                }
                Ok(())
            }
            (Mode::Canary, Mode::Live) => {
                if stats.canary_days < 14.0 {
                    return Err("need at least 14 days in Canary".into());
                }
                if stats.realized_net_lamports <= 0 {
                    return Err("realized net P&L must be positive".into());
                }
                if stats.model_error_p95_bps.abs() > 25 {
                    return Err("model error p95 must be <= 25 bps".into());
                }
                if stats.guard_violations > 0 {
                    return Err("guard violations must be zero".into());
                }
                if stats.daily_cost_cap_breached {
                    return Err("daily cost cap was breached".into());
                }
                Ok(())
            }
            _ => Err(format!(
                "invalid promotion path {} -> {}",
                current.as_str(),
                target.as_str()
            )),
        }
    }

    pub fn inflight(&self) -> usize {
        self.inflight
    }

    pub fn landed_cost_today(&self) -> u64 {
        self.landed_cost_today
    }

    pub fn snapshot(&self) -> RiskSnapshot {
        RiskSnapshot {
            state: self.state,
            reasons: self.reasons.clone(),
            inflight: self.inflight,
            landed_cost_today: self.landed_cost_today,
            balance_floor_lamports: self.controls.balance_floor_lamports,
            daily_cost_cap_lamports: self.controls.daily_landed_cost_cap_lamports,
            kill_switch_file: self.controls.kill_switch_file.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskSnapshot {
    pub state: RiskState,
    pub reasons: Vec<RiskReason>,
    pub inflight: usize,
    pub landed_cost_today: u64,
    pub balance_floor_lamports: u64,
    pub daily_cost_cap_lamports: u64,
    pub kill_switch_file: std::path::PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CostBreakdown, OutcomeLabel};
    use uuid::Uuid;

    fn opportunity(strategy: StrategyKind) -> Opportunity {
        Opportunity {
            id: Uuid::new_v4(),
            strategy,
            mode: Mode::Canary,
            slot_detected: 1,
            detected_at_ms: 0,
            legs: vec![],
            est_gross_lamports: 100_000,
            est_costs: CostBreakdown {
                tip_lamports: 10_000,
                priority_fee_lamports: 1_000,
                base_fee_lamports: 5_000,
                ..CostBreakdown::default()
            },
            est_net_lamports: 84_000,
            confidence: 1.0,
            ttl_slots: 2,
            dedupe_key: "cross_dex_arb:p1|p2:a:1".into(),
            outcome: OutcomeLabel::Open,
            meta: serde_json::Value::Null,
        }
    }

    #[test]
    fn radar_never_sends() {
        let mut engine = RiskEngine::new(RiskControls::default());
        let op = opportunity(StrategyKind::CrossDexArb);
        assert!(!engine.evaluate(&op, Mode::Radar).is_allow());
        assert!(engine.evaluate(&op, Mode::Canary).is_allow());
    }

    #[test]
    fn balance_floor_degrades() {
        let mut engine = RiskEngine::new(RiskControls::default());
        assert!(!engine.check_balance(1).is_allow());
        assert_eq!(engine.state(), RiskState::Degraded);
    }

    #[test]
    fn kill_switch_halts() {
        let dir = tempfile::tempdir().unwrap();
        let controls = RiskControls {
            kill_switch_file: dir.path().join("STOP"),
            ..RiskControls::default()
        };
        std::fs::write(&controls.kill_switch_file, b"stop").unwrap();
        let mut engine = RiskEngine::new(controls);
        assert!(!engine.check_kill_switch().is_allow());
        assert_eq!(engine.state(), RiskState::Halted);
    }

    #[test]
    fn stale_feed_blocks() {
        let mut engine = RiskEngine::new(RiskControls::default());
        assert!(!engine.check_health(5_000, 0, 0).is_allow());
        assert_eq!(engine.state(), RiskState::Degraded);
    }

    #[test]
    fn inflight_is_counted() {
        let mut engine = RiskEngine::new(RiskControls::default());
        for _ in 0..3 {
            engine.begin_attempt(StrategyKind::CrossDexArb);
        }
        assert_eq!(engine.inflight(), 3);
        // per-strategy default is 3
        let op = opportunity(StrategyKind::CrossDexArb);
        assert!(!engine.evaluate(&op, Mode::Canary).is_allow());
        engine.finish_attempt(StrategyKind::CrossDexArb);
        assert!(engine.evaluate(&op, Mode::Canary).is_allow());
    }

    #[test]
    fn model_error_blacklists_after_three() {
        let mut engine = RiskEngine::new(RiskControls::default());
        let op = opportunity(StrategyKind::CrossDexArb);
        let key = op.dedupe_key.as_str();
        assert!(!engine.record_model_error(key, 500));
        assert!(!engine.record_model_error(key, 500));
        assert!(!engine.record_model_error(key, 500));
        assert!(!engine.evaluate(&op, Mode::Canary).is_allow());
    }

    #[test]
    fn unexplained_delta_hard_halts() {
        let mut engine = RiskEngine::new(RiskControls::default());
        engine.record_unexplained_delta(100_000, 90_000);
        assert!(engine.is_halted());
    }

    #[test]
    fn promotion_gates_are_conservative() {
        let mut stats = PromotionStats::default();
        assert!(RiskEngine::promotion_allowed(Mode::Radar, Mode::Canary, &stats).is_err());
        stats.radar_days = 14.0;
        stats.dropped = 1;
        assert!(RiskEngine::promotion_allowed(Mode::Radar, Mode::Canary, &stats).is_ok());
        stats.canary_days = 14.0;
        stats.realized_net_lamports = 1;
        assert!(RiskEngine::promotion_allowed(Mode::Canary, Mode::Live, &stats).is_ok());
        stats.model_error_p95_bps = 100;
        assert!(RiskEngine::promotion_allowed(Mode::Canary, Mode::Live, &stats).is_err());
    }
}
