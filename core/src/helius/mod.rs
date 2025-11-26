//! Helius Sender helper functions.
#![allow(dead_code)]

use super::config::settings::Settings;
use std::error::Error;
use std::sync::Arc;
use super::blockchain::signer::Signer as CoreSigner;
use super::blockchain::rpc_client::RpcClient as CoreRpcClient;
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use log::debug;

// TIP accounts and constants
pub const TIP_ACCOUNTS: &[&str] = &[
    "4ACfpUFoaSD9bfPdeu6DBt89gB6ENTeHBXCAi87NhDEE",
    "D2L6yPZ2FmmmTKPgzaMKdhu6EWZcTpLy1Vhx8uvZe7NZ",
    "9bnz4RShgq1hAnLnZbP8kbgBg1kEmcJBYQq3gQbmnSta",
];

pub async fn get_dynamic_tip_amount(settings: &Arc<Settings>) -> Result<f64, Box<dyn Error + Send + Sync>> {
    if !settings.helius_use_dynamic_tips {
        let static_tip = settings.get_effective_min_tip_sol();
        debug!("Dynamic tips disabled, using static tip: {:.9} SOL", static_tip);
        return Ok(static_tip);
    }
    if settings.helius_use_swqos_only {
        let min_tip = settings.get_effective_min_tip_sol();
        debug!("SWQOS-only mode: using minimum tip {:.9} SOL", min_tip);
        return Ok(min_tip);
    }
    Ok(settings.get_effective_min_tip_sol())
}

pub async fn get_priority_fee_estimate(
    _rpc_url: &str,
    _transaction_base64: &str,
    settings: &Arc<Settings>,
) -> Result<u64, Box<dyn Error + Send + Sync>> {
    // For now, a simple fallback: use the configured multiplier and default fee
    let fallback_fee = 50_000u64;
    debug!("Using fallback priority fee: {} (multiplier {})", fallback_fee, settings.helius_priority_fee_multiplier);
    Ok(fallback_fee)
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn send_transaction_via_helius(
    _instructions: Vec<Instruction>,
    _payer: std::sync::Arc<dyn CoreSigner>,
    settings: &Arc<Settings>,
    _rpc_client: &Arc<dyn CoreRpcClient>,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    // naive native implementation: call out to Helius sender endpoint if enabled
    if !settings.helius_sender_enabled {
        return Err("Helius Sender is not enabled in settings".into());
    }
    // TODO: Implement a real sender; for now return an error indicating not implemented.
    Err("send_transaction_via_helius native implementation not yet implemented".into())
}

// wasm stub implementations
#[cfg(target_arch = "wasm32")]
pub async fn send_transaction_via_helius(
    _instructions: Vec<Instruction>,
    _payer: std::sync::Arc<dyn CoreSigner>,
    _settings: &Arc<Settings>,
    _rpc_client: &Arc<dyn CoreRpcClient>,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    Err("Helius sender not implemented for wasm yet".into())
}

pub async fn send_transaction_with_retry(
    instructions: Vec<Instruction>,
    payer: std::sync::Arc<dyn CoreSigner>,
    settings: &Arc<Settings>,
    rpc_client: &Arc<dyn CoreRpcClient>,
    max_retries: usize,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let mut last_error: Option<Box<dyn Error + Send + Sync>> = None;
    for attempt in 0..max_retries {
        match send_transaction_via_helius(instructions.clone(), payer.clone(), settings, rpc_client).await {
            Ok(sig) => return Ok(sig),
            Err(e) => {
                last_error = Some(e);
                if attempt < max_retries - 1 {
                    let delay_ms = 1000 * (2u64.pow(attempt as u32));
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                }
            }
        }
    }
    Err(last_error.unwrap_or_else(|| "unknown error".into()))
}

pub fn get_random_tip_account() -> Result<Pubkey, Box<dyn Error + Send + Sync>> {
    use rand::seq::SliceRandom;
    let account_str = TIP_ACCOUNTS
        .choose(&mut rand::thread_rng())
        .ok_or("No tip accounts available")?;
    Ok(std::str::FromStr::from_str(account_str)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::config::settings::Settings as CoreSettings;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_get_dynamic_tip_amount() {
        let s = CoreSettings::from_file("../config.example.toml").unwrap();
        let s = Arc::new(s);
        let tip = get_dynamic_tip_amount(&s).await.unwrap();
        assert!(tip >= 0.0);
    }
}
