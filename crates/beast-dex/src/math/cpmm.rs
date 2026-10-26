//! Constant-product (x*y=k) exact-input math used by Raydium AMM/CPMM,
//! PumpSwap, Meteora DAMM v2 and most long-tail AMMs.

use super::div_ceil;
use crate::DexError;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CpmmQuote {
    pub amount_out: u64,
    pub fee_paid: u64,
}

/// Uniswap-v2 style exact input with a ceiling fee, matching Solana AMMs that
/// deduct the fee from the input before applying `x*y=k`.
pub fn quote_exact_in(
    reserve_in: u64,
    reserve_out: u64,
    fee_bps: u32,
    amount_in: u64,
) -> Result<CpmmQuote, DexError> {
    if reserve_in == 0 || reserve_out == 0 {
        return Err(DexError::Math("zero reserve".into()));
    }
    if amount_in == 0 {
        return Err(DexError::Math("zero input".into()));
    }
    if fee_bps >= 10_000 {
        return Err(DexError::Math("fee >= 100%".into()));
    }
    let fee = div_ceil(amount_in as u128 * fee_bps as u128, 10_000)
        .ok_or_else(|| DexError::Math("fee overflow".into()))? as u64;
    let net_in = amount_in
        .checked_sub(fee)
        .ok_or_else(|| DexError::Math("fee exceeds input".into()))?;
    let numerator = (reserve_out as u128)
        .checked_mul(net_in as u128)
        .ok_or_else(|| DexError::Math("output overflow".into()))?;
    let denominator = (reserve_in as u128)
        .checked_add(net_in as u128)
        .ok_or_else(|| DexError::Math("input overflow".into()))?;
    let amount_out = numerator / denominator;
    if amount_out == 0 || amount_out > u64::MAX as u128 {
        return Err(DexError::Math("output out of range".into()));
    }
    Ok(CpmmQuote {
        amount_out: amount_out as u64,
        fee_paid: fee,
    })
}

/// Exact output. Returns the required input including fee.
pub fn quote_exact_out(
    reserve_in: u64,
    reserve_out: u64,
    fee_bps: u32,
    amount_out: u64,
) -> Result<u64, DexError> {
    if reserve_out <= amount_out {
        return Err(DexError::Math("insufficient output reserve".into()));
    }
    if fee_bps >= 10_000 {
        return Err(DexError::Math("fee >= 100%".into()));
    }
    let numerator = reserve_in as u128 * amount_out as u128;
    let denominator = (reserve_out - amount_out) as u128;
    let net_in =
        div_ceil(numerator, denominator).ok_or_else(|| DexError::Math("input overflow".into()))?;
    let gross = div_ceil(
        net_in
            .checked_mul(10_000)
            .ok_or_else(|| DexError::Math("gross input overflow".into()))?,
        10_000 - fee_bps as u128,
    )
    .ok_or_else(|| DexError::Math("gross input overflow".into()))?;
    if gross > u64::MAX as u128 {
        return Err(DexError::Math("gross input out of range".into()));
    }
    Ok(gross as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_known_constant_product_result() {
        // 1_000_000 in, 30 bps fee => 997_000 net; 1e12 reserves.
        let quote = quote_exact_in(1_000_000_000_000, 1_000_000_000_000, 30, 1_000_000).unwrap();
        assert_eq!(quote.fee_paid, 3_000);
        assert_eq!(quote.amount_out, 996_999);
    }

    #[test]
    fn tiny_trade_never_rounds_up() {
        assert!(quote_exact_in(1_000_000, 1_000_000, 30, 1).is_err());
    }

    #[test]
    fn exact_out_inverts_exact_in_approximately() {
        let amount_out = 500_000;
        let amount_in = quote_exact_out(1_000_000_000, 1_000_000_000, 30, amount_out).unwrap();
        let quoted = quote_exact_in(1_000_000_000, 1_000_000_000, 30, amount_in).unwrap();
        assert!(quoted.amount_out >= amount_out);
        assert!(quoted.amount_out <= amount_out + 2);
    }

    #[test]
    fn rejects_bad_inputs() {
        assert!(quote_exact_in(0, 1, 30, 1).is_err());
        assert!(quote_exact_in(1, 1, 30, 0).is_err());
        assert!(quote_exact_in(1, 1, 10_000, 1).is_err());
    }
}
