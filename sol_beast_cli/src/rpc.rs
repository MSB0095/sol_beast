use crate::models::{BondingCurveState, OffchainTokenMetadata, PriceCache};
use sol_beast_core::native::NativeRpcClient;
use sol_beast_core::rpc_client as core_rpc;
use sol_beast_core::settings::Settings;

// Transaction parsing and metadata fetching are now centralized in sol_beast_core
use log::debug;
use mpl_token_metadata::accounts::Metadata;
use solana_client::rpc_client::RpcClient;
use solana_program::pubkey::Pubkey;
use std::error::Error;
use std::sync::Arc;
use std::time::Instant;
use std::str::FromStr;
use tokio::sync::Mutex;


/// Fetches transaction details and extracts pump.fun token creation information.
/// 
/// # Returns
/// On success, returns a tuple of:
/// - `creator`: The user/signer who created the token (account index 7 in create instruction)
/// - `mint`: The mint address of the newly created token (account index 0)
/// - `curve_pda`: The bonding curve PDA address (account index 2)
/// - `holder_addr`: The associated token account owned by the bonding curve PDA
/// - `is_creation`: Whether this transaction contains a pump.fun create instruction
/// 
/// # Detection Logic
/// Uses the pump.fun `create` instruction discriminator [24, 30, 200, 40, 5, 28, 7, 119]
/// to reliably identify token creation transactions. Checks both main instructions and
/// inner instructions (for CPI cases).
/// 
/// This is now a thin wrapper around the centralized sol_beast_core::transaction_service
pub async fn fetch_transaction_details(
    signature: &str,
    rpc_client: &Arc<RpcClient>,
    settings: &Arc<Settings>,
) -> Result<(String, String, String, String, bool), Box<dyn std::error::Error + Send + Sync>> {
    use sol_beast_core::native::NativeRpcClient;
    use sol_beast_core::transaction_service::fetch_and_parse_transaction;
    
    // Create wrapper around the Arc<RpcClient>
    let rpc_wrapper = NativeRpcClient::from_arc(rpc_client.clone());
    
    // Use centralized function with retry logic
    match fetch_and_parse_transaction(signature, &rpc_wrapper, &settings.pump_fun_program, 4).await {
        Ok(parsed) => {
            // Convert ParsedTransaction to the tuple format expected by callers
            Ok((
                parsed.creator,
                parsed.mint,
                parsed.bonding_curve,
                parsed.holder_address,
                parsed.is_creation,
            ))
        }
        Err(e) => {
            // Convert CoreError to Box<dyn Error>
            Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        }
    }
}



/// Fetch token metadata (both on-chain and off-chain)
/// 
/// This is now a thin wrapper around the centralized sol_beast_core::transaction_service
pub async fn fetch_token_metadata(
    mint: &str,
    rpc_client: &Arc<RpcClient>,
    settings: &Arc<Settings>,
) -> Result<(Option<Metadata>, Option<OffchainTokenMetadata>, Option<Vec<u8>>), Box<dyn std::error::Error + Send + Sync>> {
    use sol_beast_core::native::{NativeHttpClient, NativeRpcClient};
    use sol_beast_core::transaction_service::fetch_complete_token_metadata;

    let rpc_wrapper = NativeRpcClient::from_arc(rpc_client.clone());
    let http_wrapper = NativeHttpClient::default();

    let meta = fetch_complete_token_metadata(
        mint,
        &settings.metadata_program,
        &rpc_wrapper,
        &http_wrapper,
    )
    .await
    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    Ok((meta.onchain, meta.offchain, meta.raw_account_data))
}

/// Fetch current price (SOL per token) using the bonding curve account and cache the result.
pub async fn fetch_current_price(
    mint: &str,
    price_cache: &Arc<Mutex<PriceCache>>,
    rpc_client: &Arc<RpcClient>,
    settings: &Arc<Settings>,
) -> Result<f64, Box<dyn Error + Send + Sync>> {
    debug!("fetch_current_price: {}", mint);
    // Cache lookup with TTL
    {
        let cache = price_cache.lock().await;
        if let Some((ts, price)) = cache.peek(mint) {
            if Instant::now().duration_since(*ts).as_secs() < settings.price_cache_ttl_secs {
                return Ok(*price);
            }
        }
    }

    let state = fetch_bonding_curve_state(mint, rpc_client, settings).await?;
    let price = core_rpc::calculate_price_from_bonding_curve(&state);
    if price <= 0.0 {
        return Err("Calculated price is zero".into());
    }

    let mut cache = price_cache.lock().await;
    cache.put(mint.to_string(), (Instant::now(), price));
    Ok(price)
}

/// Thin wrapper around core bonding-curve fetcher.
pub async fn fetch_bonding_curve_state(
    mint: &str,
    rpc_client: &Arc<RpcClient>,
    settings: &Arc<Settings>,
) -> Result<BondingCurveState, Box<dyn Error + Send + Sync>> {
    let mint_pk = Pubkey::from_str(mint)?;
    let pump_program = Pubkey::from_str(&settings.pump_fun_program)?;
    let (bonding_curve, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_program);

    let rpc_wrapper = NativeRpcClient::from_arc(rpc_client.clone());
    core_rpc::fetch_bonding_curve_state(mint, &bonding_curve.to_string(), &rpc_wrapper)
        .await
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
}

/// Fetch bonding curve creator (if present) using core helper.
pub async fn fetch_bonding_curve_creator(
    mint: &str,
    rpc_client: &Arc<RpcClient>,
    settings: &Arc<Settings>,
) -> Result<Option<Pubkey>, Box<dyn Error + Send + Sync>> {
    let rpc_wrapper = NativeRpcClient::from_arc(rpc_client.clone());
    core_rpc::fetch_bonding_curve_creator(mint, &settings.pump_fun_program, &rpc_wrapper)
        .await
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
}



