//! Concentrated-liquidity approximations for Radar.
//!
//! The full CLMM swap walks initialized ticks and crosses ranges. Radar only
//! needs a conservative estimate, so this module uses the virtual reserves of
//! the current range and applies the pool fee. Every live attempt MUST be
//! re-quoted and simulated; `confidence` is lowered by the caller.

use super::cpmm;
use crate::DexError;

pub const Q64: f64 = 18_446_744_073_709_551_616.0; // 2^64

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClmmQuote {
    pub amount_out: u64,
    pub fee_paid: u64,
}

pub fn sqrt_price_x64_to_price(sqrt_price_x64: u128) -> f64 {
    let sqrt = sqrt_price_x64 as f64 / Q64;
    sqrt * sqrt
}

pub fn virtual_reserve_a(sqrt_price_x64: u128, liquidity: u128) -> f64 {
    let sqrt = sqrt_price_x64 as f64 / Q64;
    if sqrt <= 0.0 {
        0.0
    } else {
        liquidity as f64 / sqrt
    }
}

pub fn virtual_reserve_b(sqrt_price_x64: u128, liquidity: u128) -> f64 {
    let sqrt = sqrt_price_x64 as f64 / Q64;
    liquidity as f64 * sqrt
}

pub fn quote_exact_in_approx(
    sqrt_price_x64: u128,
    liquidity: u128,
    fee_bps: u32,
    a_to_b: bool,
    amount_in: u64,
) -> Result<ClmmQuote, DexError> {
    if sqrt_price_x64 == 0 || liquidity == 0 {
        return Err(DexError::Math("empty concentrated pool".into()));
    }
    let ra = virtual_reserve_a(sqrt_price_x64, liquidity);
    let rb = virtual_reserve_b(sqrt_price_x64, liquidity);
    if ra < 1.0 || rb < 1.0 {
        return Err(DexError::Math("concentrated reserves too small".into()));
    }
    let (rin, rout) = if a_to_b { (ra, rb) } else { (rb, ra) };
    let rin = rin.min(u64::MAX as f64) as u64;
    let rout = rout.min(u64::MAX as f64) as u64;
    let quote = cpmm::quote_exact_in(rin, rout, fee_bps, amount_in)?;
    Ok(ClmmQuote {
        amount_out: quote.amount_out,
        fee_paid: quote.fee_paid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_one_at_q64() {
        assert!((sqrt_price_x64_to_price(1u128 << 64) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn virtual_reserves_are_symmetric_at_price_one() {
        let l = 1_000_000_000u128;
        let ra = virtual_reserve_a(1u128 << 64, l);
        let rb = virtual_reserve_b(1u128 << 64, l);
        assert!((ra - rb).abs() < 1.0);
    }

    #[test]
    fn quote_is_less_than_input() {
        let quote =
            quote_exact_in_approx(1u128 << 64, 1_000_000_000_000, 30, true, 1_000_000).unwrap();
        assert!(quote.amount_out < 1_000_000);
        assert!(quote.amount_out > 990_000);
    }
}
