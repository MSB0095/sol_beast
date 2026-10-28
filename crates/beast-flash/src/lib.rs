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

pub mod jupiter;
pub mod kamilend;
pub mod marginfi;
pub mod solend;
pub use jupiter::{
    JupiterFlashProvider, JUPITER_FLASHLOAN_ADMIN, JUPITER_FLASHLOAN_PROGRAM_ID, JUPITER_LIQUIDITY,
    JUPITER_LIQUIDITY_PROGRAM_ID, JUPITER_WSOL_BORROW_POSITION, JUPITER_WSOL_RESERVE,
};
pub use kamilend::{derive_market_authority, KaminoFlashProvider, KAMINO_KLEND_PROGRAM_ID};
pub use marginfi::{
    MarginfiFlashProvider, MARGINFI_MAIN_GROUP, MARGINFI_PROGRAM_ID, MARGINFI_WSOL_BANK,
};
pub use solend::{SolendFlashProvider, SOLEND_PROGRAM_ID};

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
    #[error("flash fee of {fee_bits} is disabled or too high")]
    FeeDisabled { fee_bits: u64 },
    #[error("flash fee ({fee}) is not smaller than the borrowed amount ({amount})")]
    FeeExceedsAmount { fee: u64, amount: u64 },
    #[error("arithmetic overflow computing the flash repayment")]
    Overflow,
    #[error("transaction assembly failed: {0}")]
    Transaction(String),
}

pub type Result<T> = std::result::Result<T, FlashError>;

/// How a protocol stores and rounds its flash fee.
///
/// The verified Solana lenders use four different fixed-point scales, and
/// guessing one from the other silently misprices every borrow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FlashFeeModel {
    /// A 2^60 fraction, rounded **up**. Kamino KLend.
    #[default]
    FixedPoint60Ceiling,
    /// A 1e18 WAD fraction, rounded to nearest with a protocol minimum.
    /// Solend/Save. A zero fee is a genuinely free flash loan, not disabled.
    Wad18Round,
    /// No flash fee at all. Marginfi v2 inside a `start_flashloan` flag:
    /// normal borrow interest does not accrue within one atomic transaction,
    /// and the bank's origination fee is rejected unless it is exactly zero.
    NoFee,
    /// A u16 fraction of 10,000 basis points, rounded **up**. Jupiter Lend's
    /// flashloan program computes `ceil(amount * fee_bps / 10_000)`. Feeless
    /// today, so a zero fee must still be honest: a nonzero value means the
    /// protocol was reconfigured and the fee is charged.
    Bps16Ceiling,
}

/// A pool of borrowable liquidity, resolved on-chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlashReserve {
    pub mint: Address,
    pub reserve: Address,
    pub supply_vault: Address,
    pub fee_vault: Address,
    pub lending_market: Address,
    /// Derived from `["lma", lending_market]` on Kamino and `[lending_market]`
    /// on Solend; pinned by each adapter's unit tests against a live account.
    pub lending_market_authority: Address,
    /// SPL Token or Token-2022, read from the reserve account.
    pub token_program: Address,
    /// Extra read-only accounts the borrow instruction requires after its
    /// fixed account list. Marginfi needs the bank and its oracle/integration
    /// keys here; Kamino and Solend leave it empty.
    #[serde(default)]
    pub remaining_accounts: Vec<Address>,
    /// Raw fee field. Its scale is defined by [`Self::fee_model`].
    pub flash_fee_bits: u64,
    /// How to interpret and round [`Self::flash_fee_bits`].
    pub fee_model: FlashFeeModel,
    /// Percentage of the fee routed to a host/frontend account (Solend only).
    pub host_fee_percentage: u8,
    /// Borrowable amount right now.
    pub available: u64,
    pub decimals: u8,
    /// Reserve status: 0 is active on Kamino; Solend has no status byte and is
    /// reported as 0 when decoded.
    pub status: u8,
}

impl FlashReserve {
    /// The exact fee the protocol charges for `amount`.
    ///
    /// Kamino stores a 2^60 fraction and rounds **up**: the observed
    /// main-market value `11_529_215_046_068` is exactly `floor(2^60 / 100_000)`,
    /// i.e. 10 ppm. A 1 SOL borrow was simulated and charged 10,000 lamports.
    ///
    /// Solend stores a 1e18 WAD fraction and rounds to nearest, with a minimum
    /// of two token units when a host fee is configured. The verified WSOL
    /// reserve charges 500 ppm: 100,000,000 lamports cost 50,000, split 40,000
    /// to the reserve and 10,000 to the host.
    pub fn flash_fee(&self, amount: u64) -> Result<u64> {
        if self.fee_model == FlashFeeModel::NoFee {
            return Ok(0);
        }
        if self.flash_fee_bits == u64::MAX {
            return Err(FlashError::FeeDisabled {
                fee_bits: self.flash_fee_bits,
            });
        }
        match self.fee_model {
            FlashFeeModel::FixedPoint60Ceiling => {
                let scale = 1u128 << 60;
                let product = (amount as u128)
                    .checked_mul(self.flash_fee_bits as u128)
                    .ok_or(FlashError::Overflow)?;
                let fee = product.checked_add(scale - 1).ok_or(FlashError::Overflow)? / scale;
                u64::try_from(fee).map_err(|_| FlashError::Overflow)
            }
            FlashFeeModel::Wad18Round => {
                // The on-chain branch is `fee_rate > 0 && amount > 0`; a zero
                // rate is a free loan, not the disabled marker.
                if self.flash_fee_bits == 0 || amount == 0 {
                    return Ok(0);
                }
                let scale = 1_000_000_000_000_000_000u128;
                // u64 * u64 always fits in u128.
                let product = (amount as u128) * (self.flash_fee_bits as u128);
                let minimum_units = if self.host_fee_percentage > 0 { 2 } else { 1 };
                let fee_decimal = product.max(minimum_units * scale);
                if fee_decimal >= (amount as u128) * scale {
                    return Err(FlashError::FeeExceedsAmount {
                        fee: u64::try_from(fee_decimal / scale).unwrap_or(u64::MAX),
                        amount,
                    });
                }
                let rounded = fee_decimal
                    .checked_add(scale / 2)
                    .ok_or(FlashError::Overflow)?
                    / scale;
                u64::try_from(rounded).map_err(|_| FlashError::Overflow)
            }
            FlashFeeModel::Bps16Ceiling => {
                // 10,000 basis points is the protocol's precision constant.
                let scale = 10_000u128;
                let product = (amount as u128)
                    .checked_mul(self.flash_fee_bits as u128)
                    .ok_or(FlashError::Overflow)?;
                let fee = product.checked_add(scale - 1).ok_or(FlashError::Overflow)? / scale;
                u64::try_from(fee).map_err(|_| FlashError::Overflow)
            }
            FlashFeeModel::NoFee => Ok(0),
        }
    }

    /// Principal plus the protocol fee. The repay instruction itself takes the
    /// **principal**; this value is what the final swap must return.
    pub fn repayment_amount(&self, amount: u64) -> Result<u64> {
        amount
            .checked_add(self.flash_fee(amount)?)
            .ok_or(FlashError::Overflow)
    }

    /// Fee rate in parts per million, for display. `None` when the reserve has
    /// disabled flash loans (`u64::MAX`).
    pub fn fee_ppm(&self) -> Option<f64> {
        if self.flash_fee_bits == u64::MAX {
            return None;
        }
        let scale = match self.fee_model {
            FlashFeeModel::FixedPoint60Ceiling => (1u64 << 60) as f64,
            FlashFeeModel::Wad18Round => 1_000_000_000_000_000_000.0,
            FlashFeeModel::Bps16Ceiling => 10_000.0,
            FlashFeeModel::NoFee => return Some(0.0),
        };
        Some(self.flash_fee_bits as f64 / scale * 1e6)
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

/// Per-cycle account state for providers whose flash loan is not a bare
/// borrow/repay pair.
///
/// Marginfi wraps the borrow in a `start_flashloan` flag on a temporary PDA
/// account. That account is created and closed inside the same transaction,
/// and every instruction that touches it must derive the exact same address.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlashLifecycle {
    /// Temporary account created for this cycle, when the provider uses one.
    pub account: Option<Address>,
    /// Protocol-specific seed used to derive `account` (Marginfi account index).
    pub account_index: u16,
    /// Protocol-specific namespace/third-party id (Marginfi third-party id).
    pub third_party_id: u16,
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

    /// Complete a pinned reserve that cannot be decoded from one account alone.
    ///
    /// Jupiter Lend's flashloan program reads a protocol-wide fee from a
    /// separate admin PDA and a per-mint debt ceiling from a separate position
    /// PDA, so it needs two more account reads to report an honest fee and a
    /// correct borrowable amount. Providers whose reserve is self-contained
    /// keep the default, which is exactly `decode_reserve`.
    #[allow(clippy::type_complexity)]
    fn resolve_pinned_reserve<'a>(
        &'a self,
        _rpc: &'a beast_rpc::RpcClient,
        address: &'a Address,
        data: &'a [u8],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FlashReserve>> + Send + 'a>>
    {
        Box::pin(async move { self.decode_reserve(address, data) })
    }
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

    /// Derive the temporary account this provider would create for `wallet` at
    /// `index`, if it needs one. The executor probes indexes in order and picks
    /// the first derived account that does not already exist, so a leftover
    /// account from another tool cannot permanently block a cycle.
    fn lifecycle_account(
        &self,
        _reserve: &FlashReserve,
        _wallet: Pubkey,
        _index: u16,
    ) -> Option<Address> {
        None
    }

    /// Lamports temporarily locked by creating the lifecycle account. The
    /// account is closed in the same atomic transaction, so this is a balance
    /// float requirement, not a cost. Zero for bare borrow/repay providers.
    fn temporary_rent_lamports(&self) -> u64 {
        0
    }

    /// Instructions that must run before the borrow, such as account
    /// initialization or a flashloan flag. Empty by default.
    fn pre_borrow(
        &self,
        _reserve: &FlashReserve,
        _wallet: Pubkey,
        _destination: Pubkey,
        _amount: u64,
        _lifecycle: &FlashLifecycle,
    ) -> Result<Vec<Instruction>> {
        Ok(Vec::new())
    }

    /// Instructions that must run after the repayment, such as clearing the
    /// flashloan flag and closing the temporary account. Empty by default.
    fn post_repay(
        &self,
        _reserve: &FlashReserve,
        _wallet: Pubkey,
        _destination: Pubkey,
        _amount: u64,
        _lifecycle: &FlashLifecycle,
    ) -> Result<Vec<Instruction>> {
        Ok(Vec::new())
    }

    /// Patch provider instructions after composition. Marginfi's
    /// `start_flashloan` stores the absolute index of `end_flashloan`, which is
    /// only known once every swap and postamble instruction is in place.
    fn finalize(
        &self,
        _instructions: &mut [Instruction],
        _lifecycle: &FlashLifecycle,
    ) -> Result<()> {
        Ok(())
    }

    /// Lifecycle-aware borrow. Providers that need the temporary account
    /// override this; the default keeps the bare borrow behavior.
    fn borrow_with_lifecycle(
        &self,
        reserve: &FlashReserve,
        wallet: Pubkey,
        destination: Pubkey,
        amount: u64,
        index: u8,
        _lifecycle: &FlashLifecycle,
    ) -> Result<FlashInstruction> {
        self.borrow(reserve, wallet, destination, amount, index)
    }

    /// Lifecycle-aware repay. Defaults to the bare repay.
    fn repay_with_lifecycle(
        &self,
        reserve: &FlashReserve,
        wallet: Pubkey,
        source: Pubkey,
        amount: u64,
        borrow_index: u8,
        _lifecycle: &FlashLifecycle,
    ) -> Result<FlashInstruction> {
        self.repay(reserve, wallet, source, amount, borrow_index)
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
            fee_model: FlashFeeModel::FixedPoint60Ceiling,
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
        assert!((reserve.fee_ppm().unwrap() - 10.0).abs() < 1e-9);
    }

    #[test]
    fn solend_wad_fee_rounds_with_a_host_minimum() {
        // Verified on-chain value for Solend's WSOL reserve: 0.05% with a 20%
        // host cut. A 100,000,000-lamport simulation paid exactly 50,000.
        let reserve = FlashReserve {
            flash_fee_bits: 500_000_000_000_000,
            fee_model: FlashFeeModel::Wad18Round,
            host_fee_percentage: 20,
            available: u64::MAX,
            ..test_reserve()
        };
        assert_eq!(reserve.flash_fee(100_000_000).unwrap(), 50_000);
        assert_eq!(reserve.repayment_amount(100_000_000).unwrap(), 100_050_000);
        assert!((reserve.fee_ppm().unwrap() - 500.0).abs() < 1e-9);
        // Round to nearest: 10,000 lamports * 5e-4 = 5 lamports exactly.
        assert_eq!(reserve.flash_fee(10_000).unwrap(), 5);
        // The protocol enforces a two-unit minimum when a host fee is owed.
        assert_eq!(reserve.flash_fee(10).unwrap(), 2);
        // `BorrowTooSmall`: the minimum fee must be strictly below the borrow.
        assert!(matches!(
            reserve.flash_fee(2),
            Err(FlashError::FeeExceedsAmount { .. })
        ));
        // Zero is a free loan, not the disabled marker.
        let free = FlashReserve {
            flash_fee_bits: 0,
            ..reserve.clone()
        };
        assert_eq!(free.flash_fee(1_000_000).unwrap(), 0);
        assert_eq!(free.repayment_amount(1_000_000).unwrap(), 1_000_000);
        assert!(free.flash_enabled());
    }

    #[test]
    fn solend_rejects_a_fee_larger_than_the_borrow() {
        let reserve = FlashReserve {
            // 200% fee.
            flash_fee_bits: 2_000_000_000_000_000_000,
            fee_model: FlashFeeModel::Wad18Round,
            host_fee_percentage: 20,
            available: u64::MAX,
            ..test_reserve()
        };
        assert!(matches!(
            reserve.flash_fee(1_000),
            Err(FlashError::FeeExceedsAmount { .. })
        ));
    }

    #[test]
    fn no_fee_model_charges_nothing_and_is_still_enabled() {
        let reserve = FlashReserve {
            fee_model: FlashFeeModel::NoFee,
            available: 1_000_000_000,
            ..test_reserve()
        };
        assert_eq!(reserve.flash_fee(1_000_000_000).unwrap(), 0);
        assert_eq!(
            reserve.repayment_amount(1_000_000_000).unwrap(),
            1_000_000_000
        );
        assert_eq!(reserve.fee_ppm(), Some(0.0));
        assert!(reserve.flash_enabled());
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
            remaining_accounts: Vec::new(),
            flash_fee_bits: 0,
            fee_model: FlashFeeModel::FixedPoint60Ceiling,
            host_fee_percentage: 0,
            available: 0,
            decimals: 9,
            status: 0,
        }
    }
}
