// Core library for sol_beast - shared between CLI and WASM builds
#![cfg_attr(target_arch = "wasm32", allow(unused_imports))]

pub mod error;
pub mod models;
pub mod rpc_client;
pub mod transaction;
pub mod wallet;
pub mod strategy;

// Re-export commonly used types
pub use error::CoreError;
pub use models::{
    BondingCurveState, 
    Holding, 
    OffchainMetadata, 
    OnchainFullMetadata,
    TradeRecord,
    UserAccount,
};
pub use transaction::{TransactionBuilder, TransactionResult};
pub use wallet::{WalletManager, WalletInfo};
pub use strategy::TradingStrategy;
pub use models::StrategyConfig;

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
