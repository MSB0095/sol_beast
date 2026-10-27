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

pub mod compose;
pub mod rent;
pub use compose::{ComposedCycle, CycleLeg};
pub use rent::{RentReclaimReport, RentReclaimer, RentScan};

pub const WSOL_MINT: &str = "So11111111111111111111111111111111111111112";
pub const GUARD_IX_VERSION: u8 = 1;
/// A single-signature transaction pays 5,000 lamports at the base rate.
pub const DEFAULT_TRANSACTION_FEE_LAMPORTS: u64 = 5_000;
/// Jito accepts at most five transactions per bundle: swaps + guard + tip.
pub const MAX_CYCLE_LEGS: usize = 3;
/// A composed cycle is a single transaction, so the leg ceiling is set by
/// Solana's 1232-byte transaction limit rather than the bundle cap. Five is
/// the practical maximum for direct-route legs with one token hop each.
pub const MAX_COMPOSED_LEGS: usize = 5;

pub type Result<T> = std::result::Result<T, ExecutionError>;

#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("configuration: {0}")]
    Config(String),
    #[error("wallet: {0}")]
    Wallet(String),
    #[error("compose: {0}")]
    Compose(#[from] crate::compose::ComposeError),
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
    /// True when every arbitrage leg was composed into a single transaction
    /// instead of one transaction per leg.
    #[serde(default)]
    pub composed: bool,
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
            composed: false,
        })
    }

    /// Prepare a whole WSOL-starting cycle as ONE transaction.
    ///
    /// Each leg is quoted through Jupiter and decomposed into raw instructions
    /// by `/swap-instructions`; the instruction sets are then concatenated in
    /// order. This turns the Jito five-transaction bundle cap into Solana's
    /// transaction-size cap, so a cycle can carry more legs than the per-leg
    /// path allows - and it pays a single network fee instead of one per leg.
    ///
    /// Safety is identical in shape to [`Self::prepare_cycle`]: the guard still
    /// runs after the composed swap and still compares the wallet's real
    /// post-swap lamports against `expected_start + min_profit + tip + fee`.
    pub async fn prepare_composed_cycle(&self, opportunity: &Opportunity) -> Result<BundlePlan> {
        let legs = &opportunity.legs;
        if legs.len() < 2 || legs.len() > MAX_COMPOSED_LEGS {
            return Err(ExecutionError::Unsupported(format!(
                "composed cycles support 2..={MAX_COMPOSED_LEGS} legs, got {}",
                legs.len()
            )));
        }
        let start = parse_mint(&legs[0].mint_in, "start mint")?;
        if start.to_string() != WSOL_MINT {
            return Err(ExecutionError::Unsupported(format!(
                "composed executor only supports WSOL-starting cycles, got {start}"
            )));
        }

        let mut mints = Vec::with_capacity(legs.len() + 1);
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
                "composed executor requires a closed cycle (end mint == start mint)".into(),
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

        // Quote every leg sequentially using the *minimum* output of the
        // previous leg, exactly as the per-leg path does.
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
                "quoted composed cycle output {leg_amount} <= input {amount_in}"
            )));
        }
        let gross_min = leg_amount - amount_in;

        // Fetch instruction sets for every leg.
        let mut cycle_legs = Vec::with_capacity(quotes.len());
        for (index, quote) in quotes.into_iter().enumerate() {
            let mut request = SwapRequest::new(self.wallet.pubkey_string(), quote);
            request.as_legacy_transaction = true;
            request.wrap_and_unwrap_sol = true;
            let instructions = self.jupiter.swap_instructions(&request).await?;
            tracing::debug!(
                leg = index,
                bytes = instructions.payload_bytes(),
                tables = instructions.address_lookup_table_addresses.len(),
                "composed leg instruction set"
            );
            cycle_legs.push(compose::CycleLeg {
                instructions,
                is_final: index + 1 == legs.len(),
            });
        }

        let composed = compose::compose_cycle(&cycle_legs)?;
        let table_keys: Vec<beast_dex::Address> = composed
            .lookup_tables
            .iter()
            .filter_map(|key| key.parse::<beast_dex::Address>().ok())
            .collect();
        let tables = resolve_lookup_tables(&self.rpc, &table_keys).await?;
        // One transaction for the whole cycle, so one base fee and one priority
        // fee - the cost model must reflect that or the guard undercharges.
        let intermediate_accounts = legs.len() - 1;
        let rent_lamports = self
            .config
            .ata_rent_lamports
            .checked_mul(intermediate_accounts as u64)
            .ok_or_else(|| ExecutionError::Config("ATA rent overflow".into()))?;
        let fees_lamports = self
            .config
            .transaction_fee_lamports
            // composed cycle + optional guard + tip
            .saturating_mul(if self.config.guard_enabled() { 3 } else { 2 })
            .checked_add(rent_lamports)
            .ok_or_else(|| ExecutionError::Config("cost overflow".into()))?;
        let tip_lamports = self.choose_tip(gross_min, fees_lamports)?;

        let (blockhash_string, _) = self.rpc.get_latest_blockhash().await?;
        let blockhash = parse_hash(&blockhash_string)?;
        let transaction = build_composed_transaction(
            &composed,
            self.wallet.pubkey(),
            blockhash,
            &self.wallet.keypair,
            &tables,
        )?;
        let cycle_encoded = base64::engine::general_purpose::STANDARD.encode(
            bincode::serialize(&transaction)
                .map_err(|error| ExecutionError::Transaction(format!("serialize v0: {error}")))?,
        );

        let mut encoded_transactions = vec![cycle_encoded];
        let mut signatures = vec![transaction.signatures[0].to_string()];
        let guard_required = if self.config.guard_enabled() {
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
            self.config.guard_threshold(
                start_balance,
                tip_lamports,
                self.config.transaction_fee_lamports,
            )?
        } else {
            0
        };
        let tip_account = self.choose_tip_account().await?;
        let tip = build_tip_transaction(
            self.wallet.pubkey(),
            tip_account,
            tip_lamports,
            blockhash,
            &self.wallet.keypair,
        )?;
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
            composed: true,
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

/// Assemble and sign a composed cycle into a **v0** transaction.
///
/// A composed multi-leg cycle cannot fit a legacy encoding: each leg adds about
/// 25 accounts and a legacy message costs 32 bytes per unique key, so even two
/// legs overflow the 1232-byte limit. Jupiter already returns the lookup tables
/// it used, so the composed transaction is rebuilt as v0 against those tables,
/// where each extra account costs one index byte.
fn build_composed_transaction(
    composed: &compose::ComposedCycle,
    wallet: solana_sdk::pubkey::Pubkey,
    blockhash: Hash,
    keypair: &Keypair,
    tables: &[compose::AddressLookupTable],
) -> Result<VersionedTransaction> {
    if composed.instructions.is_empty() {
        return Err(ExecutionError::Transaction(
            "composed cycle has no instructions".into(),
        ));
    }
    let mut transaction = compose::assemble_v0(composed, wallet, blockhash, tables)?;
    let message_bytes = transaction.message.serialize();
    if message_bytes.len() + 65 > compose::MAX_TRANSACTION_BYTES {
        return Err(ExecutionError::Unsupported(format!(
            "composed cycle message is {} bytes, above the {}-byte limit",
            message_bytes.len() + 65,
            compose::MAX_TRANSACTION_BYTES
        )));
    }
    transaction.signatures = vec![keypair.sign_message(&message_bytes)];
    Ok(transaction)
}

/// Serialized size of `LookupTableMeta`, after which the raw address list
/// begins. Verified against `solana-address-lookup-table-interface`.
const LOOKUP_TABLE_META_SIZE: usize = 56;

/// Fetch the address lookup tables a composed cycle references.
///
/// Jupiter's per-leg responses cite the ALT keys it used; the table *contents*
/// exist only on-chain, so they must be read before a v0 message can be built.
///
/// The address list has **no length prefix**: the account's data length carries
/// the count, which is why this reads `(data_len - META_SIZE) / 32`. Reading a
/// u64 count from the metadata instead returns garbage and silently produces a
/// wrong (or empty) table, so the derived count is asserted to be exact.
async fn resolve_lookup_tables(
    rpc: &RpcClient,
    tables: &[beast_dex::Address],
) -> Result<Vec<compose::AddressLookupTable>> {
    let mut unique: Vec<beast_dex::Address> = Vec::new();
    for table in tables {
        if !unique.contains(table) {
            unique.push(*table);
        }
    }
    if unique.is_empty() {
        return Ok(Vec::new());
    }
    let accounts = rpc.get_multiple_accounts(&unique).await?;
    let mut resolved = Vec::with_capacity(unique.len());
    for (key, account) in unique.iter().zip(accounts) {
        let account = account.ok_or_else(|| {
            ExecutionError::Rpc(beast_rpc::RpcError::Decode(format!(
                "address lookup table {key} not found on chain"
            )))
        })?;
        if account.data.len() < LOOKUP_TABLE_META_SIZE {
            return Err(ExecutionError::Rpc(beast_rpc::RpcError::Decode(
                "address lookup table account too small".into(),
            )));
        }
        let remainder = account
            .data
            .len()
            .checked_sub(LOOKUP_TABLE_META_SIZE)
            .ok_or_else(|| {
                ExecutionError::Rpc(beast_rpc::RpcError::Decode("alt underrun".into()))
            })?;
        if remainder % 32 != 0 {
            return Err(ExecutionError::Rpc(beast_rpc::RpcError::Decode(
                "address lookup table account length is not address-aligned".into(),
            )));
        }
        let count = remainder / 32;
        let addresses = account
            .data
            .get(LOOKUP_TABLE_META_SIZE..LOOKUP_TABLE_META_SIZE + count * 32)
            .ok_or_else(|| {
                ExecutionError::Rpc(beast_rpc::RpcError::Decode(
                    "address lookup table address range out of bounds".into(),
                ))
            })?
            .chunks_exact(32)
            .filter_map(|bytes| beast_dex::Address::from_slice(bytes).ok())
            .collect::<Vec<_>>();
        if addresses.len() != count {
            return Err(ExecutionError::Rpc(beast_rpc::RpcError::Decode(
                "address lookup table address decode produced a short table".into(),
            )));
        }
        resolved.push(compose::AddressLookupTable {
            key: *key,
            addresses,
        });
    }
    Ok(resolved)
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

    fn hex_decode(parts: &[&str]) -> Vec<u8> {
        let joined = parts.concat();
        (0..joined.len() / 2)
            .map(|index| u8::from_str_radix(&joined[index * 2..index * 2 + 2], 16).unwrap())
            .collect()
    }
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
    fn composed_cycle_pays_one_fee_not_one_per_leg() {
        // A composed cycle is a single transaction, so a 3-leg cycle costs the
        // same 3 network fees as a 1-leg cycle, not 6 like the per-leg path.
        let config = ExecutionConfig {
            guard_program: "11111111111111111111111111111112".into(),
            ..ExecutionConfig::default()
        };
        let rent = config.ata_rent_lamports * 2; // two intermediate mints
        let cycle = config.transaction_fee_lamports * 3 + rent;
        assert_eq!(
            cycle,
            config.transaction_fee_lamports * 3 + config.ata_rent_lamports * 2
        );
        // The per-leg equivalent for the same cycle would be N + guard + tip.
        let per_leg = config.transaction_fee_lamports * (3 + 2) + rent;
        assert!(
            cycle < per_leg,
            "composition must be strictly cheaper than per-leg bundles"
        );
    }

    #[test]
    fn decodes_a_real_mainnet_address_lookup_table() {
        // A real mainnet ALT (Jupiter's WSOL/USDC route), captured via
        // getAccountInfo. 7832 bytes = 56 meta + 243 * 32 addresses.
        //
        // The rule under test: the address list has NO length prefix, so the
        // count must be derived from the account length. Reading a u64 from the
        // metadata instead returns garbage and silently yields an empty table.
        let data = hex_decode(&[
            "01000000ffffffffffffffff7b14de130000000016017d0ea3424053053bbbd860f74c32f04bc3dc872adf9a4f4ed170c24ee0f43ed3000057670bc5",
            "297b9ae5c50a95679dff0df4b2498452b693b90aadaabe5f1d93d6f6c086077557cd41426ee3faa6d9506bbf1f3ba4962e17d075664df94d3ecfbd30",
            "4f01590bf86175d118e9649e5421af2afa0d48d2ad861f0e92f05ff367aa022876ee505ddd409a2f0998b9c863ceb184a889da8c3c4d0579c535a88c",
            "d1a18bb506ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a9a5d5ca9e04cf5db590b714ba2fe32cb159133fc1c192b722",
            "57fd07d39cb0401ea76d00b86927b035cfbd6d7658016e0646122d05fc2b7d41f74fcc770eef5926241db3f4cb01b146d9627b6379a1e220ced30a31",
            "b53c1f3e13a84399b845732a2cf829f2bbf05eaea0c0bf2e806135a8c00d63c2d299bb383ddccd2cc1d75402f177bd0d719f363cb0fe37a7478558c1",
            "615765fcc1950f6b4e72d494a150dd8a6399b1caa2bc81409f80b138622521ae63e5c3d017ba566d44de04b38e7793e604b87e7714f60c23bb27f12a",
            "d8421a64e7d942c46a8a3683378ca1acaab077d933a66db0f584483355e6d7c92f0304e258c713f19442a10728f97cb70d0cbe914157b0580f31c5fc",
            "e44a62582dbcf9d78ee75943a084a393b350368d228993085b5c6c8cbbea315f04c482eee76f7f8b8888afc598ecfb9d787a09b09ee9dce08826c66b",
            "d8525d5a0dbd92106a95161445e2eaa1a7a23e8af7759a3ff59177d106ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a9",
            "4bd949c43602c33f207790ed16a3524ca1b9975cf121a2a90cffec7df8b68acd06ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf585",
            "7eff00a94bd949c43602c33f207790ed16a3524ca1b9975cf121a2a90cffec7df8b68acd4157b0580f31c5fce44a62582dbcf9d78ee75943a084a393",
            "b350368d228993081d1e9569f1c1764192ee5e514f4ca7e2221f56312ffa4dfc19f5083edf1913f5b7f2f5c9dac9291ed917dd06ac2a5fc7f98d11e8",
            "717c8fbccd35f8bfd1fcad069fdbfc0f06073ec3ce9a481243acce773ed95ae6c9364001cf8326063873d380a5d5ca9e04cf5db590b714ba2fe32cb1",
            "59133fc1c192b72257fd07d39cb0401e9bcc3f1c76b5a33ce6e18aa947e36478f15a0da109459108feda0a022f55a251816e66630c3bb724dc59e49f",
            "6cc4306e603a6aacca06fa3e34e2b40ad5979d8d9f4c7f823400d2c3bd48c17c4959b81b3dc5e29332aece75c20c4a0e8892e78706ddf6e1d765a193",
            "d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a9f4bae4e3c067f75b9ce8ce2d755837998cc5cae87d2d881f2a04356c2599a20de54206c1",
            "15581714f29881aa2b849b400a328678fcaff498b9d22ddf5213ae5eb0f3695b228b31025acd1555c7929fce1ead201a971645da26e163c29e5451c2",
            "eb53fd534f1608bb3cd0374d945f488126da506d4ffc55767c0e61c81b1609d4b651c3a9e82cee459e05c9336391631daa911f159a7b3314796a4f63",
            "f498bcae06ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a94157b0580f31c5fce44a62582dbcf9d78ee75943a084a393",
            "b350368d228993084455e710092b85cb4e0aaa7b051f1cf434ff4d3f0c07314597d0d1cbd4603b10f6b6170d3fbe218df32fa40ca470bdcb02014882",
            "c11db2a7c42bed6145e8b4474bd949c43602c33f207790ed16a3524ca1b9975cf121a2a90cffec7df8b68acd95784f98357755227e7783688292e20f",
            "7e3b828be2f0a36858d1444af9871b1ae8a3c369f0b68509ecdcd38f5a34cb8888012d729dde87454a0963cf51b3d48ebbee0acee706a1b2d683f523",
            "57c43e8adcc89ab54d1f7188467d6315d7928deb06ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a9a18e27e0e0f242d1",
            "c1bbf4dfd45d68b4b83dc2bf121003f21a885d381eabefeba5d5ca9e04cf5db590b714ba2fe32cb159133fc1c192b72257fd07d39cb0401e494f90cc",
            "0da2978719bd12b7f5f45b0f8a2c5a8250531ff4e9f58654d1e1dd214ce3b0b60ceaabb165ac3c6ebae7d7f3fd104a67259dd66a7d14e068af6b075b",
            "816e66630c3bb724dc59e49f6cc4306e603a6aacca06fa3e34e2b40ad5979d8d9330e20de5d00ea74b43c798637a19204823216ecbd6eb98cc975b26",
            "e9856091854df0252708087c3661ff2fed7b82cf2ebbde99d62035ccfa638954be0e413eb35f15a0ba0261048a6790e76b094e1f14348b06409e13c5",
            "c116d739a9fca84296fd111ffb06cdaae804e391d43d3737d7196707f4f1474c138ac5a3f657c8494231c99fec20e6ac64d1d53a257ad8d3b796f67d",
            "926c3b8cb2e22416bf7559a0d500c8ab3ffad0a86bab5f536531c6dd5e62b0626d33cdeddf82ead7eec50e465c26ca264cf63bae48bc09cbae3ea86d",
            "6b8d8b19f3e05f2a7f93b6a1733bd2a4069b8857feab8184fb687f634618c035dac439dc1aeb3b5598a0f0000000000106ddf6e1ee758fde18425dbc",
            "e46ccddab61afc4d83b90d27febdf928d8a18bfc06ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a9b4a98647933dc3e8",
            "5191d43b1ec9a85367cc9a08c33ece9a4be4d8f011fa7dfa69a330f2824ef94a9e4a15cdcbb893b363785fcdbaed5c6f68df270acee9fccbc9147bd1",
            "eaa03cbe6de465c83dfa81687a4ef72293218432324fc1f6615a3f9e0c14defc825ec67694250818bb654065f4298d3156d571b4d4f8090c18e9a863",
            "fb68db380ff982cbf5f0df1e70e87b15eb28d5bd4649f39822d3c8857ab0440607b46728c503a7c81598ec5167b9b632a0da7bdce98f07c5967b10ed",
            "6f78a1ce06ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a9890ba644fe1f55aa19f11cd2d2ec14d3233b6e0a4beaeef7",
            "2b69858e21e170d689583139352480adfda27d674e2839f1d1f86aa3e75ed417f2c6968f3cae0aad8c97258f4e2489f1bb3d1029148e0d830b5a1399",
            "daff1084048e7bd8dbe9f859e54a709528839f61c0b9b86079891c139216e47a71b62fb73bec72169458745e069b8857feab8184fb687f634618c035",
            "dac439dc1aeb3b5598a0f000000000012192e97d2a45b459eafef2f05db9d0a7c3933eea962c2f123e4f86e682ed2215638373000ea22cb264d34aff",
            "64a04b5efabfbb74ddcd048997b1981547d7d1105721ed8c1e443747f23c9edb177e9557f654ea49921f0d8bcfb08763dbefd9737c36ae2a53d314d2",
            "0a30e35dcd5775874284a4f4a7061c693a6046344e2b7a68d500c8ab3ffad0a86bab5f536531c6dd5e62b0626d33cdeddf82ead7eec50e46c9ccbb63",
            "1ba500fef3343433c1b178daad18c8272196f6d02756c804265ce38b069b8857feab8184fb687f634618c035dac439dc1aeb3b5598a0f00000000001",
            "8db178275878be7d8282fb44c1fd3dfead22bd8cdd3b09237247901ec25b19b6d52ef42292901a4a02a38cae6d034ba61114a8d646a1b703b0a27d95",
            "5cb67623269a8a34c95d8034f935f7252214979be65f0d19cfc0c0420a928f018f72700906ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b3791",
            "3a8cf5857eff00a96dd151e9344d504b607c3bdc01a603c294d0fd702fd7075e53818b92faf3a78bb9e2d5ca16c40065d07fba6ae9dff573df805aa4",
            "3fdc27103909fc64e9e92a1d06ddf6e1ee758fde18425dbce46ccddab61afc4d83b90d27febdf928d8a18bfc93fcac975054bf5b87f85f8a287fb45a",
            "6a4f79611109712f82a1e3999df8f9be54c6b08f22b4d7f5c5be4eaadcbfd8231b178ac292166a79fe5fe1bfb51eb018816e66630c3bb724dc59e49f",
            "6cc4306e603a6aacca06fa3e34e2b40ad5979d8d2473f882d47f860292d78d65c5174230f7381006239db4ba487667da2a10cb01d357bab0f6f1781a",
            "6c867a9321b497c7eebb2e9fa03a1ed276adb0373548bdaba5d5ca9e04cf5db590b714ba2fe32cb159133fc1c192b72257fd07d39cb0401e6d201a8c",
            "d66a622d0d25ad8d9cc867ed9d9670edcfa15578cdc235eb82117ee91713f71817db7034e8b2c63286e66f6fc8da7e80d2f78248a536cac67da63b62",
            "247e05d373ffc2072d37470edd6ddc064be27e3f244aea1828905775e428c4e006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf585",
            "7eff00a9b2e6e51d22ca532b18a9d0f4bf7fba302dafb8bbfb2e968d5746ab0d97da359fd500c8ab3ffad0a86bab5f536531c6dd5e62b0626d33cded",
            "df82ead7eec50e46b49962131682afc65a89b8b11b20339b774f1633ff199191b5cbd8948b69d1bd94365f93d0ea53929a5bb6105b50157c34c51c11",
            "68314727b25fa0a638087c2c54b88d2d1b9516f8d56acff2814d04e5d91389aa951c08ee3541e50a7b1f708606ddf6e1d765a193d9cbe146ceeb79ac",
            "1cb485ed5f5b37913a8cf5857eff00a930d544cf9025f84e79feb42f28a4bae4b59438252a55cf06568a46a670519613ba58639f10e48b551f0270a7",
            "473f24ea28cef80531bb5dde6bba1d09d12c8d1d06ddf6e1ee758fde18425dbce46ccddab61afc4d83b90d27febdf928d8a18bfc0d318b874ac3e345",
            "cabd503be6d36ad85853842cd45e4995533295182b3daa66069b8857feab8184fb687f634618c035dac439dc1aeb3b5598a0f00000000001b270d67f",
            "a98c51cf0213051358962baf35742bed59c9d9445e9c0d0c85c7cd91e623785d13efae00a153c4000d709aa0800b316b91524148a1420c9f59ac5aa5",
            "06ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a9b61d05792df47ddbb67a322fda1c5fb78564959fbbe8e0027610b5cd",
            "3c464fe3d5d7a7271d8afc62c140177d476680d39ea1741cb1f53c955da47e15da7b69e1431cda01f58ae4ed2b9957afed2f3936d59e19bac8e344de",
            "4b65e94baf2aa79b8f5645bdb0d6793c20aec69f7d20be40b32a464d2f8baea27105c96697465c252d641949e08e0587b8b82778037ec3c1c8b247d0",
            "a9c651f7190b6f8ce70dafd45297f1b40a368e6b70871f0708e7397ec3097ff0c5368a59e617580cdbc366ea069b8857feab8184fb687f634618c035",
            "dac439dc1aeb3b5598a0f00000000001081490df9e1c1199327a3407b3050c7dad31af1dcbefda0e24881ec79f0b340104e9e12fbc84e826c932cce9",
            "e2640cce15590c1c6273b0925708ba3b8520b0bce26bca7930b4757fe30a266d8f3813aac9f7a73eb45b22bfd5efe593e470ec7382bc6b8ca2be01f5",
            "93b742a973e3370738072034efd92f08b8b19fc74e00cff1ae0fff1576966931c4b23529b708c957f1c4593ae3603fb96f3759285f207bf9a5d5ca9e",
            "04cf5db590b714ba2fe32cb159133fc1c192b72257fd07d39cb0401edd724ce82e334ebac066341e1df8a855cb45961eda10d86e332e7611bda5c927",
            "068f2734415b675c631f5b5a3de5c8cfc3cf16be98a9fb817188987b17059140bc803e64dba4634e4fb353fc01e81e88a12dcfe2a54bce77c93fd6c5",
            "7d710f15274cce36c0ef16194116a17b2b7292bef6e7354e7fc551c2b0675b3a3047b7eb4cf4b42f4e82e8a4db92e66efdb4935fe13039af83b17fa6",
            "20f42b3841c8b37006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a9816e66630c3bb724dc59e49f6cc4306e603a6aac",
            "ca06fa3e34e2b40ad5979d8dfc39066f804ec84d465acc0cafceb1debdfb9e4751e3c99ecb5463ac63cd89c96d9bfce2ad216928b9c7c3b8e28bb33e",
            "50e5a5253f9f2dc87b1319f8994dc99606ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a909a09cc52bcc7cd1e408dd9b",
            "2dd1003edf10bec8b7fbcbf664bec6065685d833638373000ea22cb264d34aff64a04b5efabfbb74ddcd048997b1981547d7d110d500c8ab3ffad0a8",
            "6bab5f536531c6dd5e62b0626d33cdeddf82ead7eec50e4603320542e0d6e0673c513f19b164d9acba28a833fbc688961ae32372b2ec228af843fbfa",
            "c079809b85c9b3125b95670c0fdc40d9989936ff32d1980c4baae29297e3eec9c73cf1699c6bd07c808ce0cb5787dfe8bc7ebcb90acfd80b32264264",
            "db2ff4d877e3d713c1c25dec96718aee30491b23caa2783edd093ff49ea6622be09781b13844c2ac8bd7bd48d37cbcdb18359214bdb56592c2aefb73",
            "f3ea15455993eaf25412f08ceac83b4aba42138aa2d5c13057d1968cbb558bfb75169fc8069b8857feab8184fb687f634618c035dac439dc1aeb3b55",
            "98a0f00000000001fcc829d58ceb917567dd1b5be01c895e118df7717c70b893647a55e8375e2edc760c5b222f9428ed0eefef10da08561a0d25fb15",
            "c708300196cfcdd0faf0a42b2672981c77ef5849f5ba9d3fbcdfb2ebfc4dffc54e2400b9dbed2b07697e201e06ddf6e1ee758fde18425dbce46ccdda",
            "b61afc4d83b90d27febdf928d8a18bfcd500c8ab3ffad0a86bab5f536531c6dd5e62b0626d33cdeddf82ead7eec50e469647cac59dd8201ae9f2ad1f",
            "91c1756a43c622415f0ddc6f6cfe32854d784224069b8857feab8184fb687f634618c035dac439dc1aeb3b5598a0f00000000001e08b3ac52a787d65",
            "0a570fbe51cb6a117973e5f104b09272f3c6d1bf6ebcdd990e5f7aec3fadfa88b978fbb3d6651c159f87cdba540f215e703147b01dc963e967fe2068",
            "9d7c686f0bc8fb3ae62e3f3785a4f2d7959bb895dcaca483e4e4953806ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a9",
            "98489a28e37e7dcd707c2c334e00adcd41102caf65557af80d3c6c9d9871cb204d7620f5ed5c758097ba3e95e0f7209bc02a36a45581617ba2c39b66",
            "736b1ddfc52e6c621a4925b89a30fb8c90bae631f9418599773276778554b7fae299fb3b4f428e983809fef02308bbf42db4a9396b0fb86b53862370",
            "769a4b2cd83e72a0069b8857feab8184fb687f634618c035dac439dc1aeb3b5598a0f00000000001b7a0115b88d287295f1948f36e3995f70cb61b1f",
            "2b46505e4368c71ea0a026e106ddf6e1ee758fde18425dbce46ccddab61afc4d83b90d27febdf928d8a18bfc5539cbf2d28d40f0191345a592663839",
            "18bf1aae7e6affb50da671f3dba5118506ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a9d500c8ab3ffad0a86bab5f53",
            "6531c6dd5e62b0626d33cdeddf82ead7eec50e4637b3f735f0e64db485a1cfe12dc4dab37819289b97018282ba8103d4583e716ba8663ff6222d8d74",
            "739bf7166d6b593bf32ec573b27e415d800972504d693e7406ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a907b46728",
            "c503a7c81598ec5167b9b632a0da7bdce98f07c5967b10ed6f78a1ce069b8857feab8184fb687f634618c035dac439dc1aeb3b5598a0f00000000001",
            "8c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859eb92122c0b8e7e523ee6dfe5dd2c60e6bf21914dd3cb90e879be95eb",
            "4542292b890ba644fe1f55aa19f11cd2d2ec14d3233b6e0a4beaeef72b69858e21e170d6e54a709528839f61c0b9b86079891c139216e47a71b62fb7",
            "3bec72169458745e838474292e675a94b436ecb0a9988942328a83ddc6233802961267c5cd6117cbd9a2a7fb25099a6676f35b12138cee1e37527f1a",
            "bba98c207272ee77d6373a580c14defc825ec67694250818bb654065f4298d3156d571b4d4f8090c18e9a863fed2111b99733ce1bbe56d32bf82b5b7",
            "53f8aea655ce1dc06be28c148c1d9eeaa26317a53ba0fd68c4c953ec30f0e09b8e7368751cb2815486cae3e99d0af9dd1016070d717316f64f99e4a0",
            "417e838ac369c0b2e385c393e7a4a196c59a1ecfb3362c51d1ed49bd0b1c88350eff40d3c486bb78646a8a9ee408e12c43995da76552164f73f1535b",
            "7570d987859fac67e953ee706fb720be8e3dcf3eedaa00e5e12bedaa1275b43c7da5f8e88778a9a04f9e1f9a3d9e00a3595e7f035052f1797efd62e4",
            "2790ed549aa1f9cf93adec7b17b4c8011bd3ba146a160b20a988aa2c816e66630c3bb724dc59e49f6cc4306e603a6aacca06fa3e34e2b40ad5979d8d",
            "99101a473e60d729787518bc52daa121bcc6a8ce8be259527b98381e0cf28826069b8857feab8184fb687f634618c035dac439dc1aeb3b5598a0f000",
            "000000018c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f85906ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b3791",
            "3a8cf5857eff00a90c14defc825ec67694250818bb654065f4298d3156d571b4d4f8090c18e9a8639c25247c7afe68779c4e7cdc4139e56b84cdee78",
            "a4ed9b474ea1fa8ee93ce2255d0e326d4c6ad7ff7d8f533fc3090854bd1fef329108400f5f55d79e4b878051890ba644fe1f55aa19f11cd2d2ec14d3",
            "233b6e0a4beaeef72b69858e21e170d696de3bd481df9130afd2296d8bb3e7d4a73e2a8c30e21049f10d39f18f8a5d6f0a14d65c8d4729b6e7c83dfd",
            "67f65575e205bef0a7b9016b9dc5395c475d6af27a011e52b6020061c8e2ce0b2c41f9ab1220b3457a98f4251278bfdf36c1469d608ccc1dfce961b4",
            "3b779c191505a6e2d3bf45d5a4db4618ad76c82d61754535e54a709528839f61c0b9b86079891c139216e47a71b62fb73bec72169458745ee004c87c",
            "eb98fa5ce47f803806fd2c7945d29524959aec00ded97814f38f7846aaaf808220b83ab9736c7e81cc3cd3a894c57504b41ac05f34015f0b68cca095",
            "06ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a906ddf6e1ee758fde18425dbce46ccddab61afc4d83b90d27febdf928",
            "d8a18bfc37fe59920374a419f70b548b800ed9b5b3b83a8fbb5bb2f7cdbfdd293b379e50c713b7d4f947cde861eed83effafd8295788582bd0c328ed",
            "b4b4000415db5f1862ba110ea124e4d52e8bd15f931f6d3c14a19cb67ab0cfc855de8f5bdc73632fddba402b4da0949fcfc98938b60d380d57900ac5",
            "b1ff3d536654b32ce373bd45277d564c1e598ed19a3fdb4e954eef9694980944ab09dd9eb50ea1d6c09e8a01fd966c613bcababe113c152a85306466",
            "9c4c4ad77e045a2e6823d545392f6b33156fd6190867067e6aeba379e1799af5d4d7264637a1c427672dc297141ff053d887b3a1e0710ed680f2ca65",
            "34638ad70221a5ab4bb214adffee56939988e1acef5b179b6bff9342ce7473a98efc3b8217882e13b320238dc2c6bb6fc4aaaebc06ddf6e1d765a193",
            "d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a94157b0580f31c5fce44a62582dbcf9d78ee75943a084a393b350368d228993084bd949c4",
            "3602c33f207790ed16a3524ca1b9975cf121a2a90cffec7df8b68acdb957597c1e7ac41f545821f5c82b8e62f66db525c237cb96c3b9dfbe5dccf224",
            "c7fda002236b2aeb79942d7fae909ba5de95c188f469a0743062ab5a31afb9eb8c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8",
            "dbe9f859608ccc1dfce961b43b779c191505a6e2d3bf45d5a4db4618ad76c82d61754535890ba644fe1f55aa19f11cd2d2ec14d3233b6e0a4beaeef7",
            "2b69858e21e170d60c14defc825ec67694250818bb654065f4298d3156d571b4d4f8090c18e9a863069b8857feab8184fb687f634618c035dac439dc",
            "1aeb3b5598a0f00000000001e54a709528839f61c0b9b86079891c139216e47a71b62fb73bec72169458745e5d0e326d4c6ad7ff7d8f533fc3090854",
            "bd1fef329108400f5f55d79e4b878051226015bc44a12170d77b01c6eab0de3a0a846bec22d0004ff742ef048747894ff238eabb977ebae027fa7267",
            "bdc3a8fcd5453d2ad9ab4de698ada25ef28ed598e004c87ceb98fa5ce47f803806fd2c7945d29524959aec00ded97814f38f7846b5b5fe85385ac661",
            "097f05c600ebb18014b9ad0b87b3ea8caa6c202ca5e91bbbdfa95d3545deed345652911b52ddf8e563aa861c78786e97cd3eedcc87c3f3f59c25247c",
            "7afe68779c4e7cdc4139e56b84cdee78a4ed9b474ea1fa8ee93ce22506ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a9",
            "c383212da273f0bf659219a6683557faf28d74923fe6d093c48c8ba12834d0724132279e97dc403190adbccf0e96868f768fb275db11b592ed5ca652",
            "ab692eed1156c7593acecfd5dd4b54945b03b62fd2525d9cba041ab0ec03852ef62a86f2b270d67fa98c51cf0213051358962baf35742bed59c9d944",
            "5e9c0d0c85c7cd9104e9e12fbc84e826c932cce9e2640cce15590c1c6273b0925708ba3b8520b0bc069b8857feab8184fb687f634618c035dac439dc",
            "1aeb3b5598a0f00000000001a6137f13168231c269bbaca9027238c0ad76b70ed2c73c0cd9140106ef1a44cb6d249b30679a54350db2f631d4947320",
            "be6b985958ca18e8f35ac5174156e0709758c29b34da7541c656856226438d37dbc104d30f77bb9c5737b75e375ff42c3fa7db917b39543b819d2f6a",
            "f14441ae78557e0f59cd93204e3040fdda2481d8888ec709e0485ba64061c712f630cc66d7fad98c15e60f449ee837f3c428113806ddf6e1d765a193",
            "d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a9b24692e923cf20df6ed1cb7a37fe72f95d43fd6d1788acf991b33e6532cc6896fa78bd63",
            "c24a13cc37600435bb2e158c29f5e8a16cb44d76aa3d6e0288187dc0a5d5ca9e04cf5db590b714ba2fe32cb159133fc1c192b72257fd07d39cb0401e",
            "06ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a9",
        ]);
        assert_eq!(data.len(), 7832);
        let count = (data.len() - LOOKUP_TABLE_META_SIZE) / 32;
        assert_eq!(LOOKUP_TABLE_META_SIZE + count * 32, data.len());
        assert_eq!(count, 243);
    }

    #[test]
    fn wallet_address_roundtrips() {
        let wallet = Wallet::from_keypair(keypair());
        assert_eq!(wallet.address().to_string(), wallet.pubkey_string());
    }
}
