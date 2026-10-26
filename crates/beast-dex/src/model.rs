//! Venue-agnostic pool snapshots and quote results.

use crate::address::Address;
use crate::math::{clmm, cpmm, stableswap, weighted};
use crate::DexError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PoolModel {
    ConstantProduct {
        reserve_a: u64,
        reserve_b: u64,
    },
    StableSwap {
        reserve_a: u64,
        reserve_b: u64,
        amp: u64,
    },
    Weighted {
        reserve_a: u64,
        reserve_b: u64,
        weight_a: f64,
        weight_b: f64,
    },
    /// Concentrated liquidity. The quote is a current-range approximation; it
    /// must be validated against a simulator/Jupiter before a live attempt.
    Concentrated {
        sqrt_price_x64: u128,
        liquidity: u128,
        tick_spacing: u16,
    },
    /// Meteora DLMM. Reserves are optional because the authoritative bin arrays
    /// live in separate accounts. Without them this model is decode-only.
    Dlmm {
        active_id: i32,
        bin_step: u16,
        reserve_a: Option<u64>,
        reserve_b: Option<u64>,
    },
    /// A venue whose quote is only available from an external oracle (Jupiter,
    /// RFQ, CLOB book, prop AMM). Never used for local opportunity math.
    External {
        note: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolSnapshot {
    pub address: Address,
    pub venue: String,
    pub program_id: Address,
    pub mint_a: Address,
    pub mint_b: Address,
    pub vault_a: Address,
    pub vault_b: Address,
    pub decimals_a: u8,
    pub decimals_b: u8,
    pub fee_bps: u32,
    pub model: PoolModel,
    pub slot: u64,
    pub confidence: f32,
}

impl PoolSnapshot {
    pub fn mints(&self) -> [Address; 2] {
        [self.mint_a, self.mint_b]
    }

    pub fn contains_mint(&self, mint: Address) -> bool {
        mint == self.mint_a || mint == self.mint_b
    }

    pub fn other_mint(&self, mint: Address) -> Option<Address> {
        if mint == self.mint_a {
            Some(self.mint_b)
        } else if mint == self.mint_b {
            Some(self.mint_a)
        } else {
            None
        }
    }

    pub fn decimals(&self, mint: Address) -> Option<u8> {
        if mint == self.mint_a {
            Some(self.decimals_a)
        } else if mint == self.mint_b {
            Some(self.decimals_b)
        } else {
            None
        }
    }

    pub fn is_locally_quotable(&self) -> bool {
        !matches!(self.model, PoolModel::External { .. })
    }

    /// Quote an exact input. `mint_in` selects the direction.
    pub fn quote(&self, mint_in: Address, amount_in: u64) -> Result<Quote, DexError> {
        if amount_in == 0 {
            return Err(DexError::Math("zero input".into()));
        }
        if mint_in == self.mint_a {
            self.quote_direction(true, amount_in)
        } else if mint_in == self.mint_b {
            self.quote_direction(false, amount_in)
        } else {
            Err(DexError::Route(format!(
                "mint {mint_in} is not in pool {}",
                self.address
            )))
        }
    }

    fn quote_direction(&self, a_to_b: bool, amount_in: u64) -> Result<Quote, DexError> {
        let (amount_out, fee_paid, note, confidence) = match &self.model {
            PoolModel::ConstantProduct {
                reserve_a,
                reserve_b,
            } => {
                let (rin, rout) = if a_to_b {
                    (*reserve_a, *reserve_b)
                } else {
                    (*reserve_b, *reserve_a)
                };
                let out = cpmm::quote_exact_in(rin, rout, self.fee_bps, amount_in)?;
                (
                    out.amount_out,
                    out.fee_paid,
                    "constant_product",
                    self.confidence,
                )
            }
            PoolModel::StableSwap {
                reserve_a,
                reserve_b,
                amp,
            } => {
                let (rin, rout) = if a_to_b {
                    (*reserve_a, *reserve_b)
                } else {
                    (*reserve_b, *reserve_a)
                };
                let out = stableswap::quote_exact_in(rin, rout, *amp, self.fee_bps, amount_in)?;
                (out.amount_out, out.fee_paid, "stable_swap", self.confidence)
            }
            PoolModel::Weighted {
                reserve_a,
                reserve_b,
                weight_a,
                weight_b,
            } => {
                let (rin, rout, win, wout) = if a_to_b {
                    (*reserve_a, *reserve_b, *weight_a, *weight_b)
                } else {
                    (*reserve_b, *reserve_a, *weight_b, *weight_a)
                };
                let out = weighted::quote_exact_in(rin, rout, win, wout, self.fee_bps, amount_in)?;
                (out.amount_out, out.fee_paid, "weighted", self.confidence)
            }
            PoolModel::Concentrated {
                sqrt_price_x64,
                liquidity,
                ..
            } => {
                let out = clmm::quote_exact_in_approx(
                    *sqrt_price_x64,
                    *liquidity,
                    self.fee_bps,
                    a_to_b,
                    amount_in,
                )?;
                (
                    out.amount_out,
                    out.fee_paid,
                    "clmm_approx",
                    self.confidence * 0.85,
                )
            }
            PoolModel::Dlmm {
                reserve_a,
                reserve_b,
                ..
            } => match (reserve_a, reserve_b) {
                (Some(ra), Some(rb)) => {
                    let (rin, rout) = if a_to_b { (*ra, *rb) } else { (*rb, *ra) };
                    let out = cpmm::quote_exact_in(rin, rout, self.fee_bps, amount_in)?;
                    (
                        out.amount_out,
                        out.fee_paid,
                        "dlmm_active_bin_approx",
                        self.confidence * 0.6,
                    )
                }
                _ => {
                    return Err(DexError::Unsupported(
                        "DLMM quote requires bin reserves or Jupiter".into(),
                    ))
                }
            },
            PoolModel::External { note } => {
                return Err(DexError::Unsupported(format!(
                    "external-only venue: {note}"
                )))
            }
        };
        if amount_out == 0 {
            return Err(DexError::Math("quote produced zero output".into()));
        }
        let impact = price_impact_bps(amount_in, amount_out, &self.model, a_to_b);
        let (mint_in, mint_out) = if a_to_b {
            (self.mint_a, self.mint_b)
        } else {
            (self.mint_b, self.mint_a)
        };
        Ok(Quote {
            pool: self.address,
            venue: self.venue.clone(),
            mint_in,
            mint_out,
            amount_in,
            amount_out,
            fee_paid,
            price_impact_bps: impact,
            model: note,
            confidence,
        })
    }

    pub fn max_input_raw(&self, mint_in: Address, fraction_bps: u32) -> u64 {
        let fraction = fraction_bps.min(10_000) as u128;
        let reserve = match (&self.model, mint_in == self.mint_a) {
            (PoolModel::ConstantProduct { reserve_a, .. }, true) => *reserve_a,
            (PoolModel::ConstantProduct { reserve_b, .. }, false) => *reserve_b,
            (PoolModel::StableSwap { reserve_a, .. }, true) => *reserve_a,
            (PoolModel::StableSwap { reserve_b, .. }, false) => *reserve_b,
            (PoolModel::Weighted { reserve_a, .. }, true) => *reserve_a,
            (PoolModel::Weighted { reserve_b, .. }, false) => *reserve_b,
            (
                PoolModel::Concentrated {
                    sqrt_price_x64,
                    liquidity,
                    ..
                },
                true,
            ) => clmm::virtual_reserve_a(*sqrt_price_x64, *liquidity).min(u64::MAX as f64) as u64,
            (
                PoolModel::Concentrated {
                    sqrt_price_x64,
                    liquidity,
                    ..
                },
                false,
            ) => clmm::virtual_reserve_b(*sqrt_price_x64, *liquidity).min(u64::MAX as f64) as u64,
            (PoolModel::Dlmm { reserve_a, .. }, true) => reserve_a.unwrap_or(0),
            (PoolModel::Dlmm { reserve_b, .. }, false) => reserve_b.unwrap_or(0),
            _ => 0,
        };
        ((reserve as u128 * fraction) / 10_000).min(u64::MAX as u128) as u64
    }

    /// Raw spot price: token B per token A (before decimals).
    pub fn spot_price_raw(&self) -> Option<f64> {
        match &self.model {
            PoolModel::ConstantProduct {
                reserve_a,
                reserve_b,
            } => (reserve_a > &0).then(|| *reserve_b as f64 / *reserve_a as f64),
            PoolModel::StableSwap {
                reserve_a,
                reserve_b,
                ..
            } => (reserve_a > &0).then(|| *reserve_b as f64 / *reserve_a as f64),
            PoolModel::Weighted {
                reserve_a,
                reserve_b,
                ..
            } => (reserve_a > &0).then(|| *reserve_b as f64 / *reserve_a as f64),
            PoolModel::Concentrated { sqrt_price_x64, .. } => {
                Some(clmm::sqrt_price_x64_to_price(*sqrt_price_x64))
            }
            PoolModel::Dlmm {
                active_id,
                bin_step,
                ..
            } => Some(crate::math::dlmm::price_from_id(*active_id, *bin_step)),
            PoolModel::External { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Quote {
    pub pool: Address,
    pub venue: String,
    pub mint_in: Address,
    pub mint_out: Address,
    pub amount_in: u64,
    pub amount_out: u64,
    pub fee_paid: u64,
    pub price_impact_bps: u32,
    pub model: &'static str,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteLeg {
    pub pool: Address,
    pub venue: String,
    pub mint_in: Address,
    pub mint_out: Address,
    pub amount_in: u64,
    pub amount_out: u64,
    pub fee_paid: u64,
    pub price_impact_bps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteQuote {
    pub start_mint: Address,
    pub end_mint: Address,
    pub amount_in: u64,
    pub amount_out: u64,
    pub gross_profit: i64,
    pub legs: Vec<RouteLeg>,
    pub min_confidence: f32,
    pub max_price_impact_bps: u32,
}

impl RouteQuote {
    pub fn pool_ids(&self) -> Vec<String> {
        self.legs.iter().map(|leg| leg.pool.to_string()).collect()
    }
}

fn price_impact_bps(amount_in: u64, amount_out: u64, model: &PoolModel, a_to_b: bool) -> u32 {
    let Some(spot) = spot_for_direction(model, a_to_b) else {
        return 0;
    };
    if spot <= 0.0 || amount_in == 0 {
        return 0;
    }
    let execution = amount_out as f64 / amount_in as f64;
    let impact = (1.0 - execution / spot).max(0.0) * 10_000.0;
    impact.min(u32::MAX as f64).round() as u32
}

fn spot_for_direction(model: &PoolModel, a_to_b: bool) -> Option<f64> {
    match model {
        PoolModel::ConstantProduct {
            reserve_a,
            reserve_b,
        } => {
            if a_to_b {
                (*reserve_a > 0).then(|| *reserve_b as f64 / *reserve_a as f64)
            } else {
                (*reserve_b > 0).then(|| *reserve_a as f64 / *reserve_b as f64)
            }
        }
        PoolModel::StableSwap {
            reserve_a,
            reserve_b,
            ..
        } => {
            if a_to_b {
                (*reserve_a > 0).then(|| *reserve_b as f64 / *reserve_a as f64)
            } else {
                (*reserve_b > 0).then(|| *reserve_a as f64 / *reserve_b as f64)
            }
        }
        PoolModel::Weighted {
            reserve_a,
            reserve_b,
            ..
        } => {
            if a_to_b {
                (*reserve_a > 0).then(|| *reserve_b as f64 / *reserve_a as f64)
            } else {
                (*reserve_b > 0).then(|| *reserve_a as f64 / *reserve_b as f64)
            }
        }
        PoolModel::Concentrated { sqrt_price_x64, .. } => {
            let p = clmm::sqrt_price_x64_to_price(*sqrt_price_x64);
            if a_to_b {
                Some(p)
            } else if p > 0.0 {
                Some(1.0 / p)
            } else {
                None
            }
        }
        PoolModel::Dlmm {
            active_id,
            bin_step,
            ..
        } => {
            let p = crate::math::dlmm::price_from_id(*active_id, *bin_step);
            if a_to_b {
                Some(p)
            } else if p > 0.0 {
                Some(1.0 / p)
            } else {
                None
            }
        }
        PoolModel::External { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(n: u8) -> Address {
        Address([n; 32])
    }

    fn cp_pool() -> PoolSnapshot {
        PoolSnapshot {
            address: address(1),
            venue: "test".into(),
            program_id: address(2),
            mint_a: address(3),
            mint_b: address(4),
            vault_a: address(5),
            vault_b: address(6),
            decimals_a: 9,
            decimals_b: 6,
            fee_bps: 30,
            model: PoolModel::ConstantProduct {
                reserve_a: 1_000_000_000,
                reserve_b: 1_000_000_000,
            },
            slot: 1,
            confidence: 1.0,
        }
    }

    #[test]
    fn quote_both_directions() {
        let pool = cp_pool();
        let a_to_b = pool.quote(pool.mint_a, 1_000_000).unwrap();
        let b_to_a = pool.quote(pool.mint_b, 1_000_000).unwrap();
        assert!(a_to_b.amount_out < 1_000_000);
        assert!(b_to_a.amount_out < 1_000_000);
        assert_eq!(a_to_b.mint_out, pool.mint_b);
        assert_eq!(b_to_a.mint_out, pool.mint_a);
    }

    #[test]
    fn external_is_not_quotable() {
        let mut pool = cp_pool();
        pool.model = PoolModel::External {
            note: "prop amm".into(),
        };
        assert!(pool.quote(pool.mint_a, 1).is_err());
        assert!(!pool.is_locally_quotable());
    }
}
