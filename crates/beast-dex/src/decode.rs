//! Verified account decoders.
//!
//! Offsets in this file are reviewed against the on-chain program IDLs/source.
//! A decoder never invents reserves: if a pool keeps its reserves in token
//! accounts, the caller must supply them in [`DecodeContext::reserves`].
//! Pools without reserves are still decoded (mints, vaults, fee) but are not
//! locally quotable until the feed refreshes those token accounts.

use crate::address::Address;
use crate::catalog::{DecoderKind, Venue};
use crate::error::{
    check_discriminator, read_address, read_u128, read_u16, read_u32, read_u64, read_u8,
};
use crate::model::{PoolModel, PoolSnapshot};
use crate::DexError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default)]
pub struct DecodeAux<'a> {
    /// Raydium AmmConfig account for CPMM/CLMM fee tiers.
    pub amm_config: Option<&'a [u8]>,
    /// Fallback mint decimals when the pool does not store them.
    pub decimals: Option<(u8, u8)>,
    /// Explicit fee override for tests/ops.
    pub fee_override_bps: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
pub struct DecodeContext {
    pub address: Address,
    pub slot: u64,
    /// Reserves ordered as the decoded `(mint_a, mint_b)`.
    pub reserves: Option<(u64, u64)>,
    pub decimals: Option<(u8, u8)>,
}

impl DecodeContext {
    pub fn new(address: Address, slot: u64) -> Self {
        Self {
            address,
            slot,
            reserves: None,
            decimals: None,
        }
    }

    pub fn with_reserves(mut self, reserves: (u64, u64)) -> Self {
        self.reserves = Some(reserves);
        self
    }

    pub fn with_decimals(mut self, decimals: (u8, u8)) -> Self {
        self.decimals = Some(decimals);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakePoolState {
    pub pool_mint: Address,
    pub total_lamports: u64,
    pub pool_token_supply: u64,
    pub exchange_rate: f64,
}

/// Byte offsets that identify the two mints in a pool account.
///
/// Discovery cannot ask `getProgramAccounts` to return only the pools it wants
/// without knowing where the mints live. These offsets come from the same
/// layouts the decoders use, so a filter can never drift away from a decoder
/// silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolLayout {
    pub mint_a_offset: usize,
    pub mint_b_offset: usize,
    /// Exact account size when the program uses a fixed-size pool account and
    /// the size has been verified on mainnet. `None` when variable-size or
    /// unverified: an incorrect `dataSize` filter returns zero pools, which is
    /// worse than filtering only by mint.
    pub account_size: Option<usize>,
}

/// Return the mint offsets and optional exact account size for a decoder.
pub fn pool_layout(decoder: DecoderKind) -> Option<PoolLayout> {
    let layout = match decoder {
        DecoderKind::RaydiumCpmm => PoolLayout {
            mint_a_offset: 168,
            mint_b_offset: 200,
            account_size: Some(637),
        },
        DecoderKind::RaydiumAmmV4 => PoolLayout {
            mint_a_offset: 400,
            mint_b_offset: 432,
            account_size: Some(752),
        },
        DecoderKind::RaydiumClmm => PoolLayout {
            mint_a_offset: 73,
            mint_b_offset: 105,
            account_size: None,
        },
        DecoderKind::OrcaWhirlpool => PoolLayout {
            mint_a_offset: 101,
            mint_b_offset: 181,
            account_size: Some(653),
        },
        DecoderKind::PumpSwap => PoolLayout {
            mint_a_offset: 43,
            mint_b_offset: 75,
            account_size: None,
        },
        DecoderKind::MeteoraDammV2 => PoolLayout {
            mint_a_offset: 168,
            mint_b_offset: 200,
            account_size: None,
        },
        DecoderKind::MeteoraDammV1 => PoolLayout {
            mint_a_offset: 40,
            mint_b_offset: 72,
            account_size: None,
        },
        DecoderKind::MeteoraDlmm => PoolLayout {
            mint_a_offset: 88,
            mint_b_offset: 120,
            account_size: None,
        },
        DecoderKind::SaberStable => PoolLayout {
            mint_a_offset: 203,
            mint_b_offset: 235,
            account_size: None,
        },
        DecoderKind::SplTokenSwap => PoolLayout {
            mint_a_offset: 131,
            mint_b_offset: 163,
            account_size: None,
        },
        DecoderKind::SplStakePool | DecoderKind::Marinade => return None,
    };
    Some(layout)
}

pub fn decode_venue(
    venue: &Venue,
    data: &[u8],
    context: &DecodeContext,
    aux: &DecodeAux<'_>,
) -> Result<PoolSnapshot, DexError> {
    match venue.decoder {
        Some(DecoderKind::RaydiumCpmm) => decode_raydium_cpmm(venue, data, context, aux),
        Some(DecoderKind::RaydiumAmmV4) => decode_raydium_amm_v4(venue, data, context, aux),
        Some(DecoderKind::RaydiumClmm) => decode_raydium_clmm(venue, data, context, aux),
        Some(DecoderKind::OrcaWhirlpool) => decode_orca_whirlpool(venue, data, context, aux),
        Some(DecoderKind::PumpSwap) => decode_pumpswap(venue, data, context, aux),
        Some(DecoderKind::MeteoraDammV2) => decode_meteora_damm_v2(venue, data, context, aux),
        Some(DecoderKind::MeteoraDammV1) => decode_meteora_damm_v1(venue, data, context, aux),
        Some(DecoderKind::MeteoraDlmm) => decode_meteora_dlmm(venue, data, context, aux),
        Some(DecoderKind::SaberStable) => decode_saber(venue, data, context, aux),
        Some(DecoderKind::SplTokenSwap) => decode_spl_token_swap(venue, data, context, aux),
        Some(DecoderKind::SplStakePool) | Some(DecoderKind::Marinade) => Err(
            DexError::Unsupported("stake pool is not a swap pool; use decode_stake_pool".into()),
        ),
        None => Err(DexError::Unsupported(format!(
            "venue {} has no account decoder",
            venue.id
        ))),
    }
}

#[allow(clippy::too_many_arguments)]
fn snapshot(
    venue: &Venue,
    context: &DecodeContext,
    mint_a: Address,
    mint_b: Address,
    vault_a: Address,
    vault_b: Address,
    decimals_a: u8,
    decimals_b: u8,
    fee_bps: u32,
    model: PoolModel,
    confidence: f32,
) -> Result<PoolSnapshot, DexError> {
    let program_id = venue
        .program_id
        .parse::<Address>()
        .map_err(|e| DexError::InvalidAddress(format!("catalogue {}: {e}", venue.id)))?;
    Ok(PoolSnapshot {
        address: context.address,
        venue: venue.id.to_string(),
        program_id,
        mint_a,
        mint_b,
        vault_a,
        vault_b,
        decimals_a,
        decimals_b,
        fee_bps,
        model,
        slot: context.slot,
        confidence,
    })
}

fn fee_from_ratio(numerator: u64, denominator: u64, fallback_bps: u32) -> u32 {
    if numerator == 0 || denominator == 0 {
        return fallback_bps;
    }
    let bps = (numerator as u128 * 10_000) / denominator as u128;
    bps.clamp(1, 9_999) as u32
}

fn read_decimals(context: &DecodeContext, aux: &DecodeAux<'_>, default: (u8, u8)) -> (u8, u8) {
    context.decimals.or(aux.decimals).unwrap_or(default)
}

// ---------------------------------------------------------------------------
// Raydium CPMM
// ---------------------------------------------------------------------------

const RAYDIUM_CPMM_POOL_DISC: [u8; 8] = [247, 237, 227, 245, 215, 195, 222, 70];
const RAYDIUM_CPMM_CONFIG_DISC: [u8; 8] = [218, 244, 33, 104, 203, 203, 43, 111];

pub fn decode_raydium_cpmm(
    venue: &Venue,
    data: &[u8],
    context: &DecodeContext,
    aux: &DecodeAux<'_>,
) -> Result<PoolSnapshot, DexError> {
    check_discriminator(data, RAYDIUM_CPMM_POOL_DISC, "raydium_cpmm")?;
    if data.len() < 637 {
        return Err(DexError::AccountTooShort {
            need: 637,
            got: data.len(),
        });
    }
    let status = read_u8(data, 329)?;
    if status & 4 != 0 {
        return Err(DexError::Unsupported("raydium cpmm swap disabled".into()));
    }
    if read_u8(data, 390)? != 0 {
        // The router quotes trade-fee-only constant product. Creator fees are
        // an extra charge on one side and would make every quote optimistic.
        return Err(DexError::Unsupported(
            "raydium cpmm creator fees enabled".into(),
        ));
    }
    let mint_a = read_address(data, 168)?;
    let mint_b = read_address(data, 200)?;
    let vault_a = read_address(data, 72)?;
    let vault_b = read_address(data, 104)?;
    let decoded_decimals = (read_u8(data, 331)?, read_u8(data, 332)?);
    let decimals = if decoded_decimals.0 == 0 && decoded_decimals.1 == 0 {
        read_decimals(context, aux, (9, 9))
    } else {
        decoded_decimals
    };
    let fee = aux.amm_config.map(cpmm_fee_bps).unwrap_or(25).max(1);
    let fee = aux.fee_override_bps.unwrap_or(fee);
    let (reserve_a, reserve_b) = context.reserves.unwrap_or((0, 0));
    // Raydium accrues protocol/fund/creator fees inside the vaults and subtracts
    // them from both balances before running the swap curve. Those amounts are
    // not swappable liquidity, so quoting gross vault balances invents profit
    // that cannot be realised. Keep the effective reserves instead.
    let pending_a = read_u64(data, 341)?
        .saturating_add(read_u64(data, 357)?)
        .saturating_add(read_u64(data, 397)?);
    let pending_b = read_u64(data, 349)?
        .saturating_add(read_u64(data, 365)?)
        .saturating_add(read_u64(data, 405)?);
    let reserve_a = reserve_a.saturating_sub(pending_a);
    let reserve_b = reserve_b.saturating_sub(pending_b);
    snapshot(
        venue,
        context,
        mint_a,
        mint_b,
        vault_a,
        vault_b,
        decimals.0,
        decimals.1,
        fee,
        PoolModel::ConstantProduct {
            reserve_a,
            reserve_b,
        },
        1.0,
    )
}

fn cpmm_fee_bps(config: &[u8]) -> u32 {
    if config.len() < 20 || config[..8] != RAYDIUM_CPMM_CONFIG_DISC {
        return 25;
    }
    // Raydium's `trade_fee_rate` is the whole swap fee, in units of 1e-6
    // (hundredths of a bip). `protocol_fee_rate`, `fund_fee_rate` and
    // `creator_fee_rate` are *shares of that fee*, not additional charges.
    // Adding them produced a nonsensical 17% fee on a 0.005% pool.
    let trade = read_u64(config, 12).unwrap_or(2_500);
    trade.div_ceil(100).clamp(1, 9_999) as u32
}

// ---------------------------------------------------------------------------
// Raydium AMM v4
// ---------------------------------------------------------------------------

pub fn decode_raydium_amm_v4(
    venue: &Venue,
    data: &[u8],
    context: &DecodeContext,
    aux: &DecodeAux<'_>,
) -> Result<PoolSnapshot, DexError> {
    // Raydium v4 is not Anchor; the account begins with the AmmInfo struct.
    if data.len() < 752 {
        return Err(DexError::AccountTooShort {
            need: 752,
            got: data.len(),
        });
    }
    let status = read_u64(data, 0)?;
    if status == 0 {
        return Err(DexError::Unsupported("raydium v4 uninitialized".into()));
    }
    let mint_a = read_address(data, 400)?;
    let mint_b = read_address(data, 432)?;
    let vault_a = read_address(data, 336)?;
    let vault_b = read_address(data, 368)?;
    let decimals = read_decimals(
        context,
        aux,
        (
            read_u64(data, 32).unwrap_or(9) as u8,
            read_u64(data, 40).unwrap_or(6) as u8,
        ),
    );
    // swapFeeNumerator/Denominator at 176/184; tradeFee at 144/152.
    let fee = fee_from_ratio(
        read_u64(data, 176).unwrap_or(0),
        read_u64(data, 184).unwrap_or(0),
        fee_from_ratio(
            read_u64(data, 144).unwrap_or(25),
            read_u64(data, 152).unwrap_or(10_000),
            25,
        ),
    );
    let (reserve_a, reserve_b) = context.reserves.unwrap_or((0, 0));
    snapshot(
        venue,
        context,
        mint_a,
        mint_b,
        vault_a,
        vault_b,
        decimals.0,
        decimals.1,
        aux.fee_override_bps.unwrap_or(fee),
        PoolModel::ConstantProduct {
            reserve_a,
            reserve_b,
        },
        1.0,
    )
}

// ---------------------------------------------------------------------------
// PumpSwap
// ---------------------------------------------------------------------------

const PUMPSWAP_POOL_DISC: [u8; 8] = [241, 154, 109, 4, 17, 177, 109, 188];

pub fn decode_pumpswap(
    venue: &Venue,
    data: &[u8],
    context: &DecodeContext,
    aux: &DecodeAux<'_>,
) -> Result<PoolSnapshot, DexError> {
    check_discriminator(data, PUMPSWAP_POOL_DISC, "pumpswap")?;
    if data.len() < 271 {
        return Err(DexError::AccountTooShort {
            need: 271,
            got: data.len(),
        });
    }
    let mint_a = read_address(data, 43)?;
    let mint_b = read_address(data, 75)?;
    let vault_a = read_address(data, 139)?;
    let vault_b = read_address(data, 171)?;
    let creator_fee = read_u64(data, 261).unwrap_or(0).min(100) as u32;
    let decimals = read_decimals(context, aux, (9, 6));
    let (reserve_a, reserve_b) = context.reserves.unwrap_or((0, 0));
    snapshot(
        venue,
        context,
        mint_a,
        mint_b,
        vault_a,
        vault_b,
        decimals.0,
        decimals.1,
        aux.fee_override_bps.unwrap_or(25 + creator_fee),
        PoolModel::ConstantProduct {
            reserve_a,
            reserve_b,
        },
        1.0,
    )
}

// ---------------------------------------------------------------------------
// Orca Whirlpool
// ---------------------------------------------------------------------------

pub fn decode_orca_whirlpool(
    venue: &Venue,
    data: &[u8],
    context: &DecodeContext,
    aux: &DecodeAux<'_>,
) -> Result<PoolSnapshot, DexError> {
    if data.len() < 653 {
        return Err(DexError::AccountTooShort {
            need: 653,
            got: data.len(),
        });
    }
    let mint_a = read_address(data, 101)?;
    let mint_b = read_address(data, 181)?;
    let vault_a = read_address(data, 133)?;
    let vault_b = read_address(data, 213)?;
    let fee_rate = read_u16(data, 45)?;
    let fee_bps = (fee_rate as u32 / 100).clamp(1, 9_999);
    let sqrt_price = read_u128(data, 65)?;
    let liquidity = read_u128(data, 49)?;
    let tick_spacing = read_u16(data, 41)?;
    let decimals = read_decimals(context, aux, (9, 6));
    if sqrt_price == 0 || liquidity == 0 {
        return Err(DexError::Unsupported(
            "orca whirlpool has no liquidity".into(),
        ));
    }
    snapshot(
        venue,
        context,
        mint_a,
        mint_b,
        vault_a,
        vault_b,
        decimals.0,
        decimals.1,
        aux.fee_override_bps.unwrap_or(fee_bps),
        PoolModel::Concentrated {
            sqrt_price_x64: sqrt_price,
            liquidity,
            tick_spacing,
        },
        0.8,
    )
}

// ---------------------------------------------------------------------------
// Raydium CLMM
// ---------------------------------------------------------------------------

const RAYDIUM_CLMM_POOL_DISC: [u8; 8] = [247, 237, 227, 245, 215, 195, 222, 70];

pub fn decode_raydium_clmm(
    venue: &Venue,
    data: &[u8],
    context: &DecodeContext,
    aux: &DecodeAux<'_>,
) -> Result<PoolSnapshot, DexError> {
    check_discriminator(data, RAYDIUM_CLMM_POOL_DISC, "raydium_clmm")?;
    if data.len() < 1544 {
        return Err(DexError::AccountTooShort {
            need: 1544,
            got: data.len(),
        });
    }
    let mint_a = read_address(data, 73)?;
    let mint_b = read_address(data, 105)?;
    let vault_a = read_address(data, 137)?;
    let vault_b = read_address(data, 169)?;
    let decimals = (read_u8(data, 233)?, read_u8(data, 234)?);
    let tick_spacing = read_u16(data, 235)?;
    let liquidity = read_u128(data, 237)?;
    let sqrt_price = read_u128(data, 253)?;
    if sqrt_price == 0 || liquidity == 0 {
        return Err(DexError::Unsupported(
            "raydium clmm has no liquidity".into(),
        ));
    }
    let fee = aux
        .amm_config
        .map(raydium_clmm_config_fee)
        .unwrap_or(1)
        .max(1);
    snapshot(
        venue,
        context,
        mint_a,
        mint_b,
        vault_a,
        vault_b,
        decimals.0,
        decimals.1,
        aux.fee_override_bps.unwrap_or(fee),
        PoolModel::Concentrated {
            sqrt_price_x64: sqrt_price,
            liquidity,
            tick_spacing,
        },
        0.8,
    )
}

fn raydium_clmm_config_fee(config: &[u8]) -> u32 {
    if config.len() < 57 {
        return 1;
    }
    let protocol = read_u32(config, 43).unwrap_or(0);
    let trade = read_u32(config, 47).unwrap_or(100);
    let fund = read_u32(config, 53).unwrap_or(0);
    (trade.saturating_add(protocol).saturating_add(fund) / 100).clamp(1, 9_999) as u32
}

// ---------------------------------------------------------------------------
// Meteora DAMM v2
// ---------------------------------------------------------------------------

pub fn decode_meteora_damm_v2(
    venue: &Venue,
    data: &[u8],
    context: &DecodeContext,
    aux: &DecodeAux<'_>,
) -> Result<PoolSnapshot, DexError> {
    if data.len() < 704 {
        return Err(DexError::AccountTooShort {
            need: 704,
            got: data.len(),
        });
    }
    let mint_a = read_address(data, 168)?;
    let mint_b = read_address(data, 200)?;
    let vault_a = read_address(data, 232)?;
    let vault_b = read_address(data, 264)?;
    let token_a_amount = read_u64(data, 680)?;
    let token_b_amount = read_u64(data, 688)?;
    let sqrt_price = read_u128(data, 456)?;
    let liquidity = read_u128(data, 360)?;
    let base_fee = read_u64(data, 8).unwrap_or(0);
    let fee_bps = if base_fee > 0 {
        (base_fee / 100_000).clamp(1, 9_999) as u32
    } else {
        30
    };
    let reserves = context.reserves.or_else(|| {
        (token_a_amount > 0 && token_b_amount > 0).then_some((token_a_amount, token_b_amount))
    });
    let model = match reserves {
        Some((reserve_a, reserve_b)) => PoolModel::ConstantProduct {
            reserve_a,
            reserve_b,
        },
        None if sqrt_price > 0 && liquidity > 0 => PoolModel::Concentrated {
            sqrt_price_x64: sqrt_price,
            liquidity,
            tick_spacing: 1,
        },
        None => PoolModel::External {
            note: "meteora damm v2 reserves require vault balances".into(),
        },
    };
    let decimals = read_decimals(context, aux, (9, 6));
    snapshot(
        venue,
        context,
        mint_a,
        mint_b,
        vault_a,
        vault_b,
        decimals.0,
        decimals.1,
        aux.fee_override_bps.unwrap_or(fee_bps),
        model,
        0.85,
    )
}

// ---------------------------------------------------------------------------
// Meteora DAMM v1
// ---------------------------------------------------------------------------

pub fn decode_meteora_damm_v1(
    venue: &Venue,
    data: &[u8],
    context: &DecodeContext,
    aux: &DecodeAux<'_>,
) -> Result<PoolSnapshot, DexError> {
    if data.len() < 168 {
        return Err(DexError::AccountTooShort {
            need: 168,
            got: data.len(),
        });
    }
    let mint_a = read_address(data, 40)?;
    let mint_b = read_address(data, 72)?;
    let vault_a = read_address(data, 104)?;
    let vault_b = read_address(data, 136)?;
    let decimals = read_decimals(context, aux, (9, 6));
    let model = match context.reserves {
        Some((reserve_a, reserve_b)) => PoolModel::ConstantProduct {
            reserve_a,
            reserve_b,
        },
        None => PoolModel::External {
            note: "meteora damm v1 reserves require vault accounts".into(),
        },
    };
    snapshot(
        venue,
        context,
        mint_a,
        mint_b,
        vault_a,
        vault_b,
        decimals.0,
        decimals.1,
        aux.fee_override_bps.unwrap_or(30),
        model,
        0.75,
    )
}

// ---------------------------------------------------------------------------
// Meteora DLMM
// ---------------------------------------------------------------------------

const METEORA_LB_PAIR_DISC: [u8; 8] = [33, 11, 49, 98, 181, 101, 177, 13];

pub fn decode_meteora_dlmm(
    venue: &Venue,
    data: &[u8],
    context: &DecodeContext,
    aux: &DecodeAux<'_>,
) -> Result<PoolSnapshot, DexError> {
    check_discriminator(data, METEORA_LB_PAIR_DISC, "meteora_dlmm")?;
    if data.len() < 904 {
        return Err(DexError::AccountTooShort {
            need: 904,
            got: data.len(),
        });
    }
    let active_id = read_u32(data, 76)? as i32;
    let bin_step = read_u16(data, 80)?;
    let status = read_u8(data, 82)?;
    if status == 2 {
        return Err(DexError::Unsupported("meteora dlmm disabled".into()));
    }
    let mint_a = read_address(data, 88)?;
    let mint_b = read_address(data, 120)?;
    let vault_a = read_address(data, 152)?;
    let vault_b = read_address(data, 184)?;
    let base_factor = read_u16(data, 8).unwrap_or(10_000) as u64;
    let power = read_u8(data, 26).unwrap_or(0) as u32;
    let fee_bps = ((base_factor
        .saturating_mul(bin_step as u64)
        .saturating_mul(10)
        .saturating_mul(10u64.saturating_pow(power.min(6))))
        / 100_000)
        .clamp(1, 9_999) as u32;
    let (reserve_a, reserve_b) = match context.reserves {
        Some((a, b)) => (Some(a), Some(b)),
        None => (None, None),
    };
    let decimals = read_decimals(context, aux, (9, 6));
    snapshot(
        venue,
        context,
        mint_a,
        mint_b,
        vault_a,
        vault_b,
        decimals.0,
        decimals.1,
        aux.fee_override_bps.unwrap_or(fee_bps),
        PoolModel::Dlmm {
            active_id,
            bin_step,
            reserve_a,
            reserve_b,
        },
        0.7,
    )
}

// ---------------------------------------------------------------------------
// Saber Stable Swap
// ---------------------------------------------------------------------------

pub fn decode_saber(
    venue: &Venue,
    data: &[u8],
    context: &DecodeContext,
    aux: &DecodeAux<'_>,
) -> Result<PoolSnapshot, DexError> {
    if data.len() < 395 {
        return Err(DexError::AccountTooShort {
            need: 395,
            got: data.len(),
        });
    }
    if data[0] == 0 {
        return Err(DexError::Unsupported("saber pool uninitialized".into()));
    }
    if data[1] != 0 {
        return Err(DexError::Unsupported("saber pool paused".into()));
    }
    let amp = read_u64(data, 3)?;
    let reserve_a = read_u64(data, 107)?;
    let reserve_b = read_u64(data, 139)?;
    let mint_a = read_address(data, 203)?;
    let mint_b = read_address(data, 235)?;
    let fee = fee_from_ratio(read_u64(data, 363)?, read_u64(data, 371)?, 4);
    let decimals = read_decimals(context, aux, (9, 9));
    snapshot(
        venue,
        context,
        mint_a,
        mint_b,
        Address::ZERO,
        Address::ZERO,
        decimals.0,
        decimals.1,
        aux.fee_override_bps.unwrap_or(fee),
        PoolModel::StableSwap {
            reserve_a,
            reserve_b,
            amp,
        },
        0.95,
    )
}

// ---------------------------------------------------------------------------
// SPL Token Swap
// ---------------------------------------------------------------------------

pub fn decode_spl_token_swap(
    venue: &Venue,
    data: &[u8],
    context: &DecodeContext,
    aux: &DecodeAux<'_>,
) -> Result<PoolSnapshot, DexError> {
    if data.len() < 291 {
        return Err(DexError::AccountTooShort {
            need: 291,
            got: data.len(),
        });
    }
    if data[0] != 1 || data[1] == 0 {
        return Err(DexError::Unsupported(
            "spl token swap invalid version".into(),
        ));
    }
    let mint_a = read_address(data, 131)?;
    let mint_b = read_address(data, 163)?;
    let vault_a = read_address(data, 35)?;
    let vault_b = read_address(data, 67)?;
    let fee = fee_from_ratio(read_u64(data, 227)?, read_u64(data, 235)?, 30);
    let decimals = read_decimals(context, aux, (9, 9));
    let (reserve_a, reserve_b) = context.reserves.unwrap_or((0, 0));
    let curve = data.get(291).copied().unwrap_or(0);
    let model = if curve == 1 {
        let amp = read_u64(data, 292).unwrap_or(100);
        PoolModel::StableSwap {
            reserve_a,
            reserve_b,
            amp,
        }
    } else {
        PoolModel::ConstantProduct {
            reserve_a,
            reserve_b,
        }
    };
    let confidence = if curve == 1 { 0.95 } else { 0.9 };
    snapshot(
        venue,
        context,
        mint_a,
        mint_b,
        vault_a,
        vault_b,
        decimals.0,
        decimals.1,
        aux.fee_override_bps.unwrap_or(fee),
        model,
        confidence,
    )
}

// ---------------------------------------------------------------------------
// SPL Stake Pool
// ---------------------------------------------------------------------------

pub fn decode_spl_stake_pool(data: &[u8]) -> Result<StakePoolState, DexError> {
    if data.len() < 274 {
        return Err(DexError::AccountTooShort {
            need: 274,
            got: data.len(),
        });
    }
    let pool_mint = read_address(data, 162)?;
    let total_lamports = read_u64(data, 258)?;
    let pool_token_supply = read_u64(data, 266)?;
    if pool_token_supply == 0 {
        return Err(DexError::Math("stake pool has zero supply".into()));
    }
    Ok(StakePoolState {
        pool_mint,
        total_lamports,
        pool_token_supply,
        exchange_rate: total_lamports as f64 / pool_token_supply as f64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::venue;

    fn put_u64(data: &mut [u8], offset: usize, value: u64) {
        data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u16(data: &mut [u8], offset: usize, value: u16) {
        data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u128(data: &mut [u8], offset: usize, value: u128) {
        data[offset..offset + 16].copy_from_slice(&value.to_le_bytes());
    }

    fn put_addr(data: &mut [u8], offset: usize, address: &str) {
        let parsed: Address = address.parse().unwrap();
        data[offset..offset + 32].copy_from_slice(parsed.as_bytes());
    }

    #[test]
    fn raydium_cpmm_decodes_mints_and_reserves() {
        let mut data = vec![0u8; 637];
        data[..8].copy_from_slice(&RAYDIUM_CPMM_POOL_DISC);
        put_addr(&mut data, 72, "So11111111111111111111111111111111111111112");
        put_addr(
            &mut data,
            104,
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
        );
        put_addr(
            &mut data,
            168,
            "So11111111111111111111111111111111111111112",
        );
        put_addr(
            &mut data,
            200,
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
        );
        data[331] = 9;
        data[332] = 6;
        data[329] = 0;
        // Booked-but-uncollected fees are not swappable liquidity.
        put_u64(&mut data, 341, 2);
        put_u64(&mut data, 349, 3);
        put_u64(&mut data, 357, 1);
        put_u64(&mut data, 365, 4);
        put_u64(&mut data, 397, 1);
        put_u64(&mut data, 405, 2);
        let context = DecodeContext::new(Address([7; 32]), 1).with_reserves((10, 20));
        let aux = DecodeAux {
            fee_override_bps: Some(25),
            ..Default::default()
        };
        let pool =
            decode_raydium_cpmm(venue("raydium_cpmm").unwrap(), &data, &context, &aux).unwrap();
        assert_eq!(pool.fee_bps, 25);
        assert_eq!(pool.decimals_a, 9);
        assert_eq!(
            pool.model,
            PoolModel::ConstantProduct {
                reserve_a: 6,
                reserve_b: 11
            }
        );
    }

    #[test]
    fn raydium_cpmm_rejects_creator_fee_pools() {
        let mut data = vec![0u8; 637];
        data[..8].copy_from_slice(&RAYDIUM_CPMM_POOL_DISC);
        data[390] = 1;
        let context = DecodeContext::new(Address([7; 32]), 1).with_reserves((10, 20));
        let error = decode_raydium_cpmm(
            venue("raydium_cpmm").unwrap(),
            &data,
            &context,
            &DecodeAux::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("creator fees"));
    }

    #[test]
    fn cpmm_fee_uses_only_the_trade_rate() {
        let mut config = vec![0u8; 236];
        config[..8].copy_from_slice(&RAYDIUM_CPMM_CONFIG_DISC);
        put_u64(&mut config, 12, 2_500);
        put_u64(&mut config, 20, 120_000);
        put_u64(&mut config, 28, 40_000);
        put_u64(&mut config, 108, 14_950);
        assert_eq!(cpmm_fee_bps(&config), 25);
        put_u64(&mut config, 12, 50);
        assert_eq!(cpmm_fee_bps(&config), 1);
    }

    #[test]
    fn orca_decodes_price_and_liquidity() {
        let mut data = vec![0u8; 653];
        put_addr(
            &mut data,
            101,
            "So11111111111111111111111111111111111111112",
        );
        put_addr(
            &mut data,
            181,
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
        );
        put_u16(&mut data, 41, 64);
        put_u16(&mut data, 45, 3_000);
        put_u128(&mut data, 49, 1_000_000_000);
        put_u128(&mut data, 65, 1u128 << 64);
        let context = DecodeContext::new(Address([7; 32]), 2).with_decimals((9, 6));
        let pool = decode_orca_whirlpool(
            venue("orca_whirlpool").unwrap(),
            &data,
            &context,
            &DecodeAux::default(),
        )
        .unwrap();
        assert_eq!(pool.fee_bps, 30);
        assert!(matches!(pool.model, PoolModel::Concentrated { .. }));
    }

    #[test]
    fn saber_decodes_stable_reserves() {
        let mut data = vec![0u8; 395];
        data[0] = 1;
        put_u64(&mut data, 3, 100);
        put_u64(&mut data, 107, 1_000_000);
        put_u64(&mut data, 139, 1_000_000);
        put_addr(
            &mut data,
            203,
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
        );
        put_addr(
            &mut data,
            235,
            "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB",
        );
        put_u64(&mut data, 363, 4);
        put_u64(&mut data, 371, 10_000);
        let context = DecodeContext::new(Address([7; 32]), 3).with_decimals((6, 6));
        let pool = decode_saber(
            venue("saber_stable").unwrap(),
            &data,
            &context,
            &DecodeAux::default(),
        )
        .unwrap();
        assert_eq!(pool.fee_bps, 4);
        assert!(matches!(pool.model, PoolModel::StableSwap { .. }));
    }

    #[test]
    fn stake_pool_exchange_rate() {
        let mut data = vec![0u8; 300];
        put_addr(
            &mut data,
            162,
            "So11111111111111111111111111111111111111112",
        );
        put_u64(&mut data, 258, 2_000_000_000);
        put_u64(&mut data, 266, 1_000_000_000);
        let state = decode_spl_stake_pool(&data).unwrap();
        assert!((state.exchange_rate - 2.0).abs() < 1e-9);
    }

    #[test]
    fn short_account_is_rejected() {
        assert!(decode_spl_stake_pool(&[0u8; 10]).is_err());
    }

    #[test]
    fn discovery_layout_matches_verified_mint_offsets() {
        assert_eq!(
            pool_layout(DecoderKind::RaydiumCpmm),
            Some(PoolLayout {
                mint_a_offset: 168,
                mint_b_offset: 200,
                account_size: Some(637),
            })
        );
        assert_eq!(
            pool_layout(DecoderKind::OrcaWhirlpool),
            Some(PoolLayout {
                mint_a_offset: 101,
                mint_b_offset: 181,
                account_size: Some(653),
            })
        );
        assert_eq!(
            pool_layout(DecoderKind::RaydiumAmmV4),
            Some(PoolLayout {
                mint_a_offset: 400,
                mint_b_offset: 432,
                account_size: Some(752),
            })
        );
        // Every swap decoder must expose offsets; a new decoder without one
        // would make `beast discover --mint` fail instead of silently scanning
        // the whole program.
        for decoder in [
            DecoderKind::RaydiumClmm,
            DecoderKind::PumpSwap,
            DecoderKind::MeteoraDammV2,
            DecoderKind::MeteoraDammV1,
            DecoderKind::MeteoraDlmm,
            DecoderKind::SaberStable,
            DecoderKind::SplTokenSwap,
        ] {
            let layout = pool_layout(decoder).expect("swap decoder needs discovery offsets");
            assert_ne!(layout.mint_a_offset, layout.mint_b_offset);
            assert!(layout.mint_a_offset + 32 <= layout.mint_b_offset);
            if let Some(size) = layout.account_size {
                assert!(layout.mint_b_offset + 32 <= size);
            }
        }
        assert_eq!(pool_layout(DecoderKind::SplStakePool), None);
        assert_eq!(pool_layout(DecoderKind::Marinade), None);
    }
}
