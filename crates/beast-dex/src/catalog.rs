//! The complete Solana liquidity venue catalogue.
//!
//! `Coverage` is deliberately honest:
//! - `Modeled`: decoder + local quote math is implemented.
//! - `Decoded`: account metadata is decoded, but pricing needs external data
//!   (DLMM bin arrays, stake-pool exchange rates) or is gated.
//! - `QuoteOnly`: registered and executable via Jupiter/simulation only.
//! - `Catalogued`: known program ID, adapter not implemented yet.
//! - `StakePool`: liquid-staking rate source, not a swap venue.
//!
//! Adding a venue here does NOT enable live execution. Radar only evaluates
//! `Modeled` venues, and every live path still requires simulation and the
//! on-chain profit guard.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VenueKind {
    ConstantProduct,
    Concentrated,
    Dlmm,
    Stable,
    Weighted,
    Clob,
    PropAmm,
    BondingCurve,
    Rfq,
    Aggregator,
    Launchpad,
    Vault,
    StakePool,
    Other,
}

impl std::str::FromStr for VenueKind {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "constant_product" | "cpmm" | "amm" => Ok(Self::ConstantProduct),
            "concentrated" | "clmm" => Ok(Self::Concentrated),
            "dlmm" => Ok(Self::Dlmm),
            "stable" | "stableswap" => Ok(Self::Stable),
            "weighted" => Ok(Self::Weighted),
            "clob" | "orderbook" => Ok(Self::Clob),
            "prop_amm" | "propamm" => Ok(Self::PropAmm),
            "bonding_curve" | "bondingcurve" => Ok(Self::BondingCurve),
            "rfq" => Ok(Self::Rfq),
            "aggregator" => Ok(Self::Aggregator),
            "launchpad" => Ok(Self::Launchpad),
            "vault" => Ok(Self::Vault),
            "stake_pool" | "stakepool" => Ok(Self::StakePool),
            "other" => Ok(Self::Other),
            other => Err(format!("unknown venue kind {other:?}")),
        }
    }
}

impl VenueKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ConstantProduct => "constant_product",
            Self::Concentrated => "concentrated",
            Self::Dlmm => "dlmm",
            Self::Stable => "stable",
            Self::Weighted => "weighted",
            Self::Clob => "clob",
            Self::PropAmm => "prop_amm",
            Self::BondingCurve => "bonding_curve",
            Self::Rfq => "rfq",
            Self::Aggregator => "aggregator",
            Self::Launchpad => "launchpad",
            Self::Vault => "vault",
            Self::StakePool => "stake_pool",
            Self::Other => "other",
        }
    }

    pub fn is_spot_dex(self) -> bool {
        matches!(
            self,
            Self::ConstantProduct
                | Self::Concentrated
                | Self::Dlmm
                | Self::Stable
                | Self::Weighted
                | Self::Clob
                | Self::PropAmm
                | Self::BondingCurve
                | Self::Rfq
                | Self::Launchpad
                | Self::Other
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Coverage {
    Modeled,
    Decoded,
    QuoteOnly,
    Catalogued,
    StakePool,
    Disabled,
}

impl Coverage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Modeled => "modeled",
            Self::Decoded => "decoded",
            Self::QuoteOnly => "quote_only",
            Self::Catalogued => "catalogued",
            Self::StakePool => "stake_pool",
            Self::Disabled => "disabled",
        }
    }

    pub fn is_radar_model(self) -> bool {
        matches!(self, Self::Modeled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecoderKind {
    RaydiumCpmm,
    RaydiumAmmV4,
    RaydiumClmm,
    OrcaWhirlpool,
    PumpSwap,
    MeteoraDammV2,
    MeteoraDammV1,
    MeteoraDlmm,
    SaberStable,
    SplTokenSwap,
    SplStakePool,
    Marinade,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MathModel {
    ConstantProduct,
    StableSwap,
    Weighted,
    ConcentratedApprox,
    Dlmm,
    StakePool,
    External,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Venue {
    pub id: &'static str,
    pub name: &'static str,
    pub program_id: &'static str,
    pub kind: VenueKind,
    pub coverage: Coverage,
    pub decoder: Option<DecoderKind>,
    pub math: MathModel,
    pub docs: &'static str,
    pub notes: &'static str,
}

const JUP_DOCS: &str = "https://station.jup.ag/docs/apis/swap-api";
const RAY_DOCS: &str = "https://docs.raydium.io/";
const ORCA_DOCS: &str = "https://docs.orca.so/";
const METEORA_DOCS: &str = "https://docs.meteora.ag/";
const PUMP_DOCS: &str = "https://github.com/pump-fun/pump-public-docs";
const PHOENIX_DOCS: &str = "https://docs.phoenix.trade/";
const OPENBOOK_DOCS: &str = "https://openbook-dex.notion.site/";
const SABER_DOCS: &str = "https://docs.saber.so/";
const SPL_DOCS: &str = "https://spl.solana.com/";

/// Every venue registered by BEAST 2.0. Keep entries sorted by family, not by
/// TVL, so diffs are reviewable.
pub const VENUES: &[Venue] = &[
    // ---------------------------------------------------------------------
    // Constant-product AMMs with local decoders
    // ---------------------------------------------------------------------
    Venue {
        id: "raydium_cpmm",
        name: "Raydium CPMM",
        program_id: "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C",
        kind: VenueKind::ConstantProduct,
        coverage: Coverage::Modeled,
        decoder: Some(DecoderKind::RaydiumCpmm),
        math: MathModel::ConstantProduct,
        docs: RAY_DOCS,
        notes: "Exact pool/vault/mint layout; fee read from AmmConfig.",
    },
    Venue {
        id: "raydium_amm_v4",
        name: "Raydium AMM v4",
        program_id: "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8",
        kind: VenueKind::ConstantProduct,
        coverage: Coverage::Modeled,
        decoder: Some(DecoderKind::RaydiumAmmV4),
        math: MathModel::ConstantProduct,
        docs: RAY_DOCS,
        notes: "Legacy x*y=k pools; OpenBook market accounts are not required for pricing.",
    },
    Venue {
        id: "pumpswap",
        name: "Pump.fun Swap",
        program_id: "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA",
        kind: VenueKind::ConstantProduct,
        coverage: Coverage::Modeled,
        decoder: Some(DecoderKind::PumpSwap),
        math: MathModel::ConstantProduct,
        docs: PUMP_DOCS,
        notes: "Pool layout verified; fees include coin-creator and LP shares.",
    },
    Venue {
        id: "meteora_damm_v2",
        name: "Meteora DAMM v2",
        program_id: "cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG",
        kind: VenueKind::ConstantProduct,
        coverage: Coverage::Modeled,
        decoder: Some(DecoderKind::MeteoraDammV2),
        math: MathModel::ConstantProduct,
        docs: METEORA_DOCS,
        notes: "Price snapshot needs token vault balances; pool metadata offsets verified.",
    },
    Venue {
        id: "spl_token_swap",
        name: "SPL Token Swap",
        program_id: "SwaPpA9LAaLfeLi3a68M4DjnLqgtticKg6CnyNwgAC8",
        kind: VenueKind::ConstantProduct,
        coverage: Coverage::Modeled,
        decoder: Some(DecoderKind::SplTokenSwap),
        math: MathModel::ConstantProduct,
        docs: SPL_DOCS,
        notes: "Supports constant-product and stable curves; token accounts supplied separately.",
    },
    // ---------------------------------------------------------------------
    // Stable / weighted
    // ---------------------------------------------------------------------
    Venue {
        id: "saber_stable",
        name: "Saber Stable Swap",
        program_id: "SSwpkEEcbUqx4vtoEByFjSkhKdCT862DNVb52nZg1UZ",
        kind: VenueKind::Stable,
        coverage: Coverage::Modeled,
        decoder: Some(DecoderKind::SaberStable),
        math: MathModel::StableSwap,
        docs: SABER_DOCS,
        notes: "SwapInfo layout verified; amp and fee are on-chain.",
    },
    Venue {
        id: "stabble_stable",
        name: "Stabble Stable Swap",
        program_id: "swapNyd8XiQwJ6ianp9snpu4brUqFxadzvHebnAXjJZ",
        kind: VenueKind::Stable,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://docs.stabble.org/",
        notes: "Stable math exists; account adapter pending fork verification.",
    },
    Venue {
        id: "stabble_weighted",
        name: "Stabble Weighted Swap",
        program_id: "swapFpHZwjELNnjvThjajtiVmkz3yPQEHjLtka2fwHW",
        kind: VenueKind::Weighted,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://docs.stabble.org/",
        notes: "Weighted math exists; account adapter pending fork verification.",
    },
    Venue {
        id: "mercurial_stable",
        name: "Mercurial Stable Swap",
        program_id: "MERLuDFBMmsHnsBPZw2sDQZHvXFMwp8EdjudcU2HKky",
        kind: VenueKind::Stable,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://docs.mercurial.finance/",
        notes: "Legacy stable pools; quote-only until layout/fork verification.",
    },
    Venue {
        id: "invariant",
        name: "Invariant",
        program_id: "HyaB3W9q6XdA5xwpU4XnSZV94htfmbmqJXZcEbRaJutt",
        kind: VenueKind::Concentrated,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://docs.invariant.app/",
        notes: "Concentrated/weighted pools; Jupiter fallback.",
    },
    // ---------------------------------------------------------------------
    // Concentrated liquidity with local metadata decoders
    // ---------------------------------------------------------------------
    Venue {
        id: "orca_whirlpool",
        name: "Orca Whirlpool",
        program_id: "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc",
        kind: VenueKind::Concentrated,
        coverage: Coverage::Modeled,
        decoder: Some(DecoderKind::OrcaWhirlpool),
        math: MathModel::ConcentratedApprox,
        docs: ORCA_DOCS,
        notes: "Whirlpool layout verified; current-range approximation, simulation required.",
    },
    Venue {
        id: "raydium_clmm",
        name: "Raydium CLMM",
        program_id: "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK",
        kind: VenueKind::Concentrated,
        coverage: Coverage::Modeled,
        decoder: Some(DecoderKind::RaydiumClmm),
        math: MathModel::ConcentratedApprox,
        docs: RAY_DOCS,
        notes: "Pool/AmmConfig layout verified; current-range approximation, simulation required.",
    },
    Venue {
        id: "meteora_dlmm",
        name: "Meteora DLMM",
        program_id: "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo",
        kind: VenueKind::Dlmm,
        coverage: Coverage::Decoded,
        decoder: Some(DecoderKind::MeteoraDlmm),
        math: MathModel::Dlmm,
        docs: METEORA_DOCS,
        notes: "LbPair offsets verified; bin-array walker and Jupiter validation gated.",
    },
    Venue {
        id: "crema",
        name: "Crema",
        program_id: "CLMM9tUoggJu2wagPkkqs9eFG4BWhVBZWkP1qv3Sp7tR",
        kind: VenueKind::Concentrated,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://docs.crema.finance/",
        notes: "Concentrated long tail; Jupiter fallback.",
    },
    Venue {
        id: "exponent_v2_rclmm",
        name: "Exponent V2 rCLMM",
        program_id: "XPC1MM4dYACDfykNuXYZ5una2DsMDWL24CrYubCvarC",
        kind: VenueKind::Concentrated,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://docs.exponent.finance/",
        notes: "Rate/concentrated pool; quote-only.",
    },
    // ---------------------------------------------------------------------
    // Meteora legacy / vaults
    // ---------------------------------------------------------------------
    Venue {
        id: "meteora_damm_v1",
        name: "Meteora Pool (DAMM v1)",
        program_id: "Eo7WjKq67rjJQSZxS6z3YkapzY3eMj6Xy8X5EQVn5UaB",
        kind: VenueKind::ConstantProduct,
        coverage: Coverage::Decoded,
        decoder: Some(DecoderKind::MeteoraDammV1),
        math: MathModel::ConstantProduct,
        docs: METEORA_DOCS,
        notes: "Pool metadata verified; reserves require Meteora vault accounts, so quote-gated.",
    },
    Venue {
        id: "meteora_vault",
        name: "Meteora Vault",
        program_id: "24Uqj9JCLxUeoC3hGfh5W3s9FM9uCHDS2SG3LYwBpyTi",
        kind: VenueKind::Vault,
        coverage: Coverage::Catalogued,
        decoder: None,
        math: MathModel::External,
        docs: METEORA_DOCS,
        notes: "Yield vault, not a swap venue; used only for reserve accounting.",
    },
    // ---------------------------------------------------------------------
    // Launchpads / bonding curves
    // ---------------------------------------------------------------------
    Venue {
        id: "pump_fun",
        name: "Pump.fun",
        program_id: "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P",
        kind: VenueKind::BondingCurve,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: PUMP_DOCS,
        notes: "Bonding curve; legacy sniper owns this path, engine only quotes/observes.",
    },
    Venue {
        id: "raydium_launchlab",
        name: "Raydium LaunchLab",
        program_id: "LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj",
        kind: VenueKind::Launchpad,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: RAY_DOCS,
        notes: "Launch/bonding venue; quote-only.",
    },
    Venue {
        id: "meteora_dbc",
        name: "Meteora Dynamic Bonding Curve",
        program_id: "dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN",
        kind: VenueKind::BondingCurve,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: METEORA_DOCS,
        notes: "Bonding curve; quote-only.",
    },
    Venue {
        id: "alphaqa",
        name: "AlphaQa",
        program_id: "ALPHAQmeA7bjrVuccPsYPiCvsi428SNwte66Srvs4pHA",
        kind: VenueKind::BondingCurve,
        coverage: Coverage::Catalogued,
        decoder: None,
        math: MathModel::External,
        docs: "https://alphaqa.io/",
        notes: "Long-tail launch venue; registered for quote routing.",
    },
    // ---------------------------------------------------------------------
    // CLOBs
    // ---------------------------------------------------------------------
    Venue {
        id: "phoenix",
        name: "Phoenix",
        program_id: "PhoeNiXZ8ByJGLkxNfZRnkUfjvmuYqLR89jjFHGqdXY",
        kind: VenueKind::Clob,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: PHOENIX_DOCS,
        notes: "Order book; execute only through Jupiter/market-specific builders.",
    },
    Venue {
        id: "openbook_v2",
        name: "OpenBook v2",
        program_id: "opnb2LAfJYbRMAHHvqjCwQxanZn7ReEHp1k81EohpZb",
        kind: VenueKind::Clob,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: OPENBOOK_DOCS,
        notes: "Order book; top-of-book adapter gated.",
    },
    Venue {
        id: "manifest",
        name: "Manifest CLOB",
        program_id: "MNFSTqtC93rEfYHB6hF82sKdZpUDFWkViLByLd1k1Ms",
        kind: VenueKind::Clob,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://docs.manifest.trade/",
        notes: "Order book; quote-only until market decoder is fork-tested.",
    },
    Venue {
        id: "futarchy_amm",
        name: "FutarchyAMM (MetaDAO)",
        program_id: "FUTARELBfJfQ8RDGhg1wdhddq1odMAJUePHFuBYfUxKq",
        kind: VenueKind::Other,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://docs.metadao.fi/",
        notes: "Conditional-token AMM; oracle/condition-aware execution gated.",
    },
    // ---------------------------------------------------------------------
    // Prop AMMs / RFQ (external quote only)
    // ---------------------------------------------------------------------
    Venue {
        id: "lifinity_v1",
        name: "Lifinity v1",
        program_id: "EewxydAPCCVuNEyrVN68PuSYdQ7wKn27V9Gjeoi8dy3S",
        kind: VenueKind::PropAmm,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://docs.lifinity.io/",
        notes: "Oracle-based prop AMM; never locally modeled.",
    },
    Venue {
        id: "lifinity_v2",
        name: "Lifinity v2",
        program_id: "2wT8Yq49kHgDzXuPxZSaeLaH1qbmGXtEyPy64bL7aD3c",
        kind: VenueKind::PropAmm,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://docs.lifinity.io/",
        notes: "Oracle-based prop AMM; never locally modeled.",
    },
    Venue {
        id: "obric_v2",
        name: "Obric V2",
        program_id: "obriQD1zbpyLz95G5n7nJe6a4DPjpFwa5XYPoNm113y",
        kind: VenueKind::PropAmm,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://docs.obric.xyz/",
        notes: "Prop AMM; quote-only.",
    },
    Venue {
        id: "solfi",
        name: "SolFi",
        program_id: "SoLFiHG9TfgtdUXUjWAxi3LtvYuFyDLVhBWxdMZxyCe",
        kind: VenueKind::PropAmm,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://solfi.pro/",
        notes: "Prop AMM; quote-only.",
    },
    Venue {
        id: "solfi_v2",
        name: "SolFi V2",
        program_id: "SV2EYYJyRz2YhfXwXnhNAevDEui5Q6yrfyo13WtupPF",
        kind: VenueKind::PropAmm,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://solfi.pro/",
        notes: "Prop AMM; quote-only.",
    },
    Venue {
        id: "zerofi",
        name: "ZeroFi",
        program_id: "ZERor4xhbUycZ6gb9ntrhqscUcZmAbQDjEAtCf4hbZY",
        kind: VenueKind::PropAmm,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://docs.zerofi.xyz/",
        notes: "Prop AMM; quote-only.",
    },
    Venue {
        id: "goonfi",
        name: "Goonfi",
        program_id: "goonERTdGsjnkZqWuVjs73BZ3Pb9qoCUdBUL17BnS5j",
        kind: VenueKind::PropAmm,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://goonfi.fun/",
        notes: "Prop AMM; quote-only.",
    },
    Venue {
        id: "goonfi_v2",
        name: "Goonfi V2",
        program_id: "goonuddtQRrWqqn5nFyczVKaie28f3kDkHWkHtURSLE",
        kind: VenueKind::PropAmm,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://goonfi.fun/",
        notes: "Prop AMM; quote-only.",
    },
    Venue {
        id: "byreal",
        name: "Byreal",
        program_id: "REALQqNEomY6cQGZJUGwywTBD2UmDT32rZcNnfxQ5N2",
        kind: VenueKind::PropAmm,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://byreal.io/",
        notes: "Prop AMM; quote-only.",
    },
    Venue {
        id: "bisonfi",
        name: "BisonFi",
        program_id: "BiSoNHVpsVZW2F7rx2eQ59yQwKxzU5NvBcmKshCSUypi",
        kind: VenueKind::PropAmm,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://bisonfi.com/",
        notes: "Prop AMM; quote-only.",
    },
    Venue {
        id: "fusion_amm",
        name: "FusionAMM",
        program_id: "fUSioN9YKKSa3CUC2YUc4tPkHJ5Y6XW1yz8y6F7qWz9",
        kind: VenueKind::PropAmm,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://fusionamm.xyz/",
        notes: "Prop AMM; quote-only.",
    },
    Venue {
        id: "woofi",
        name: "Woofi",
        program_id: "WooFif76YGRNjk1pA8wCsN67aQsD9f9iLsz4NcJ1AVb",
        kind: VenueKind::PropAmm,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://woo.org/",
        notes: "Prop AMM; quote-only.",
    },
    Venue {
        id: "humidifi",
        name: "Humidifi",
        program_id: "9H6tua7jkLhdm3w8BvgpTn5LZNU7g4ZynDmCiNN3q6Rp",
        kind: VenueKind::PropAmm,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://humidifi.xyz/",
        notes: "Prop AMM; quote-only.",
    },
    Venue {
        id: "quantum",
        name: "Quantum",
        program_id: "QuaNtZsgYRe5Z9Bk4LZ4cTD9tbkVoyCNf1R2BN9bBDv",
        kind: VenueKind::PropAmm,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://quantum.xyz/",
        notes: "Prop AMM; quote-only.",
    },
    Venue {
        id: "whale_street",
        name: "Whale Street",
        program_id: "FW6zUqn4iKRaeopwwhwsquTY6ABWLLgjxtrC3VPnaWBf",
        kind: VenueKind::PropAmm,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://whalestreet.xyz/",
        notes: "Prop AMM; quote-only.",
    },
    Venue {
        id: "hadronfi_v2",
        name: "HadronFi V2",
        program_id: "HADRoNbLovyqhCsocfYQYB7QdfCAAinN9HTePvBCVDQ8",
        kind: VenueKind::PropAmm,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://hadron.fi/",
        notes: "Prop AMM; quote-only.",
    },
    Venue {
        id: "jupiter_order_engine",
        name: "Jupiter Order Engine (RFQ)",
        program_id: "61DFfeTKM7trxYcPQCM78bJ794ddZprZpAwAnLiwTpYH",
        kind: VenueKind::Rfq,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: JUP_DOCS,
        notes: "RFQ; never locally modeled.",
    },
    // ---------------------------------------------------------------------
    // Long-tail spot venues registered for Jupiter fallback
    // ---------------------------------------------------------------------
    Venue {
        id: "1dex",
        name: "1Dex",
        program_id: "DEXYosS6oEGvk8uCDayvwEZz4qEyDJRf9nFgYCaqPMTm",
        kind: VenueKind::Other,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://1dex.io/",
        notes: "Jupiter fallback.",
    },
    Venue {
        id: "archer",
        name: "Archer",
        program_id: "Archer8kgiavM61GyusMzaaS2ft5sALtNsD1HxkUPMhy",
        kind: VenueKind::Other,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://archer.exchange/",
        notes: "Jupiter fallback.",
    },
    Venue {
        id: "aldrin_amm_v2",
        name: "Aldrin AMM v2",
        program_id: "CURVGoZn8zycx6FXwwevgBTB2gVvdbGTEpvMJDbgs2t4",
        kind: VenueKind::ConstantProduct,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::ConstantProduct,
        docs: "https://docs.aldrin.com/",
        notes: "Jupiter fallback until account adapter is verified.",
    },
    Venue {
        id: "bonkswap",
        name: "Bonkswap",
        program_id: "BSwp6bEBihVLdqJRKGgzjcGLHkcTuzmSo1TQkHepzH8p",
        kind: VenueKind::ConstantProduct,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::ConstantProduct,
        docs: "https://bonkswap.io/",
        notes: "Jupiter fallback.",
    },
    Venue {
        id: "dexlab",
        name: "Dexlab",
        program_id: "DSwpgjMvXhtGn6BsbqmacdBZyfLj6jSWf3HJpdJtmg6N",
        kind: VenueKind::ConstantProduct,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::ConstantProduct,
        docs: "https://dexlab.io/",
        notes: "Jupiter fallback.",
    },
    Venue {
        id: "fluxbeam",
        name: "Fluxbeam",
        program_id: "FLUXubRmkEi2q6K3Y9kBPg9248ggaZVsoSFhtJHSrm1X",
        kind: VenueKind::ConstantProduct,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::ConstantProduct,
        docs: "https://fluxbeam.xyz/",
        notes: "Jupiter fallback.",
    },
    Venue {
        id: "goosefx_v1",
        name: "GooseFX V1",
        program_id: "GAMMA7meSFWaBXF25oSUgmGRwaW6sCMFLmBNiMSdbHVT",
        kind: VenueKind::ConstantProduct,
        coverage: Coverage::Disabled,
        decoder: None,
        math: MathModel::External,
        docs: "https://docs.goosefx.io/",
        notes: "Legacy venue; disabled until liveness and layout are re-verified.",
    },
    Venue {
        id: "guacswap",
        name: "Guacswap",
        program_id: "Gswppe6ERWKpUTXvRPfXdzHhiCyJvLadVvXGfdpBqcE1",
        kind: VenueKind::ConstantProduct,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::ConstantProduct,
        docs: "https://guacswap.io/",
        notes: "Jupiter fallback.",
    },
    Venue {
        id: "orca_v2",
        name: "Orca v2 (legacy)",
        program_id: "9W959DqEETiGZocYWCQPaJ6sBmUzgfxXfqGeTEdp3aQP",
        kind: VenueKind::ConstantProduct,
        coverage: Coverage::Catalogued,
        decoder: None,
        math: MathModel::ConstantProduct,
        docs: ORCA_DOCS,
        notes: "Legacy; Whirlpool is the preferred Orca path.",
    },
    Venue {
        id: "ondo_global_market",
        name: "Ondo Global Market",
        program_id: "XzTT4XB8m7sLD2xi6snefSasaswsKCxx5Tifjondogm",
        kind: VenueKind::Other,
        coverage: Coverage::Catalogued,
        decoder: None,
        math: MathModel::External,
        docs: "https://ondo.finance/",
        notes: "RWA market; quote-only.",
    },
    Venue {
        id: "saros_amm",
        name: "Saros AMM",
        program_id: "SSwapUtytfBdBn1b9NUGG6foMVPtcWgpRU32HToDUZr",
        kind: VenueKind::ConstantProduct,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::ConstantProduct,
        docs: "https://docs.saros.xyz/",
        notes: "Jupiter fallback.",
    },
    Venue {
        id: "scorch",
        name: "Scorch",
        program_id: "SCoRcH8c2dpjvcJD6FiPbCSQyQgu3PcUAWj2Xxx3mqn",
        kind: VenueKind::Other,
        coverage: Coverage::Catalogued,
        decoder: None,
        math: MathModel::External,
        docs: "https://scorch.exchange/",
        notes: "Long-tail venue; registered.",
    },
    Venue {
        id: "aquifer",
        name: "Aquifer",
        program_id: "AQU1FRd7papthgdrwPTTq5JacJh8YtwEXaBfKU3bTz45",
        kind: VenueKind::Other,
        coverage: Coverage::Catalogued,
        decoder: None,
        math: MathModel::External,
        docs: "https://aquifer.fi/",
        notes: "Long-tail venue; registered.",
    },
    // ---------------------------------------------------------------------
    // Aggregator / router
    // ---------------------------------------------------------------------
    Venue {
        id: "jupiter_aggregator",
        name: "Jupiter Aggregator",
        program_id: "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4",
        kind: VenueKind::Aggregator,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: JUP_DOCS,
        notes: "Used to quote/route every registered long-tail venue; not a pool.",
    },
    // ---------------------------------------------------------------------
    // Stake pools / LST liquidity
    // ---------------------------------------------------------------------
    Venue {
        id: "spl_stake_pool",
        name: "SPL Stake Pool",
        program_id: "SPoo1Ku8WFXoNDMHPsrGSTSG1Y47rzgn41SLUNakuHy",
        kind: VenueKind::StakePool,
        coverage: Coverage::StakePool,
        decoder: Some(DecoderKind::SplStakePool),
        math: MathModel::StakePool,
        docs: "https://spl.solana.com/stake-pool",
        notes: "Used for JitoSOL and other SPL-stake-pool LST rates.",
    },
    Venue {
        id: "marinade",
        name: "Marinade mSOL",
        program_id: "MarBmsSgKXdrN1egZf5sqe1TMai9K1rChYNDJgjq7aD",
        kind: VenueKind::StakePool,
        coverage: Coverage::StakePool,
        decoder: Some(DecoderKind::Marinade),
        math: MathModel::StakePool,
        docs: "https://docs.marinade.finance/",
        notes: "Marinade state layout differs from SPL; adapter gated for fork tests.",
    },
    Venue {
        id: "sanctum_router",
        name: "Sanctum Router",
        program_id: "stkitrT1Uoy18Dk1fTrgPw8W6MVzoCfYoAFT4MLsmhq",
        kind: VenueKind::Aggregator,
        coverage: Coverage::QuoteOnly,
        decoder: None,
        math: MathModel::External,
        docs: "https://docs.sanctum.so/",
        notes: "LST router; quote-only.",
    },
    Venue {
        id: "sanctum_socean",
        name: "Sanctum Program (Socean)",
        program_id: "5ocnV1qiCgaQR8Jb8xWnVbApfaygJ8tNoZfgPwsgx9kx",
        kind: VenueKind::StakePool,
        coverage: Coverage::Decoded,
        decoder: Some(DecoderKind::SplStakePool),
        math: MathModel::StakePool,
        docs: "https://docs.sanctum.so/",
        notes: "Stake-pool-like account; verify layout version before live.",
    },
];

pub fn venue(id: &str) -> Option<&'static Venue> {
    VENUES.iter().find(|venue| venue.id == id)
}

pub fn venue_by_program(program_id: &str) -> Option<&'static Venue> {
    VENUES.iter().find(|venue| venue.program_id == program_id)
}

pub fn venues_by_kind(kind: VenueKind) -> impl Iterator<Item = &'static Venue> {
    VENUES.iter().filter(move |venue| venue.kind == kind)
}

pub fn modeled_venues() -> impl Iterator<Item = &'static Venue> {
    VENUES
        .iter()
        .filter(|venue| venue.coverage.is_radar_model())
}

pub fn quote_only_venues() -> impl Iterator<Item = &'static Venue> {
    VENUES
        .iter()
        .filter(|venue| venue.coverage == Coverage::QuoteOnly)
}

pub fn catalog_summary() -> BTreeMap<&'static str, usize> {
    let mut summary = BTreeMap::new();
    for venue in VENUES {
        *summary.entry(venue.coverage.as_str()).or_insert(0) += 1;
        *summary.entry(venue.kind.as_str()).or_insert(0) += 1;
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Address;
    use std::collections::HashSet;

    #[test]
    fn every_program_id_is_a_valid_pubkey() {
        for venue in VENUES {
            venue
                .program_id
                .parse::<Address>()
                .unwrap_or_else(|e| panic!("{} has invalid program id: {e}", venue.id));
        }
    }

    #[test]
    fn ids_are_unique() {
        let mut ids = HashSet::new();
        for venue in VENUES {
            assert!(ids.insert(venue.id), "duplicate venue id {}", venue.id);
        }
    }

    #[test]
    fn all_major_families_are_registered() {
        let ids: HashSet<_> = VENUES.iter().map(|v| v.id).collect();
        for expected in [
            "raydium_amm_v4",
            "raydium_cpmm",
            "raydium_clmm",
            "orca_whirlpool",
            "meteora_dlmm",
            "meteora_damm_v1",
            "meteora_damm_v2",
            "pumpswap",
            "saber_stable",
            "phoenix",
            "openbook_v2",
            "manifest",
            "lifinity_v2",
            "solfi_v2",
            "obric_v2",
            "jupiter_aggregator",
            "spl_stake_pool",
            "marinade",
        ] {
            assert!(ids.contains(expected), "missing venue {expected}");
        }
    }

    #[test]
    fn coverage_is_explicit() {
        assert!(modeled_venues().count() >= 8);
        assert!(quote_only_venues().count() >= 20);
        assert_eq!(
            VENUES
                .iter()
                .filter(|v| v.coverage == Coverage::Modeled)
                .count(),
            modeled_venues().count()
        );
    }

    #[test]
    fn lookup_helpers_work() {
        assert_eq!(
            venue("orca_whirlpool").unwrap().kind,
            VenueKind::Concentrated
        );
        assert_eq!(
            venue_by_program("CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C")
                .unwrap()
                .id,
            "raydium_cpmm"
        );
        assert!(venue("does_not_exist").is_none());
    }
}
