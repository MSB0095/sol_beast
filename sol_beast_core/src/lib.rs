// Sol Beast Core Library
// Platform-agnostic trading logic

pub mod models;
pub mod error;
pub mod tx_builder;
pub mod tx_parser;
pub mod metadata;
pub mod storage_trait;
pub mod idl;
pub mod settings;
pub mod buyer;
pub mod rpc_client;
pub mod transaction_service;
pub mod transaction_signer;
pub mod price_subscriber;
pub mod buy_service;

pub mod strategy;
pub mod pipeline;
pub mod dev_fee;

#[cfg(feature = "native")]
pub mod native;

#[cfg(all(feature = "wasm", target_arch = "wasm32"))]
pub mod wasm;
pub mod sell_service;

// Re-exports
pub use error::CoreError;
pub use models::*;
pub use settings::Settings;
pub use buyer::*;
pub use rpc_client::*;
pub use tx_parser::*;
pub use metadata::*;
pub use storage_trait::*;
pub use transaction_service::*;
pub use transaction_signer::*;
pub use price_subscriber::*;
pub use buy_service::*;
pub use sell_service::*;

