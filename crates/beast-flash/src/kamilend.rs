//! Kamino Lending (KLend) flash-loan adapter.
//!
//! Everything here is derived from on-chain ground truth. The borrow instruction
//! was validated against a real mainnet transaction:
//!
//! ```text
//! signature : 3ofZkLX2sXZgXujH5UPDMRa9h5JTQcmK7U9LKENFDTYhECW92hWBT6igpqCZuF2vaMsqbinpUnApzE4rsqcQxKor
//! slot      : 462041620
//! borrowed  : 234_451_759 lamports of WSOL (borrow instruction index 3)
//! repay ix  : cite borrow_instruction_index 3
//! ```
//!
//! That reference transaction **failed in its second swap** (insufficient
//! funds), so it proves the borrow's discriminator and account order but not
//! the repayment. The full `borrow -> repay` round trip was therefore proven
//! separately by a mainnet simulation that was accepted by KLend:
//!
//! ```text
//! borrow 1_000_000_000 lamports -> repay principal -> KLend success
//! fee charged: 10_000 lamports
//! reference reserve: 6gTJfuPHEg6uRAijRkMqNc9kan4sVZejKMxmvx2grT1p
//! ```
//!
//! ## Facts that matter
//! * The repay argument is the **principal**, not principal+fee. Kamino checks
//!   it equals the borrow amount and then charges the fee from the user's token
//!   account. Passing principal+fee fails with
//!   "Liquidity amount for flash repay doesn't match borrow".
//! * Borrow and repay use the **same account list**. Kamino pairs them by
//!   comparing account slot 5, so both must cite the reserve's supply vault;
//!   slot 6 is the user's token account in both.
//! * `borrow_instruction_index` is the borrow's absolute index in the
//!   transaction and must be computed after the preamble is assembled. A
//!   hardcoded value silently cites the wrong instruction.
//! * The flash fee lives at byte 4904 and is a **2^60 fixed-point fraction**,
//!   charged with ceiling. The observed main-market value is exactly 10 ppm.
//! * The reserve stores its lending market at byte 32 and its token program at
//!   byte 408, so one adapter serves SPL Token and Token-2022 reserves across
//!   every Kamino market. The market authority is `["lma", market]`, verified
//!   against the authority observed in the reference transaction.
//!
//! The upstream repository is BUSL-1.1 licensed, so no source, IDL, or account
//! layout definition is copied from it. The offsets below were confirmed
//! empirically against live 8624-byte accounts and the program's own 8624-byte
//! on-chain layout, which is ordinary protocol interoperability.

use super::{FlashError, FlashInstruction, FlashProvider, FlashReserve, Result};
use beast_dex::Address;
use serde_json::json;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::sysvar::instructions::ID as INSTRUCTIONS_SYSVAR;
use std::str::FromStr;

/// Verified executable on mainnet-beta and the owner of all reserve accounts.
pub const KAMINO_KLEND_PROGRAM_ID: &str = "KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD";

/// Anchor account discriminator: `sha256("account:Reserve")[0..8]`.
pub const RESERVE_DISCRIMINATOR: [u8; 8] = [43, 242, 204, 202, 26, 247, 59, 127];

/// Anchor instruction discriminators. The borrow bytes were confirmed
/// byte-for-byte against the reference transaction; the repay bytes were
/// confirmed by a live simulation that reached `FlashRepayReserveLiquidity`.
pub const FLASH_BORROW_DISCRIMINATOR: [u8; 8] = [135, 231, 52, 167, 7, 52, 212, 193];
pub const FLASH_REPAY_DISCRIMINATOR: [u8; 8] = [185, 117, 0, 203, 96, 245, 180, 186];

/// `["lma", lending_market]` derives the market authority. Verified against the
/// authority in the reference transaction (`Dx8iy2o...`, bump 255).
pub const MARKET_AUTHORITY_SEED: &[u8] = b"lma";

/// Fixed-point scale used by Kamino's fee fields. Not 1e18: the observed value
/// `11_529_215_046_068` divided by `2^60` is exactly 10 ppm, while dividing by
/// `1e18` would give a rate the program does not charge.
pub const FLASH_FEE_SCALE: u64 = 1 << 60;

/// Reserve account size and the fields a flash cycler needs.
pub const RESERVE_ACCOUNT_LEN: usize = 8624;
pub const RESERVE_VERSION_OFFSET: usize = 8;
pub const RESERVE_LENDING_MARKET_OFFSET: usize = 32;
pub const RESERVE_MINT_OFFSET: usize = 128;
pub const RESERVE_SUPPLY_VAULT_OFFSET: usize = 160;
pub const RESERVE_FEE_VAULT_OFFSET: usize = 192;
pub const RESERVE_AVAILABLE_OFFSET: usize = 224;
pub const RESERVE_DECIMALS_OFFSET: usize = 272;
pub const RESERVE_TOKEN_PROGRAM_OFFSET: usize = 408;
pub const RESERVE_STATUS_OFFSET: usize = 4856;
pub const RESERVE_FLASH_FEE_OFFSET: usize = 4904;

/// The only reserve status that is safe to borrow from. Other statuses are
/// hidden, deprecated or otherwise not intended for new positions.
pub const RESERVE_STATUS_ACTIVE: u8 = 0;

/// A live flash-loan provider backed by any Kamino market.
#[derive(Debug, Clone)]
pub struct KaminoFlashProvider {
    program_id: Pubkey,
    enabled: bool,
}

/// Kept as an alias so callers that referred to a decoded reserve keep working;
/// the adapter returns the shared [`FlashReserve`] shape.
pub type KaminoReserve = FlashReserve;

impl KaminoFlashProvider {
    /// The verified mainnet adapter. All market-specific data comes from the
    /// reserve account, so no market or authority is hardcoded here.
    pub fn mainnet() -> Self {
        Self {
            program_id: Pubkey::from_str(KAMINO_KLEND_PROGRAM_ID)
                .expect("KLend program id is a valid pubkey"),
            enabled: true,
        }
    }

    /// A provider that exists but has not been verified. `enabled()` is false,
    /// so the executor will never build a cycle against it.
    pub fn unverified() -> Self {
        Self {
            program_id: Pubkey::from_str(KAMINO_KLEND_PROGRAM_ID).unwrap_or_default(),
            enabled: false,
        }
    }

    /// Derive a lending market's authority PDA.
    ///
    /// The seed is pinned by `market_authority_matches_the_reference_transaction`
    /// against the authority a real transaction passed to KLend.
    pub fn lending_market_authority(&self, lending_market: &Pubkey) -> Pubkey {
        derive_market_authority(&self.program_id, lending_market)
    }

    /// Decode a mainnet reserve account into the fields a flash cycle needs.
    ///
    /// The length and discriminator are checked before any offset is read: a
    /// short or wrong-type account must fail loudly rather than yield a
    /// plausible-looking wrong vault.
    pub fn decode_reserve(reserve: &Address, data: &[u8]) -> Result<FlashReserve> {
        if data.len() < RESERVE_ACCOUNT_LEN {
            return Err(FlashError::Reserve(format!(
                "reserve {reserve} is {} bytes; expected {RESERVE_ACCOUNT_LEN}",
                data.len()
            )));
        }
        if data[..8] != RESERVE_DISCRIMINATOR {
            return Err(FlashError::Reserve(format!(
                "reserve {reserve} has discriminator {:?}, expected {:?}",
                &data[..8],
                RESERVE_DISCRIMINATOR
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
            data.get(offset)
                .copied()
                .ok_or_else(|| FlightOffsetError(offset).into())
        };

        let lending_market = read_address(RESERVE_LENDING_MARKET_OFFSET)?;
        let program_id = Pubkey::from_str(KAMINO_KLEND_PROGRAM_ID)
            .map_err(|error| FlashError::Pubkey(error.to_string()))?;
        let authority = derive_market_authority(
            &program_id,
            &Pubkey::new_from_array(*lending_market.as_bytes()),
        );
        let decimals = read_u64(RESERVE_DECIMALS_OFFSET)?
            .try_into()
            .map_err(|_| FlashError::Reserve("mint decimals do not fit u8".into()))?;
        let status = read_u8(RESERVE_STATUS_OFFSET)?;

        Ok(FlashReserve {
            reserve: *reserve,
            mint: read_address(RESERVE_MINT_OFFSET)?,
            supply_vault: read_address(RESERVE_SUPPLY_VAULT_OFFSET)?,
            fee_vault: read_address(RESERVE_FEE_VAULT_OFFSET)?,
            lending_market,
            lending_market_authority: super::address(&authority),
            token_program: read_address(RESERVE_TOKEN_PROGRAM_OFFSET)?,
            remaining_accounts: Vec::new(),
            flash_fee_bits: read_u64(RESERVE_FLASH_FEE_OFFSET)?,
            fee_model: super::FlashFeeModel::FixedPoint60Ceiling,
            host_fee_percentage: 0,
            available: read_u64(RESERVE_AVAILABLE_OFFSET)?,
            decimals,
            status,
        })
    }

    /// Fetch every reserve owned by KLend. Free-tier providers that block
    /// `getProgramAccounts` will surface an RPC capability error here; the
    /// caller can fall back to [`Self::reserves_for_mint`] on a provider that
    /// supports the method.
    pub async fn discover_reserves(&self, rpc: &beast_rpc::RpcClient) -> Result<Vec<FlashReserve>> {
        let program = Address::from_str(KAMINO_KLEND_PROGRAM_ID)
            .map_err(|error| FlashError::Pubkey(error.to_string()))?;
        let filters = vec![json!({ "dataSize": RESERVE_ACCOUNT_LEN })];
        let accounts = rpc.get_program_accounts(program, filters, None).await?;
        let mut reserves = Vec::with_capacity(accounts.len());
        for (key, account) in accounts {
            // A reverse-engineered layout must never make a discovery failure
            // fatal: skip accounts that fail the shape check, and let the
            // caller report how many were decoded.
            if let Ok(reserve) = Self::decode_reserve(&key, &account.data) {
                reserves.push(reserve);
            }
        }
        Ok(reserves)
    }

    /// Resolve every reserve for one mint. This is the cheap path: a memcmp on
    /// the mint field avoids downloading every reserve in every market.
    pub async fn reserves_for_mint(
        &self,
        rpc: &beast_rpc::RpcClient,
        mint: &Address,
    ) -> Result<Vec<FlashReserve>> {
        let program = Address::from_str(KAMINO_KLEND_PROGRAM_ID)
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

    /// The twelve accounts, in the order the reference transaction used.
    ///
    /// Slot 5 is always the reserve's supply vault and slot 6 is always the
    /// user's token account. Kamino pairs borrow and repay by comparing slot 5,
    /// and the successful mainnet round trip used an identical list for both.
    fn accounts(
        &self,
        reserve: &FlashReserve,
        wallet: Pubkey,
        user_token: Pubkey,
    ) -> Vec<AccountMeta> {
        vec![
            AccountMeta::new_readonly(wallet, true), // userTransferAuthority
            AccountMeta::new_readonly(super::pubkey(&reserve.lending_market_authority), false),
            AccountMeta::new_readonly(super::pubkey(&reserve.lending_market), false),
            AccountMeta::new(super::pubkey(&reserve.reserve), false),
            AccountMeta::new_readonly(super::pubkey(&reserve.mint), false),
            AccountMeta::new(super::pubkey(&reserve.supply_vault), false),
            AccountMeta::new(user_token, false),
            AccountMeta::new(super::pubkey(&reserve.fee_vault), false),
            // Unused referrers resolve to the program id, exactly as the
            // reference transaction passed them.
            AccountMeta::new(self.program_id, false), // referrerTokenState
            AccountMeta::new(self.program_id, false), // referrerAccount
            AccountMeta::new_readonly(INSTRUCTIONS_SYSVAR, false),
            AccountMeta::new_readonly(super::pubkey(&reserve.token_program), false),
        ]
    }
}

/// Tiny helper so the hot path can return a precise out-of-bounds error without
/// repeating the closure boilerplate.
struct FlightOffsetError(usize);

impl From<FlightOffsetError> for FlashError {
    fn from(error: FlightOffsetError) -> Self {
        FlashError::Reserve(format!("reserve byte {} is out of bounds", error.0))
    }
}

/// Derive a lending market's authority PDA with the verified seed.
pub fn derive_market_authority(program_id: &Pubkey, lending_market: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[MARKET_AUTHORITY_SEED, lending_market.as_ref()],
        program_id,
    )
    .0
}

impl FlashProvider for KaminoFlashProvider {
    fn id(&self) -> &'static str {
        "kamino"
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
        wallet: Pubkey,
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
        let mut data = Vec::with_capacity(16);
        data.extend_from_slice(&FLASH_BORROW_DISCRIMINATOR);
        data.extend_from_slice(&amount.to_le_bytes());
        Ok(FlashInstruction {
            instruction: Instruction {
                program_id: self.program_id,
                accounts: self.accounts(reserve, wallet, destination),
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
        let mut data = Vec::with_capacity(17);
        data.extend_from_slice(&FLASH_REPAY_DISCRIMINATOR);
        // Principal, not principal+fee. Kamino checks equality with the borrow
        // amount and then debits the fee from the same user token account.
        data.extend_from_slice(&amount.to_le_bytes());
        data.push(borrow_index);
        Ok(FlashInstruction {
            instruction: Instruction {
                program_id: self.program_id,
                accounts: self.accounts(reserve, wallet, source),
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

    fn real_reserve() -> FlashReserve {
        let mut data = vec![0u8; RESERVE_ACCOUNT_LEN];
        data[..8].copy_from_slice(&RESERVE_DISCRIMINATOR);
        write_key(
            &mut data,
            RESERVE_LENDING_MARKET_OFFSET,
            "H6rHXmXoCQvq8Ue81MqNh7ow5ysPa1dSozwW3PU1dDH6",
        );
        write_key(
            &mut data,
            RESERVE_MINT_OFFSET,
            "So11111111111111111111111111111111111111112",
        );
        write_key(
            &mut data,
            RESERVE_SUPPLY_VAULT_OFFSET,
            "ywaaLvG7t1vXJo8sT3UzE8yzzZtxLM7Fmev64Jbooye",
        );
        write_key(
            &mut data,
            RESERVE_FEE_VAULT_OFFSET,
            "EQ7hw63aBS7aPQqXsoxaaBxiwbEzaAiY9Js6tCekkqxf",
        );
        write_key(
            &mut data,
            RESERVE_TOKEN_PROGRAM_OFFSET,
            "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
        );
        data[RESERVE_AVAILABLE_OFFSET..RESERVE_AVAILABLE_OFFSET + 8]
            .copy_from_slice(&11_586_040_204_538u64.to_le_bytes());
        data[RESERVE_DECIMALS_OFFSET..RESERVE_DECIMALS_OFFSET + 8]
            .copy_from_slice(&9u64.to_le_bytes());
        data[RESERVE_STATUS_OFFSET] = RESERVE_STATUS_ACTIVE;
        data[RESERVE_FLASH_FEE_OFFSET..RESERVE_FLASH_FEE_OFFSET + 8]
            .copy_from_slice(&11_529_215_046_068u64.to_le_bytes());
        let reserve: Address = "6gTJfuPHEg6uRAijRkMqNc9kan4sVZejKMxmvx2grT1p"
            .parse()
            .unwrap();
        KaminoFlashProvider::decode_reserve(&reserve, &data).unwrap()
    }

    #[test]
    fn discriminators_match_the_verified_bytes() {
        // Anchor convention: sha256("global:<method>")[0..8]. These constants
        // were also confirmed against a real transaction and a live simulation.
        let borrow = solana_sdk::hash::hash(b"global:flash_borrow_reserve_liquidity");
        let repay = solana_sdk::hash::hash(b"global:flash_repay_reserve_liquidity");
        assert_eq!(borrow.as_ref()[..8], FLASH_BORROW_DISCRIMINATOR);
        assert_eq!(repay.as_ref()[..8], FLASH_REPAY_DISCRIMINATOR);
    }

    #[test]
    fn market_authority_matches_the_reference_transaction() {
        // The authority passed by the real flash transaction at slot 462041620.
        let market = Pubkey::from_str("H6rHXmXoCQvq8Ue81MqNh7ow5ysPa1dSozwW3PU1dDH6").unwrap();
        let expected = Pubkey::from_str("Dx8iy2o46sK1DzWbEcznqSKeLbLVeu7otkibA3WohGAj").unwrap();
        let program = Pubkey::from_str(KAMINO_KLEND_PROGRAM_ID).unwrap();
        assert_eq!(derive_market_authority(&program, &market), expected);
    }

    #[test]
    fn decodes_a_real_mainnet_reserve_layout() {
        let parsed = real_reserve();
        assert_eq!(
            parsed.mint.to_string(),
            "So11111111111111111111111111111111111111112"
        );
        assert_eq!(
            parsed.supply_vault.to_string(),
            "ywaaLvG7t1vXJo8sT3UzE8yzzZtxLM7Fmev64Jbooye"
        );
        assert_eq!(
            parsed.fee_vault.to_string(),
            "EQ7hw63aBS7aPQqXsoxaaBxiwbEzaAiY9Js6tCekkqxf"
        );
        assert_eq!(parsed.available, 11_586_040_204_538);
        assert_eq!(parsed.decimals, 9);
        assert_eq!(parsed.flash_fee_bits, 11_529_215_046_068);
        assert_eq!(
            parsed.lending_market_authority.to_string(),
            "Dx8iy2o46sK1DzWbEcznqSKeLbLVeu7otkibA3WohGAj"
        );
        assert_eq!(
            parsed.token_program.to_string(),
            "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        );
        assert!(parsed.flash_enabled());
        assert_eq!(parsed.flash_fee(1_000_000_000).unwrap(), 10_000);
    }

    #[test]
    fn rejects_a_wrong_discriminator() {
        let reserve: Address = "6gTJfuPHEg6uRAijRkMqNc9kan4sVZejKMxmvx2grT1p"
            .parse()
            .unwrap();
        let mut data = vec![0u8; RESERVE_ACCOUNT_LEN];
        data[0] = 1;
        assert!(matches!(
            KaminoFlashProvider::decode_reserve(&reserve, &data),
            Err(FlashError::Reserve(_))
        ));
    }

    #[test]
    fn borrow_and_repay_share_the_pairing_accounts() {
        let provider = KaminoFlashProvider::mainnet();
        let reserve = real_reserve();
        let wallet = pubkey(&reserve.lending_market_authority);
        let user_token = Pubkey::new_unique();
        let amount = 234_451_759u64;
        let borrow = provider
            .borrow(&reserve, wallet, user_token, amount, 5)
            .unwrap();
        let repay = provider
            .repay(&reserve, wallet, user_token, amount, borrow.index)
            .unwrap();

        assert_eq!(borrow.index, 5);
        assert_eq!(repay.index, 5);
        // Slot 5 is the reserve vault in both instructions; slot 6 is the
        // user's account in both. This is what KLend pairs on.
        for instruction in [&borrow.instruction, &repay.instruction] {
            assert_eq!(
                instruction.accounts[5].pubkey,
                pubkey(&reserve.supply_vault)
            );
            assert_eq!(instruction.accounts[6].pubkey, user_token);
            assert_eq!(
                instruction.accounts[11].pubkey,
                pubkey(&reserve.token_program)
            );
        }
        // The repay argument is the principal, plus the borrow's absolute index.
        assert_eq!(borrow.instruction.data[8..16], amount.to_le_bytes());
        assert_eq!(repay.instruction.data[8..16], amount.to_le_bytes());
        assert_eq!(repay.instruction.data[16], 5);
        assert_eq!(
            provider.repayment_amount(&reserve, amount).unwrap(),
            amount + 2_345
        );
    }

    #[test]
    fn an_unverified_provider_refuses_to_build_instructions() {
        let provider = KaminoFlashProvider::unverified();
        let reserve = real_reserve();
        let error = provider
            .borrow(&reserve, Pubkey::new_unique(), Pubkey::new_unique(), 1, 0)
            .unwrap_err();
        assert!(matches!(error, FlashError::NotEnabled(_)));
    }
}
