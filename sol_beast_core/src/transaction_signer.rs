// Platform-agnostic transaction signing abstraction
// Allows both native (keypair-based) and WASM (browser wallet) signing

use crate::error::CoreError;
use async_trait::async_trait;
use solana_program::instruction::Instruction;
use solana_program::pubkey::Pubkey;

pub type SignerResult<T> = Result<T, CoreError>;

/// Platform-agnostic transaction signer trait
/// Implementations exist for:
/// - Native: Uses Solana keypair for signing
/// - WASM: Defers to browser wallet (e.g., Phantom, Magic Eden)
// Native needs Send + Sync for threading; WASM keeps non-Send
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg(not(target_arch = "wasm32"))]
pub trait TransactionSigner: Send + Sync {
    /// Get the public key of this signer
    fn public_key(&self) -> Pubkey;

    /// Sign a list of instructions and return the serialized transaction
    /// Returns the signed transaction bytes ready to be sent to the network
    async fn sign_instructions(&self, instructions: Vec<Instruction>, recent_blockhash: &str) -> SignerResult<Vec<u8>>;

    /// Sign raw transaction bytes
    async fn sign_transaction_bytes(&self, transaction_bytes: &[u8]) -> SignerResult<Vec<u8>>;

    /// Check if this signer is ready to sign (for browser wallets, checks if connected)
    async fn is_ready(&self) -> bool;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg(target_arch = "wasm32")]
pub trait TransactionSigner {
    fn public_key(&self) -> Pubkey;
    async fn sign_instructions(&self, instructions: Vec<Instruction>, recent_blockhash: &str) -> SignerResult<Vec<u8>>;
    async fn sign_transaction_bytes(&self, transaction_bytes: &[u8]) -> SignerResult<Vec<u8>>;
    async fn is_ready(&self) -> bool;
}

