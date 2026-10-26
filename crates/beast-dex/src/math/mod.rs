pub mod clmm;
pub mod cpmm;
pub mod dlmm;
pub mod stableswap;
pub mod weighted;

/// Ceiling division for u128.
pub(crate) fn div_ceil(numerator: u128, denominator: u128) -> Option<u128> {
    if denominator == 0 {
        return None;
    }
    numerator
        .checked_add(denominator.checked_sub(1)?)?
        .checked_div(denominator)
}
