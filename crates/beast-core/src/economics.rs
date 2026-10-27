//! Pure cost, tip and expected-value math (§5 of the roadmap).

use crate::types::CostBreakdown;
use serde::{Deserialize, Serialize};

pub const BASE_FEE_PER_SIGNATURE: u64 = 5_000;
pub const MIN_JITO_TIP_LAMPORTS: u64 = 1_000;
pub const SPL_TOKEN_ACCOUNT_RENT_LAMPORTS: u64 = 2_039_280;
pub const DURABLE_NONCE_RENT_LAMPORTS: u64 = 1_447_680;
pub const RENT_PER_BYTE_LAMPORTS: u64 = 6_960;

/// Rent-exempt minimum for an account with `data_len` bytes.
pub fn rent_exempt_minimum(data_len: usize) -> u64 {
    (128u64.saturating_add(data_len as u64))
        .saturating_mul(3_480)
        .saturating_mul(2)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TipPolicy {
    /// Fraction of estimated gross profit offered as a tip.
    pub alpha: f64,
    pub floor_lamports: u64,
    pub cap_lamports: u64,
}

impl Default for TipPolicy {
    fn default() -> Self {
        Self {
            alpha: 0.50,
            floor_lamports: MIN_JITO_TIP_LAMPORTS,
            cap_lamports: 2_000_000,
        }
    }
}

impl TipPolicy {
    pub fn validate(&self) -> Result<(), String> {
        if !self.alpha.is_finite() || !(0.01..=0.90).contains(&self.alpha) {
            return Err("tip alpha must be finite and within [0.01, 0.90]".into());
        }
        if self.floor_lamports < MIN_JITO_TIP_LAMPORTS {
            return Err(format!(
                "tip floor must be at least {MIN_JITO_TIP_LAMPORTS} lamports"
            ));
        }
        if self.cap_lamports < self.floor_lamports {
            return Err("tip cap must be >= tip floor".into());
        }
        Ok(())
    }

    /// `clamp(alpha * gross, floor, cap)`. Negative/zero gross still yields the
    /// floor so callers can reason about the guard; the evaluator must then
    /// reject the opportunity because net profit cannot cover the floor.
    pub fn tip_for(&self, gross_lamports: i64) -> u64 {
        if gross_lamports <= 0 {
            return self.floor_lamports.min(self.cap_lamports);
        }
        let scaled = (gross_lamports as f64 * self.alpha).round();
        let scaled = if scaled.is_finite() && scaled >= 0.0 {
            scaled as u64
        } else {
            self.floor_lamports
        };
        scaled.clamp(self.floor_lamports, self.cap_lamports)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CostModel {
    pub n_signatures: u64,
    pub priority_fee_lamports: u64,
    pub new_token_accounts: u64,
    /// Probability that ATA rent is refunded by a later close. 1.0 = fully
    /// refundable, 0.0 = treated as spent.
    pub rent_refund_probability: f64,
    pub guard_cu_lamports: u64,
}

impl Default for CostModel {
    fn default() -> Self {
        Self {
            n_signatures: 1,
            priority_fee_lamports: 1_000,
            new_token_accounts: 0,
            rent_refund_probability: 1.0,
            guard_cu_lamports: 0,
        }
    }
}

impl CostModel {
    pub fn base_fee_lamports(&self) -> u64 {
        BASE_FEE_PER_SIGNATURE.saturating_mul(self.n_signatures.max(1))
    }

    pub fn amortized_rent_lamports(&self) -> u64 {
        let gross_rent = SPL_TOKEN_ACCOUNT_RENT_LAMPORTS.saturating_mul(self.new_token_accounts);
        let retained = (1.0 - self.rent_refund_probability.clamp(0.0, 1.0)) * gross_rent as f64;
        retained.round() as u64
    }

    /// Build a complete cost breakdown. The tip is supplied by the planner
    /// because it depends on bundle contention and selected Jito account.
    pub fn breakdown(&self, tip: u64) -> CostBreakdown {
        CostBreakdown {
            tip_lamports: tip,
            priority_fee_lamports: self.priority_fee_lamports,
            base_fee_lamports: self.base_fee_lamports(),
            rent_lamports: self.amortized_rent_lamports(),
            guard_lamports: self.guard_cu_lamports,
            flash_fee_lamports: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Economics {
    pub gross_profit_lamports: i64,
    pub costs: u64,
    pub net_profit_lamports: i64,
}

impl Economics {
    pub fn new(gross_profit_lamports: i64, costs: u64) -> Self {
        Self {
            gross_profit_lamports,
            costs,
            net_profit_lamports: gross_profit_lamports.saturating_sub(costs as i64),
        }
    }

    pub fn passes(&self, min_net_lamports: u64, balance_ok: bool) -> bool {
        balance_ok && self.net_profit_lamports >= min_net_lamports as i64
    }
}

/// Expected value of one attempt. Bundle drops pay zero fees, but the caller
/// may still pass an infrastructure-cost estimate.
pub fn expected_value(
    p_land: f64,
    net_profit_lamports: i64,
    cost_if_not_landed_lamports: u64,
) -> f64 {
    let p = p_land.clamp(0.0, 1.0);
    p * net_profit_lamports as f64 - (1.0 - p) * cost_if_not_landed_lamports as f64
}

/// Optimal input for two constant-product pools.
///
/// Pool 1 is the buy venue (`x1` quote, `y1` base); pool 2 is the sell venue
/// (`x2` quote, `y2` base). Fees are given in basis points.
///
/// Returns `None` when the spread does not cover both fees.
pub fn optimal_two_pool_input(
    x1: u64,
    y1: u64,
    fee1_bps: u64,
    x2: u64,
    y2: u64,
    fee2_bps: u64,
) -> Option<u64> {
    if x1 == 0 || y1 == 0 || x2 == 0 || y2 == 0 || fee1_bps >= 10_000 || fee2_bps >= 10_000 {
        return None;
    }
    let gamma1 = 1.0 - fee1_bps as f64 / 10_000.0;
    let gamma2 = 1.0 - fee2_bps as f64 / 10_000.0;
    let k = gamma1 * gamma2 * x2 as f64 * y1 as f64;
    let m = x1 as f64 * y2 as f64;
    let n = gamma1 * (y2 as f64 + gamma2 * y1 as f64);
    if n <= 0.0 || k <= m || !k.is_finite() || !m.is_finite() {
        return None;
    }
    let a = ((k * m).sqrt() - m) / n;
    if !a.is_finite() || a <= 0.0 {
        return None;
    }
    Some(a.floor() as u64)
}

/// Profit of the two-pool plan at input `a` (all raw units of the quote mint).
pub fn two_pool_profit(
    a: u64,
    x1: u64,
    y1: u64,
    fee1_bps: u64,
    x2: u64,
    y2: u64,
    fee2_bps: u64,
) -> i128 {
    let gamma1 = (10_000u128 - fee1_bps as u128) as f64 / 10_000.0;
    let gamma2 = (10_000u128 - fee2_bps as u128) as f64 / 10_000.0;
    let amount_in = a as f64 * gamma1;
    let base_out = amount_in * y1 as f64 / (x1 as f64 + amount_in);
    let quote_in = base_out * gamma2;
    let quote_out = quote_in * x2 as f64 / (y2 as f64 + quote_in);
    (quote_out - a as f64).round() as i128
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rent_constants_match_known_values() {
        assert_eq!(rent_exempt_minimum(165), SPL_TOKEN_ACCOUNT_RENT_LAMPORTS);
        assert_eq!(rent_exempt_minimum(80), DURABLE_NONCE_RENT_LAMPORTS);
    }

    #[test]
    fn tip_is_clamped() {
        let policy = TipPolicy {
            alpha: 0.5,
            floor_lamports: 1_000,
            cap_lamports: 10_000,
        };
        assert_eq!(policy.tip_for(1_000), 1_000);
        assert_eq!(policy.tip_for(10_000), 5_000);
        assert_eq!(policy.tip_for(100_000), 10_000);
        assert_eq!(policy.tip_for(-5), 1_000);
    }

    #[test]
    fn repay_only_is_not_a_profit_guard() {
        // One lamport of gross profit cannot pay a 50k tip. This is exactly the
        // mistake the on-chain guard must prevent.
        let model = CostModel {
            priority_fee_lamports: 1_000,
            ..CostModel::default()
        };
        let costs = model.breakdown(50_000);
        let econ = Economics::new(1, costs.total());
        assert!(econ.net_profit_lamports < 0);
        assert!(!econ.passes(20_000, true));
    }

    #[test]
    fn base_fee_and_rent() {
        let model = CostModel {
            n_signatures: 2,
            new_token_accounts: 1,
            rent_refund_probability: 0.5,
            ..CostModel::default()
        };
        assert_eq!(model.base_fee_lamports(), 10_000);
        assert_eq!(model.amortized_rent_lamports(), 1_019_640);
    }

    #[test]
    fn optimal_size_finds_profitable_spread() {
        // Pool 1 is cheap, pool 2 is rich.
        let a = optimal_two_pool_input(1_000_000, 1_000_000, 30, 1_100_000, 1_000_000, 30)
            .expect("spread should exist");
        assert!(a > 0);
        let profit = two_pool_profit(a, 1_000_000, 1_000_000, 30, 1_100_000, 1_000_000, 30);
        assert!(profit > 0, "profit was {profit}");
        // One lamport more should not be materially better; curve is concave.
        let profit_plus = two_pool_profit(
            a + 1_000,
            1_000_000,
            1_000_000,
            30,
            1_100_000,
            1_000_000,
            30,
        );
        assert!(profit_plus <= profit + (profit.abs() / 20 + 2));
    }

    #[test]
    fn no_arb_when_fees_exceed_spread() {
        assert!(
            optimal_two_pool_input(1_000_000, 1_000_000, 30, 1_000_001, 1_000_000, 30).is_none()
        );
    }

    #[test]
    fn expected_value_math() {
        assert!((expected_value(0.1, 100_000, 0) - 10_000.0).abs() < 1e-9);
        assert_eq!(expected_value(0.0, 100_000, 5_000), -5_000.0);
    }
}
