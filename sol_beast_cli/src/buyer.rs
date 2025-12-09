use crate::{
    models::{Holding, PriceCache},
    rpc::{fetch_current_price, fetch_bonding_curve_state},
};
use sol_beast_core::settings::Settings;
use sol_beast_core::buy_service::{BuyService, BuyConfig};
use sol_beast_core::native::{NativeRpcClient, transaction_signer::NativeTransactionSigner};
use solana_client::rpc_client::RpcClient;
use std::sync::Arc;
use tokio::sync::Mutex;
use solana_sdk::signature::Keypair;
use log::info;
use chrono::Utc;

#[allow(dead_code)]
pub async fn buy_token(
    mint: &str,
    sol_amount: f64,
    is_real: bool,
    keypair: Option<&Keypair>,
    _simulate_keypair: Option<&Keypair>,
    price_cache: Arc<Mutex<PriceCache>>,
    rpc_client: &Arc<RpcClient>,
    settings: &Arc<Settings>,
) -> Result<Holding, Box<dyn std::error::Error + Send + Sync>> {
    // fetch_current_price now returns SOL per token
    let buy_price_sol = fetch_current_price(mint, &price_cache, rpc_client, settings).await?;
    
    // Fetch bonding curve state if needed for heuristics
    let bonding_curve_state = if settings.enable_safer_sniping {
        fetch_bonding_curve_state(mint, rpc_client, settings).await.ok()
    } else {
        None
    };

    if is_real {
        let payer = keypair.ok_or("Keypair required")?;
        let owned_kp = Keypair::try_from(&payer.to_bytes()[..]).map_err(|e| format!("Keypair error: {}", e))?;
        let signer = NativeTransactionSigner::new(owned_kp);
        let rpc_wrapper = NativeRpcClient::from_arc(rpc_client.clone());

        let config = BuyConfig {
            mint: mint.to_string(),
            sol_amount,
            current_price_sol: buy_price_sol,
            bonding_curve_state,
        };

        let result = BuyService::execute_buy(config, &rpc_wrapper, &signer, settings).await?;

        Ok(Holding {
            mint: mint.to_string(),
            amount: result.token_amount,
            buy_price: buy_price_sol,
            buy_time: chrono::DateTime::from_timestamp(result.timestamp, 0).unwrap_or(Utc::now()),
            creator: None,
            metadata: None,
            onchain_raw: None,
            onchain: None,
        })
    } else {
        info!("Simulating buy for {} (Dry Run)", mint);
        let token_amount = ((sol_amount / buy_price_sol) * 1_000_000.0) as u64;
        
        Ok(Holding {
            mint: mint.to_string(),
            amount: token_amount,
            buy_price: buy_price_sol,
            buy_time: Utc::now(),
            creator: None,
            metadata: None,
            onchain_raw: None,
            onchain: None,
        })
    }
}