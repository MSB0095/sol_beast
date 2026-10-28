//! A venue-neutral view of one constant-product pool.
//!
//! The driver used to be Raydium-CPMM-only, and it priced CPMM legs with the
//! trade fee rounded to whole basis points. Both of those lose money:
//!
//! * rounding the fee to bps truncates a 1e-6-unit rate, so the local quote is
//!   a few lamports optimistic on every leg, which is precisely the amount by
//!   which a marginal cycle fails on-chain;
//! * a CPMM-only search cannot see the venue that holds the depth.
//!
//! This module keeps each venue's own exact formula and exposes one interface,
//! so a cycle can pair a CPMM pool with an AMM v4 pool and still be priced the
//! way each program will price it. Every quote here uses the program's own
//! rounding — ceiling on the fee, floor on the output — because the goal is
//! that the local number and the on-chain number are the same number.

use crate::amm_v4::{self, AmmV4Pool};
use crate::cpmm::{self, CpmmPool};
use crate::{ExecutionError, Result};
use beast_dex::Address;
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;

/// One pool on one venue, plus whatever that venue needs to quote it.
#[derive(Debug, Clone)]
pub enum VenuePool {
    /// Raydium CPMM. The trade fee is the program's 1e-6-unit rate, not bps.
    Cpmm {
        pool: CpmmPool,
        trade_fee_rate: u64,
    },
    /// Raydium AMM v4.
    AmmV4(AmmV4Pool),
}

impl VenuePool {
    /// Human-facing label used in reports.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Cpmm { .. } => "raydium-cpmm",
            Self::AmmV4(_) => "raydium-amm-v4",
        }
    }

    /// Label used in `LegPlan::venue`, matching the registered venue ids.
    pub fn leg_label(&self) -> &'static str {
        match self {
            Self::Cpmm { .. } => "raydium_cpmm",
            Self::AmmV4(_) => "raydium_amm_v4",
        }
    }

    pub fn address(&self) -> Pubkey {
        match self {
            Self::Cpmm { pool, .. } => pool.address,
            Self::AmmV4(pool) => pool.address,
        }
    }

    /// `[token0, token1]` in the pool's own storage order.
    pub fn mints(&self) -> [Pubkey; 2] {
        match self {
            Self::Cpmm { pool, .. } => pool.mints,
            Self::AmmV4(pool) => pool.mints,
        }
    }

    pub fn mint_index(&self, mint: &Pubkey) -> Option<usize> {
        self.mints().iter().position(|candidate| candidate == mint)
    }

    pub fn vaults(&self) -> [Pubkey; 2] {
        match self {
            Self::Cpmm { pool, .. } => pool.vaults,
            Self::AmmV4(pool) => pool.vaults,
        }
    }

    /// Convert a raw vault balance into the reserve the program actually
    /// prices against.
    ///
    /// CPMM subtracts fees it has booked but not yet collected; AMM v4 prices
    /// against the vault balance directly, which the mainnet probe confirmed
    /// to the lamport.
    pub fn effective_reserve(&self, side: usize, raw: u64) -> Option<u64> {
        match self {
            Self::Cpmm { pool, .. } => pool.effective_reserve(side, raw).ok(),
            Self::AmmV4(pool) => Some(pool.effective_reserve(side, raw)),
        }
    }

    /// Exact-in output for one direction, using the venue's own formula.
    pub fn quote(
        &self,
        amount_in: u64,
        reserve_in: u64,
        reserve_out: u64,
    ) -> Result<u64> {
        match self {
            Self::Cpmm {
                pool,
                trade_fee_rate,
            } => cpmm::quote_base_input(amount_in, reserve_in, reserve_out, *trade_fee_rate)
                .map_err(|error| ExecutionError::Unsupported(error.to_string()))
                .and_then(|out| {
                    if out == 0 {
                        Err(ExecutionError::Unsupported(format!(
                            "CPMM pool {} quotes zero output",
                            pool.address
                        )))
                    } else {
                        Ok(out)
                    }
                }),
            Self::AmmV4(pool) => {
                // `quote_base_in` takes the pool for its fee, which is read
                // from the pool account rather than assumed.
                amm_v4::quote_base_in(pool, amount_in, reserve_in, reserve_out)
                    .map_err(|error| ExecutionError::Unsupported(error.to_string()))
            }
        }
    }

    /// Build one exact-in swap instruction for this venue.
    pub fn swap_instruction(
        &self,
        payer: Pubkey,
        input_mint: Pubkey,
        output_mint: Pubkey,
        amount_in: u64,
        minimum_amount_out: u64,
    ) -> Result<Instruction> {
        match self {
            Self::Cpmm { pool, .. } => Ok(cpmm::swap_base_input_instruction(
                pool,
                payer,
                input_mint,
                output_mint,
                amount_in,
                minimum_amount_out,
            )?),
            Self::AmmV4(pool) => Ok(amm_v4::swap_base_in_instruction(
                pool,
                payer,
                input_mint,
                output_mint,
                amount_in,
                minimum_amount_out,
            )?),
        }
    }

    /// Wrap a swap in the Jupiter instruction envelope the composer expects.
    pub fn as_swap_instructions(
        &self,
        instruction: Instruction,
    ) -> Result<beast_dex::jupiter::SwapInstructions> {
        match self {
            Self::Cpmm { .. } => Ok(cpmm::as_swap_instructions(instruction)?),
            Self::AmmV4(_) => Ok(amm_v4::as_swap_instructions(instruction)?),
        }
    }

    /// The token program both venues use for the given side.
    ///
    /// AMM v4 is classic SPL Token only. CPMM stores its two token programs, so
    /// a Token-2022 side is visible here rather than assumed away.
    pub fn token_program(&self, side: usize) -> Pubkey {
        match self {
            Self::Cpmm { pool, .. } => pool.token_programs[side],
            Self::AmmV4(_) => spl_token::id(),
        }
    }

    /// Whether every side of this pool is classic SPL Token.
    ///
    /// The flash cycle unwraps WSOL with `close_account` and moves the middle
    /// mint with plain transfers, so a Token-2022 side with transfer fees would
    /// make the composed balance arithmetic wrong. Those pools are skipped
    /// instead of being quoted optimistically.
    pub fn is_classic_spl(&self) -> bool {
        let classic = spl_token::id();
        self.token_program(0) == classic && self.token_program(1) == classic
    }

    /// Vault addresses as RPC addresses, in pool order.
    pub fn vault_addresses(&self) -> [Address; 2] {
        let vaults = self.vaults();
        [
            Address::from_slice(vaults[0].as_ref()).expect("32-byte pubkey"),
            Address::from_slice(vaults[1].as_ref()).expect("32-byte pubkey"),
        ]
    }
}
