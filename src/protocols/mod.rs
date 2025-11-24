//! Protocol adapters for different memecoin launch platforms.

use solana_program::pubkey::Pubkey;
use std::collections::HashMap;

pub trait Protocol: Send + Sync {
    /// Return the program id associated with this protocol
    fn program_id(&self) -> Pubkey;

    /// Get a human-friendly name for the protocol
    fn name(&self) -> String;

    /// Map of additional config or metadata for the protocol
    fn metadata(&self) -> HashMap<String, String> { HashMap::new() }
}

pub mod pumpfun;