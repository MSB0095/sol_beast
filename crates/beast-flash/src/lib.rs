//! Flash-loan providers for Solana.
//!
//! Solana has no EVM-style callback flash loans. Capital-free arbitrage here
//! means borrowing from a lending protocol and repaying in the *same
//! transaction*, which is strictly atomic: if the repayment does not clear, the
//! whole transaction reverts and nothing is lost but the network fee.
//!
//! Every constant in this crate was derived from **on-chain ground truth**, not
//! from a vendor SDK, because the protocols that publish an interface do so
//! under licences that cannot be vendored into this workspace. Each adapter
//! documents the reference transaction and the live simulation that validated
//! it.
//!
//! ## What "verified" means here
//! For a provider to be enabled it must pass all of:
//! 1. its program ID is executable on the target cluster;
//! 2. its borrow/repay discriminators are accepted by the program;
//! 3. its instruction account order matches a real landed transaction;
//! 4. a borrow+repay round trip simulates successfully on-chain.
//!
//! A provider missing any of those is registered but `enabled() == false`, so
//! the radar can never build an unvalidated flash cycle.

use beast_dex::Address;
use serde::{Deserialize, Serialize};
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;
use thiserror::Error;

pub mod kamilend;
pub use kamilend::{derive_market_authority, KaminoFlashProvider, KAMINO_KLEND_PROGRAM_ID};

/// Jito bundles cap at five transactions; a flash cycle needs the composed
/// cycle, the guard and the tip, so four swap legs is the safe ceiling.
pub const MAX_FLASH_CYCLE_LEGS: usize = 4;

#[derive(Debug, Error)]
pub enum FlashError {
    #[error("provider {0} is not enabled on this cluster")]
    NotEnabled(String),
    #[error("provider {0} is unknown")]
    UnknownProvider(String),
    #[error("configuration: {0}")]
    Config(String),
    #[error("rpc: {0}")]
    Rpc(#[from] beast_rpc::RpcError),
    #[error("reserve account decode failed: {0}")]
    Reserve(String),
    #[error("program id parse failed: {0}")]
    Pubkey(String),
    #[error("no reserve configured for mint {0}")]
    NoReserve(String),
    #[error("no active reserve with liquidity for mint {0}")]
    NoLiquidity(String),
    #[error("insufficient available liquidity: available {available}, requested {requested}")]
    InsufficientLiquidity { available: u64, requested: u64 },
    #[error("flash fee of {fee_bits} (2^60 scale) is disabled or too high")]
    FeeDisabled { fee_bits: u64 },
    #[error("arithmetic overflow computing the flash repayment")]
    Overflow,
    #[error("transaction assembly failed: {0}")]
    Transaction(String),
}

pub type Result<T> = std::result::Result<T, FlashError>;

/// A pool of borrowable liquidity, resolved on-chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlashReserve {
    pub mint: Address,
    pub reserve: Address,
    pub supply_vault: Address,
    pub fee_vault: Address,
    pub lending_market: Address,
    /// Derived from `["lma", lending_market]` and pinned by a unit test against
    /// the authority observed in a real transaction.
    pub lending_market_authority: Address,
    /// SPL Token or Token-2022, read from the reserve account.
    pub token_program: Address,
    /// Flash fee as a 2^60 fixed-point fraction. `u64::MAX` means disabled.
    pub flash_fee_bits: u64,
    /// Borrowable amount right now.
    pub available: u64,
    pub decimals: u8,
    /// Reserve status: 0 is active on Kamino.
    pub status: u8,
}

impl FlashReserve {
    /// The exact fee Kamino charges for `amount`, rounded **up**.
    ///
    /// The fee is stored as a 2^60 fixed-point fraction, not basis points:
    /// the observed main-market value `11_529_215_046_068` is exactly
    /// `floor(2^60 / 100_000)`, i.e. 10 ppm. A 1 SOL borrow was simulated and
    /// charged 10,000 lamports; 234,451,759 lamports was charged 2,345.
    pub fn flash_fee(&self, amount: u64) -> Result<u64> {
        if self.flash_fee_bits == u64::MAX {
            return Err(FlashError::FeeDisabled {
                fee_bits: self.flash_fee_bits,
            });
        }
        let scale = 1u128 << 60;
        let product = (amount as u128)
            .checked_mul(self.flash_fee_bits as u128)
            .ok_or(FlashError::Overflow)?;
        let fee = product.checked_add(scale - 1).ok_or(FlashError::Overflow)? / scale;
        u64::try_from(fee).map_err(|_| FlashError::Overflow)
    }

    /// Principal plus the protocol fee. The repay instruction itself takes the
    /// **principal**; this value is what the final swap must return.
    pub fn repayment_amount(&self, amount: u64) -> Result<u64> {
        amount
            .checked_add(self.flash_fee(amount)?)
            .ok_or(FlashError::Overflow)
    }

    /// Whether this reserve can currently serve a flash loan.
    pub fn flash_enabled(&self) -> bool {
        self.status == 0 && self.flash_fee_bits != u64::MAX && self.available > 0
    }
}

pub fn pubkey(address: &Address) -> Pubkey {
    Pubkey::new_from_array(*address.as_bytes())
}

pub fn address(pubkey: &Pubkey) -> Address {
    Address::from(pubkey.to_bytes())
}

/// One borrow or repay instruction, ready to be appended to a transaction.
#[derive(Debug, Clone)]
pub struct FlashInstruction {
    pub instruction: Instruction,
    /// Absolute instruction index inside the transaction. Kamino's repay cites
    /// the borrow's index, and it must be computed after the whole preamble is
    /// known (a hardcoded 3 was wrong as soon as an ATA creation was added).
    pub index: u8,
}

/// What a provider needs in order to borrow and repay atomically.
///
/// Deliberately synchronous: reserves are resolved through the provider's async
/// discovery methods and passed in, so instruction building is pure and easy to
/// pin with unit tests.
pub trait FlashProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn program_id(&self) -> Pubkey;
    /// Whether this provider has passed every verification step.
    fn enabled(&self) -> bool;
    /// Decode a reserve account. The executor re-reads the reserve immediately
    /// before building a cycle, so availability and fees are never stale.
    fn decode_reserve(&self, address: &Address, data: &[u8]) -> Result<FlashReserve>;
    /// Build the borrow instruction.
    fn borrow(
        &self,
        reserve: &FlashReserve,
        wallet: Pubkey,
        destination: Pubkey,
        amount: u64,
        index: u8,
    ) -> Result<FlashInstruction>;
    /// Build the matching repayment instruction. `amount` is the borrowed
    /// principal, exactly as the protocol's repay argument requires.
    fn repay(
        &self,
        reserve: &FlashReserve,
        wallet: Pubkey,
        source: Pubkey,
        amount: u64,
        borrow_index: u8,
    ) -> Result<FlashInstruction>;
    /// Lamports/tokens the borrower must have available at repay time.
    fn repayment_amount(&self, reserve: &FlashReserve, amount: u64) -> Result<u64> {
        reserve.repayment_amount(amount)
    }
    /// Protocol fee for `amount`.
    fn flash_fee(&self, reserve: &FlashReserve, amount: u64) -> Result<u64> {
        reserve.flash_fee(amount)
    }
}

/// Registry of flash providers, built from on-chain verification.
#[derive(Clone, Default)]
pub struct FlashRegistry {
    providers: Vec<Arc<dyn FlashProvider>>,
}

impl FlashRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a provider. Providers whose verification is incomplete still
    /// register, so the UI can report *why* they are unavailable.
    pub fn register(mut self, provider: Arc<dyn FlashProvider>) -> Self {
        self.providers.push(provider);
        self
    }

    pub fn providers(&self) -> &[Arc<dyn FlashProvider>] {
        &self.providers
    }

    pub fn enabled(&self) -> impl Iterator<Item = &Arc<dyn FlashProvider>> {
        self.providers.iter().filter(|provider| provider.enabled())
    }

    pub fn get(&self, id: &str) -> Option<&Arc<dyn FlashProvider>> {
        self.providers.iter().find(|provider| provider.id() == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_reports_unverified_providers_as_disabled() {
        let registry =
            FlashRegistry::new().register(Arc::new(kamilend::KaminoFlashProvider::unverified()));
        assert_eq!(registry.providers().len(), 1);
        assert_eq!(registry.enabled().count(), 0);
        assert!(registry.get("kamino").is_some());
    }

    #[test]
    fn fee_is_ceiling_at_2_60() {
        // Observed on-chain value for the main-market WSOL/USDC/USDT reserves.
        let bits = 11_529_215_046_068u64;
        let reserve = FlashReserve {
            flash_fee_bits: bits,
            status: 0,
            available: u64::MAX,
            ..test_reserve()
        };
        assert_eq!(reserve.flash_fee(1_000_000_000).unwrap(), 10_000);
        assert_eq!(reserve.flash_fee(2_000_000_000).unwrap(), 20_000);
        assert_eq!(reserve.flash_fee(234_451_759).unwrap(), 2_345);
        assert_eq!(reserve.flash_fee(0).unwrap(), 0);
        assert_eq!(
            reserve.repayment_amount(1_000_000_000).unwrap(),
            1_000_010_000
        );
    }

    #[test]
    fn fee_disabled_is_rejected() {
        let reserve = FlashReserve {
            flash_fee_bits: u64::MAX,
            ..test_reserve()
        };
        assert!(matches!(
            reserve.flash_fee(1),
            Err(FlashError::FeeDisabled { .. })
        ));
        assert!(!reserve.flash_enabled());
    }

    fn test_reserve() -> FlashReserve {
        FlashReserve {
            mint: Address::ZERO,
            reserve: Address::ZERO,
            supply_vault: Address::ZERO,
            fee_vault: Address::ZERO,
            lending_market: Address::ZERO,
            lending_market_authority: Address::ZERO,
            token_program: Address::ZERO,
            flash_fee_bits: 0,
            available: 0,
            decimals: 9,
            status: 0,
        }
    }
}
