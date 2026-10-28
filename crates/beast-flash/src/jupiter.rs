//! Jupiter Lend flash-loan adapter.
//!
//! Jupiter Lend is Fluid's modular liquidity layer under the Jupiter brand. Its
//! flash loans live in a **dedicated program**, separate from the lending
//! program, and they are the cheapest on Solana: the protocol-wide
//! `flashloan_fee` is **zero basis points**, so a cycle only repays the
//! principal.
//!
//! ## Proof
//! A full `borrow -> repay` round trip was simulated on mainnet-beta against
//! the live WSOL liquidity reserve. The address list was produced by the
//! vendor's SDK as a *reference* and then re-derived independently from the
//! published IDL's PDA seeds; both matched, so this adapter is not a
//! transcription of the client:
//!
//! ```text
//! borrow 1_000_000_000 lamports -> repay principal -> err: none
//! fee charged: 0 lamports
//! borrow position: BZZKgXxhxVkzx3NN8RfBPwU7ZmnQbDtp3ezcsXbiALL6
//! admin:          ALXWtv2P4GqH1B7Lq731joag52yRBRqmHV4naiXPTYWL
//! flashloanBorrow   67134e18f009873f
//! flashloanPayback  d52f998954f35ee8
//! ```
//!
//! The same simulation is shipped as
//! `cargo run -p beast-flash --example jupiter_roundtrip`.
//!
//! ## Facts that matter
//! * Two instructions and **no borrow index**: unlike Kamino and Solend, the
//!   payback does not carry the borrow's instruction index. The program finds
//!   the borrow by walking the `instruction_sysvar`, which is why that sysvar
//!   is in both instructions' account lists.
//! * The fee is protocol-wide (`FlashloanAdmin.flashloan_fee`, a u16 in basis
//!   points of 10,000) rather than per-reserve, and it is read from the admin
//!   account before every cycle.
//! * `available` is bounded by two independent limits and takes the minimum:
//!   the reserve's own utilisation ceiling
//!   (`supply * max_utilization - borrow`, then capped by the vault's live
//!   token balance) and the borrow position's `max_debt_ceiling - amount`.
//!   For WSOL the position ceiling is the binding one (about 15,000 SOL).
//! * The borrow position is owned by the **flashloan admin**, so there is
//!   nothing for a wallet to create. This provider needs no lifecycle hooks.

use super::{
    address, pubkey, FlashError, FlashFeeModel, FlashInstruction, FlashProvider, FlashReserve,
    Result,
};
use beast_dex::Address;
use serde_json::json;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::program_pack::Pack;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::sysvar::instructions::ID as INSTRUCTIONS_SYSVAR;
use std::str::FromStr;

/// Verified executable on mainnet-beta; owns the admin account.
pub const JUPITER_FLASHLOAN_PROGRAM_ID: &str = "jupgfSgfuAXv4B6R2Uxu85Z1qdzgju79s6MfZekN6XS";

/// Jupiter Lend's liquidity program: every reserve, rate model and borrow
/// position is owned by it.
pub const JUPITER_LIQUIDITY_PROGRAM_ID: &str = "jupeiUmn818Jg1ekPURTpr4mFo29p46vygyykFJ3wZC";

/// Verified `FlashloanAdmin` PDA for the `main` market.
pub const JUPITER_FLASHLOAN_ADMIN: &str = "ALXWtv2P4GqH1B7Lq731joag52yRBRqmHV4naiXPTYWL";

/// Verified `Liquidity` PDA for the `main` market; owns the per-token vaults.
pub const JUPITER_LIQUIDITY: &str = "7s1da8DduuBFqGra5bJBjpnvL5E9mGzCuMk1Qkh4or2Z";

/// Verified WSOL `TokenReserve` PDA.
pub const JUPITER_WSOL_RESERVE: &str = "4Y66HtUEqbbbpZdENGtFdVhUMS3tnagffn3M4do59Nfy";

/// Verified WSOL `UserBorrowPosition` PDA.
pub const JUPITER_WSOL_BORROW_POSITION: &str = "BZZKgXxhxVkzx3NN8RfBPwU7ZmnQbDtp3ezcsXbiALL6";

/// Verified WSOL liquidity vault, i.e. the token account holding deposits.
pub const JUPITER_WSOL_VAULT: &str = "5JP5zgYCb9W37QQLgAHRHuinFLrKt87akDY1CgZoTPzr";

/// `flashloanBorrow`.
pub const FLASH_BORROW_DISCRIMINATOR: [u8; 8] = [0x67, 0x13, 0x4e, 0x18, 0xf0, 0x09, 0x87, 0x3f];
/// `flashloanPayback`.
pub const FLASH_PAYBACK_DISCRIMINATOR: [u8; 8] = [0xd5, 0x2f, 0x99, 0x89, 0x54, 0xf3, 0x5e, 0xe8];

/// The protocol's basis-point precision: `fee = ceil(amount * bps / 10_000)`.
pub const FEE_BASIS_POINTS_SCALE: u64 = 10_000;

/// Account sizes, verified against live mainnet accounts.
pub const FLASHLOAN_ADMIN_LEN: usize = 93;
pub const TOKEN_RESERVE_LEN: usize = 192;
pub const USER_BORROW_POSITION_LEN: usize = 120;

/// Account discriminators, taken from the published IDL.
pub const FLASHLOAN_ADMIN_DISCRIMINATOR: [u8; 8] = [0xa2, 0xa1, 0x2d, 0x1c, 0x83, 0x5b, 0xca, 0x58];
pub const TOKEN_RESERVE_DISCRIMINATOR: [u8; 8] = [0x15, 0x12, 0x3b, 0x87, 0x78, 0x14, 0x1f, 0x0c];
pub const USER_BORROW_POSITION_DISCRIMINATOR: [u8; 8] =
    [0x49, 0x7e, 0x41, 0x7b, 0xdc, 0x7e, 0xc5, 0x18];

/// `FlashloanAdmin` field offsets, from the published IDL type.
pub const ADMIN_LIQUIDITY_PROGRAM_OFFSET: usize = 40;
pub const ADMIN_STATUS_OFFSET: usize = 72;
pub const ADMIN_FLASHLOAN_FEE_OFFSET: usize = 73;
pub const ADMIN_IS_ACTIVE_OFFSET: usize = 83;
pub const ADMIN_ACTIVE_AMOUNT_OFFSET: usize = 84;

/// `TokenReserve` field offsets, from the published IDL type.
pub const RESERVE_MINT_OFFSET: usize = 8;
pub const RESERVE_VAULT_OFFSET: usize = 40;
pub const RESERVE_MAX_UTILIZATION_OFFSET: usize = 102;
pub const RESERVE_TOTAL_SUPPLY_OFFSET: usize = 104;
pub const RESERVE_TOTAL_BORROW_OFFSET: usize = 120;

/// `UserBorrowPosition` field offsets, from the published IDL type.
pub const POSITION_PROTOCOL_OFFSET: usize = 8;
pub const POSITION_MINT_OFFSET: usize = 40;
pub const POSITION_AMOUNT_OFFSET: usize = 73;
pub const POSITION_MAX_DEBT_CEILING_OFFSET: usize = 111;

/// Decoded fields of one account slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JupiterAdmin {
    pub authority: Pubkey,
    pub liquidity_program: Pubkey,
    pub active: bool,
    pub fee_bps: u64,
    pub in_flight: bool,
    pub in_flight_amount: u64,
}

/// Decoded borrowability of one mint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JupiterLiquidity {
    pub mint: Pubkey,
    pub vault: Pubkey,
    pub max_utilization_bps: u16,
    pub total_supply: u64,
    pub total_borrow: u64,
}

/// Decoded state of one flashloan borrow position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JupiterBorrowPosition {
    pub protocol: Pubkey,
    pub mint: Pubkey,
    pub amount: u64,
    pub max_debt_ceiling: u64,
}

fn slice_at<'a>(data: &'a [u8], offset: usize, len: usize, what: &str) -> Result<&'a [u8]> {
    data.get(offset..offset + len)
        .ok_or_else(|| FlashError::Reserve(format!("{what}: field at {offset} is out of bounds")))
}

fn key_at(data: &[u8], offset: usize, what: &str) -> Result<Pubkey> {
    Ok(Pubkey::new_from_array(
        slice_at(data, offset, 32, what)?
            .try_into()
            .expect("32 bytes"),
    ))
}

fn u16_at(data: &[u8], offset: usize, what: &str) -> Result<u16> {
    Ok(u16::from_le_bytes(
        slice_at(data, offset, 2, what)?
            .try_into()
            .expect("2 bytes"),
    ))
}

fn u64_at(data: &[u8], offset: usize, what: &str) -> Result<u64> {
    Ok(u64::from_le_bytes(
        slice_at(data, offset, 8, what)?
            .try_into()
            .expect("8 bytes"),
    ))
}

fn bool_at(data: &[u8], offset: usize, what: &str) -> Result<bool> {
    match data.get(offset) {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        _ => Err(FlashError::Reserve(format!(
            "{what}: expected a 0/1 bool at {offset}"
        ))),
    }
}

/// A live flash-loan provider backed by the Jupiter Lend flashloan program.
#[derive(Debug, Clone)]
pub struct JupiterFlashProvider {
    flashloan_program: Pubkey,
    liquidity_program: Pubkey,
    enabled: bool,
}

impl JupiterFlashProvider {
    /// The verified mainnet adapter.
    pub fn mainnet() -> Self {
        Self {
            flashloan_program: Pubkey::from_str(JUPITER_FLASHLOAN_PROGRAM_ID)
                .expect("Jupiter flashloan program id is a valid pubkey"),
            liquidity_program: Pubkey::from_str(JUPITER_LIQUIDITY_PROGRAM_ID)
                .expect("Jupiter liquidity program id is a valid pubkey"),
            enabled: true,
        }
    }

    /// A provider that exists but has not been verified on this cluster.
    pub fn unverified() -> Self {
        Self {
            flashloan_program: Pubkey::default(),
            liquidity_program: Pubkey::default(),
            enabled: false,
        }
    }

    /// `["flashloan_admin"]` under the flashloan program.
    pub fn flashloan_admin(&self) -> Pubkey {
        Pubkey::find_program_address(&[b"flashloan_admin"], &self.flashloan_program).0
    }

    /// `["liquidity"]` under the liquidity program.
    pub fn liquidity(&self) -> Pubkey {
        Pubkey::find_program_address(&[b"liquidity"], &self.liquidity_program).0
    }

    /// `["reserve", mint]` under the liquidity program.
    pub fn liquidity_reserve(&self, mint: &Pubkey) -> Pubkey {
        Pubkey::find_program_address(&[b"reserve", mint.as_ref()], &self.liquidity_program).0
    }

    /// `["rate_model", mint]` under the liquidity program.
    pub fn rate_model(&self, mint: &Pubkey) -> Pubkey {
        Pubkey::find_program_address(&[b"rate_model", mint.as_ref()], &self.liquidity_program).0
    }

    /// `["user_borrow_position", mint, flashloan_admin]` under the liquidity
    /// program: the account the flashloan program borrows through.
    pub fn borrow_position(&self, mint: &Pubkey) -> Pubkey {
        Pubkey::find_program_address(
            &[
                b"user_borrow_position",
                mint.as_ref(),
                self.flashloan_admin().as_ref(),
            ],
            &self.liquidity_program,
        )
        .0
    }

    /// The token account that holds a mint's deposit liquidity.
    pub fn liquidity_vault(&self, mint: &Pubkey, token_program: &Pubkey) -> Pubkey {
        spl_associated_token_account::get_associated_token_address_with_program_id(
            &self.liquidity(),
            mint,
            token_program,
        )
    }

    /// Decode a `FlashloanAdmin` account.
    ///
    /// `active` is the protocol's own activation flag, so a paused protocol is
    /// `active == false` rather than a silent zero fee.
    pub fn decode_admin(data: &[u8]) -> Result<JupiterAdmin> {
        if data.len() < FLASHLOAN_ADMIN_LEN {
            return Err(FlashError::Reserve(format!(
                "flashloan admin is {} bytes, expected {FLASHLOAN_ADMIN_LEN}",
                data.len()
            )));
        }
        if data[..8] != FLASHLOAN_ADMIN_DISCRIMINATOR {
            return Err(FlashError::Reserve(
                "flashloan admin has the wrong discriminator".into(),
            ));
        }
        Ok(JupiterAdmin {
            authority: key_at(data, 8, "flashloan admin")?,
            liquidity_program: key_at(data, ADMIN_LIQUIDITY_PROGRAM_OFFSET, "flashloan admin")?,
            active: bool_at(data, ADMIN_STATUS_OFFSET, "flashloan admin")?,
            fee_bps: u16_at(data, ADMIN_FLASHLOAN_FEE_OFFSET, "flashloan admin")? as u64,
            in_flight: bool_at(data, ADMIN_IS_ACTIVE_OFFSET, "flashloan admin")?,
            in_flight_amount: u64_at(&data[..93], ADMIN_ACTIVE_AMOUNT_OFFSET, "flashloan admin")?,
        })
    }

    /// Decode a `TokenReserve` account.
    pub fn decode_reserve_fields(data: &[u8]) -> Result<JupiterLiquidity> {
        if data.len() < TOKEN_RESERVE_LEN {
            return Err(FlashError::Reserve(format!(
                "token reserve is {} bytes, expected {TOKEN_RESERVE_LEN}",
                data.len()
            )));
        }
        if data[..8] != TOKEN_RESERVE_DISCRIMINATOR {
            return Err(FlashError::Reserve(
                "token reserve has the wrong discriminator".into(),
            ));
        }
        Ok(JupiterLiquidity {
            mint: key_at(data, RESERVE_MINT_OFFSET, "token reserve")?,
            vault: key_at(data, RESERVE_VAULT_OFFSET, "token reserve")?,
            max_utilization_bps: u16_at(data, RESERVE_MAX_UTILIZATION_OFFSET, "token reserve")?,
            total_supply: u64_at(data, RESERVE_TOTAL_SUPPLY_OFFSET, "token reserve")?,
            total_borrow: u64_at(data, RESERVE_TOTAL_BORROW_OFFSET, "token reserve")?,
        })
    }

    /// Decode a `UserBorrowPosition` account.
    pub fn decode_borrow_position(data: &[u8]) -> Result<JupiterBorrowPosition> {
        if data.len() < USER_BORROW_POSITION_LEN {
            return Err(FlashError::Reserve(format!(
                "borrow position is {} bytes, expected {USER_BORROW_POSITION_LEN}",
                data.len()
            )));
        }
        if data[..8] != USER_BORROW_POSITION_DISCRIMINATOR {
            return Err(FlashError::Reserve(
                "borrow position has the wrong discriminator".into(),
            ));
        }
        Ok(JupiterBorrowPosition {
            protocol: key_at(data, POSITION_PROTOCOL_OFFSET, "borrow position")?,
            mint: key_at(data, POSITION_MINT_OFFSET, "borrow position")?,
            amount: u64_at(data, POSITION_AMOUNT_OFFSET, "borrow position")?,
            max_debt_ceiling: u64_at(data, POSITION_MAX_DEBT_CEILING_OFFSET, "borrow position")?,
        })
    }

    /// The borrowable amount for one mint.
    ///
    /// Two independent ceilings apply and the minimum wins: the reserve's
    /// utilisation ceiling (`supply * max_utilization - borrow`) and the
    /// position's own `max_debt_ceiling - amount`. The vault's live token
    /// balance caps both, because that is all that can actually be moved.
    pub fn available_for(
        max_utilization_bps: u64,
        total_supply: u64,
        total_borrow: u64,
        debt: u64,
        max_debt_ceiling: u64,
        vault_balance: u64,
    ) -> Result<u64> {
        if max_utilization_bps > FEE_BASIS_POINTS_SCALE {
            return Err(FlashError::Config(
                "reserve max utilization exceeds 100%".into(),
            ));
        }
        let utilisation_cap = ((total_supply as u128) * (max_utilization_bps as u128))
            / (FEE_BASIS_POINTS_SCALE as u128);
        let available = utilisation_cap
            .saturating_sub(total_borrow as u128)
            .min((max_debt_ceiling as u128).saturating_sub(debt as u128))
            .min(vault_balance as u128);
        u64::try_from(available).map_err(|_| FlashError::Overflow)
    }

    /// Read the protocol-wide flash fee, in basis points, from the live admin.
    ///
    /// A paused protocol is an error rather than a zero fee, so a paused
    /// protocol never looks borrowable.
    pub async fn protocol_fee_bps(&self, rpc: &beast_rpc::RpcClient) -> Result<u64> {
        let admin = self.flashloan_admin();
        let account = rpc
            .get_account_info(address(&admin))
            .await?
            .ok_or_else(|| FlashError::Reserve("Jupiter flashloan admin not found".into()))?;
        if account.account.owner != super::address(&self.flashloan_program) {
            return Err(FlashError::Reserve(
                "flashloan admin is not owned by the Jupiter flashloan program".into(),
            ));
        }
        let admin = Self::decode_admin(&account.account.data)?;
        if !admin.active {
            return Err(FlashError::Reserve(
                "Jupiter flashloan protocol is paused".into(),
            ));
        }
        Ok(admin.fee_bps)
    }

    /// Build a complete reserve for one mint from live accounts.
    ///
    /// Working from live state means the reported `available` is the amount
    /// that can actually be borrowed right now, not a cached snapshot.
    pub async fn resolve_reserve(
        &self,
        rpc: &beast_rpc::RpcClient,
        mint: &Address,
    ) -> Result<FlashReserve> {
        let mint_key = pubkey(mint);
        let reserve_key = self.liquidity_reserve(&mint_key);
        let account = rpc
            .get_account_info(address(&reserve_key))
            .await?
            .ok_or_else(|| {
                FlashError::Reserve(format!(
                    "Jupiter liquidity reserve for mint {mint} not found"
                ))
            })?;
        let liquidity = Self::decode_reserve_fields(&account.account.data)?;
        if address(&liquidity.mint) != *mint {
            return Err(FlashError::Reserve(format!(
                "reserve is for mint {}, expected {mint}",
                address(&liquidity.mint)
            )));
        }

        // Token program and decimals come from the mint, so a Token-2022
        // reserve is rejected here instead of being silently mis-built.
        let mint_account = rpc
            .get_account_info(*mint)
            .await?
            .ok_or_else(|| FlashError::Reserve(format!("mint {mint} not found on chain")))?;
        let token_program = mint_account.account.owner;
        if token_program != address(&spl_token::id()) {
            return Err(FlashError::Reserve(format!(
                "mint {mint} uses token program {token_program}, not classic SPL Token"
            )));
        }
        let decimals = spl_token::state::Mint::unpack_from_slice(&mint_account.account.data)
            .map_err(|error| {
                FlashError::Reserve(format!("mint {mint} is not an SPL mint: {error}"))
            })?
            .decimals;

        let fee_bits = self.protocol_fee_bps(rpc).await?;
        let position_key = self.borrow_position(&mint_key);
        let position = rpc.get_account_info(address(&position_key)).await?;
        let (debt, max_debt_ceiling) = match &position {
            Some(account) => {
                let position = Self::decode_borrow_position(&account.account.data)?;
                if position.protocol != self.flashloan_admin() {
                    return Err(FlashError::Reserve(
                        "borrow position is controlled by another protocol".into(),
                    ));
                }
                (position.amount, position.max_debt_ceiling)
            }
            // A position only exists once the protocol has activated the mint;
            // without one there is nothing to borrow through.
            None => (0, 0),
        };
        let vault_balance = rpc
            .get_token_account_balance(address(&liquidity.vault))
            .await?
            .amount;

        Ok(FlashReserve {
            mint: *mint,
            reserve: address(&reserve_key),
            supply_vault: address(&liquidity.vault),
            fee_vault: address(&position_key),
            lending_market: address(&self.liquidity()),
            lending_market_authority: address(&self.flashloan_admin()),
            token_program,
            remaining_accounts: vec![address(&self.rate_model(&mint_key))],
            flash_fee_bits: fee_bits,
            fee_model: FlashFeeModel::Bps16Ceiling,
            host_fee_percentage: 0,
            available: Self::available_for(
                liquidity.max_utilization_bps as u64,
                liquidity.total_supply,
                liquidity.total_borrow,
                debt,
                max_debt_ceiling,
                vault_balance,
            )?,
            decimals,
            status: 0,
        })
    }

    /// Fetch every reserve for the mints that have an active flashloan borrow
    /// position. Walking the positions rather than the reserves is the only
    /// correct order: a reserve without a position cannot be borrowed through.
    pub async fn discover_reserves(&self, rpc: &beast_rpc::RpcClient) -> Result<Vec<FlashReserve>> {
        let liquidity_program = Address::from_str(JUPITER_LIQUIDITY_PROGRAM_ID)
            .map_err(|error| FlashError::Pubkey(error.to_string()))?;
        let filters = vec![
            json!({ "dataSize": USER_BORROW_POSITION_LEN }),
            json!({
                "memcmp": {
                    "offset": POSITION_PROTOCOL_OFFSET,
                    "bytes": self.flashloan_admin().to_string(),
                }
            }),
        ];
        let positions = rpc
            .get_program_accounts(liquidity_program, filters, None)
            .await?;
        let mut reserves = Vec::with_capacity(positions.len());
        for (_, account) in positions {
            let position = match Self::decode_borrow_position(&account.data) {
                Ok(position) => position,
                Err(error) => {
                    tracing::debug!(%error, "skipping an undecodable Jupiter Lend position");
                    continue;
                }
            };
            match self.resolve_reserve(rpc, &address(&position.mint)).await {
                Ok(reserve) => reserves.push(reserve),
                Err(error) => {
                    tracing::debug!(
                        mint = %address(&position.mint),
                        %error,
                        "skipping a Jupiter Lend mint"
                    );
                }
            }
        }
        Ok(reserves)
    }

    /// Resolve every Jupiter Lend reserve for one mint.
    pub async fn reserves_for_mint(
        &self,
        rpc: &beast_rpc::RpcClient,
        mint: &Address,
    ) -> Result<Vec<FlashReserve>> {
        Ok(vec![self.resolve_reserve(rpc, mint).await?])
    }

    /// Pick the deepest active reserve for a mint.
    pub async fn best_reserve_for_mint(
        &self,
        rpc: &beast_rpc::RpcClient,
        mint: &Address,
    ) -> Result<FlashReserve> {
        self.reserves_for_mint(rpc, mint)
            .await?
            .into_iter()
            .filter(FlashReserve::flash_enabled)
            .max_by_key(|reserve| reserve.available)
            .ok_or_else(|| FlashError::NoLiquidity(mint.to_string()))
    }

    /// Account list for both instructions, in the published IDL's order.
    ///
    /// `instruction_sysvar` is how the payback locates the borrow, so there is
    /// no borrow index to patch. `remaining_accounts[0]` is the per-mint rate
    /// model PDA the decoder populates.
    fn accounts(
        &self,
        wallet: &Pubkey,
        source: &Pubkey,
        reserve: &FlashReserve,
    ) -> Result<Vec<AccountMeta>> {
        let rate_model = reserve.remaining_accounts.first().ok_or_else(|| {
            FlashError::Reserve("Jupiter reserve has no rate model account".into())
        })?;
        Ok(vec![
            AccountMeta::new(*wallet, true),
            AccountMeta::new(self.flashloan_admin(), false),
            AccountMeta::new(*source, false),
            AccountMeta::new_readonly(pubkey(&reserve.mint), false),
            AccountMeta::new(pubkey(&reserve.reserve), false),
            AccountMeta::new(pubkey(&reserve.fee_vault), false),
            AccountMeta::new_readonly(pubkey(rate_model), false),
            AccountMeta::new(pubkey(&reserve.supply_vault), false),
            AccountMeta::new_readonly(pubkey(&reserve.lending_market), false),
            AccountMeta::new_readonly(self.liquidity_program, false),
            AccountMeta::new_readonly(pubkey(&reserve.token_program), false),
            AccountMeta::new_readonly(spl_associated_token_account::id(), false),
            AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
            AccountMeta::new_readonly(INSTRUCTIONS_SYSVAR, false),
        ])
    }
}

impl FlashProvider for JupiterFlashProvider {
    fn id(&self) -> &'static str {
        "jupiter"
    }

    fn program_id(&self) -> Pubkey {
        self.flashloan_program
    }

    fn enabled(&self) -> bool {
        self.enabled
    }

    fn decode_reserve(&self, address: &Address, data: &[u8]) -> Result<FlashReserve> {
        let liquidity = Self::decode_reserve_fields(data)?;
        // Protocol-side ceiling only; the position's debt ceiling and the live
        // vault balance need more reads, which `resolve_pinned_reserve` does.
        let available = Self::available_for(
            liquidity.max_utilization_bps as u64,
            liquidity.total_supply,
            liquidity.total_borrow,
            0,
            u64::MAX,
            u64::MAX,
        )?;
        Ok(FlashReserve {
            mint: super::address(&liquidity.mint),
            reserve: *address,
            supply_vault: super::address(&liquidity.vault),
            fee_vault: super::address(&self.borrow_position(&liquidity.mint)),
            lending_market: super::address(&self.liquidity()),
            lending_market_authority: super::address(&self.flashloan_admin()),
            token_program: super::address(&spl_token::id()),
            remaining_accounts: vec![super::address(&self.rate_model(&liquidity.mint))],
            flash_fee_bits: 0,
            fee_model: FlashFeeModel::Bps16Ceiling,
            host_fee_percentage: 0,
            available,
            decimals: 0,
            status: 0,
        })
    }

    fn resolve_pinned_reserve<'a>(
        &'a self,
        rpc: &'a beast_rpc::RpcClient,
        address: &'a Address,
        data: &'a [u8],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FlashReserve>> + Send + 'a>>
    {
        Box::pin(async move {
            // `flash.reserve` can name the liquidity reserve or the mint; both
            // resolve to the same liquidity, and the mint path is the only one
            // that can report the position's debt ceiling.
            let mint = if Self::decode_reserve_fields(data)?.mint != super::pubkey(address) {
                *address
            } else {
                super::address(&Self::decode_reserve_fields(data)?.mint)
            };
            self.resolve_reserve(rpc, &mint).await
        })
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
            return Err(FlashError::NotEnabled(self.id().into()));
        }
        let mut data = Vec::with_capacity(16);
        data.extend_from_slice(&FLASH_BORROW_DISCRIMINATOR);
        data.extend_from_slice(&amount.to_le_bytes());
        Ok(FlashInstruction {
            instruction: Instruction {
                program_id: self.flashloan_program,
                accounts: self.accounts(&wallet, &destination, reserve)?,
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
            return Err(FlashError::NotEnabled(self.id().into()));
        }
        let mut data = Vec::with_capacity(16);
        data.extend_from_slice(&FLASH_PAYBACK_DISCRIMINATOR);
        data.extend_from_slice(&amount.to_le_bytes());
        Ok(FlashInstruction {
            instruction: Instruction {
                program_id: self.flashloan_program,
                accounts: self.accounts(&wallet, &source, reserve)?,
                data,
            },
            index: borrow_index,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(key: &Pubkey) -> Address {
        Address::from_slice(key.as_ref()).expect("32 bytes")
    }

    fn base_reserve() -> FlashReserve {
        FlashReserve {
            mint: address(&Pubkey::new_unique()),
            reserve: address(&Pubkey::new_unique()),
            supply_vault: address(&Pubkey::new_unique()),
            fee_vault: address(&Pubkey::new_unique()),
            lending_market: address(&Pubkey::new_unique()),
            lending_market_authority: address(&Pubkey::new_unique()),
            token_program: address(&spl_token::id()),
            remaining_accounts: vec![address(&Pubkey::new_unique())],
            flash_fee_bits: 0,
            fee_model: FlashFeeModel::Bps16Ceiling,
            host_fee_percentage: 0,
            available: 1,
            decimals: 9,
            status: 0,
        }
    }

    fn admin_bytes(fee: u16, active: bool, in_flight: bool, in_flight_amount: u64) -> Vec<u8> {
        let mut data = vec![0u8; FLASHLOAN_ADMIN_LEN];
        data[..8].copy_from_slice(&FLASHLOAN_ADMIN_DISCRIMINATOR);
        data[ADMIN_FLASHLOAN_FEE_OFFSET..ADMIN_FLASHLOAN_FEE_OFFSET + 2]
            .copy_from_slice(&fee.to_le_bytes());
        data[ADMIN_STATUS_OFFSET] = u8::from(active);
        data[ADMIN_IS_ACTIVE_OFFSET] = u8::from(in_flight);
        data[ADMIN_ACTIVE_AMOUNT_OFFSET..ADMIN_ACTIVE_AMOUNT_OFFSET + 8]
            .copy_from_slice(&in_flight_amount.to_le_bytes());
        data
    }

    fn reserve_bytes(
        mint: &Pubkey,
        vault: &Pubkey,
        max_utilization: u16,
        supply: u64,
        borrow: u64,
    ) -> Vec<u8> {
        let mut data = vec![0u8; TOKEN_RESERVE_LEN];
        data[..8].copy_from_slice(&TOKEN_RESERVE_DISCRIMINATOR);
        data[RESERVE_MINT_OFFSET..RESERVE_MINT_OFFSET + 32].copy_from_slice(mint.as_ref());
        data[RESERVE_VAULT_OFFSET..RESERVE_VAULT_OFFSET + 32].copy_from_slice(vault.as_ref());
        data[RESERVE_MAX_UTILIZATION_OFFSET..RESERVE_MAX_UTILIZATION_OFFSET + 2]
            .copy_from_slice(&max_utilization.to_le_bytes());
        data[RESERVE_TOTAL_SUPPLY_OFFSET..RESERVE_TOTAL_SUPPLY_OFFSET + 8]
            .copy_from_slice(&supply.to_le_bytes());
        data[RESERVE_TOTAL_BORROW_OFFSET..RESERVE_TOTAL_BORROW_OFFSET + 8]
            .copy_from_slice(&borrow.to_le_bytes());
        data
    }

    fn position_bytes(
        protocol: &Pubkey,
        mint: &Pubkey,
        amount: u64,
        max_debt_ceiling: u64,
    ) -> Vec<u8> {
        let mut data = vec![0u8; USER_BORROW_POSITION_LEN];
        data[..8].copy_from_slice(&USER_BORROW_POSITION_DISCRIMINATOR);
        data[POSITION_PROTOCOL_OFFSET..POSITION_PROTOCOL_OFFSET + 32]
            .copy_from_slice(protocol.as_ref());
        data[POSITION_MINT_OFFSET..POSITION_MINT_OFFSET + 32].copy_from_slice(mint.as_ref());
        data[POSITION_AMOUNT_OFFSET..POSITION_AMOUNT_OFFSET + 8]
            .copy_from_slice(&amount.to_le_bytes());
        data[POSITION_MAX_DEBT_CEILING_OFFSET..POSITION_MAX_DEBT_CEILING_OFFSET + 8]
            .copy_from_slice(&max_debt_ceiling.to_le_bytes());
        data
    }

    #[test]
    fn the_discriminators_are_distinct() {
        assert_eq!(FLASH_BORROW_DISCRIMINATOR.len(), 8);
        assert_eq!(FLASH_PAYBACK_DISCRIMINATOR.len(), 8);
        assert_ne!(FLASH_BORROW_DISCRIMINATOR, FLASH_PAYBACK_DISCRIMINATOR);
    }

    #[test]
    fn the_pdas_match_the_live_accounts() {
        let provider = JupiterFlashProvider::mainnet();
        assert_eq!(
            provider.flashloan_admin().to_string(),
            JUPITER_FLASHLOAN_ADMIN
        );
        assert_eq!(provider.liquidity().to_string(), JUPITER_LIQUIDITY);
        let wsol = spl_token::native_mint::id();
        assert_eq!(
            provider.liquidity_reserve(&wsol).to_string(),
            JUPITER_WSOL_RESERVE
        );
        assert_eq!(
            provider.borrow_position(&wsol).to_string(),
            JUPITER_WSOL_BORROW_POSITION
        );
        assert_eq!(
            provider
                .liquidity_vault(&wsol, &spl_token::id())
                .to_string(),
            JUPITER_WSOL_VAULT
        );
    }

    #[test]
    fn decodes_a_feeless_active_admin() {
        let admin = JupiterFlashProvider::decode_admin(&admin_bytes(0, true, false, 0)).unwrap();
        assert!(admin.active);
        assert_eq!(admin.fee_bps, 0);
        assert!(!admin.in_flight);
        assert_eq!(admin.in_flight_amount, 0);
    }

    #[test]
    fn decodes_a_paused_admin_as_inactive() {
        let admin = JupiterFlashProvider::decode_admin(&admin_bytes(0, false, false, 0)).unwrap();
        assert!(!admin.active);
        assert_eq!(admin.fee_bps, 0);
    }

    #[test]
    fn decodes_a_nonzero_fee() {
        let admin = JupiterFlashProvider::decode_admin(&admin_bytes(13, true, false, 0)).unwrap();
        assert_eq!(admin.fee_bps, 13);
    }

    #[test]
    fn rejects_an_admin_with_a_wrong_discriminator() {
        let mut data = admin_bytes(0, true, false, 0);
        data[0] ^= 0xff;
        assert!(JupiterFlashProvider::decode_admin(&data).is_err());
    }

    #[test]
    fn rejects_a_truncated_admin() {
        assert!(JupiterFlashProvider::decode_admin(&[0u8; 8]).is_err());
    }

    #[test]
    fn decodes_the_live_wsol_reserve_shape() {
        let mint = spl_token::native_mint::id();
        let vault = Pubkey::new_unique();
        let liquidity = JupiterFlashProvider::decode_reserve_fields(&reserve_bytes(
            &mint, &vault, 10_000, 1_000, 400,
        ))
        .unwrap();
        assert_eq!(liquidity.mint, mint);
        assert_eq!(liquidity.vault, vault);
        assert_eq!(liquidity.max_utilization_bps, 10_000);
        assert_eq!(liquidity.total_supply, 1_000);
        assert_eq!(liquidity.total_borrow, 400);
    }

    #[test]
    fn decodes_the_live_wsol_position_shape() {
        let protocol = Pubkey::new_unique();
        let mint = spl_token::native_mint::id();
        let position = JupiterFlashProvider::decode_borrow_position(&position_bytes(
            &protocol, &mint, 72, 15_000,
        ))
        .unwrap();
        assert_eq!(position.protocol, protocol);
        assert_eq!(position.mint, mint);
        assert_eq!(position.amount, 72);
        assert_eq!(position.max_debt_ceiling, 15_000);
    }

    #[test]
    fn the_position_ceiling_is_usually_the_binding_limit() {
        // 395,828 SOL in the vault and 411,268 SOL of utilisation headroom,
        // but the borrow position may only reach a 15,000 SOL ceiling.
        assert_eq!(
            JupiterFlashProvider::available_for(
                10_000,
                411_268_131_781_081 + 192_242_472_275_291,
                192_242_472_275_291,
                72,
                15_000_000_000_000,
                395_828_385_252_217,
            )
            .unwrap(),
            15_000_000_000_000 - 72
        );
    }

    #[test]
    fn available_takes_the_smallest_ceiling() {
        // Utilisation wins when it is the smallest term.
        assert_eq!(
            JupiterFlashProvider::available_for(10_000, 100, 40, 0, u64::MAX, u64::MAX).unwrap(),
            60
        );
        // A half-utilised reserve can only lend up to 50 units.
        assert_eq!(
            JupiterFlashProvider::available_for(5_000, 1_000, 0, 0, u64::MAX, u64::MAX).unwrap(),
            500
        );
        // The real token balance wins when it is the smallest term.
        assert_eq!(
            JupiterFlashProvider::available_for(10_000, 1_000, 0, 0, u64::MAX, 7).unwrap(),
            7
        );
    }

    #[test]
    fn rejects_an_absurd_max_utilization() {
        assert!(JupiterFlashProvider::available_for(10_001, 1, 0, 0, 0, 0).is_err());
    }

    #[test]
    fn the_bps_fee_model_charges_ceil_amount_times_bps() {
        // 1 bp of 1 SOL is 100,000 lamports.
        let reserve = FlashReserve {
            fee_model: FlashFeeModel::Bps16Ceiling,
            flash_fee_bits: 1,
            ..base_reserve()
        };
        assert_eq!(reserve.flash_fee(1_000_000_000).unwrap(), 100_000);
        // 13 bps of 100 USDC is 130,000 units, rounding up.
        let reserve = FlashReserve {
            fee_model: FlashFeeModel::Bps16Ceiling,
            flash_fee_bits: 13,
            decimals: 6,
            ..base_reserve()
        };
        assert_eq!(reserve.flash_fee(100_000_000).unwrap(), 130_000);
        assert_eq!(reserve.flash_fee(1).unwrap(), 1);
        assert_eq!(reserve.flash_fee(0).unwrap(), 0);
        // Zero basis points is a free loan, exactly as the protocol reports.
        let free = FlashReserve {
            fee_model: FlashFeeModel::Bps16Ceiling,
            flash_fee_bits: 0,
            ..base_reserve()
        };
        assert_eq!(free.flash_fee(u64::MAX).unwrap(), 0);
        assert_eq!(free.repayment_amount(1_000_000).unwrap(), 1_000_000);
    }

    #[test]
    fn borrow_and_repay_share_one_account_list() {
        let provider = JupiterFlashProvider::mainnet();
        let reserve = base_reserve();
        let wallet = Pubkey::new_unique();
        let source = Pubkey::new_unique();
        let borrow = provider
            .borrow(&reserve, wallet, source, 1_000_000, 0)
            .unwrap();
        let repay = provider
            .repay(&reserve, wallet, source, 1_000_000, borrow.index)
            .unwrap();
        assert_eq!(
            borrow
                .instruction
                .accounts
                .iter()
                .map(|meta| meta.pubkey)
                .collect::<Vec<_>>(),
            repay
                .instruction
                .accounts
                .iter()
                .map(|meta| meta.pubkey)
                .collect::<Vec<_>>()
        );
        assert_eq!(borrow.instruction.accounts.len(), 14);
        assert_eq!(borrow.instruction.accounts[0].pubkey, wallet);
        assert!(borrow.instruction.accounts[0].is_signer);
        assert_eq!(
            borrow.instruction.accounts[1].pubkey,
            provider.flashloan_admin()
        );
        assert_eq!(borrow.instruction.accounts[2].pubkey, source);
        assert_eq!(
            borrow.instruction.accounts[13].pubkey,
            solana_sdk::sysvar::instructions::ID
        );
        // Both instructions carry only the principal and its own discriminator.
        let mut borrow_data = FLASH_BORROW_DISCRIMINATOR.to_vec();
        borrow_data.extend_from_slice(&1_000_000u64.to_le_bytes());
        assert_eq!(borrow.instruction.data, borrow_data);
        let mut repay_data = FLASH_PAYBACK_DISCRIMINATOR.to_vec();
        repay_data.extend_from_slice(&1_000_000u64.to_le_bytes());
        assert_eq!(repay.instruction.data, repay_data);
    }

    #[test]
    fn a_deep_reserve_needs_a_rate_model_account() {
        let provider = JupiterFlashProvider::mainnet();
        let mut broken = base_reserve();
        broken.remaining_accounts.clear();
        assert!(provider
            .borrow(&broken, Pubkey::new_unique(), Pubkey::new_unique(), 1, 0)
            .is_err());
    }

    #[test]
    fn an_unverified_provider_refuses_to_build_instructions() {
        let provider = JupiterFlashProvider::unverified();
        let error = provider
            .borrow(
                &base_reserve(),
                Pubkey::new_unique(),
                Pubkey::new_unique(),
                1,
                0,
            )
            .unwrap_err();
        assert!(matches!(error, FlashError::NotEnabled(_)));
    }
}
