//! Two-asset StableSwap math (Saber, SPL Token Swap stable curve, Stabble
//! stable pools). The invariant is the standard Curve `A*n^n` formulation.
//!
//! Amounts are raw token units and must be normalised to the same scale by the
//! caller (stable pools almost always use equal decimals).

use crate::DexError;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StableQuote {
    pub amount_out: u64,
    pub fee_paid: u64,
}

fn compute_d(amp: u64, x: f64, y: f64) -> Option<f64> {
    if x <= 0.0 || y <= 0.0 || !x.is_finite() || !y.is_finite() {
        return None;
    }
    let ann = amp as f64 * 4.0; // n^n, n = 2
    if ann <= 1.0 {
        return None;
    }
    let sum = x + y;
    let mut d = sum;
    for _ in 0..128 {
        let d_p = d * d * d / (4.0 * x * y);
        let numerator = (ann * sum + 2.0 * d_p) * d;
        let denominator = (ann - 1.0) * d + 3.0 * d_p;
        if denominator <= 0.0 || !numerator.is_finite() || !denominator.is_finite() {
            return None;
        }
        let next = numerator / denominator;
        if (next - d).abs() < 0.5 {
            d = next;
            break;
        }
        d = next;
    }
    (d > 0.0 && d.is_finite()).then_some(d)
}

fn get_y(amp: u64, x_new: f64, d: f64) -> Option<f64> {
    if x_new <= 0.0 || d <= 0.0 {
        return None;
    }
    let ann = amp as f64 * 4.0;
    // ann*y^2 + (ann*x + D - ann*D)*y - D^3/(4*x) = 0
    let a = ann;
    let b = ann * x_new + d - ann * d;
    let c = -(d * d * d) / (4.0 * x_new);
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 || !discriminant.is_finite() {
        return None;
    }
    let y = (-b + discriminant.sqrt()) / (2.0 * a);
    (y > 0.0 && y.is_finite()).then_some(y)
}

pub fn quote_exact_in(
    reserve_in: u64,
    reserve_out: u64,
    amp: u64,
    fee_bps: u32,
    amount_in: u64,
) -> Result<StableQuote, DexError> {
    if amount_in == 0 {
        return Err(DexError::Math("zero input".into()));
    }
    if fee_bps >= 10_000 {
        return Err(DexError::Math("fee >= 100%".into()));
    }
    let x = reserve_in as f64;
    let y = reserve_out as f64;
    let d = compute_d(amp, x, y).ok_or_else(|| DexError::Math("stable D failed".into()))?;
    let x_new = x + amount_in as f64;
    let y_new = get_y(amp, x_new, d).ok_or_else(|| DexError::Math("stable Y failed".into()))?;
    let dy = (y - y_new).max(0.0);
    let fee = (dy * fee_bps as f64 / 10_000.0).ceil();
    let net = (dy - fee).max(0.0);
    if net < 1.0 || net > u64::MAX as f64 {
        return Err(DexError::Math("stable output out of range".into()));
    }
    Ok(StableQuote {
        amount_out: net as u64,
        fee_paid: fee.min(u64::MAX as f64) as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balanced_pool_is_near_one_to_one() {
        let quote = quote_exact_in(1_000_000_000, 1_000_000_000, 100, 4, 1_000_000).unwrap();
        assert!(quote.amount_out >= 999_000);
        assert!(quote.amount_out <= 1_000_000);
    }

    #[test]
    fn imbalance_moves_price() {
        let quote = quote_exact_in(1_000_000_000, 2_000_000_000, 100, 4, 1_000_000).unwrap();
        assert!(quote.amount_out > 900_000);
        assert!(quote.amount_out < 1_100_000);
    }

    #[test]
    fn rejects_zero_reserve() {
        assert!(quote_exact_in(0, 1, 100, 4, 1).is_err());
    }
}
