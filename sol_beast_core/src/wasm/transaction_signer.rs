// WASM transaction signer using browser wallet integration
// This trait implementation defers signing to the browser (e.g., Phantom, Magic Eden)

use crate::error::CoreError;
use crate::transaction_signer::TransactionSigner;
use solana_program::instruction::Instruction;
use solana_program::pubkey::Pubkey;

/// WASM transaction signer - delegates to browser wallet
pub struct WasmTransactionSigner {
    /// The wallet's public key
    pub_key: Pubkey,
    /// Callback to actual signing (provided by JavaScript)
    sign_callback: Box<dyn Fn(&[u8]) -> Option<Vec<u8>>>,
}

impl WasmTransactionSigner {
    pub fn new(pub_key: Pubkey, sign_callback: Box<dyn Fn(&[u8]) -> Option<Vec<u8>>>) -> Self {
        Self {
            pub_key,
            sign_callback,
        }
    }
}

#[async_trait::async_trait(?Send)]
impl TransactionSigner for WasmTransactionSigner {
    fn public_key(&self) -> Pubkey {
        self.pub_key
    }

    async fn sign_instructions(&self, _instructions: Vec<Instruction>, _recent_blockhash: &str) -> Result<Vec<u8>, CoreError> {
        // In WASM, we don't have direct instruction building
        // Instead, the JS layer builds the transaction and passes it to sign_transaction_bytes
        Err(CoreError::Validation(
            "WASM signer requires pre-built transaction bytes; use sign_transaction_bytes instead".to_string(),
        ))
    }

    async fn sign_transaction_bytes(&self, transaction_bytes: &[u8]) -> Result<Vec<u8>, CoreError> {
        (self.sign_callback)(transaction_bytes)
            .ok_or_else(|| CoreError::Validation("Wallet signing failed".to_string()))
    }

    async fn is_ready(&self) -> bool {
        // In a real implementation, check if wallet is connected
        true
    }
}

