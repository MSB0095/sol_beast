// Core library for sol_beast - shared between CLI and WASM builds
#![cfg_attr(target_arch = "wasm32", allow(unused_imports))]

pub mod core {
    pub mod error;
    pub mod models;
    pub mod state;
}

pub mod blockchain {
    pub mod rpc_client;
    pub mod rpc;
    pub mod rpc_helpers;
    pub mod signer;
    pub mod transaction;
    pub mod tx_builder;
}

pub mod trading {
    pub mod strategy;
    pub mod buyer;
    pub mod monitor;
}

pub mod config {
    pub mod settings;
    pub mod wallet;
}

pub mod protocols {
    pub mod idl;
    pub mod pumpfun;
}

pub mod helius;

pub mod connectivity {
    pub mod ws;
    pub mod api;
}

// Re-export commonly used types
pub use core::error::CoreError;
pub use core::models::{
    BondingCurveState,
    Holding,
    OffchainMetadata,
    OnchainFullMetadata,
    TradeRecord,
    UserAccount,
};
pub use blockchain::transaction::{TransactionBuilder, TransactionResult};
pub use config::wallet::{WalletManager, WalletInfo};
pub use trading::strategy::TradingStrategy;
pub use core::models::StrategyConfig;
pub use config::settings::Settings;
pub use blockchain::signer::*;
pub use config::settings::{load_keypair_from_env_var, parse_private_key_string};
pub use protocols::idl::SimpleIdl;

// Re-export modules for backward compatibility with main.rs and other consumers
pub use core::models as models;
pub use core::state as state;
pub use blockchain::rpc_client as rpc_client;
pub use blockchain::rpc as rpc;
pub use blockchain::rpc_helpers as rpc_helpers;
pub use blockchain::signer as signer;
pub use trading::buyer as buyer;
pub use trading::monitor as monitor;
pub use connectivity::api as api;
pub use connectivity::ws as ws;
pub use config::settings as settings;
pub use protocols::idl as idl;
// If optional helius features are required they are implemented in a separate
// crate `sol_beast_helius` to keep core free of platform-specific network code.

// Platform-specific initialization
#[cfg(target_arch = "wasm32")]
pub fn init() {
    use log::Level;
    console_log::init_with_level(Level::Info).expect("Failed to initialize logger");
}

#[cfg(all(not(target_arch = "wasm32"), feature = "env_logger"))]
pub fn init() {
    env_logger::init();
}

#[cfg(all(not(target_arch = "wasm32"), not(feature = "env_logger")))]
pub fn init() {
    // No-op when env_logger is not enabled
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn initialize_wasm() {
    init();
    log::info!("sol_beast WASM module initialized");
}
