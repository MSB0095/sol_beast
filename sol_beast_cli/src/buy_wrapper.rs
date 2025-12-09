// Thin CLI wrapper coordinating between Core services and CLI-specific logic
// Integration point for IDL detection, Helius Sender, dev fees, and transaction building

#![allow(dead_code)] // Functions will be integrated in Phase 6

use crate::buyer;
use crate::models::{PriceCache, BondingCurveState};
use crate::rpc::{fetch_bonding_curve_state, fetch_current_price};
use log::{debug, info};
use sol_beast_core::buy_service::{BuyConfig, BuyService};
use sol_beast_core::native::{self, transaction_signer::NativeTransactionSigner};
use sol_beast_core::settings::Settings;
use solana_client::rpc_client::RpcClient;
use solana_sdk::signature::Keypair;
use std::sync::Arc;
use tokio::sync::Mutex;

/// CLI buy wrapper - coordinates between Core validation and CLI-specific transaction building
/// This is Phase 6a: migration point from monolithic buyer.rs to Core-coordinated flow
pub async fn execute_buy_token(
    mint: &str,
    sol_amount: f64,
    is_real: bool,
    keypair: Option<&Keypair>,
    simulate_keypair: Option<&Keypair>,
    price_cache: Arc<Mutex<PriceCache>>,
    rpc_client: &Arc<RpcClient>,
    settings: &Arc<Settings>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Shared: fetch current price and optional bonding curve state for heuristics
    let price_sol = fetch_current_price(mint, &price_cache, rpc_client, settings).await?;
    let bc_state: Option<BondingCurveState> = fetch_bonding_curve_state(mint, rpc_client, settings).await.ok();

    if is_real {
        let payer = keypair.ok_or("Keypair required for real buy")?;
        let owned_kp = solana_sdk::signature::Keypair::try_from(&payer.to_bytes()[..])
            .map_err(|e| Box::<dyn std::error::Error + Send + Sync>::from(format!("Failed to clone keypair: {}", e)))?;
        let rpc_wrapper = native::NativeRpcClient::from_arc(rpc_client.clone());
        let signer = NativeTransactionSigner::new(owned_kp);

        let cfg = BuyConfig {
            mint: mint.to_string(),
            sol_amount,
            current_price_sol: price_sol,
            bonding_curve_state: bc_state,
        };

        debug!("Executing core buy service for mint {} amount {} SOL", mint, sol_amount);
        let res = BuyService::execute_buy(cfg, &rpc_wrapper, &signer, settings.as_ref())
            .await
            .map_err(|e| Box::<dyn std::error::Error + Send + Sync>::from(e))?;

        info!("Buy sent: tx={} tokens={}", res.transaction_signature, res.token_amount);
        Ok(res.transaction_signature)
    } else {
        // Dry-run: keep legacy simulation path
        let holding = buyer::buy_token(
            mint,
            sol_amount,
            is_real,
            keypair,
            simulate_keypair,
            price_cache,
            rpc_client,
            settings,
        )
        .await?;

        info!("Dry-run buy completed: {} tokens at {:.18} SOL/token", holding.amount, holding.buy_price);
        Ok(format!("simulate_{}_{}", mint, holding.amount))
    }
}

