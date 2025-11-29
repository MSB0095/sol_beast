use async_trait::async_trait;
use crate::core_mod::error::CoreError;
use solana_program::pubkey::Pubkey;
use solana_sdk::transaction::Transaction;

#[async_trait]
pub trait Signer: Send + Sync {
    /// Return public key for the signer
    fn pubkey(&self) -> Pubkey;

    /// Sign the transaction using the signer's private key(s) and provided recent blockhash
    async fn sign_transaction(&self, tx: &mut Transaction, recent_blockhash: solana_sdk::hash::Hash) -> Result<(), CoreError>;
}

#[cfg(feature = "native")]
pub mod native {
    use super::*;
    use solana_sdk::signature::Keypair;
    use solana_sdk::signature::Signer as _;
    use std::sync::Arc;
    use solana_sdk::hash::Hash;

    pub struct NativeKeypairSigner {
        pub keypair: Arc<Keypair>,
    }

    impl NativeKeypairSigner {
        pub fn new(keypair: Arc<Keypair>) -> Self {
            Self { keypair }
        }
    }

    #[async_trait]
    impl Signer for NativeKeypairSigner {
        fn pubkey(&self) -> Pubkey {
            self.keypair.pubkey()
        }

        async fn sign_transaction(&self, tx: &mut Transaction, recent_blockhash: Hash) -> Result<(), CoreError> {
            // Use the Keypair to sign; the call is synchronous, but we keep the async API
            tx.sign(&[self.keypair.as_ref()], recent_blockhash);
            Ok(())
        }
    }
}

#[cfg(all(test, feature = "native"))]
mod tests {
    use super::native::NativeKeypairSigner;
    use super::Signer;
    use solana_sdk::signature::Keypair;
    use solana_sdk::hash::Hash;
    use solana_sdk::transaction::Transaction;
    use std::sync::Arc;

    #[tokio::test]
    async fn native_keypair_signer_can_sign_tx() {
        let kp = Keypair::new();
        let signer = NativeKeypairSigner::new(Arc::new(kp));
        // build dummy transaction
        let instructions: Vec<solana_program::instruction::Instruction> = Vec::new();
        let payer_pubkey = signer.pubkey();
        let mut tx = Transaction::new_with_payer(&instructions, Some(&payer_pubkey));
        let blockhash = Hash::default();
        signer
            .sign_transaction(&mut tx, blockhash)
            .await
            .expect("sign transaction");
        assert!(tx.signatures.len() > 0);
    }
}

#[cfg(target_arch = "wasm32")]
pub mod wasm {
    use super::*;
    use solana_sdk::hash::Hash;

    pub struct WasmStubSigner {}

    impl WasmStubSigner {
        pub fn new() -> Self { Self {} }
    }

    #[async_trait]
    impl Signer for WasmStubSigner {
        fn pubkey(&self) -> Pubkey {
            // placeholder; WASM signer should be implemented via wallet interop
            Pubkey::default()
        }

        async fn sign_transaction(&self, _tx: &mut Transaction, _recent_blockhash: Hash) -> Result<(), CoreError> {
            Err(CoreError::Internal("WASM signer not implemented".to_string()))
        }
    }
}
