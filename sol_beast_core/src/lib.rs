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

#[cfg(feature = "native")]
pub mod native;

#[cfg(feature = "wasm")]
pub mod wasm;

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
