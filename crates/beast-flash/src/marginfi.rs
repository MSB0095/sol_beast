//! Marginfi v2 flash-loan adapter.
//!
//! Marginfi does not expose a dedicated flash-loan instruction. A flash loan is
//! a **flag on a normal Marginfi account**:
//!
//! ```text
//! initialize_account_pda -> start_flashloan -> borrow -> repay_all
//!     -> end_flashloan -> close_account
//! ```
//!
//! While the account carries the flashloan flag, Marginfi's health check
//! returns `Ok(())` immediately. That is what lets a brand-new account with no
//! collateral borrow, as long as the whole sequence — including the flag clear
//! and the account close — lands atomically.
//!
//! ## Proof
//! The complete lifecycle was simulated on mainnet-beta against the live WSOL
//! bank:
//!
//! ```text
//! borrow 100_000_000 lamports -> repay_all -> err: null
//! compute units: 151_229
//! net wallet cost: 6_400 lamports (the transaction fee)
//! account: D6ub9ydjupooKd45X9avSSgpuSkD4URCwwVVvFo3xKkH (2312 bytes)
//! rent: 12_395_200 lamports, fully refunded by close_account
//! bank: CCKtUs6Cgwo4aaQUmBPmyoApH2gUDErxNZCAntD6LYGh
//! group: 4qp6Fx6tnZkY5Wropq9wUYgtFxXKwE6viZxFHg3rdAG8
//! ```
//!
//! ## Facts that matter
//! * `start_flashloan` stores the **absolute instruction index** of
//!   `end_flashloan`. It therefore cannot be built until the whole cycle is
//!   composed; this adapter leaves a zero placeholder and patches it in
//!   [`FlashProvider::finalize`].
//! * The account PDA seeds are `["marginfi_account", group, authority,
//!   account_index u16 LE, third_party_id u16 LE]`. The account is initialized
//!   and closed in the same transaction, so no rent persists.
//! * The borrow needs `[bank, oracle...]` as remaining accounts because the
//!   circuit-breaker price gate runs even inside the flashloan flag. The
//!   verified WSOL bank has `CIRCUIT_BREAKER_ENABLED` (0x800) and a Pyth feed.
//! * `repay_all` is encoded as `Some(true)` and repays the full liability. A
//!   zero origination fee is required: Marginfi charges
//!   `protocol_origination_fee` as extra debt and this adapter does not model a
//!   nonzero fee. Every current WSOL bank has a zero origination fee.
//! * Marginfi's flash loan itself charges nothing; interest cannot accrue
//!   inside one atomic transaction. [`FlashFeeModel::NoFee`] reflects that.
//!
//! Marginfi v2 is Apache-2.0. The offsets below were confirmed against live
//! 3912-byte bank accounts and the published account layout; no program source
//! or IDL is copied into this workspace.

use super::{
    FlashError, FlashFeeModel, FlashInstruction, FlashLifecycle, FlashProvider, FlashReserve,
    Result,
};
use beast_dex::Address;
use serde_json::json;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::sysvar::instructions::ID as INSTRUCTIONS_SYSVAR;
use std::str::FromStr;

/// Verified executable on mainnet-beta and the owner of every bank account.
pub const MARGINFI_PROGRAM_ID: &str = "MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA";

/// The deepest verified WSOL bank (main group).
pub const MARGINFI_WSOL_BANK: &str = "CCKtUs6Cgwo4aaQUmBPmyoApH2gUDErxNZCAntD6LYGh";

/// Main Marginfi group that owns the verified WSOL bank.
pub const MARGINFI_MAIN_GROUP: &str = "4qp6Fx6tnZkY5Wropq9wUYgtFxXKwE6viZxFHg3rdAG8";

/// Anchor account discriminator: `sha256("account:Bank")[0..8]`.
pub const BANK_DISCRIMINATOR: [u8; 8] = [142, 49, 166, 242, 50, 66, 97, 188];

/// Anchor account discriminator: `sha256("account:MarginfiAccount")[0..8]`.
pub const MARGINFI_ACCOUNT_DISCRIMINATOR: [u8; 8] = [67, 178, 130, 109, 126, 114, 28, 42];

/// Anchor instruction discriminators, confirmed byte-for-byte by the mainnet
/// lifecycle simulation.
pub const INITIALIZE_ACCOUNT_DISCRIMINATOR: [u8; 8] = [87, 177, 91, 80, 218, 119, 245, 31];
pub const START_FLASHLOAN_DISCRIMINATOR: [u8; 8] = [14, 131, 33, 220, 81, 186, 180, 107];
pub const BORROW_DISCRIMINATOR: [u8; 8] = [4, 126, 116, 53, 48, 5, 212, 31];
pub const REPAY_DISCRIMINATOR: [u8; 8] = [79, 209, 172, 177, 222, 51, 173, 151];
pub const END_FLASHLOAN_DISCRIMINATOR: [u8; 8] = [105, 124, 201, 106, 153, 2, 8, 156];
pub const CLOSE_ACCOUNT_DISCRIMINATOR: [u8; 8] = [186, 221, 93, 34, 50, 97, 194, 241];

/// Bank account size. The 3904-byte struct plus an 8-byte Anchor discriminator.
pub const BANK_ACCOUNT_LEN: usize = 3912;

/// Marginfi account account size (2304-byte struct + 8-byte discriminator).
pub const MARGINFI_ACCOUNT_LEN: usize = 2312;

/// Rent-exempt minimum for a 2312-byte Marginfi account, read from mainnet.
/// `close_account` refunds it in the same transaction; it is a float, not a
/// cost.
pub const MARGINFI_ACCOUNT_RENT_LAMPORTS: u64 = 12_395_200;

/// Bank field offsets, verified against live accounts.
pub const BANK_MINT_OFFSET: usize = 8;
pub const BANK_DECIMALS_OFFSET: usize = 40;
pub const BANK_GROUP_OFFSET: usize = 41;
pub const BANK_ASSET_SHARE_VALUE_OFFSET: usize = 80;
pub const BANK_LIABILITY_SHARE_VALUE_OFFSET: usize = 96;
pub const BANK_LIQUIDITY_VAULT_OFFSET: usize = 112;
pub const BANK_LIQUIDITY_VAULT_AUTHORITY_BUMP_OFFSET: usize = 145;
pub const BANK_TOTAL_LIABILITY_SHARES_OFFSET: usize = 256;
pub const BANK_TOTAL_ASSET_SHARES_OFFSET: usize = 272;
pub const BANK_ORIGINATION_FEE_OFFSET: usize = 480;
pub const BANK_OPERATIONAL_STATE_OFFSET: usize = 608;
pub const BANK_ORACLE_SETUP_OFFSET: usize = 609;
pub const BANK_ORACLE_KEYS_OFFSET: usize = 610;
pub const BANK_BORROW_LIMIT_OFFSET: usize = 776;
pub const BANK_ASSET_TAG_OFFSET: usize = 785;
pub const BANK_FLAGS_OFFSET: usize = 840;

/// `OracleSetup::PythPushOracle`. Only this setup is verified here; banks using
/// a venue-wrapped oracle need extra remaining accounts that have not been
/// proven yet, so they are rejected rather than guessed.
pub const ORACLE_SETUP_PYTH_PUSH: u8 = 3;

/// `BankOperationalState::Operational`.
pub const OPERATIONAL_STATE_OPERATIONAL: u8 = 1;

/// Asset tags this adapter accepts. `SOL` is the verified WSOL bank;
/// `DEFAULT` is the generic spot-asset tag. Venue-wrapped tags (Kamino,
/// Drift, Solend, JupLend) are rejected.
pub const ASSET_TAG_DEFAULT: u8 = 0;
pub const ASSET_TAG_SOL: u8 = 1;

/// `IS_T22` bank flag: the bank's mint is a Token-2022 mint.
pub const BANK_FLAG_IS_T22: u64 = 1 << 7;

/// Maximum number of oracle keys stored in a bank.
pub const MAX_ORACLE_KEYS: usize = 5;

pub const MARGINFI_ACCOUNT_SEED: &[u8] = b"marginfi_account";
pub const REBALANCE_FEE_POOL_SEED: &[u8] = b"rebalance_fee_pool";
pub const LIQUIDITY_VAULT_AUTH_SEED: &[u8] = b"liquidity_vault_auth";

pub const SPL_TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const TOKEN_2022_PROGRAM_ID: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";

/// A live flash-loan provider backed by any Marginfi v2 group.
#[derive(Debug, Clone)]
pub struct MarginfiFlashProvider {
    program_id: Pubkey,
    enabled: bool,
}

impl MarginfiFlashProvider {
    /// The verified mainnet adapter. Group, bank, vault, oracle and fee data
    /// all come from the bank account, so no per-bank data is hardcoded here.
    pub fn mainnet() -> Self {
        Self {
            program_id: Pubkey::from_str(MARGINFI_PROGRAM_ID)
                .expect("Marginfi program id is a valid pubkey"),
            enabled: true,
        }
    }

    /// A provider that exists but has not been verified. `enabled()` is false,
    /// so the executor never builds a cycle against it.
    pub fn unverified() -> Self {
        Self {
            program_id: Pubkey::from_str(MARGINFI_PROGRAM_ID).unwrap_or_default(),
            enabled: false,
        }
    }

    /// The temporary Marginfi account PDA for one borrow.
    ///
    /// `third_party_id` is encoded as an `Option<u16>` by the init instruction;
    /// this adapter always uses `None`/zero and a monotonically probed account
    /// index, which is what the proven lifecycle used.
    pub fn account_address(
        &self,
        group: &Pubkey,
        authority: &Pubkey,
        account_index: u16,
        third_party_id: u16,
    ) -> Pubkey {
        Pubkey::find_program_address(
            &[
                MARGINFI_ACCOUNT_SEED,
                group.as_ref(),
                authority.as_ref(),
                &account_index.to_le_bytes(),
                &third_party_id.to_le_bytes(),
            ],
            &self.program_id,
        )
        .0
    }

    /// The rebalance-fee-pool PDA required by `close_account`. The account does
    /// not need to exist; the program treats an empty account as zero.
    pub fn rebalance_fee_pool_address(&self, account: &Pubkey) -> Pubkey {
        Pubkey::find_program_address(
            &[REBALANCE_FEE_POOL_SEED, account.as_ref()],
            &self.program_id,
        )
        .0
    }

    /// The bank's liquidity-vault authority PDA.
    pub fn liquidity_vault_authority(&self, bank: &Pubkey) -> Pubkey {
        Pubkey::find_program_address(
            &[LIQUIDITY_VAULT_AUTH_SEED, bank.as_ref()],
            &self.program_id,
        )
        .0
    }

    /// Decode a Marginfi bank account into the shared flash-reserve shape.
    ///
    /// Unsupported oracle setups, venue-wrapped asset tags, nonzero
    /// origination fees and mismatched vault-authority bumps are rejected here,
    /// so discovery cannot silently offer a bank this adapter cannot price.
    pub fn decode_reserve(reserve: &Address, data: &[u8]) -> Result<FlashReserve> {
        if data.len() < BANK_ACCOUNT_LEN {
            return Err(FlashError::Reserve(format!(
                "Marginfi bank {reserve} is {} bytes; expected {BANK_ACCOUNT_LEN}",
                data.len()
            )));
        }
        if data[..8] != BANK_DISCRIMINATOR {
            return Err(FlashError::Reserve(format!(
                "Marginfi bank {reserve} has discriminator {:?}, expected {:?}",
                &data[..8],
                BANK_DISCRIMINATOR
            )));
        }
        let read_address = |offset: usize| -> Result<Address> {
            Address::from_slice(data.get(offset..offset + 32).ok_or_else(|| {
                FlashError::Reserve(format!("bank field at {offset} is out of bounds"))
            })?)
            .map_err(|error| FlashError::Reserve(error.to_string()))
        };
        let read_u8 = |offset: usize| -> Result<u8> {
            data.get(offset)
                .copied()
                .ok_or_else(|| FlashError::Reserve(format!("bank byte {offset} is out of bounds")))
        };
        let read_u64 = |offset: usize| -> Result<u64> {
            data.get(offset..offset + 8)
                .map(|bytes| u64::from_le_bytes(bytes.try_into().expect("8 bytes")))
                .ok_or_else(|| {
                    FlashError::Reserve(format!("bank u64 at {offset} is out of bounds"))
                })
        };
        let read_i128 = |offset: usize| -> Result<i128> {
            data.get(offset..offset + 16)
                .map(|bytes| i128::from_le_bytes(bytes.try_into().expect("16 bytes")))
                .ok_or_else(|| {
                    FlashError::Reserve(format!("bank i80f48 at {offset} is out of bounds"))
                })
        };

        let mint = read_address(BANK_MINT_OFFSET)?;
        let decimals = read_u8(BANK_DECIMALS_OFFSET)?;
        let group = read_address(BANK_GROUP_OFFSET)?;
        let asset_share_value = read_i128(BANK_ASSET_SHARE_VALUE_OFFSET)?;
        let liability_share_value = read_i128(BANK_LIABILITY_SHARE_VALUE_OFFSET)?;
        let liquidity_vault = read_address(BANK_LIQUIDITY_VAULT_OFFSET)?;
        let vault_authority_bump = read_u8(BANK_LIQUIDITY_VAULT_AUTHORITY_BUMP_OFFSET)?;
        let total_liability_shares = read_i128(BANK_TOTAL_LIABILITY_SHARES_OFFSET)?;
        let total_asset_shares = read_i128(BANK_TOTAL_ASSET_SHARES_OFFSET)?;
        let origination_fee = read_i128(BANK_ORIGINATION_FEE_OFFSET)?;
        let operational_state = read_u8(BANK_OPERATIONAL_STATE_OFFSET)?;
        let oracle_setup = read_u8(BANK_ORACLE_SETUP_OFFSET)?;
        let borrow_limit = read_u64(BANK_BORROW_LIMIT_OFFSET)?;
        let asset_tag = read_u8(BANK_ASSET_TAG_OFFSET)?;
        let flags = read_u64(BANK_FLAGS_OFFSET)?;

        if origination_fee != 0 {
            return Err(FlashError::Reserve(format!(
                "Marginfi bank {reserve} charges a nonzero origination fee ({origination_fee} raw I80F48); \
                 this adapter requires zero"
            )));
        }
        if oracle_setup != ORACLE_SETUP_PYTH_PUSH {
            return Err(FlashError::Reserve(format!(
                "Marginfi bank {reserve} uses oracle setup {oracle_setup}; only \
                 PythPushOracle ({ORACLE_SETUP_PYTH_PUSH}) is verified"
            )));
        }
        if !matches!(asset_tag, ASSET_TAG_DEFAULT | ASSET_TAG_SOL) {
            return Err(FlashError::Reserve(format!(
                "Marginfi bank {reserve} has asset tag {asset_tag}; only default/SOL banks are verified"
            )));
        }

        let bank_key = super::pubkey(reserve);
        let program_id = Pubkey::from_str(MARGINFI_PROGRAM_ID)
            .map_err(|error| FlashError::Pubkey(error.to_string()))?;
        let authority = Pubkey::find_program_address(
            &[LIQUIDITY_VAULT_AUTH_SEED, bank_key.as_ref()],
            &program_id,
        );
        if authority.1 != vault_authority_bump {
            return Err(FlashError::Reserve(format!(
                "Marginfi bank {reserve} stores vault-authority bump {vault_authority_bump}, \
                 but the canonical PDA bump is {}",
                authority.1
            )));
        }

        let mut oracle_keys = Vec::with_capacity(MAX_ORACLE_KEYS);
        for index in 0..MAX_ORACLE_KEYS {
            let key = read_address(BANK_ORACLE_KEYS_OFFSET + index * 32)?;
            if key != Address::ZERO {
                oracle_keys.push(key);
            }
        }
        if oracle_keys.is_empty() {
            return Err(FlashError::Reserve(format!(
                "Marginfi bank {reserve} has no oracle key"
            )));
        }

        let mut remaining_accounts = Vec::with_capacity(1 + oracle_keys.len());
        remaining_accounts.push(*reserve);
        remaining_accounts.extend(oracle_keys);

        let available = Self::available_liquidity(
            total_asset_shares,
            asset_share_value,
            total_liability_shares,
            liability_share_value,
            borrow_limit,
        )?;

        let token_program = if flags & BANK_FLAG_IS_T22 != 0 {
            Pubkey::from_str(TOKEN_2022_PROGRAM_ID)
                .map_err(|error| FlashError::Pubkey(error.to_string()))?
        } else {
            Pubkey::from_str(SPL_TOKEN_PROGRAM_ID)
                .map_err(|error| FlashError::Pubkey(error.to_string()))?
        };

        Ok(FlashReserve {
            reserve: *reserve,
            mint,
            supply_vault: liquidity_vault,
            // Marginfi has no flash fee receiver. Reusing the liquidity vault
            // keeps the shared shape honest: the field is not spent by this
            // adapter and `fee_model` is NoFee.
            fee_vault: liquidity_vault,
            lending_market: group,
            lending_market_authority: super::address(&authority.0),
            token_program: super::address(&token_program),
            remaining_accounts,
            flash_fee_bits: 0,
            fee_model: FlashFeeModel::NoFee,
            host_fee_percentage: 0,
            available,
            decimals,
            // Normalize Marginfi's Operational (1) to the shared "active" (0).
            // Every other state is nonzero and therefore disables the reserve.
            status: if operational_state == OPERATIONAL_STATE_OPERATIONAL {
                0
            } else {
                1
            },
        })
    }

    /// Internal liquidity in native token units:
    /// `asset_shares * asset_share_value - liability_shares * liability_share_value`,
    /// capped by the remaining borrow limit.
    ///
    /// Both operands are I80F48 raw integers. Multiplying two raw values would
    /// overflow i128 (the true product needs ~142 bits), so 24 low fractional
    /// bits are dropped from each side first. The resulting relative error is
    /// below 2^-24, far smaller than one lamport at any realistic bank size.
    fn available_liquidity(
        asset_shares: i128,
        asset_share_value: i128,
        liability_shares: i128,
        liability_share_value: i128,
        borrow_limit: u64,
    ) -> Result<u64> {
        let assets = i80f48_mul_floor(asset_shares, asset_share_value)?;
        let liabilities = i80f48_mul_floor(liability_shares, liability_share_value)?;
        let mut available = assets.saturating_sub(liabilities).max(0);
        if borrow_limit > 0 {
            let room = (borrow_limit as i128).saturating_sub(liabilities).max(0);
            available = available.min(room);
        }
        u64::try_from(available).map_err(|_| FlashError::Overflow)
    }

    /// Fetch every Marginfi bank. Free-tier RPC providers often deprioritise
    /// `getProgramAccounts`; the engine uses the mint-filtered path below.
    pub async fn discover_reserves(&self, rpc: &beast_rpc::RpcClient) -> Result<Vec<FlashReserve>> {
        let program = Address::from_str(MARGINFI_PROGRAM_ID)
            .map_err(|error| FlashError::Pubkey(error.to_string()))?;
        let filters = vec![
            json!({ "dataSize": BANK_ACCOUNT_LEN }),
            json!({ "memcmp": { "offset": 0, "bytes": "QnTef4UXSzF" } }),
        ];
        let accounts = rpc.get_program_accounts(program, filters, None).await?;
        let mut reserves = Vec::with_capacity(accounts.len());
        for (key, account) in accounts {
            if let Ok(reserve) = Self::decode_reserve(&key, &account.data) {
                reserves.push(reserve);
            }
        }
        Ok(reserves)
    }

    /// Resolve every bank for one mint. Mint filtering is the cheap path and is
    /// accepted by free-tier providers that reject unfiltered scans.
    pub async fn reserves_for_mint(
        &self,
        rpc: &beast_rpc::RpcClient,
        mint: &Address,
    ) -> Result<Vec<FlashReserve>> {
        let program = Address::from_str(MARGINFI_PROGRAM_ID)
            .map_err(|error| FlashError::Pubkey(error.to_string()))?;
        let filters = vec![
            json!({ "dataSize": BANK_ACCOUNT_LEN }),
            json!({ "memcmp": { "offset": BANK_MINT_OFFSET, "bytes": mint.to_string() } }),
        ];
        let accounts = rpc.get_program_accounts(program, filters, None).await?;
        let mut reserves = Vec::with_capacity(accounts.len());
        for (key, account) in accounts {
            if let Ok(reserve) = Self::decode_reserve(&key, &account.data) {
                if &reserve.mint == mint {
                    reserves.push(reserve);
                }
            }
        }
        Ok(reserves)
    }

    /// Pick the deepest active bank for a mint.
    pub async fn best_reserve_for_mint(
        &self,
        rpc: &beast_rpc::RpcClient,
        mint: &Address,
    ) -> Result<FlashReserve> {
        let reserves = self.reserves_for_mint(rpc, mint).await?;
        reserves
            .into_iter()
            .filter(FlashReserve::flash_enabled)
            .max_by_key(|reserve| reserve.available)
            .ok_or_else(|| FlashError::NoLiquidity(mint.to_string()))
    }

    fn require_lifecycle(&self, lifecycle: &FlashLifecycle) -> Result<Pubkey> {
        lifecycle
            .account
            .as_ref()
            .map(super::pubkey)
            .ok_or_else(|| {
                FlashError::Config(
                    "Marginfi requires a lifecycle account; build with borrow_with_lifecycle"
                        .into(),
                )
            })
    }
}

/// Multiply two I80F48 raw values and return the integer part in native units.
fn i80f48_mul_floor(left: i128, right: i128) -> Result<i128> {
    if left < 0 || right < 0 {
        return Err(FlashError::Reserve(
            "negative I80F48 share or share value".into(),
        ));
    }
    let left = left >> 24;
    let right = right >> 24;
    left.checked_mul(right)
        .map(|product| product >> 48)
        .ok_or(FlashError::Overflow)
}

impl FlashProvider for MarginfiFlashProvider {
    fn id(&self) -> &'static str {
        "marginfi"
    }

    fn program_id(&self) -> Pubkey {
        self.program_id
    }

    fn enabled(&self) -> bool {
        self.enabled
    }

    fn decode_reserve(&self, address: &Address, data: &[u8]) -> Result<FlashReserve> {
        Self::decode_reserve(address, data)
    }

    fn borrow(
        &self,
        _reserve: &FlashReserve,
        _wallet: Pubkey,
        _destination: Pubkey,
        _amount: u64,
        _index: u8,
    ) -> Result<FlashInstruction> {
        if !self.enabled {
            return Err(FlashError::NotEnabled(self.id().to_string()));
        }
        Err(FlashError::Config(
            "Marginfi borrow requires a lifecycle account; use borrow_with_lifecycle".into(),
        ))
    }

    fn repay(
        &self,
        _reserve: &FlashReserve,
        _wallet: Pubkey,
        _source: Pubkey,
        _amount: u64,
        _borrow_index: u8,
    ) -> Result<FlashInstruction> {
        if !self.enabled {
            return Err(FlashError::NotEnabled(self.id().to_string()));
        }
        Err(FlashError::Config(
            "Marginfi repay requires a lifecycle account; use repay_with_lifecycle".into(),
        ))
    }

    fn lifecycle_account(
        &self,
        reserve: &FlashReserve,
        wallet: Pubkey,
        index: u16,
    ) -> Option<Address> {
        let group = super::pubkey(&reserve.lending_market);
        Some(super::address(
            &self.account_address(&group, &wallet, index, 0),
        ))
    }

    fn temporary_rent_lamports(&self) -> u64 {
        MARGINFI_ACCOUNT_RENT_LAMPORTS
    }

    fn pre_borrow(
        &self,
        reserve: &FlashReserve,
        wallet: Pubkey,
        _destination: Pubkey,
        _amount: u64,
        lifecycle: &FlashLifecycle,
    ) -> Result<Vec<Instruction>> {
        if !self.enabled {
            return Err(FlashError::NotEnabled(self.id().to_string()));
        }
        let account = self.require_lifecycle(lifecycle)?;
        let group = super::pubkey(&reserve.lending_market);
        let expected = self.account_address(
            &group,
            &wallet,
            lifecycle.account_index,
            lifecycle.third_party_id,
        );
        if expected != account {
            return Err(FlashError::Config(format!(
                "Marginfi lifecycle account {account} does not match the derived PDA {expected}"
            )));
        }

        // initialize_account_pda(account_index, None)
        let mut init_data = Vec::with_capacity(11);
        init_data.extend_from_slice(&INITIALIZE_ACCOUNT_DISCRIMINATOR);
        init_data.extend_from_slice(&lifecycle.account_index.to_le_bytes());
        init_data.push(0); // Option<u16>::None -> third_party_id 0
        let init = Instruction {
            program_id: self.program_id,
            accounts: vec![
                AccountMeta::new_readonly(group, false),
                AccountMeta::new(account, false),
                AccountMeta::new_readonly(wallet, true),
                AccountMeta::new(wallet, true),
                AccountMeta::new_readonly(INSTRUCTIONS_SYSVAR, false),
                AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
            ],
            data: init_data,
        };

        // start_flashloan(end_index). The real index is patched by finalize()
        // after the whole cycle is composed.
        let mut start_data = Vec::with_capacity(16);
        start_data.extend_from_slice(&START_FLASHLOAN_DISCRIMINATOR);
        start_data.extend_from_slice(&0u64.to_le_bytes());
        let start = Instruction {
            program_id: self.program_id,
            accounts: vec![
                AccountMeta::new(account, false),
                AccountMeta::new_readonly(wallet, true),
                AccountMeta::new_readonly(INSTRUCTIONS_SYSVAR, false),
            ],
            data: start_data,
        };
        Ok(vec![init, start])
    }

    fn post_repay(
        &self,
        reserve: &FlashReserve,
        wallet: Pubkey,
        _destination: Pubkey,
        _amount: u64,
        lifecycle: &FlashLifecycle,
    ) -> Result<Vec<Instruction>> {
        if !self.enabled {
            return Err(FlashError::NotEnabled(self.id().to_string()));
        }
        let account = self.require_lifecycle(lifecycle)?;
        let group = super::pubkey(&reserve.lending_market);
        let rebalance_pool = self.rebalance_fee_pool_address(&account);
        let end = Instruction {
            program_id: self.program_id,
            accounts: vec![
                AccountMeta::new(account, false),
                AccountMeta::new_readonly(group, false),
                AccountMeta::new_readonly(wallet, true),
            ],
            data: END_FLASHLOAN_DISCRIMINATOR.to_vec(),
        };
        let close = Instruction {
            program_id: self.program_id,
            accounts: vec![
                AccountMeta::new(account, false),
                AccountMeta::new_readonly(wallet, true),
                AccountMeta::new(wallet, true),
                AccountMeta::new_readonly(rebalance_pool, false),
            ],
            data: CLOSE_ACCOUNT_DISCRIMINATOR.to_vec(),
        };
        Ok(vec![end, close])
    }

    fn finalize(
        &self,
        instructions: &mut [Instruction],
        _lifecycle: &FlashLifecycle,
    ) -> Result<()> {
        let start_index = instructions.iter().position(|instruction| {
            instruction.program_id == self.program_id
                && instruction.data.get(..8) == Some(START_FLASHLOAN_DISCRIMINATOR.as_slice())
        });
        let end_index = instructions.iter().position(|instruction| {
            instruction.program_id == self.program_id
                && instruction.data.get(..8) == Some(END_FLASHLOAN_DISCRIMINATOR.as_slice())
        });
        let (Some(start_index), Some(end_index)) = (start_index, end_index) else {
            return Err(FlashError::Config(
                "Marginfi start/end flashloan instructions are missing from the composed cycle"
                    .into(),
            ));
        };
        let start = instructions.get_mut(start_index).ok_or_else(|| {
            FlashError::Config("Marginfi start_flashloan index is out of bounds".into())
        })?;
        if start.data.len() != 16 {
            return Err(FlashError::Config(format!(
                "Marginfi start_flashloan data is {} bytes; expected 16",
                start.data.len()
            )));
        }
        let end_index = u64::try_from(end_index)
            .map_err(|_| FlashError::Config("instruction index does not fit u64".into()))?;
        start.data[8..16].copy_from_slice(&end_index.to_le_bytes());
        Ok(())
    }

    fn borrow_with_lifecycle(
        &self,
        reserve: &FlashReserve,
        wallet: Pubkey,
        destination: Pubkey,
        amount: u64,
        index: u8,
        lifecycle: &FlashLifecycle,
    ) -> Result<FlashInstruction> {
        if !self.enabled {
            return Err(FlashError::NotEnabled(self.id().to_string()));
        }
        if amount == 0 {
            return Err(FlashError::Config("flash borrow amount must be > 0".into()));
        }
        let account = self.require_lifecycle(lifecycle)?;
        let group = super::pubkey(&reserve.lending_market);
        let vault_authority = self.liquidity_vault_authority(&super::pubkey(&reserve.reserve));

        let mut accounts = vec![
            AccountMeta::new_readonly(group, false),
            AccountMeta::new(account, false),
            AccountMeta::new_readonly(wallet, true),
            AccountMeta::new(super::pubkey(&reserve.reserve), false),
            AccountMeta::new(destination, false),
            AccountMeta::new_readonly(vault_authority, false),
            AccountMeta::new(super::pubkey(&reserve.supply_vault), false),
            AccountMeta::new_readonly(super::pubkey(&reserve.token_program), false),
        ];
        // The price gate runs even inside the flashloan flag, so the bank and
        // its oracle keys must be repeated as read-only remaining accounts.
        accounts.extend(
            reserve
                .remaining_accounts
                .iter()
                .map(|key| AccountMeta::new_readonly(super::pubkey(key), false)),
        );

        let mut data = Vec::with_capacity(16);
        data.extend_from_slice(&BORROW_DISCRIMINATOR);
        data.extend_from_slice(&amount.to_le_bytes());
        Ok(FlashInstruction {
            instruction: Instruction {
                program_id: self.program_id,
                accounts,
                data,
            },
            index,
        })
    }

    fn repay_with_lifecycle(
        &self,
        reserve: &FlashReserve,
        wallet: Pubkey,
        source: Pubkey,
        amount: u64,
        borrow_index: u8,
        lifecycle: &FlashLifecycle,
    ) -> Result<FlashInstruction> {
        if !self.enabled {
            return Err(FlashError::NotEnabled(self.id().to_string()));
        }
        if amount == 0 {
            return Err(FlashError::Config("flash repay amount must be > 0".into()));
        }
        let account = self.require_lifecycle(lifecycle)?;
        let group = super::pubkey(&reserve.lending_market);
        let mut data = Vec::with_capacity(18);
        data.extend_from_slice(&REPAY_DISCRIMINATOR);
        // repay_all: the amount is ignored by the program, but the argument is
        // still a u64 followed by Option<bool> = Some(true).
        data.extend_from_slice(&0u64.to_le_bytes());
        data.push(1);
        data.push(1);
        Ok(FlashInstruction {
            instruction: Instruction {
                program_id: self.program_id,
                accounts: vec![
                    AccountMeta::new_readonly(group, false),
                    AccountMeta::new(account, false),
                    AccountMeta::new_readonly(wallet, true),
                    AccountMeta::new(super::pubkey(&reserve.reserve), false),
                    AccountMeta::new(source, false),
                    AccountMeta::new(super::pubkey(&reserve.supply_vault), false),
                    AccountMeta::new_readonly(super::pubkey(&reserve.token_program), false),
                ],
                data,
            },
            index: borrow_index,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pubkey;

    fn b58(s: &str) -> Vec<u8> {
        base58::FromBase58::from_base58(s).expect("valid base58")
    }

    fn write_key(data: &mut [u8], offset: usize, key: &str) {
        data[offset..offset + 32].copy_from_slice(&b58(key));
    }

    fn real_wsol_bank_bytes() -> Vec<u8> {
        let mut data = vec![0u8; BANK_ACCOUNT_LEN];
        data[..8].copy_from_slice(&BANK_DISCRIMINATOR);
        write_key(
            &mut data,
            BANK_MINT_OFFSET,
            "So11111111111111111111111111111111111111112",
        );
        data[BANK_DECIMALS_OFFSET] = 9;
        write_key(
            &mut data,
            BANK_GROUP_OFFSET,
            "4qp6Fx6tnZkY5Wropq9wUYgtFxXKwE6viZxFHg3rdAG8",
        );
        // Exact raw I80F48 values read from the live bank (not rounded
        // decimals): ash 1.104099..., lsh 1.233292..., and the two share
        // totals. The expected liquidity below is derived with the same
        // integer algorithm the adapter uses.
        data[BANK_ASSET_SHARE_VALUE_OFFSET..BANK_ASSET_SHARE_VALUE_OFFSET + 16]
            .copy_from_slice(&310_777_332_422_436i128.to_le_bytes());
        data[BANK_LIABILITY_SHARE_VALUE_OFFSET..BANK_LIABILITY_SHARE_VALUE_OFFSET + 16]
            .copy_from_slice(&347_141_959_784_425i128.to_le_bytes());
        write_key(
            &mut data,
            BANK_LIQUIDITY_VAULT_OFFSET,
            "2eicbpitfJXDwqCuFAmPgDP7t2oUotnAzbGzRKLMgSLe",
        );
        data[BANK_LIQUIDITY_VAULT_AUTHORITY_BUMP_OFFSET] = 255;
        data[BANK_TOTAL_LIABILITY_SHARES_OFFSET..BANK_TOTAL_LIABILITY_SHARES_OFFSET + 16]
            .copy_from_slice(&21_592_899_878_857_637_408_350_957_549i128.to_le_bytes());
        data[BANK_TOTAL_ASSET_SHARES_OFFSET..BANK_TOTAL_ASSET_SHARES_OFFSET + 16]
            .copy_from_slice(&25_713_600_285_753_184_175_526_944_499i128.to_le_bytes());
        data[BANK_ORIGINATION_FEE_OFFSET..BANK_ORIGINATION_FEE_OFFSET + 16]
            .copy_from_slice(&0i128.to_le_bytes());
        data[BANK_OPERATIONAL_STATE_OFFSET] = OPERATIONAL_STATE_OPERATIONAL;
        data[BANK_ORACLE_SETUP_OFFSET] = ORACLE_SETUP_PYTH_PUSH;
        write_key(
            &mut data,
            BANK_ORACLE_KEYS_OFFSET,
            "7AviUf9nL62mcxNbQGKm4nKDQnPjswo6c5MX4D57HmyE",
        );
        data[BANK_BORROW_LIMIT_OFFSET..BANK_BORROW_LIMIT_OFFSET + 8]
            .copy_from_slice(&500_000_000_000_000u64.to_le_bytes());
        data[BANK_ASSET_TAG_OFFSET] = ASSET_TAG_SOL;
        data[BANK_FLAGS_OFFSET..BANK_FLAGS_OFFSET + 8].copy_from_slice(&0x800u64.to_le_bytes());
        data
    }

    fn real_wsol_bank() -> FlashReserve {
        let bank: Address = "CCKtUs6Cgwo4aaQUmBPmyoApH2gUDErxNZCAntD6LYGh"
            .parse()
            .unwrap();
        MarginfiFlashProvider::decode_reserve(&bank, &real_wsol_bank_bytes()).unwrap()
    }

    #[test]
    fn discriminators_match_the_verified_anchor_hashes() {
        for (name, expected) in [
            (
                "marginfi_account_initialize_pda",
                INITIALIZE_ACCOUNT_DISCRIMINATOR,
            ),
            (
                "lending_account_start_flashloan",
                START_FLASHLOAN_DISCRIMINATOR,
            ),
            ("lending_account_borrow", BORROW_DISCRIMINATOR),
            ("lending_account_repay", REPAY_DISCRIMINATOR),
            ("lending_account_end_flashloan", END_FLASHLOAN_DISCRIMINATOR),
            ("marginfi_account_close", CLOSE_ACCOUNT_DISCRIMINATOR),
        ] {
            let hash = solana_sdk::hash::hash(format!("global:{name}").as_bytes());
            assert_eq!(hash.as_ref()[..8], expected, "{name}");
        }
        let bank_hash = solana_sdk::hash::hash(b"account:Bank");
        assert_eq!(bank_hash.as_ref()[..8], BANK_DISCRIMINATOR);
        let account_hash = solana_sdk::hash::hash(b"account:MarginfiAccount");
        assert_eq!(account_hash.as_ref()[..8], MARGINFI_ACCOUNT_DISCRIMINATOR);
    }

    #[test]
    fn account_and_rebalance_pdas_match_the_proven_lifecycle() {
        let provider = MarginfiFlashProvider::mainnet();
        let group = Pubkey::from_str("4qp6Fx6tnZkY5Wropq9wUYgtFxXKwE6viZxFHg3rdAG8").unwrap();
        let wallet = Pubkey::from_str("EtW8SkG2fFiLWzLpuFawvaESLTtn4qkQKvGa8cyBfV6x").unwrap();
        let account = provider.account_address(&group, &wallet, 0, 0);
        assert_eq!(
            account.to_string(),
            "D6ub9ydjupooKd45X9avSSgpuSkD4URCwwVVfFo3xKkH"
        );
        assert_eq!(
            provider.rebalance_fee_pool_address(&account).to_string(),
            "BTC1QcTRmKbZLxMvNSWsP4mdQr1PiooN4cMPNN7BPnkq"
        );
        let bank = Pubkey::from_str("CCKtUs6Cgwo4aaQUmBPmyoApH2gUDErxNZCAntD6LYGh").unwrap();
        assert_eq!(
            provider.liquidity_vault_authority(&bank).to_string(),
            "DD3AeAssFvjqTvRTrRAtpfjkBF8FpVKnFuwnMLN9haXD"
        );
    }

    #[test]
    fn decodes_the_live_wsol_bank_layout() {
        let parsed = real_wsol_bank();
        assert_eq!(
            parsed.mint.to_string(),
            "So11111111111111111111111111111111111111112"
        );
        assert_eq!(
            parsed.lending_market.to_string(),
            "4qp6Fx6tnZkY5Wropq9wUYgtFxXKwE6viZxFHg3rdAG8"
        );
        assert_eq!(
            parsed.supply_vault.to_string(),
            "2eicbpitfJXDwqCuFAmPgDP7t2oUotnAzbGzRKLMgSLe"
        );
        assert_eq!(
            parsed.lending_market_authority.to_string(),
            "DD3AeAssFvjqTvRTrRAtpfjkBF8FpVKnFuwnMLN9haXD"
        );
        assert_eq!(parsed.fee_model, FlashFeeModel::NoFee);
        assert_eq!(parsed.flash_fee(1_000_000_000).unwrap(), 0);
        assert_eq!(
            parsed.repayment_amount(1_000_000_000).unwrap(),
            1_000_000_000
        );
        assert_eq!(parsed.available, 6_252_857_088_587);
        assert_eq!(parsed.flash_fee_bits, 0);
        assert_eq!(parsed.decimals, 9);
        assert_eq!(parsed.status, 0);
        assert!(parsed.flash_enabled());
        assert_eq!(parsed.remaining_accounts.len(), 2);
        assert_eq!(
            parsed.remaining_accounts[0].to_string(),
            "CCKtUs6Cgwo4aaQUmBPmyoApH2gUDErxNZCAntD6LYGh"
        );
        assert_eq!(
            parsed.remaining_accounts[1].to_string(),
            "7AviUf9nL62mcxNbQGKm4nKDQnPjswo6c5MX4D57HmyE"
        );
    }

    #[test]
    fn rejects_a_nonzero_origination_fee() {
        let mut data = real_wsol_bank_bytes();
        data[BANK_ORIGINATION_FEE_OFFSET..BANK_ORIGINATION_FEE_OFFSET + 16]
            .copy_from_slice(&(1i128 << 48).to_le_bytes());
        let bank: Address = "CCKtUs6Cgwo4aaQUmBPmyoApH2gUDErxNZCAntD6LYGh"
            .parse()
            .unwrap();
        assert!(matches!(
            MarginfiFlashProvider::decode_reserve(&bank, &data),
            Err(FlashError::Reserve(message)) if message.contains("origination")
        ));
    }

    #[test]
    fn rejects_an_unverified_oracle_setup() {
        let mut data = vec![0u8; BANK_ACCOUNT_LEN];
        data[..8].copy_from_slice(&BANK_DISCRIMINATOR);
        write_key(
            &mut data,
            BANK_MINT_OFFSET,
            "So11111111111111111111111111111111111111112",
        );
        write_key(
            &mut data,
            BANK_LIQUIDITY_VAULT_OFFSET,
            "2eicbpitfJXDwqCuFAmPgDP7t2oUotnAzbGzRKLMgSLe",
        );
        data[BANK_LIQUIDITY_VAULT_AUTHORITY_BUMP_OFFSET] = 255;
        data[BANK_OPERATIONAL_STATE_OFFSET] = OPERATIONAL_STATE_OPERATIONAL;
        data[BANK_ORACLE_SETUP_OFFSET] = 6; // KaminoPythPush
        data[BANK_ASSET_TAG_OFFSET] = ASSET_TAG_SOL;
        let bank: Address = "CCKtUs6Cgwo4aaQUmBPmyoApH2gUDErxNZCAntD6LYGh"
            .parse()
            .unwrap();
        assert!(matches!(
            MarginfiFlashProvider::decode_reserve(&bank, &data),
            Err(FlashError::Reserve(message)) if message.contains("oracle setup")
        ));
    }

    #[test]
    fn lifecycle_instructions_use_the_verified_account_lists() {
        let provider = MarginfiFlashProvider::mainnet();
        let reserve = real_wsol_bank();
        let wallet = Pubkey::new_unique();
        let destination = Pubkey::new_unique();
        let lifecycle = FlashLifecycle {
            account: provider.lifecycle_account(&reserve, wallet, 0),
            account_index: 0,
            third_party_id: 0,
        };
        let account = pubkey(&lifecycle.account.unwrap());

        let pre = provider
            .pre_borrow(&reserve, wallet, destination, 1_000, &lifecycle)
            .unwrap();
        assert_eq!(pre.len(), 2);
        assert_eq!(pre[0].data.len(), 11);
        assert_eq!(&pre[0].data[..8], &INITIALIZE_ACCOUNT_DISCRIMINATOR);
        assert_eq!(pre[0].accounts[1].pubkey, account);
        assert_eq!(pre[0].accounts.len(), 6);
        assert_eq!(pre[1].data.len(), 16);
        assert_eq!(&pre[1].data[..8], &START_FLASHLOAN_DISCRIMINATOR);
        assert_eq!(&pre[1].data[8..16], &0u64.to_le_bytes());

        let borrow = provider
            .borrow_with_lifecycle(&reserve, wallet, destination, 1_000, 9, &lifecycle)
            .unwrap();
        assert_eq!(borrow.index, 9);
        assert_eq!(borrow.instruction.accounts.len(), 10);
        assert_eq!(borrow.instruction.accounts[1].pubkey, account);
        assert_eq!(
            borrow.instruction.accounts[7].pubkey,
            pubkey(&reserve.token_program)
        );
        assert_eq!(
            borrow.instruction.accounts[8].pubkey,
            pubkey(&reserve.reserve)
        );
        assert_eq!(
            borrow.instruction.accounts[9].pubkey,
            pubkey(&reserve.remaining_accounts[1])
        );

        let repay = provider
            .repay_with_lifecycle(
                &reserve,
                wallet,
                destination,
                1_000,
                borrow.index,
                &lifecycle,
            )
            .unwrap();
        assert_eq!(repay.index, 9);
        assert_eq!(repay.instruction.accounts.len(), 7);
        assert_eq!(&repay.instruction.data[8..16], &0u64.to_le_bytes());
        assert_eq!(&repay.instruction.data[16..18], &[1, 1]);

        let post = provider
            .post_repay(&reserve, wallet, destination, 1_000, &lifecycle)
            .unwrap();
        assert_eq!(post.len(), 2);
        assert_eq!(&post[0].data, &END_FLASHLOAN_DISCRIMINATOR);
        assert_eq!(&post[1].data, &CLOSE_ACCOUNT_DISCRIMINATOR);
        assert_eq!(
            post[1].accounts[3].pubkey,
            provider.rebalance_fee_pool_address(&account)
        );
    }

    #[test]
    fn finalize_patches_the_absolute_end_index() {
        let provider = MarginfiFlashProvider::mainnet();
        let reserve = real_wsol_bank();
        let wallet = Pubkey::new_unique();
        let lifecycle = FlashLifecycle {
            account: provider.lifecycle_account(&reserve, wallet, 0),
            account_index: 0,
            third_party_id: 0,
        };
        let borrow = provider
            .borrow_with_lifecycle(&reserve, wallet, wallet, 1, 6, &lifecycle)
            .unwrap();
        let repay = provider
            .repay_with_lifecycle(&reserve, wallet, wallet, 1, borrow.index, &lifecycle)
            .unwrap();
        let mut pre = provider
            .pre_borrow(&reserve, wallet, wallet, 1, &lifecycle)
            .unwrap();
        let mut post = provider
            .post_repay(&reserve, wallet, wallet, 1, &lifecycle)
            .unwrap();
        let mut instructions = Vec::new();
        instructions.append(&mut pre);
        instructions.push(borrow.instruction);
        instructions.push(repay.instruction);
        instructions.append(&mut post);
        // end_flashloan is at index 4.
        provider.finalize(&mut instructions, &lifecycle).unwrap();
        assert_eq!(&instructions[1].data[8..16], &4u64.to_le_bytes());
    }

    #[test]
    fn an_unverified_provider_refuses_to_build_instructions() {
        let provider = MarginfiFlashProvider::unverified();
        let reserve = real_wsol_bank();
        let error = provider
            .borrow(&reserve, Pubkey::new_unique(), Pubkey::new_unique(), 1, 0)
            .unwrap_err();
        assert!(matches!(error, FlashError::NotEnabled(_)));
    }
}
