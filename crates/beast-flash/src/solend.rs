//! Solend / Save flash-loan adapter.
//!
//! Solend is the older lending market that now runs under the Save brand. Its
//! program is still `So1endDq2YkqhipRh3WViPa8hdiSpxWy6z3Z6tMCpAo` and the
//! modern program accepts the host-fee-bearing flash instructions described
//! below.
//!
//! ## Proof
//! A full `borrow -> repay` round trip was simulated on mainnet-beta against
//! the live WSOL reserve:
//!
//! ```text
//! borrow 100_000_000 lamports -> repay principal -> err: none
//! fee charged: 50_000 lamports (40_000 reserve + 10_000 host)
//! reserve: 8PbodeaosQP19SjYFx855UMqWxH2HynZLdBXmsrbac36
//! market:  4UpD2fh7xH3VP9QQaXtsS1YY3bxzWhtfpks7FatyKvdY
//! ```
//!
//! The program log lines `Instruction: Flash Borrow Reserve Liquidity` and
//! `Instruction: Flash Repay Reserve Liquidity`, plus the exact fee delta on
//! the fee-receiver token account, prove the instruction data and account
//! order. The same simulation is shipped as
//! `cargo run -p beast-flash --example solend_roundtrip`.
//!
//! ## Facts that matter
//! * The borrow tag is `19`, the repay tag is `20`. Both carry the principal
//!   as a little-endian `u64`; repay additionally carries the borrow's absolute
//!   instruction index as a `u8`.
//! * Repay transfers the **principal** to the supply vault, then the host fee
//!   and the reserve fee from the same source token account. The source must
//!   therefore hold principal + total fee.
//! * The fee receiver at reserve offset 339 is a **token account**, not the
//!   fee authority. Passing it as both the reserve fee receiver and the host
//!   fee receiver is accepted and was proven by the simulation.
//! * The market authority is `[lending_market]` (no prefix). Solend stores its
//!   bump in the market account; the canonical PDA derivation matched the live
//!   supply-vault owner at bump 255.
//! * The fee is a 1e18 WAD fraction, rounded to nearest, with a two-unit
//!   minimum when a host fee is configured. Zero is a free flash loan;
//!   `u64::MAX` is the disabled marker.
//!
//! The reference implementation is Apache-2.0; no source is copied here. The
//! offsets were independently confirmed against live 619-byte reserve accounts
//! and the program's own account layout, which is ordinary protocol
//! interoperability.

use super::{FlashError, FlashFeeModel, FlashInstruction, FlashProvider, FlashReserve, Result};
use beast_dex::Address;
use serde_json::json;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::sysvar::instructions::ID as INSTRUCTIONS_SYSVAR;
use std::str::FromStr;

/// Verified executable on mainnet-beta and the owner of every reserve account.
pub const SOLEND_PROGRAM_ID: &str = "So1endDq2YkqhipRh3WViPa8hdiSpxWy6z3Z6tMCpAo";

/// Deepest verified WSOL reserve at the time of the proof.
pub const SOLEND_WSOL_RESERVE: &str = "8PbodeaosQP19SjYFx855UMqWxH2HynZLdBXmsrbac36";

/// Main market that owns the WSOL reserve.
pub const SOLEND_MAIN_MARKET: &str = "4UpD2fh7xH3VP9QQaXtsS1YY3bxzWhtfpks7FatyKvdY";

/// Solend uses classic SPL Token for every market it currently operates. The
/// program itself CPIs into `spl_token`, so a Token-2022 reserve would be
/// rejected atomically by the program rather than silently mis-built.
pub const SPL_TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

/// `FlashBorrowReserveLiquidity`.
pub const FLASH_BORROW_TAG: u8 = 19;
/// `FlashRepayReserveLiquidity`.
pub const FLASH_REPAY_TAG: u8 = 20;

/// 1e18 fixed-point scale for `flash_loan_fee_wad`.
pub const FLASH_FEE_SCALE: u64 = 1_000_000_000_000_000_000;

/// Reserve account size and the fields a flash cycler needs.
pub const RESERVE_ACCOUNT_LEN: usize = 619;
pub const RESERVE_VERSION_OFFSET: usize = 0;
pub const RESERVE_LENDING_MARKET_OFFSET: usize = 10;
pub const RESERVE_MINT_OFFSET: usize = 42;
pub const RESERVE_DECIMALS_OFFSET: usize = 74;
pub const RESERVE_SUPPLY_VAULT_OFFSET: usize = 75;
pub const RESERVE_AVAILABLE_OFFSET: usize = 171;
pub const RESERVE_FLASH_FEE_OFFSET: usize = 314;
pub const RESERVE_HOST_FEE_PERCENTAGE_OFFSET: usize = 322;
pub const RESERVE_FEE_RECEIVER_OFFSET: usize = 339;

/// Solend's reserve layout version. Version 0 is uninitialized; a future
/// version is rejected rather than decoded with stale offsets.
pub const RESERVE_VERSION: u8 = 1;

/// A live flash-loan provider backed by any Solend market.
#[derive(Debug, Clone)]
pub struct SolendFlashProvider {
    program_id: Pubkey,
    token_program: Pubkey,
    enabled: bool,
}

impl SolendFlashProvider {
    /// The verified mainnet adapter. Market, vaults and fees are decoded from
    /// the reserve, so no market-specific data is hardcoded per reserve.
    pub fn mainnet() -> Self {
        Self {
            program_id: Pubkey::from_str(SOLEND_PROGRAM_ID)
                .expect("Solend program id is a valid pubkey"),
            token_program: Pubkey::from_str(SPL_TOKEN_PROGRAM_ID)
                .expect("SPL Token program id is a valid pubkey"),
            enabled: true,
        }
    }

    /// A provider that exists but has not been verified. `enabled()` is false,
    /// so the executor never builds a cycle against it.
    pub fn unverified() -> Self {
        Self {
            program_id: Pubkey::from_str(SOLEND_PROGRAM_ID).unwrap_or_default(),
            token_program: Pubkey::from_str(SPL_TOKEN_PROGRAM_ID).unwrap_or_default(),
            enabled: false,
        }
    }

    /// Derive a lending market's authority PDA. Solend seeds with the market
    /// address alone; no prefix and no stored bump are needed, because
    /// `find_program_address` returns the same canonical bump the market stores
    /// (255 for the main market).
    pub fn lending_market_authority(&self, lending_market: &Pubkey) -> Pubkey {
        Pubkey::find_program_address(&[lending_market.as_ref()], &self.program_id).0
    }

    /// Decode a Solend reserve account into the fields a flash cycle needs.
    pub fn decode_reserve(reserve: &Address, data: &[u8]) -> Result<FlashReserve> {
        if data.len() < RESERVE_ACCOUNT_LEN {
            return Err(FlashError::Reserve(format!(
                "Solend reserve {reserve} is {} bytes; expected {RESERVE_ACCOUNT_LEN}",
                data.len()
            )));
        }
        let version = data[RESERVE_VERSION_OFFSET];
        if version != RESERVE_VERSION {
            return Err(FlashError::Reserve(format!(
                "Solend reserve {reserve} has version {version}; expected {RESERVE_VERSION}"
            )));
        }
        let read_address = |offset: usize| -> Result<Address> {
            Address::from_slice(data.get(offset..offset + 32).ok_or_else(|| {
                FlashError::Reserve(format!("reserve field at {offset} is out of bounds"))
            })?)
            .map_err(|error| FlashError::Reserve(error.to_string()))
        };
        let read_u64 = |offset: usize| -> Result<u64> {
            data.get(offset..offset + 8)
                .map(|bytes| u64::from_le_bytes(bytes.try_into().expect("8 bytes")))
                .ok_or_else(|| {
                    FlashError::Reserve(format!("reserve u64 at {offset} is out of bounds"))
                })
        };
        let read_u8 = |offset: usize| -> Result<u8> {
            data.get(offset).copied().ok_or_else(|| {
                FlashError::Reserve(format!("reserve byte {offset} is out of bounds"))
            })
        };

        let lending_market = read_address(RESERVE_LENDING_MARKET_OFFSET)?;
        let program_id = Pubkey::from_str(SOLEND_PROGRAM_ID)
            .map_err(|error| FlashError::Pubkey(error.to_string()))?;
        let token_program = Pubkey::from_str(SPL_TOKEN_PROGRAM_ID)
            .map_err(|error| FlashError::Pubkey(error.to_string()))?;
        let authority = Pubkey::find_program_address(
            &[Pubkey::new_from_array(*lending_market.as_bytes()).as_ref()],
            &program_id,
        )
        .0;

        Ok(FlashReserve {
            reserve: *reserve,
            mint: read_address(RESERVE_MINT_OFFSET)?,
            supply_vault: read_address(RESERVE_SUPPLY_VAULT_OFFSET)?,
            fee_vault: read_address(RESERVE_FEE_RECEIVER_OFFSET)?,
            lending_market,
            lending_market_authority: super::address(&authority),
            token_program: super::address(&token_program),
            flash_fee_bits: read_u64(RESERVE_FLASH_FEE_OFFSET)?,
            fee_model: FlashFeeModel::Wad18Round,
            host_fee_percentage: read_u8(RESERVE_HOST_FEE_PERCENTAGE_OFFSET)?,
            available: read_u64(RESERVE_AVAILABLE_OFFSET)?,
            decimals: read_u8(RESERVE_DECIMALS_OFFSET)?,
            // Solend has no status byte. `available` and the disabled fee marker
            // carry the borrowability signal in `FlashReserve::flash_enabled`.
            status: 0,
        })
    }

    /// Fetch every reserve owned by Solend. Free-tier providers that deprioritise
    /// `getProgramAccounts` surface an RPC capability error here; pinning
    /// `flash.reserve` or filtering by mint is the cheap path.
    pub async fn discover_reserves(&self, rpc: &beast_rpc::RpcClient) -> Result<Vec<FlashReserve>> {
        let program = Address::from_str(SOLEND_PROGRAM_ID)
            .map_err(|error| FlashError::Pubkey(error.to_string()))?;
        let filters = vec![json!({ "dataSize": RESERVE_ACCOUNT_LEN })];
        let accounts = rpc.get_program_accounts(program, filters, None).await?;
        let mut reserves = Vec::with_capacity(accounts.len());
        for (key, account) in accounts {
            if let Ok(reserve) = Self::decode_reserve(&key, &account.data) {
                reserves.push(reserve);
            }
        }
        Ok(reserves)
    }

    /// Resolve every Solend reserve for one mint.
    pub async fn reserves_for_mint(
        &self,
        rpc: &beast_rpc::RpcClient,
        mint: &Address,
    ) -> Result<Vec<FlashReserve>> {
        let program = Address::from_str(SOLEND_PROGRAM_ID)
            .map_err(|error| FlashError::Pubkey(error.to_string()))?;
        let filters = vec![
            json!({ "dataSize": RESERVE_ACCOUNT_LEN }),
            json!({ "memcmp": { "offset": RESERVE_MINT_OFFSET, "bytes": mint.to_string() } }),
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

    /// Pick the deepest active reserve for a mint.
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
}

impl FlashProvider for SolendFlashProvider {
    fn id(&self) -> &'static str {
        "solend"
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
        reserve: &FlashReserve,
        _wallet: Pubkey,
        destination: Pubkey,
        amount: u64,
        index: u8,
    ) -> Result<FlashInstruction> {
        if !self.enabled {
            return Err(FlashError::NotEnabled(self.id().to_string()));
        }
        if amount == 0 {
            return Err(FlashError::Config("flash borrow amount must be > 0".into()));
        }
        // The borrow's transfer authority is the market PDA, so the wallet is
        // not an account on this instruction.
        let mut data = Vec::with_capacity(9);
        data.push(FLASH_BORROW_TAG);
        data.extend_from_slice(&amount.to_le_bytes());
        Ok(FlashInstruction {
            instruction: Instruction {
                program_id: self.program_id,
                accounts: vec![
                    AccountMeta::new(super::pubkey(&reserve.supply_vault), false),
                    AccountMeta::new(destination, false),
                    AccountMeta::new(super::pubkey(&reserve.reserve), false),
                    AccountMeta::new_readonly(super::pubkey(&reserve.lending_market), false),
                    AccountMeta::new_readonly(
                        super::pubkey(&reserve.lending_market_authority),
                        false,
                    ),
                    AccountMeta::new_readonly(INSTRUCTIONS_SYSVAR, false),
                    AccountMeta::new_readonly(self.token_program, false),
                ],
                data,
            },
            index,
        })
    }

    fn repay(
        &self,
        reserve: &FlashReserve,
        wallet: Pubkey,
        source: Pubkey,
        amount: u64,
        borrow_index: u8,
    ) -> Result<FlashInstruction> {
        if !self.enabled {
            return Err(FlashError::NotEnabled(self.id().to_string()));
        }
        if amount == 0 {
            return Err(FlashError::Config("flash repay amount must be > 0".into()));
        }
        let mut data = Vec::with_capacity(10);
        data.push(FLASH_REPAY_TAG);
        data.extend_from_slice(&amount.to_le_bytes());
        data.push(borrow_index);
        // The reserve's configured fee receiver is a token account. Paying both
        // the host and the reserve cut into it was proven on mainnet; no second
        // account is required.
        Ok(FlashInstruction {
            instruction: Instruction {
                program_id: self.program_id,
                accounts: vec![
                    AccountMeta::new(source, false),
                    AccountMeta::new(super::pubkey(&reserve.supply_vault), false),
                    AccountMeta::new(super::pubkey(&reserve.fee_vault), false),
                    AccountMeta::new(super::pubkey(&reserve.fee_vault), false),
                    AccountMeta::new(super::pubkey(&reserve.reserve), false),
                    AccountMeta::new_readonly(super::pubkey(&reserve.lending_market), false),
                    AccountMeta::new_readonly(wallet, true),
                    AccountMeta::new_readonly(INSTRUCTIONS_SYSVAR, false),
                    AccountMeta::new_readonly(self.token_program, false),
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

    fn real_wsol_reserve() -> FlashReserve {
        let mut data = vec![0u8; RESERVE_ACCOUNT_LEN];
        data[RESERVE_VERSION_OFFSET] = RESERVE_VERSION;
        write_key(&mut data, RESERVE_LENDING_MARKET_OFFSET, SOLEND_MAIN_MARKET);
        write_key(
            &mut data,
            RESERVE_MINT_OFFSET,
            "So11111111111111111111111111111111111111112",
        );
        write_key(
            &mut data,
            RESERVE_SUPPLY_VAULT_OFFSET,
            "8UviNr47S8eL6J3WfDxMRa3hvLta1VDJwNWqsDgtN3Cv",
        );
        write_key(
            &mut data,
            RESERVE_FEE_RECEIVER_OFFSET,
            "5wo1tFpi4HaVKnemqaXeQnBEpezrJXcXvuztYaPhvgC7",
        );
        data[RESERVE_DECIMALS_OFFSET] = 9;
        data[RESERVE_AVAILABLE_OFFSET..RESERVE_AVAILABLE_OFFSET + 8]
            .copy_from_slice(&77_780_055_921_664u64.to_le_bytes());
        data[RESERVE_FLASH_FEE_OFFSET..RESERVE_FLASH_FEE_OFFSET + 8]
            .copy_from_slice(&500_000_000_000_000u64.to_le_bytes());
        data[RESERVE_HOST_FEE_PERCENTAGE_OFFSET] = 20;
        let reserve: Address = SOLEND_WSOL_RESERVE.parse().unwrap();
        SolendFlashProvider::decode_reserve(&reserve, &data).unwrap()
    }

    #[test]
    fn decodes_the_live_wsol_reserve_layout() {
        let parsed = real_wsol_reserve();
        assert_eq!(
            parsed.mint.to_string(),
            "So11111111111111111111111111111111111111112"
        );
        assert_eq!(
            parsed.supply_vault.to_string(),
            "8UviNr47S8eL6J3WfDxMRa3hvLta1VDJwNWqsDgtN3Cv"
        );
        assert_eq!(
            parsed.fee_vault.to_string(),
            "5wo1tFpi4HaVKnemqaXeQnBEpezrJXcXvuztYaPhvgC7"
        );
        assert_eq!(parsed.lending_market.to_string(), SOLEND_MAIN_MARKET);
        assert_eq!(
            parsed.lending_market_authority.to_string(),
            "DdZR6zRFiUt4S5mg7AV1uKB2z1f1WzcNYCaTEEWPAuby"
        );
        assert_eq!(parsed.token_program.to_string(), SPL_TOKEN_PROGRAM_ID);
        assert_eq!(parsed.available, 77_780_055_921_664);
        assert_eq!(parsed.decimals, 9);
        assert_eq!(parsed.flash_fee_bits, 500_000_000_000_000);
        assert_eq!(parsed.fee_model, FlashFeeModel::Wad18Round);
        assert_eq!(parsed.host_fee_percentage, 20);
        assert!(parsed.flash_enabled());
        assert_eq!(parsed.flash_fee(100_000_000).unwrap(), 50_000);
        assert_eq!(parsed.repayment_amount(100_000_000).unwrap(), 100_050_000);
    }

    #[test]
    fn rejects_an_uninitialized_or_future_reserve() {
        let reserve: Address = SOLEND_WSOL_RESERVE.parse().unwrap();
        for version in [0u8, 2u8] {
            let mut data = vec![0u8; RESERVE_ACCOUNT_LEN];
            data[RESERVE_VERSION_OFFSET] = version;
            assert!(matches!(
                SolendFlashProvider::decode_reserve(&reserve, &data),
                Err(FlashError::Reserve(_))
            ));
        }
    }

    #[test]
    fn borrow_and_repay_carry_the_verified_account_order() {
        let provider = SolendFlashProvider::mainnet();
        let reserve = real_wsol_reserve();
        let wallet = Pubkey::new_unique();
        let user_token = Pubkey::new_unique();
        let amount = 100_000_000u64;
        let borrow = provider
            .borrow(&reserve, wallet, user_token, amount, 7)
            .unwrap();
        let repay = provider
            .repay(&reserve, wallet, user_token, amount, borrow.index)
            .unwrap();

        assert_eq!(borrow.instruction.data[0], FLASH_BORROW_TAG);
        assert_eq!(borrow.instruction.data[1..9], amount.to_le_bytes());
        assert_eq!(repay.instruction.data[0], FLASH_REPAY_TAG);
        assert_eq!(repay.instruction.data[1..9], amount.to_le_bytes());
        assert_eq!(repay.instruction.data[9], 7);

        let borrow_accounts: Vec<Pubkey> = borrow
            .instruction
            .accounts
            .iter()
            .map(|meta| meta.pubkey)
            .collect();
        assert_eq!(
            borrow_accounts,
            vec![
                pubkey(&reserve.supply_vault),
                user_token,
                pubkey(&reserve.reserve),
                pubkey(&reserve.lending_market),
                pubkey(&reserve.lending_market_authority),
                INSTRUCTIONS_SYSVAR,
                provider.token_program,
            ]
        );

        let repay_accounts: Vec<Pubkey> = repay
            .instruction
            .accounts
            .iter()
            .map(|meta| meta.pubkey)
            .collect();
        assert_eq!(
            repay_accounts,
            vec![
                user_token,
                pubkey(&reserve.supply_vault),
                pubkey(&reserve.fee_vault),
                pubkey(&reserve.fee_vault),
                pubkey(&reserve.reserve),
                pubkey(&reserve.lending_market),
                wallet,
                INSTRUCTIONS_SYSVAR,
                provider.token_program,
            ]
        );
        assert!(repay.instruction.accounts[6].is_signer);
        assert_eq!(
            provider.repayment_amount(&reserve, amount).unwrap(),
            amount + 50_000
        );
    }

    #[test]
    fn an_unverified_provider_refuses_to_build_instructions() {
        let provider = SolendFlashProvider::unverified();
        let reserve = real_wsol_reserve();
        let error = provider
            .borrow(&reserve, Pubkey::new_unique(), Pubkey::new_unique(), 1, 0)
            .unwrap_err();
        assert!(matches!(error, FlashError::NotEnabled(_)));
    }
}
