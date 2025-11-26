use crate::core::error::CoreError;
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionResult {
    pub signature: String,
    pub success: bool,
    pub error: Option<String>,
}

/// Transaction builder for creating buy/sell transactions
pub struct TransactionBuilder {
    pump_program: Pubkey,
}

impl TransactionBuilder {
    pub fn new(pump_program: String) -> Result<Self, CoreError> {
        let pump_program = Pubkey::from_str(&pump_program)
            .map_err(|e| CoreError::Config(format!("Invalid pump program: {}", e)))?;
        
        Ok(Self { pump_program })
    }

    /// Calculate the bonding curve PDA for a mint
    pub fn get_bonding_curve_pda(&self, mint: &str) -> Result<String, CoreError> {
        let mint_pubkey = Pubkey::from_str(mint)
            .map_err(|e| CoreError::Parse(format!("Invalid mint: {}", e)))?;
        
        let (pda, _bump) = Pubkey::find_program_address(
            &[b"bonding-curve", mint_pubkey.as_ref()],
            &self.pump_program,
        );
        
        Ok(pda.to_string())
    }

    /// Calculate expected token output for a given SOL input
    /// 
    /// Note: This method accepts f64 for convenience but converts to lamports internally.
    /// For production use with large amounts, consider accepting lamports directly as u64
    /// to avoid floating-point precision issues. The current implementation is acceptable
    /// for typical trading amounts (< 1000 SOL) where precision loss is negligible.
    pub fn calculate_token_output(
        &self,
        sol_amount: f64,
        virtual_sol_reserves: u64,
        virtual_token_reserves: u64,
    ) -> u64 {
        // Convert SOL to lamports (1 SOL = 1e9 lamports)
        // Precision loss is acceptable for typical trading amounts
        let sol_lamports = (sol_amount * 1e9) as u64;
        
        // Constant product formula: k = virtual_sol * virtual_token
        // After buy: (virtual_sol + sol_lamports) * (virtual_token - tokens_out) = k
        // tokens_out = virtual_token - (k / (virtual_sol + sol_lamports))
        
        let k = virtual_sol_reserves as u128 * virtual_token_reserves as u128;
        let new_sol_reserves = virtual_sol_reserves as u128 + sol_lamports as u128;
        let new_token_reserves = k / new_sol_reserves;
        let tokens_out = virtual_token_reserves as u128 - new_token_reserves;
        
        tokens_out as u64
    }

    /// Calculate SOL output for selling tokens
    pub fn calculate_sol_output(
        &self,
        token_amount: u64,
        virtual_sol_reserves: u64,
        virtual_token_reserves: u64,
    ) -> u64 {
        // Constant product formula for selling
        let k = virtual_sol_reserves as u128 * virtual_token_reserves as u128;
        let new_token_reserves = virtual_token_reserves as u128 + token_amount as u128;
        let new_sol_reserves = k / new_token_reserves;
        let sol_out = virtual_sol_reserves as u128 - new_sol_reserves;
        
        sol_out as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_token_output() {
        let builder = TransactionBuilder::new(
            "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P".to_string()
        ).unwrap();
        
        // Simple test case
        let virtual_sol = 30_000_000_000u64; // 30 SOL
        let virtual_tokens = 1_000_000_000_000u64; // 1M tokens (6 decimals)
        let buy_amount = 0.1; // 0.1 SOL
        
        let tokens_out = builder.calculate_token_output(buy_amount, virtual_sol, virtual_tokens);
        
        // Should receive approximately (0.1 / 30) * 1M = ~3,333 tokens
        assert!(tokens_out > 3_000_000_000 && tokens_out < 3_400_000_000);
    }

    #[test]
    fn test_bonding_curve_pda() {
        let builder = TransactionBuilder::new(
            "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P".to_string()
        ).unwrap();
        
        let mint = "11111111111111111111111111111111";
        let pda = builder.get_bonding_curve_pda(mint).unwrap();
        
        // Should return a valid base58 pubkey
        assert!(!pda.is_empty());
        assert!(Pubkey::from_str(&pda).is_ok());
    }
}
