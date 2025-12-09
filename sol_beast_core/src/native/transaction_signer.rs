// Native transaction signer using Solana keypair

use crate::error::CoreError;
use crate::transaction_signer::TransactionSigner;
use solana_program::instruction::Instruction;
use solana_program::pubkey::Pubkey;
use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::Transaction,
};

pub struct NativeTransactionSigner {
    keypair: Keypair,
}

impl NativeTransactionSigner {
    pub fn new(keypair: Keypair) -> Self {
        Self { keypair }
    }

    /// Create from a 64-byte seed (using generate_from_seed if you have a seed)
    /// For loading from file, use std::fs and Keypair::read_from_file pattern
    pub fn keypair(&self) -> &Keypair {
        &self.keypair
    }
}

#[async_trait::async_trait]
impl TransactionSigner for NativeTransactionSigner {
    fn public_key(&self) -> Pubkey {
        self.keypair.pubkey()
    }

    async fn sign_instructions(&self, instructions: Vec<Instruction>, recent_blockhash: &str) -> Result<Vec<u8>, CoreError> {
        use std::str::FromStr;

        let blockhash = solana_program::hash::Hash::from_str(recent_blockhash)
            .map_err(|e| CoreError::Validation(format!("Invalid blockhash: {}", e)))?;

        let message = solana_sdk::message::Message::new(&instructions, Some(&self.keypair.pubkey()));

        let tx = Transaction::new(&[&self.keypair], message, blockhash);

        Ok(bincode::serialize(&tx)
            .map_err(|e| CoreError::Validation(format!("Failed to serialize transaction: {}", e)))?)
    }

    async fn sign_transaction_bytes(&self, transaction_bytes: &[u8]) -> Result<Vec<u8>, CoreError> {
        // Deserialize, re-sign, and re-serialize
        let mut tx: Transaction = bincode::deserialize(transaction_bytes)
            .map_err(|e| CoreError::Validation(format!("Failed to deserialize transaction: {}", e)))?;

        tx.sign(&[&self.keypair], tx.message.recent_blockhash);

        bincode::serialize(&tx)
            .map_err(|e| CoreError::Validation(format!("Failed to serialize transaction: {}", e)))
    }

    async fn is_ready(&self) -> bool {
        true // Native signer is always ready
    }
}

