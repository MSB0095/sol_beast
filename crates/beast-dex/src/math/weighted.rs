//! Weighted-pool math (Balancer-style), used by Stabble weighted pools and
//! similar multi-asset AMMs.

use crate::DexError;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeightedQuote {
    pub amount_out: u64,
    pub fee_paid: u64,
}

pub fn quote_exact_in(
    reserve_in: u64,
    reserve_out: u64,
    weight_in: f64,
    weight_out: f64,
    fee_bps: u32,
    amount_in: u64,
) -> Result<WeightedQuote, DexError> {
    if reserve_in == 0 || reserve_out == 0 {
        return Err(DexError::Math("zero reserve".into()));
    }
    if amount_in == 0 {
        return Err(DexError::Math("zero input".into()));
    }
    if fee_bps >= 10_000
        || !weight_in.is_finite()
        || !weight_out.is_finite()
        || weight_in <= 0.0
        || weight_out <= 0.0
    {
        return Err(DexError::Math("invalid fee or weights".into()));
    }
    let fee = (amount_in as f64 * fee_bps as f64 / 10_000.0).ceil();
    let net_in = amount_in as f64 - fee;
    if net_in <= 0.0 {
        return Err(DexError::Math("fee consumes input".into()));
    }
    let ratio = reserve_in as f64 / (reserve_in as f64 + net_in);
    let exponent = weight_in / weight_out;
    let out = reserve_out as f64 * (1.0 - ratio.powf(exponent));
    let out = out.floor();
    if out < 1.0 || out > u64::MAX as f64 {
        return Err(DexError::Math("weighted output out of range".into()));
    }
    Ok(WeightedQuote {
        amount_out: out as u64,
        fee_paid: fee.min(u64::MAX as f64) as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balanced_50_50_is_constant_product() {
        let weighted = quote_exact_in(1_000_000, 1_000_000, 0.5, 0.5, 30, 10_000).unwrap();
        let cp = crate::math::cpmm::quote_exact_in(1_000_000, 1_000_000, 30, 10_000).unwrap();
        assert!((weighted.amount_out as i64 - cp.amount_out as i64).abs() <= 2);
    }

    #[test]
    fn rejects_bad_weights() {
        assert!(quote_exact_in(1, 1, 0.0, 1.0, 30, 1).is_err());
        assert!(quote_exact_in(1, 1, f64::NAN, 1.0, 30, 1).is_err());
    }
}
