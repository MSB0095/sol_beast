use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// Every strategy family wired into the 2.0 engine.
///
/// Tracks that are catalogued but not implemented are intentionally absent:
/// the dashboard and config only expose strategies that actually detect and
/// execute. The legacy pump.fun sniper lives in the root package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategyKind {
    CrossDexArb,
    TriangularArb,
    LstArb,
    StableArb,
    RentReclaim,
    /// A cycle funded by an atomic flash loan instead of wallet capital.
    FlashArb,
}

impl StrategyKind {
    pub const ALL: [StrategyKind; 6] = [
        Self::CrossDexArb,
        Self::TriangularArb,
        Self::LstArb,
        Self::StableArb,
        Self::RentReclaim,
        Self::FlashArb,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::CrossDexArb => "cross_dex_arb",
            Self::TriangularArb => "triangular_arb",
            Self::LstArb => "lst_arb",
            Self::StableArb => "stable_arb",
            Self::RentReclaim => "rent_reclaim",
            Self::FlashArb => "flash_arb",
        }
    }

    /// True for strategies whose transactions must be bundle-submitted with an
    /// on-chain profit guard. Rent recovery moves only the owner's own empty
    /// token accounts and is exempt.
    pub fn requires_bundle_guard(self) -> bool {
        !matches!(self, Self::RentReclaim)
    }
}

impl fmt::Display for StrategyKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for StrategyKind {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|kind| kind.as_str() == s)
            .ok_or_else(|| format!("unknown strategy kind {s:?}"))
    }
}

/// Operating mode. The global runtime ceiling is also a `Mode`; strategy modes
/// can never exceed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Off,
    #[default]
    Radar,
    Canary,
    Live,
}

impl Mode {
    pub fn rank(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::Radar => 1,
            Self::Canary => 2,
            Self::Live => 3,
        }
    }

    pub fn is_live(self) -> bool {
        matches!(self, Self::Canary | Self::Live)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Radar => "radar",
            Self::Canary => "canary",
            Self::Live => "live",
        }
    }

    /// Never allow a strategy to be more aggressive than the process ceiling.
    pub fn capped_by(self, ceiling: Mode) -> Mode {
        if self.rank() <= ceiling.rank() {
            self
        } else {
            ceiling
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Mode {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "off" => Ok(Self::Off),
            "radar" => Ok(Self::Radar),
            "canary" => Ok(Self::Canary),
            "live" => Ok(Self::Live),
            other => Err(format!("unknown mode {other:?}")),
        }
    }
}

/// State update emitted by every feed adapter. Bytes only: feed dependencies
/// stay isolated from the rest of the workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StateUpdate {
    Slot {
        slot: u64,
        parent: Option<u64>,
        observed_at_ms: i64,
    },
    Account {
        pubkey: String,
        owner: String,
        slot: u64,
        write_version: u64,
        data: Vec<u8>,
        observed_at_ms: i64,
    },
    Transaction {
        signature: String,
        slot: u64,
        success: bool,
        observed_at_ms: i64,
        meta: serde_json::Value,
    },
}

impl StateUpdate {
    pub fn slot(&self) -> u64 {
        match self {
            Self::Slot { slot, .. }
            | Self::Account { slot, .. }
            | Self::Transaction { slot, .. } => *slot,
        }
    }
}

/// One leg of an abstract plan. Instruction building happens later in
/// `beast-exec`; Radar only needs the economics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LegPlan {
    pub venue: String,
    pub pool: String,
    pub mint_in: String,
    pub mint_out: String,
    pub amount_in: u64,
    pub min_out: u64,
    /// Expected price impact in basis points at the quoted size.
    pub price_impact_bps: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CostBreakdown {
    pub tip_lamports: u64,
    pub priority_fee_lamports: u64,
    pub base_fee_lamports: u64,
    pub rent_lamports: u64,
    pub guard_lamports: u64,
    /// Protocol fee charged by the flash-loan reserve. Zero for cycles that
    /// spend wallet capital.
    #[serde(default)]
    pub flash_fee_lamports: u64,
}

impl CostBreakdown {
    pub fn total(&self) -> u64 {
        self.tip_lamports
            .saturating_add(self.priority_fee_lamports)
            .saturating_add(self.base_fee_lamports)
            .saturating_add(self.rent_lamports)
            .saturating_add(self.guard_lamports)
            .saturating_add(self.flash_fee_lamports)
    }
}

/// The unit of work shared by Radar, the evaluator and the executor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Opportunity {
    pub id: Uuid,
    pub strategy: StrategyKind,
    pub mode: Mode,
    pub slot_detected: u64,
    pub detected_at_ms: i64,
    pub legs: Vec<LegPlan>,
    /// Gross profit in lamports (or lamport-equivalent for a token quote).
    pub est_gross_lamports: i64,
    pub est_costs: CostBreakdown,
    pub est_net_lamports: i64,
    pub confidence: f32,
    pub ttl_slots: u8,
    pub dedupe_key: String,
    pub outcome: OutcomeLabel,
    pub meta: serde_json::Value,
}

impl Opportunity {
    pub fn recompute_net(&mut self) {
        self.est_net_lamports = self
            .est_gross_lamports
            .saturating_sub(self.est_costs.total() as i64);
    }

    pub fn is_profitable(&self, min_net_lamports: u64) -> bool {
        self.est_net_lamports >= min_net_lamports as i64
    }
}

/// Competitor/outcome labels used by the Radar learning loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeLabel {
    #[default]
    Unlabeled,
    Open,
    OpenExpired,
    TakenSameSlot,
    TakenNextSlot,
    TakenLate,
    NeverTaken,
    Skipped,
    Dropped,
    Landed,
    Reverted,
}

impl OutcomeLabel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unlabeled => "unlabeled",
            Self::Open => "open",
            Self::OpenExpired => "open_expired",
            Self::TakenSameSlot => "taken_same_slot",
            Self::TakenNextSlot => "taken_next_slot",
            Self::TakenLate => "taken_late",
            Self::NeverTaken => "never_taken",
            Self::Skipped => "skipped",
            Self::Dropped => "dropped",
            Self::Landed => "landed",
            Self::Reverted => "reverted",
        }
    }
}

/// Settlement is recorded from on-chain pre/post balances, never from the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementResult {
    pub opportunity_id: Uuid,
    pub signature: String,
    pub slot: u64,
    pub realized_gross_lamports: i64,
    pub realized_tip_lamports: u64,
    pub realized_fee_lamports: u64,
    pub realized_net_lamports: i64,
    pub modeled_net_lamports: i64,
    pub settled_at_ms: i64,
}

impl SettlementResult {
    pub fn model_error_bps(&self) -> i64 {
        let denominator = self.modeled_net_lamports.unsigned_abs().max(1) as i64;
        (self.realized_net_lamports - self.modeled_net_lamports)
            .saturating_mul(10_000)
            .saturating_div(denominator)
    }
}

/// Statistics required to decide whether a strategy may be promoted.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromotionStats {
    pub radar_days: f64,
    pub canary_days: f64,
    pub landed: u64,
    pub dropped: u64,
    pub realized_net_lamports: i64,
    pub model_error_p95_bps: i64,
    pub guard_violations: u64,
    pub daily_cost_cap_breached: bool,
}

/// A compact, stable key used to suppress duplicate in-flight opportunities.
pub fn dedupe_key(
    strategy: StrategyKind,
    pools: &[&str],
    direction: &str,
    size_bucket: u64,
) -> String {
    format!(
        "{}:{}:{}:{}",
        strategy.as_str(),
        pools.join("|"),
        direction,
        size_bucket
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_cap_is_monotonic() {
        assert_eq!(Mode::Live.capped_by(Mode::Radar), Mode::Radar);
        assert_eq!(Mode::Canary.capped_by(Mode::Live), Mode::Canary);
        assert_eq!(Mode::Off.capped_by(Mode::Live), Mode::Off);
    }

    #[test]
    fn strategy_roundtrip() {
        for kind in StrategyKind::ALL {
            let parsed: StrategyKind = kind.as_str().parse().unwrap();
            assert_eq!(parsed, kind);
        }
    }

    #[test]
    fn net_profit_subtracts_every_cost() {
        let mut opp = Opportunity {
            id: Uuid::new_v4(),
            strategy: StrategyKind::CrossDexArb,
            mode: Mode::Radar,
            slot_detected: 1,
            detected_at_ms: 0,
            legs: vec![],
            est_gross_lamports: 100_000,
            est_costs: CostBreakdown {
                tip_lamports: 40_000,
                priority_fee_lamports: 1_000,
                base_fee_lamports: 5_000,
                rent_lamports: 0,
                guard_lamports: 0,
                flash_fee_lamports: 0,
            },
            est_net_lamports: 0,
            confidence: 1.0,
            ttl_slots: 2,
            dedupe_key: String::new(),
            outcome: OutcomeLabel::Open,
            meta: serde_json::Value::Null,
        };
        opp.recompute_net();
        assert_eq!(opp.est_net_lamports, 54_000);
        assert!(opp.is_profitable(50_000));
        assert!(!opp.is_profitable(60_000));
    }

    #[test]
    fn flash_fee_is_part_of_total_cost() {
        let costs = CostBreakdown {
            tip_lamports: 1_000,
            priority_fee_lamports: 2_000,
            base_fee_lamports: 5_000,
            rent_lamports: 7_000,
            guard_lamports: 0,
            flash_fee_lamports: 11_000,
        };
        assert_eq!(costs.total(), 26_000);
    }

    #[test]
    fn model_error_sign_and_scale() {
        let result = SettlementResult {
            opportunity_id: Uuid::new_v4(),
            signature: "sig".into(),
            slot: 1,
            realized_gross_lamports: 0,
            realized_tip_lamports: 0,
            realized_fee_lamports: 0,
            realized_net_lamports: 95,
            modeled_net_lamports: 100,
            settled_at_ms: 0,
        };
        assert_eq!(result.model_error_bps(), -500);
    }
}
