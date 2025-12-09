use crate::error::CoreError;
use crate::rpc_client::{RpcClient, fetch_bonding_curve_state, calculate_price_from_bonding_curve, calculate_liquidity_sol};
use crate::metadata::HttpClient;
use crate::settings::Settings;
use crate::transaction_service::{fetch_and_parse_transaction, fetch_complete_token_metadata};
use crate::buyer::{evaluate_buy_heuristics};
use crate::models::BondingCurveState;
use log::{info, warn};

#[derive(Debug, Clone)]
pub struct DetectedTokenResult {
    pub signature: String,
    pub mint: String,
    pub creator: String,
    pub bonding_curve: String,
    pub holder_address: String,
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub image_uri: Option<String>,
    pub description: Option<String>,
    pub should_buy: bool,
    pub evaluation_reason: String,
    pub token_amount: u64,
    pub buy_price_sol: f64,
    pub liquidity_sol: Option<f64>,
    pub bonding_curve_state: Option<BondingCurveState>,
}

/// Process a newly detected token signature through the full pipeline:
/// 1. Fetch and parse transaction to get mint/creator
/// 2. Fetch token metadata (on-chain and off-chain)
/// 3. Fetch bonding curve state for price and liquidity
/// 4. Evaluate buy heuristics
pub async fn process_new_token<R: RpcClient + ?Sized, H: HttpClient + ?Sized>(
    signature: String,
    rpc_client: &R,
    http_client: &H,
    settings: &Settings,
) -> Result<DetectedTokenResult, CoreError> {
    info!("Processing detected signature: {}", signature);

    // Step 1: Fetch and parse transaction
    // We use a hardcoded retry count here, or we could add it to settings
    let max_retries = 3;
    let parsed_tx = fetch_and_parse_transaction(
        &signature,
        rpc_client,
        &settings.pump_fun_program,
        max_retries,
    ).await?;

    info!("Successfully parsed transaction: mint={}, creator={}", parsed_tx.mint, parsed_tx.creator);

    // Step 2: Fetch token metadata
    // We don't fail the whole pipeline if metadata fails, just log warning
    let metadata = match fetch_complete_token_metadata(
        &parsed_tx.mint,
        &settings.metadata_program,
        rpc_client,
        http_client,
    ).await {
        Ok(meta) => {
            info!("Successfully fetched metadata for {}", parsed_tx.mint);
            meta
        },
        Err(e) => {
            warn!("Failed to fetch metadata for {}: {:?}", parsed_tx.mint, e);
            // Return empty metadata
            crate::metadata::TokenMetadata {
                onchain: None,
                offchain: None,
                raw_account_data: None,
            }
        }
    };

    // Step 3: Fetch bonding curve state
    let (bonding_curve_state, estimated_price, liquidity_sol) = match fetch_bonding_curve_state(
        &parsed_tx.mint,
        &parsed_tx.bonding_curve,
        rpc_client,
    ).await {
        Ok(state) => {
            let price = calculate_price_from_bonding_curve(&state);
            let liquidity = calculate_liquidity_sol(&state);
            info!("Fetched bonding curve state: price={:.8} SOL, liquidity={:.4} SOL", price, liquidity);
            (Some(state), price, Some(liquidity))
        },
        Err(e) => {
            warn!("Failed to fetch bonding curve state for {}: {:?}, using fallback price", parsed_tx.mint, e);
            // Fallback price (0.00001 SOL)
            (None, 0.00001, None)
        }
    };

    // Step 4: Evaluate buy heuristics
    let evaluation = evaluate_buy_heuristics(
        &parsed_tx.mint,
        settings.buy_amount,
        estimated_price,
        bonding_curve_state.as_ref(),
        settings,
    );

    info!("Buy evaluation for {}: should_buy={}, reason={}", 
          parsed_tx.mint, evaluation.should_buy, evaluation.reason);

    // Extract metadata fields
    let name = metadata.offchain.as_ref().and_then(|m| m.name.clone())
        .or_else(|| metadata.onchain.as_ref().map(|m| m.name.clone()));
    let symbol = metadata.offchain.as_ref().and_then(|m| m.symbol.clone())
        .or_else(|| metadata.onchain.as_ref().map(|m| m.symbol.clone()));
    let image_uri = metadata.offchain.as_ref().and_then(|m| m.image.clone());
    let description = metadata.offchain.as_ref().and_then(|m| m.description.clone());

    Ok(DetectedTokenResult {
        signature,
        mint: parsed_tx.mint,
        creator: parsed_tx.creator,
        bonding_curve: parsed_tx.bonding_curve,
        holder_address: parsed_tx.holder_address,
        name,
        symbol,
        image_uri,
        description,
        should_buy: evaluation.should_buy,
        evaluation_reason: evaluation.reason,
        token_amount: evaluation.token_amount,
        buy_price_sol: evaluation.buy_price_sol,
        liquidity_sol,
        bonding_curve_state,
    })
}
