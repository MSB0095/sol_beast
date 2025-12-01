// Sol Beast Core Library
// Platform-agnostic trading logic

pub mod models;
pub mod error;
pub mod tx_builder;
pub mod idl;
pub mod settings;

#[cfg(feature = "native")]
pub mod native;

#[cfg(feature = "wasm")]
pub mod wasm;

// Re-exports
pub use error::CoreError;
pub use models::*;
pub use settings::Settings;
