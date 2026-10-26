//! SOL BEAST 2.0 core types and policy.
//!
//! This crate deliberately has no Solana dependency. Everything here is plain
//! data, pure math, configuration and risk policy so it can be exhaustively
//! unit-tested without an RPC endpoint.

pub mod config;
pub mod economics;
pub mod error;
pub mod metrics;
pub mod risk;
pub mod supervisor;
pub mod types;

pub use config::{BeastConfig, LoadedConfig, Secrets};
pub use error::{BeastError, Result};
pub use types::*;

/// Version of the 2.0 configuration schema understood by this build.
pub const CONFIG_VERSION: u32 = 2;
