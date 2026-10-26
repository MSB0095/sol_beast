//! Guard-gated execution primitives.
//!
//! This crate can build and sign transactions, but it deliberately does not
//! invent DEX swap instructions. The current live path uses Jupiter for both
//! legs of a two-hop cycle, then adds:
//!
//! 1. an on-chain profit-guard transaction;
//! 2. a Jito tip transaction.
//!
//! The bundle order is `swap1, swap2, guard, tip`. If the guard transaction
//! fails, the bundle must be dropped by the block engine before the tip is
//! paid. The guard checks the wallet's actual lamport balance, not the model.
//!
//! Safety posture:
//! * only WSOL-starting two-leg cycles are executable here;
//! * a live submission requires `real=true`, Jito, and a configured guard;
//! * the tip and estimated fees are included in the guard threshold;
//! * all transactions are simulated before submission by the caller.

use base64::Engine;
use beast_core::economics::{TipPolicy, MIN_JITO_TIP_LAMPORTS, SPL_TOKEN_ACCOUNT_RENT_LAMPORTS};
use beast_core::types::{Mode, Opportunity};
use beast_dex::jupiter::{JupiterClient, JupiterQuote, QuoteRequest, SwapRequest};
use beast_dex::Address;
use beast_rpc::{JitoClient, RpcClient};
use bincode::serialize;
use serde::{Deserialize, Serialize};
use solana_sdk::hash::Hash;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signature};
use solana_sdk::signer::Signer;
#[allow(deprecated)]
use solana_sdk::system_instruction;
use solana_sdk::transaction::{Transaction, VersionedTransaction};
use std::path::Path;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use thiserror::Error;

pub mod rent;
pub use rent::{RentReclaimReport, RentReclaimer, RentScan};

pub const WSOL_MINT: &str = "So11111111111111111111111111111111111111112";
pub const GUARD_IX_VERSION: u8 = 1;
/// A single-signature transaction pays 5,000 lamports at the base rate.
pub const DEFAULT_TRANSACTION_FEE_LAMPORTS: u64 = 5_000;
/// Jito accepts at most five transactions per bundle: swaps + guard + tip.
pub const MAX_CYCLE_LEGS: usize = 3;

pub type Result<T> = std::result::Result<T, ExecutionError>;

#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("configuration: {0}")]
    Config(String),
    #[error("wallet: {0}")]
    Wallet(String),
    #[error("rpc: {0}")]
    Rpc(#[from] beast_rpc::RpcError),
    #[error("jupiter: {0}")]
    Jupiter(#[from] beast_dex::DexError),
    #[error("transaction: {0}")]
    Transaction(String),
    #[error("live execution is disabled")]
    LiveDisabled,
    #[error("jito is not configured")]
    JitoDisabled,
    #[error("guard program is not configured")]
    GuardDisabled,
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("unprofitable: {0}")]
    Unprofitable(String),
    #[error("insufficient balance: {0}")]
    InsufficientBalance(String),
    #[error("guard: {0}")]
    Guard(String),
}

impl From<solana_sdk::signer::SignerError> for ExecutionError {
    fn from(error: solana_sdk::signer::SignerError) -> Self {
        Self::Transaction(error.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionConfig {
    pub real: bool,
    pub min_profit_lamports: u64,
    pub balance_floor_lamports: u64,
    pub slippage_bps: u16,
    pub tip_alpha: f64,
    pub max_tip_lamports: u64,
    pub guard_program: String,
    /// Base fee charged once per bundle transaction (one signature each).
    pub transaction_fee_lamports: u64,
    /// Worst-case token-account rent retained per intermediate mint.
    pub ata_rent_lamports: u64,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            real: false,
            min_profit_lamports: 20_000,
            balance_floor_lamports: 50_000_000,
            slippage_bps: 50,
            tip_alpha: 0.5,
            max_tip_lamports: 2_000_000,
            guard_program: String::new(),
            transaction_fee_lamports: DEFAULT_TRANSACTION_FEE_LAMPORTS,
            ata_rent_lamports: SPL_TOKEN_ACCOUNT_RENT_LAMPORTS,
        }
    }
}

impl ExecutionConfig {
    pub fn guard_enabled(&self) -> bool {
        !self.guard_program.is_empty()
            && self
                .guard_program
                .parse::<Pubkey>()
                .map(|pubkey| pubkey != Pubkey::default())
                .unwrap_or(false)
    }

    pub fn validate(&self) -> Result<()> {
        if self.min_profit_lamports == 0 {
            return Err(ExecutionError::Config(
                "min_profit_lamports must be > 0".into(),
            ));
        }
        if self.balance_floor_lamports == 0 {
            return Err(ExecutionError::Config(
                "balance_floor_lamports must be > 0".into(),
            ));
        }
        if !(1..=500).contains(&self.slippage_bps) {
            return Err(ExecutionError::Config(
                "slippage_bps must be 1..=500".into(),
            ));
        }
        if !self.tip_alpha.is_finite() || !(0.01..=0.90).contains(&self.tip_alpha) {
            return Err(ExecutionError::Config(
                "tip_alpha must be within [0.01, 0.90]".into(),
            ));
        }
        if self.max_tip_lamports < MIN_JITO_TIP_LAMPORTS {
            return Err(ExecutionError::Config(
                "max_tip_lamports must be >= 1000".into(),
            ));
        }
        if !self.guard_program.is_empty() {
            self.guard_program
                .parse::<Pubkey>()
                .map_err(|error| ExecutionError::Config(format!("guard_program: {error}")))?;
        }
        if self.transaction_fee_lamports == 0 {
            return Err(ExecutionError::Config(
                "transaction_fee_lamports must be > 0".into(),
            ));
        }
        Ok(())
    }

    fn total_costs(&self, transaction_count: usize, intermediate_accounts: usize) -> Result<u64> {
        let fees = self
            .transaction_fee_lamports
            .checked_mul(transaction_count as u64)
            .ok_or_else(|| ExecutionError::Config("transaction fee overflow".into()))?;
        let rent = self
            .ata_rent_lamports
            .checked_mul(intermediate_accounts as u64)
            .ok_or_else(|| ExecutionError::Config("ATA rent overflow".into()))?;
        fees.checked_add(rent)
            .ok_or_else(|| ExecutionError::Config("cost overflow".into()))
    }

    /// Balance required *at guard execution time* (after the swap and guard
    /// transaction fees and any ATA rent have already left the wallet). Only
    /// the tip transfer and its transaction fee remain to be paid, so those
    /// are the future costs the guard must reserve.
    fn guard_threshold(
        &self,
        start_lamports: u64,
        tip_lamports: u64,
        remaining_fee_lamports: u64,
    ) -> Result<u64> {
        start_lamports
            .checked_add(self.min_profit_lamports)
            .and_then(|value| value.checked_add(tip_lamports))
            .and_then(|value| value.checked_add(remaining_fee_lamports))
            .ok_or_else(|| ExecutionError::Guard("guard threshold overflow".into()))
    }
}

pub struct Wallet {
    keypair: Keypair,
}

impl Wallet {
    pub fn load(path: &Path, require_0600: bool) -> Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(path)
                .map_err(|error| ExecutionError::Wallet(format!("{}: {error}", path.display())))?;
            if require_0600 && metadata.permissions().mode() & 0o077 != 0 {
                return Err(ExecutionError::Wallet(format!(
                    "{} must not be readable by group/other (chmod 600)",
                    path.display()
                )));
            }
        }
        let keypair = solana_sdk::signer::keypair::read_keypair_file(path)
            .map_err(|error| ExecutionError::Wallet(format!("{}: {error}", path.display())))?;
        Ok(Self { keypair })
    }

    pub fn from_json(json: &str) -> Result<Self> {
        let bytes: Vec<u8> = serde_json::from_str(json)
            .map_err(|error| ExecutionError::Wallet(format!("keypair JSON: {error}")))?;
        let keypair = Keypair::try_from(bytes.as_slice())
            .map_err(|error| ExecutionError::Wallet(format!("keypair bytes: {error}")))?;
        Ok(Self { keypair })
    }

    pub fn from_keypair(keypair: Keypair) -> Self {
        Self { keypair }
    }

    pub fn pubkey(&self) -> Pubkey {
        self.keypair.pubkey()
    }

    pub fn address(&self) -> Address {
        Address::from_slice(self.pubkey().as_ref()).expect("32-byte pubkey")
    }

    pub fn pubkey_string(&self) -> String {
        self.pubkey().to_string()
    }

    pub fn keypair(&self) -> &Keypair {
        &self.keypair
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedTransaction {
    pub encoded: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundlePlan {
    pub opportunity_id: String,
    pub mode: Mode,
    /// Mint path from the start mint back to itself.
    pub route_mints: Vec<Address>,
    pub amount_in: u64,
    pub quoted_out_min: u64,
    pub expected_start_lamports: u64,
    pub min_profit_lamports: u64,
    pub tip_lamports: u64,
    pub tip_account: Address,
    pub fees_lamports: u64,
    pub guard_required_lamports: u64,
    pub blockhash: String,
    pub encoded_transactions: Vec<String>,
    pub signatures: Vec<String>,
}

pub struct Executor {
    rpc: Arc<RpcClient>,
    jupiter: Arc<JupiterClient>,
    jito: Option<Arc<JitoClient>>,
    wallet: Arc<Wallet>,
    config: ExecutionConfig,
    tip_accounts: Vec<Address>,
    tip_cursor: AtomicUsize,
}

impl Executor {
    pub fn new(
        rpc: Arc<RpcClient>,
        jupiter: Arc<JupiterClient>,
        jito: Option<Arc<JitoClient>>,
        wallet: Arc<Wallet>,
        config: ExecutionConfig,
        tip_accounts: Vec<Address>,
    ) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            rpc,
            jupiter,
            jito,
            wallet,
            config,
            tip_accounts,
            tip_cursor: AtomicUsize::new(0),
        })
    }

    pub fn config(&self) -> &ExecutionConfig {
        &self.config
    }

    pub fn wallet(&self) -> &Wallet {
        &self.wallet
    }

    /// Prepare a closed Jupiter cycle with an optional on-chain profit guard.
    ///
    /// The cycle may contain two or three legs (Jito caps a bundle at five
    /// transactions). This function performs quotes and signing only. It never
    /// sends a transaction; `submit` is a separate, explicit step.
    pub async fn prepare_cycle(&self, opportunity: &Opportunity) -> Result<BundlePlan> {
        let legs = &opportunity.legs;
        if !(2..=MAX_CYCLE_LEGS).contains(&legs.len()) {
            return Err(ExecutionError::Unsupported(format!(
                "live executor supports 2..={MAX_CYCLE_LEGS} legs, got {}",
                legs.len()
            )));
        }
        let start = parse_mint(&legs[0].mint_in, "start mint")?;
        if start.to_string() != WSOL_MINT {
            return Err(ExecutionError::Unsupported(format!(
                "live executor only supports WSOL-starting cycles, got {start}"
            )));
        }

        let mut mints = Vec::with_capacity(legs.len());
        let mut expected_in = start;
        for (index, leg) in legs.iter().enumerate() {
            let mint_in = parse_mint(&leg.mint_in, "leg input")?;
            let mint_out = parse_mint(&leg.mint_out, "leg output")?;
            if mint_in != expected_in {
                return Err(ExecutionError::Unsupported(format!(
                    "leg {index} starts at {mint_in}, expected {expected_in}"
                )));
            }
            mints.push(mint_in);
            expected_in = mint_out;
        }
        if expected_in != start {
            return Err(ExecutionError::Unsupported(
                "live executor requires a closed cycle (end mint == start mint)".into(),
            ));
        }
        mints.push(start);

        let amount_in = legs[0].amount_in;
        if amount_in == 0 {
            return Err(ExecutionError::Unprofitable("zero input amount".into()));
        }
        let start_balance = self.rpc.get_balance(self.wallet.address()).await?;
        let required = amount_in
            .checked_add(self.config.balance_floor_lamports)
            .ok_or_else(|| ExecutionError::InsufficientBalance("balance overflow".into()))?;
        if start_balance < required {
            return Err(ExecutionError::InsufficientBalance(format!(
                "wallet has {start_balance}, need {required} (amount + floor)"
            )));
        }

        let mut quotes = Vec::with_capacity(legs.len());
        let mut leg_amount = amount_in;
        for index in 0..legs.len() {
            let mut request = QuoteRequest::new(mints[index], mints[index + 1], leg_amount);
            request.slippage_bps = self.config.slippage_bps;
            request.only_direct_routes = true;
            let quote = self.jupiter.quote(&request).await?;
            let min_out = quote.minimum_out()?;
            if min_out == 0 {
                return Err(ExecutionError::Unprofitable(format!(
                    "leg {index} minOut=0"
                )));
            }
            leg_amount = min_out;
            quotes.push(quote);
        }
        if leg_amount <= amount_in {
            return Err(ExecutionError::Unprofitable(format!(
                "quoted cycle output {leg_amount} <= input {amount_in}"
            )));
        }
        let gross_min = leg_amount - amount_in;

        let guard_enabled = self.config.guard_enabled();
        // Guarded bundle: swaps + guard + tip (4 tx max for 2 legs). Unguarded: swaps + tip (3 tx max for 2 legs).
        let transaction_count = legs.len() + if guard_enabled { 2 } else { 1 };
        let intermediate_accounts = legs.len() - 1;
        let fees_lamports = self
            .config
            .total_costs(transaction_count, intermediate_accounts)?;
        let tip_lamports = self.choose_tip(gross_min, fees_lamports)?;
        let guard_required = if guard_enabled {
            self.config.guard_threshold(
                start_balance,
                tip_lamports,
                self.config.transaction_fee_lamports,
            )?
        } else {
            0
        };

        let (blockhash_string, _) = self.rpc.get_latest_blockhash().await?;
        let blockhash = parse_hash(&blockhash_string)?;
        let mut swaps = Vec::with_capacity(quotes.len());
        for quote in quotes {
            swaps.push(self.build_swap_transaction(quote, blockhash).await?);
        }
        let tip_account = self.choose_tip_account().await?;
        let tip = build_tip_transaction(
            self.wallet.pubkey(),
            tip_account,
            tip_lamports,
            blockhash,
            &self.wallet.keypair,
        )?;

        let mut encoded_transactions = Vec::with_capacity(transaction_count);
        let mut signatures = Vec::with_capacity(transaction_count);
        for swap in swaps {
            encoded_transactions.push(swap.encoded);
            signatures.push(swap.signature);
        }
        if guard_enabled {
            let guard = build_guard_transaction(
                &self.config.guard_program,
                self.wallet.pubkey(),
                start_balance,
                self.config.min_profit_lamports,
                tip_lamports,
                self.config.transaction_fee_lamports,
                blockhash,
                &self.wallet.keypair,
            )?;
            encoded_transactions.push(guard.encoded);
            signatures.push(guard.signature);
        }
        encoded_transactions.push(tip.encoded);
        signatures.push(tip.signature);

        Ok(BundlePlan {
            opportunity_id: opportunity.id.to_string(),
            mode: opportunity.mode,
            route_mints: mints,
            amount_in,
            quoted_out_min: leg_amount,
            expected_start_lamports: start_balance,
            min_profit_lamports: self.config.min_profit_lamports,
            tip_lamports,
            tip_account,
            fees_lamports,
            guard_required_lamports: guard_required,
            blockhash: blockhash_string,
            encoded_transactions,
            signatures,
        })
    }

    fn choose_tip(&self, gross_min: u64, fees_lamports: u64) -> Result<u64> {
        let policy = TipPolicy {
            alpha: self.config.tip_alpha,
            floor_lamports: MIN_JITO_TIP_LAMPORTS,
            cap_lamports: self.config.max_tip_lamports.max(MIN_JITO_TIP_LAMPORTS),
        };
        policy.validate().map_err(ExecutionError::Config)?;
        let desired = policy.tip_for(gross_min as i64);
        let max_affordable = gross_min
            .saturating_sub(self.config.min_profit_lamports)
            .saturating_sub(fees_lamports);
        if max_affordable < MIN_JITO_TIP_LAMPORTS {
            return Err(ExecutionError::Unprofitable(format!(
                "gross {gross_min} cannot cover min_profit, fees and 1000-lamport tip"
            )));
        }
        Ok(desired.min(max_affordable))
    }

    async fn choose_tip_account(&self) -> Result<Address> {
        let cursor = self.tip_cursor.fetch_add(1, Ordering::Relaxed);
        if !self.tip_accounts.is_empty() {
            return Ok(self.tip_accounts[cursor % self.tip_accounts.len()]);
        }
        let jito = self.jito.as_ref().ok_or(ExecutionError::JitoDisabled)?;
        let accounts = jito.tip_accounts().await?;
        if accounts.is_empty() {
            return Err(ExecutionError::JitoDisabled);
        }
        Ok(accounts[cursor % accounts.len()])
    }

    async fn build_swap_transaction(
        &self,
        quote: JupiterQuote,
        blockhash: Hash,
    ) -> Result<SignedTransaction> {
        let mut request = SwapRequest::new(self.wallet.pubkey_string(), quote);
        request.as_legacy_transaction = true;
        request.wrap_and_unwrap_sol = true;
        let swap = self.jupiter.swap(&request).await?;
        sign_swap_transaction(&swap.swap_transaction, &self.wallet.keypair, blockhash)
    }

    pub async fn simulate(&self, plan: &BundlePlan) -> Result<serde_json::Value> {
        let jito = self.jito.as_ref().ok_or(ExecutionError::JitoDisabled)?;
        Ok(jito.simulate_bundle(&plan.encoded_transactions).await?)
    }

    /// Submit a prepared bundle. Never called from Radar.
    pub async fn submit(&self, plan: &BundlePlan) -> Result<String> {
        if !self.config.real || !plan.mode.is_live() {
            return Err(ExecutionError::LiveDisabled);
        }
        let jito = self.jito.as_ref().ok_or(ExecutionError::JitoDisabled)?;
        let bundle_id = jito.send_bundle(&plan.encoded_transactions).await?;
        tracing::info!(
            bundle_id,
            opportunity = %plan.opportunity_id,
            tip = plan.tip_lamports,
            signatures = ?plan.signatures,
            "Jito bundle submitted"
        );
        Ok(bundle_id)
    }
}

fn parse_hash(value: &str) -> Result<Hash> {
    Hash::from_str(value)
        .map_err(|error| ExecutionError::Transaction(format!("blockhash: {error}")))
}

fn parse_mint(value: &str, label: &str) -> Result<Address> {
    value
        .parse::<Address>()
        .map_err(|error| ExecutionError::Config(format!("{label}: {error}")))
}

/// Sign a Jupiter legacy (or versioned) transaction with the wallet keypair and
/// replace its blockhash so all bundle transactions are valid in one slot.
pub fn sign_swap_transaction(
    encoded: &str,
    wallet: &Keypair,
    blockhash: Hash,
) -> Result<SignedTransaction> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| ExecutionError::Transaction(format!("base64: {error}")))?;
    match bincode::deserialize::<Transaction>(&bytes) {
        Ok(mut tx) if plausible_legacy(&tx) => {
            if tx.message.account_keys.first() != Some(&wallet.pubkey()) {
                return Err(ExecutionError::Transaction(
                    "Jupiter legacy transaction fee payer is not the configured wallet".into(),
                ));
            }
            tx.message.recent_blockhash = blockhash;
            tx.signatures =
                vec![Signature::default(); tx.message.header.num_required_signatures as usize];
            tx.try_sign(&[wallet], blockhash)?;
            let signature = tx
                .signatures
                .first()
                .copied()
                .ok_or_else(|| ExecutionError::Transaction("missing swap signature".into()))?;
            let encoded =
                base64::engine::general_purpose::STANDARD.encode(bincode::serialize(&tx).map_err(
                    |error| ExecutionError::Transaction(format!("serialize legacy: {error}")),
                )?);
            Ok(SignedTransaction {
                encoded,
                signature: signature.to_string(),
            })
        }
        _ => {
            let mut tx: VersionedTransaction = bincode::deserialize(&bytes).map_err(|error| {
                ExecutionError::Transaction(format!("deserialize versioned: {error}"))
            })?;
            let required = tx.message.header().num_required_signatures as usize;
            if required != 1 || tx.signatures.len() != 1 {
                return Err(ExecutionError::Unsupported(format!(
                    "Jupiter transaction needs {required} signatures; only one wallet signer is supported"
                )));
            }
            if tx.message.static_account_keys().first() != Some(&wallet.pubkey()) {
                return Err(ExecutionError::Transaction(
                    "Jupiter versioned transaction fee payer is not the configured wallet".into(),
                ));
            }
            tx.message.set_recent_blockhash(blockhash);
            tx.signatures[0] = wallet.sign_message(&tx.message.serialize());
            let signature = tx.signatures[0];
            let encoded =
                base64::engine::general_purpose::STANDARD.encode(bincode::serialize(&tx).map_err(
                    |error| ExecutionError::Transaction(format!("serialize versioned: {error}")),
                )?);
            Ok(SignedTransaction {
                encoded,
                signature: signature.to_string(),
            })
        }
    }
}

fn plausible_legacy(tx: &Transaction) -> bool {
    tx.signatures.len() == tx.message.header.num_required_signatures as usize
        && !tx.message.account_keys.is_empty()
        && tx.message.recent_blockhash != Hash::default()
}

/// Build the native guard program instruction.
///
/// Layout: `version u8 | expected_start u64 | min_profit u64 | tip u64 | fees u64`.
pub fn guard_instruction(
    program_id: &str,
    wallet: Pubkey,
    expected_start_lamports: u64,
    min_profit_lamports: u64,
    tip_lamports: u64,
    fees_lamports: u64,
) -> Result<Instruction> {
    let program_id = Pubkey::from_str(program_id)
        .map_err(|error| ExecutionError::Guard(format!("program id: {error}")))?;
    let mut data = Vec::with_capacity(33);
    data.push(GUARD_IX_VERSION);
    data.extend_from_slice(&expected_start_lamports.to_le_bytes());
    data.extend_from_slice(&min_profit_lamports.to_le_bytes());
    data.extend_from_slice(&tip_lamports.to_le_bytes());
    data.extend_from_slice(&fees_lamports.to_le_bytes());
    Ok(Instruction {
        program_id,
        accounts: vec![AccountMeta::new(wallet, true)],
        data,
    })
}

#[allow(clippy::too_many_arguments)]
fn build_guard_transaction(
    program_id: &str,
    wallet: Pubkey,
    expected_start_lamports: u64,
    min_profit_lamports: u64,
    tip_lamports: u64,
    fees_lamports: u64,
    blockhash: Hash,
    keypair: &Keypair,
) -> Result<SignedTransaction> {
    let instruction = guard_instruction(
        program_id,
        wallet,
        expected_start_lamports,
        min_profit_lamports,
        tip_lamports,
        fees_lamports,
    )?;
    let tx =
        Transaction::new_signed_with_payer(&[instruction], Some(&wallet), &[keypair], blockhash);
    encode_signed(&tx)
}

fn build_tip_transaction(
    wallet: Pubkey,
    tip_account: Address,
    tip_lamports: u64,
    blockhash: Hash,
    keypair: &Keypair,
) -> Result<SignedTransaction> {
    if tip_lamports == 0 {
        return Err(ExecutionError::JitoDisabled);
    }
    let tip_pubkey =
        Pubkey::from_str(&tip_account.to_string()).map_err(|_| ExecutionError::JitoDisabled)?;
    let instruction = system_instruction::transfer(&wallet, &tip_pubkey, tip_lamports);
    let tx =
        Transaction::new_signed_with_payer(&[instruction], Some(&wallet), &[keypair], blockhash);
    encode_signed(&tx)
}

fn encode_signed(tx: &Transaction) -> Result<SignedTransaction> {
    let signature = tx
        .signatures
        .first()
        .copied()
        .ok_or_else(|| ExecutionError::Transaction("unsigned transaction".into()))?;
    let encoded = base64::engine::general_purpose::STANDARD
        .encode(serialize(tx).map_err(|error| ExecutionError::Transaction(error.to_string()))?);
    Ok(SignedTransaction {
        encoded,
        signature: signature.to_string(),
    })
}

/// Parse the guard instruction payload. Used by the on-chain program tests.
pub fn parse_guard_data(data: &[u8]) -> Result<(u64, u64, u64, u64)> {
    if data.len() != 33 || data[0] != GUARD_IX_VERSION {
        return Err(ExecutionError::Guard("invalid guard data".into()));
    }
    let start = u64::from_le_bytes(data[1..9].try_into().expect("slice length"));
    let min_profit = u64::from_le_bytes(data[9..17].try_into().expect("slice length"));
    let tip = u64::from_le_bytes(data[17..25].try_into().expect("slice length"));
    let fees = u64::from_le_bytes(data[25..33].try_into().expect("slice length"));
    Ok((start, min_profit, tip, fees))
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::signature::Keypair;

    fn keypair() -> Keypair {
        Keypair::new()
    }

    #[test]
    fn guard_instruction_roundtrip() {
        let wallet = keypair().pubkey();
        let instruction =
            guard_instruction("11111111111111111111111111111111", wallet, 100, 20, 10, 5).unwrap();
        assert_eq!(instruction.accounts.len(), 1);
        assert!(instruction.accounts[0].is_signer);
        assert!(instruction.accounts[0].is_writable);
        let (start, min_profit, tip, fees) = parse_guard_data(&instruction.data).unwrap();
        assert_eq!((start, min_profit, tip, fees), (100, 20, 10, 5));
    }

    #[test]
    fn signs_a_legacy_swap_transaction() {
        let wallet = keypair();
        let blockhash = Hash::new_unique();
        let tx = Transaction::new_signed_with_payer(
            &[system_instruction::transfer(
                &wallet.pubkey(),
                &Pubkey::new_unique(),
                1,
            )],
            Some(&wallet.pubkey()),
            &[&wallet],
            blockhash,
        );
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(bincode::serialize(&tx).unwrap());
        let signed = sign_swap_transaction(&encoded, &wallet, blockhash).unwrap();
        assert!(!signed.encoded.is_empty());
        assert_ne!(signed.signature, Signature::default().to_string());
    }

    #[test]
    fn accepts_empty_guard_program_for_unguarded_mode() {
        let config = ExecutionConfig::default();
        assert!(config.validate().is_ok());
        let config = ExecutionConfig {
            guard_program: "11111111111111111111111111111112".into(), // not system program
            ..ExecutionConfig::default()
        };
        assert!(config.validate().is_ok());
        let config = ExecutionConfig {
            guard_program: "11111111111111111111111111111112".into(),
            ..ExecutionConfig::default()
        };
        assert!(config.guard_enabled());
    }

    #[test]
    fn guard_threshold_reserves_only_future_costs() {
        let config = ExecutionConfig {
            guard_program: "11111111111111111111111111111111".into(),
            ..ExecutionConfig::default()
        };
        let threshold = config
            .guard_threshold(1_000_000, 10_000, config.transaction_fee_lamports)
            .unwrap();
        assert_eq!(
            threshold,
            1_000_000 + config.min_profit_lamports + 10_000 + config.transaction_fee_lamports
        );
    }

    #[test]
    fn total_costs_counts_all_transactions_and_rent() {
        let config = ExecutionConfig {
            guard_program: "11111111111111111111111111111111".into(),
            ..ExecutionConfig::default()
        };
        assert_eq!(
            config.total_costs(5, 2).unwrap(),
            config.transaction_fee_lamports * 5 + config.ata_rent_lamports * 2
        );
    }

    #[test]
    fn wallet_address_roundtrips() {
        let wallet = Wallet::from_keypair(keypair());
        assert_eq!(wallet.address().to_string(), wallet.pubkey_string());
    }
}
