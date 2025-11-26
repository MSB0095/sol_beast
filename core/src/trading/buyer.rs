//! Buyer module
//!
//! This file contains the canonical buying logic for the Sol Beast trading bot. The project
//! previously had a second copy at `buyer_new.rs`. That file has been consolidated into
//! this module to avoid duplication and to centralize buy instruction building and
//! simulation/real flows.
//!
//! NOTE: Keep this module platform-agnostic by using `RpcClient` and `Signer` trait
//! abstractions; CLI and WASM should be thin wrappers that instantiate the appropriate
//! concrete implementations.
use serde_json::{json, Value};
use crate::{
    core::models::{Holding, PriceCache},
    rpc::{fetch_current_price, fetch_bonding_curve_state, fetch_global_fee_recipient, detect_idl_for_mint, fetch_bonding_curve_creator, build_missing_ata_preinstructions, fetch_with_fallback},
    idl::load_all_idls,
};
use crate::{config::settings::Settings, blockchain::tx_builder::{BUY_DISCRIMINATOR, build_buy_instruction, BuyArgs}};
use crate::rpc_client::RpcClient as CoreRpcClient;
use std::{sync::Arc, collections::HashMap};
use tokio::sync::Mutex;
use solana_sdk::{
    signature::Keypair,
    transaction::Transaction, pubkey::Pubkey,
};
use crate::Signer as CoreSigner;
use crate::signer::native::NativeKeypairSigner;
use log::{info, warn, debug};
use std::str::FromStr;
use spl_associated_token_account::instruction::create_associated_token_account;
use spl_associated_token_account::get_associated_token_address;
use chrono::Utc;

fn calculate_token_amount(sol_amount: f64, buy_price_sol: f64) -> u64 {
    ((sol_amount / buy_price_sol) * 1_000_000.0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockchain::rpc_client::RpcClient;
    use std::sync::Arc;
    use std::time::Instant;
    use lru::LruCache;
    use std::num::NonZeroUsize;
    use solana_sdk::hash::Hash;
    use serde_json::json;
    use async_trait::async_trait;

    struct MockRpcClient;
    #[async_trait]
    impl RpcClient for MockRpcClient {
        async fn get_account_info(&self, _pubkey: &str) -> Result<Option<Vec<u8>>, crate::core::error::CoreError> { Ok(None) }
        async fn get_balance(&self, _pubkey: &str) -> Result<u64, crate::core::error::CoreError> { Ok(1_000_000_000) }
        async fn send_transaction(&self, _transaction: &[u8]) -> Result<String, crate::core::error::CoreError> { Ok("SIG".to_string()) }
        async fn confirm_transaction(&self, _signature: &str) -> Result<bool, crate::core::error::CoreError> { Ok(true) }
        async fn get_latest_blockhash(&self) -> Result<Hash, crate::core::error::CoreError> { Ok(Hash::default()) }
        async fn simulate_transaction_with_config(&self, _tx: &solana_sdk::transaction::Transaction, _config: serde_json::Value) -> Result<serde_json::Value, crate::core::error::CoreError> { Ok(json!({"value": {"err": null, "units_consumed": 100}})) }
        async fn send_and_confirm_transaction(&self, _tx: &solana_sdk::transaction::Transaction) -> Result<String, crate::core::error::CoreError> { Ok("SIG".to_string()) }
    }

    // MockSigner not needed for dry-run test

    #[test]
    fn it_calculates_token_amount() {
        let res = calculate_token_amount(1.0, 0.5);
        assert_eq!(res, 2_000_000u64);
    }

    #[test]
    fn it_calculates_max_sol_cost_with_slippage() {
        let res = calculate_max_sol_cost_with_slippage(1.0, 100);
        // 1 SOL = 1_000_000_000 lamports * 1.01 = 1_010_000_000
        assert_eq!(res, 1_010_000_000u64);
    }

    #[tokio::test]
    async fn dry_run_buy_builds_and_simulates() {
        let rpc_client: Arc<dyn RpcClient> = Arc::new(MockRpcClient{});
        let mut cache = LruCache::new(NonZeroUsize::new(16).unwrap());
        let mint_pub = solana_program::pubkey::Pubkey::new_unique();
        let mint = mint_pub.to_string();
        cache.put(mint.clone(), (Instant::now(), 0.0001f64));
        let price_cache = Arc::new(Mutex::new(cache));

        let pump_program = solana_program::pubkey::Pubkey::new_unique().to_string();
        let metadata_program = solana_program::pubkey::Pubkey::new_unique().to_string();
        let settings_toml = format!(r#"
    solana_ws_urls = []
    solana_rpc_urls = ["http://localhost:8899"]
    pump_fun_program = "{}"
    metadata_program = "{}"
    tp_percent = 30.0
    sl_percent = -20.0
    timeout_secs = 3600
    cache_capacity = 16
    price_cache_ttl_secs = 60
    buy_amount = 0.1
    "#, pump_program, metadata_program);
        let settings = Arc::new(Settings::from_toml_str(&settings_toml).unwrap());
        // mint is created above as a unique Pubkey string

        let price_val = fetch_current_price(mint.as_str(), &price_cache, &rpc_client, &settings).await.unwrap();
        assert!((price_val - 0.0001f64).abs() < f64::EPSILON);
        // Install a test RpcProvider that returns predictable JSON for RPC fetches used in helpers
        struct MockProvider;
        #[async_trait]
        impl crate::rpc::RpcProvider for MockProvider {
            async fn send_json(&self, request: serde_json::Value) -> Result<serde_json::Value, crate::core::error::CoreError> {
                let method = request.get("method").and_then(|v| v.as_str()).unwrap_or("");
                match method {
                    "getAccountInfo" => {
                        // Return a minimal Global PDA structure with valid discriminator and fee_recipient
                        let mut data = vec![0u8; 73];
                        let global_discriminator: [u8; 8] = [0xa7, 0xe8, 0xe8, 0xb1, 0xc8, 0x6c, 0x72, 0x7f];
                        data[0..8].copy_from_slice(&global_discriminator);
                        let fee_recipient_pk = solana_program::pubkey::Pubkey::new_unique();
                        data[8+33..8+33+32].copy_from_slice(fee_recipient_pk.as_ref());
                        let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &data);
                        Ok(json!({"jsonrpc":"2.0","id":1,"result": {"value": {"data": [encoded, "base64"]}}}))
                    }
                    _ => Ok(json!({"jsonrpc":"2.0","id":1,"result": null})),
                }
            }
        }
        crate::rpc::set_global_json_rpc_provider(Some(Arc::new(MockProvider))).await;

        let result = buy_token(
            mint.as_str(),
            0.01,
            false,
            None,
            None,
            price_cache.clone(),
            &rpc_client,
            &settings,
        ).await;
        if let Err(e) = &result { panic!("buy_token failed: {}", e); }
        let holding = result.unwrap();
        assert_eq!(holding.mint, mint.to_string());
        assert!(holding.amount > 0);
    }
}

async fn build_context_for_buy(
    mint: &str,
    payer_pubkey: &Pubkey,
    rpc_client: &Arc<dyn CoreRpcClient>,
    settings: &Arc<Settings>,
) -> Result<(HashMap<String, Pubkey>, Option<Pubkey>), Box<dyn std::error::Error + Send + Sync>> {
    let mint_pk = Pubkey::from_str(mint)?;
    let creator_opt = fetch_bonding_curve_creator(mint, rpc_client, settings).await.ok().flatten();
    let mut context: HashMap<String, Pubkey> = HashMap::new();
    context.insert("mint".to_string(), mint_pk);
    context.insert("user".to_string(), *payer_pubkey);
    if let Some(c) = creator_opt {
        context.insert("bonding_curve.creator".to_string(), c);
    }
    let pump_program_pk = Pubkey::from_str(&settings.pump_fun_program)?;
    let (curve_pda_fallback, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_program_pk);
    context.insert("bonding_curve".to_string(), curve_pda_fallback);
    if let Some(creator) = context.get("bonding_curve.creator") {
        let (creator_vault, _) = Pubkey::find_program_address(&[b"creator-vault", creator.as_ref()], &pump_program_pk);
        context.insert("creator_vault".to_string(), creator_vault);
    }
    Ok((context, creator_opt))
}

fn calculate_max_sol_cost_with_slippage(sol_amount: f64, slippage_bps: u64) -> u64 {
    let base_cost_lamports = (sol_amount * 1_000_000_000.0) as u64;
    let slippage_multiplier = 1.0 + (slippage_bps as f64 / 10000.0);
    (base_cost_lamports as f64 * slippage_multiplier) as u64
}

fn build_data_for_buy(token_amount: u64, max_sol_cost_with_slippage: u64) -> Vec<u8> {
    let mut d = BUY_DISCRIMINATOR.to_vec();
    d.extend(borsh::to_vec(&BuyArgs { amount: token_amount, max_sol_cost: max_sol_cost_with_slippage, track_volume: Some(false) }).unwrap());
    d
}

pub async fn buy_token(
    mint: &str,
    sol_amount: f64,
    is_real: bool,
    keypair: Option<Arc<dyn CoreSigner>>,
    simulate_keypair: Option<Arc<dyn CoreSigner>>,
    price_cache: Arc<Mutex<PriceCache>>,
    rpc_client: &Arc<dyn CoreRpcClient>,
    settings: &Arc<Settings>,
) -> Result<Holding, Box<dyn std::error::Error + Send + Sync>> {
    let buy_price_sol = fetch_current_price(mint, &price_cache, rpc_client, settings).await?;
    let token_amount = calculate_token_amount(sol_amount, buy_price_sol);

    if settings.enable_safer_sniping {
        if token_amount < settings.min_tokens_threshold {
            return Err(format!(
                "Token amount {} is below minimum threshold {} (price too high: {:.18} SOL/token)",
                token_amount, settings.min_tokens_threshold, buy_price_sol
            ).into());
        }
        if buy_price_sol > settings.max_sol_per_token {
            return Err(format!(
                "Token price {:.18} SOL/token exceeds maximum {:.18} SOL/token (already too expensive)",
                buy_price_sol, settings.max_sol_per_token
            ).into());
        }

        if let Ok(state) = fetch_bonding_curve_state(mint, rpc_client, settings).await {
            let real_sol = state.real_sol_reserves as f64 / 1_000_000_000.0;
            if real_sol < settings.min_liquidity_sol {
                return Err(format!(
                    "Liquidity {:.4} SOL is below minimum {:.4} SOL (too risky)",
                    real_sol, settings.min_liquidity_sol
                ).into());
            }
            if real_sol > settings.max_liquidity_sol {
                return Err(format!(
                    "Liquidity {:.4} SOL exceeds maximum {:.4} SOL (too late)",
                    real_sol, settings.max_liquidity_sol
                ).into());
            }
        }
    }

    info!(
        "Buy {}: {} tokens for {} SOL (price: {:.18} SOL/token)",
        mint,
        token_amount,
        sol_amount,
        buy_price_sol
    );

    let fee_recipient = fetch_global_fee_recipient(rpc_client, settings).await?;

    if is_real {
        let payer = keypair.as_ref().ok_or("Signer required")?;
        debug!("Preparing buy TX for mint {} amount {} SOL (real)", mint, sol_amount);

        let detected_idl_opt = detect_idl_for_mint(mint, rpc_client, settings).await;
        let mut built_instr: Option<solana_program::instruction::Instruction> = None;
        let mut last_err: Option<String> = None;
        let mint_pk = Pubkey::from_str(mint)?;
        let payer_pubkey = payer.pubkey();
        let (mut context, creator_opt) = build_context_for_buy(mint, &payer_pubkey, rpc_client, settings).await?;
        context.insert("fee_recipient".to_string(), fee_recipient);

        debug!("Context for buy instruction building:");
        for (k, v) in context.iter() { debug!("  {}: {}", k, v); }

        let try_idls: Vec<crate::idl::SimpleIdl> = if let Some(idl) = detected_idl_opt { vec![idl] } else { load_all_idls()?.into_values().collect() };
        for idl in try_idls {
            debug!("Trying IDL with program_id: {}", idl.address);
            match idl.build_accounts_for("buy", &context) {
                Ok(metas) => {
                    warn!("IDL {} build_accounts_for(buy) succeeded with {} accounts - USING THIS IDL", idl.address, metas.len());
                    for (i, meta) in metas.iter().enumerate() { debug!("  [{}] {} (signer={}, writable={})", i, meta.pubkey, meta.is_signer, meta.is_writable); }
                    let max_sol_cost_with_slippage = calculate_max_sol_cost_with_slippage(sol_amount, settings.slippage_bps);
                    let d = build_data_for_buy(token_amount, max_sol_cost_with_slippage);
                    built_instr = Some(solana_program::instruction::Instruction { program_id: idl.address, accounts: metas, data: d });
                    break;
                }
                Err(e) => last_err = Some(e.to_string()),
            }
        }

        let instruction = if let Some(instr) = built_instr { instr } else {
            if let Some(e) = last_err { debug!("IDL buy build errors: {}", e); }
            let program_id = Pubkey::from_str(&settings.pump_fun_program)?;
            let max_sol_cost_with_slippage = calculate_max_sol_cost_with_slippage(sol_amount, settings.slippage_bps);
            build_buy_instruction(&program_id, mint, token_amount, max_sol_cost_with_slippage, Some(false), &payer_pubkey, &fee_recipient, creator_opt)?
        };

        let _ata = get_associated_token_address(&payer_pubkey, &mint_pk);
        let _pre_instructions: Vec<solana_program::instruction::Instruction> = vec![create_associated_token_account(&payer_pubkey, &payer_pubkey, &mint_pk, &spl_token::id())];
        let mut real_context: HashMap<String, Pubkey> = HashMap::new();
        real_context.insert("mint".to_string(), mint_pk);
        real_context.insert("user".to_string(), payer_pubkey);
        real_context.insert("payer".to_string(), payer_pubkey);
        if let Some(c) = creator_opt { real_context.insert("bonding_curve.creator".to_string(), c); }
        if let Some(bc) = context.get("bonding_curve") { real_context.insert("bonding_curve".to_string(), *bc); }
        if let Some(cv) = context.get("creator_vault") { real_context.insert("creator_vault".to_string(), *cv); }
        let ata_pre = build_missing_ata_preinstructions(&real_context).await?;
        let mut all_instrs: Vec<solana_program::instruction::Instruction> = Vec::new();
        for pi in ata_pre.into_iter() { all_instrs.push(pi); }
        all_instrs.push(instruction);

        if settings.helius_sender_enabled {
            info!("Using Helius Sender for buy transaction of mint {}", mint);
            let signature = crate::helius::send_transaction_with_retry(all_instrs, payer.clone(), settings, rpc_client, 3).await?;
            info!("Buy transaction sent via Helius Sender: {}", signature);
        } else {
            let mut tx = Transaction::new_with_payer(&all_instrs, Some(&payer.pubkey()));
            let blockhash = rpc_client.get_latest_blockhash().await?;
            payer.sign_transaction(&mut tx, blockhash).await?;
            rpc_client.send_and_confirm_transaction(&tx).await?;
        }
    } else {
        // we'll use rpc_client trait methods directly
        let sim_payer_ref: Arc<dyn CoreSigner> = if let Some(k) = simulate_keypair { k } else { Arc::new(NativeKeypairSigner::new(Arc::new(Keypair::new()))) };
        debug!("Preparing simulated buy TX for mint {} amount {} SOL (dry run)", mint, sol_amount);
        let program_id = Pubkey::from_str(&settings.pump_fun_program)?;
        let creator_opt = fetch_bonding_curve_creator(mint, rpc_client, settings).await.ok().flatten();
        let sim_payer_pubkey = sim_payer_ref.pubkey();

        let idls = load_all_idls()?;
        let mut instruction_opt: Option<solana_program::instruction::Instruction> = None;
        let mut last_err: Option<String> = None;
        let mint_pk = Pubkey::from_str(mint)?;
        if let Some(idl) = idls.get("pumpfun") {
            let mut context: HashMap<String, Pubkey> = HashMap::new();
            context.insert("mint".to_string(), mint_pk);
            context.insert("user".to_string(), sim_payer_pubkey);
            if let Some(c) = creator_opt { context.insert("bonding_curve.creator".to_string(), c); }
            let pump_program_pk = Pubkey::from_str(&settings.pump_fun_program)?;
            let (curve_pda, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_program_pk);
            context.insert("bonding_curve".to_string(), curve_pda);
            if let Some(creator) = context.get("bonding_curve.creator").cloned() { let (creator_vault, _) = Pubkey::find_program_address(&[b"creator-vault", creator.as_ref()], &pump_program_pk); context.insert("creator_vault".to_string(), creator_vault); }
            let fee_recipient = Pubkey::from_str("39azUYFWPz3VHgKCf3VChUwbpURdCHRxjWVowf5jUJjg")?;
            context.insert("fee_recipient".to_string(), fee_recipient);
            match idl.build_accounts_for("buy", &context) { Ok(metas) => { debug!("IDL build_accounts_for(buy) succeeded with {} accounts (dry-run)", metas.len()); for (i, meta) in metas.iter().enumerate() { debug!("  [{}] {} (signer={}, writable={})", i, meta.pubkey, meta.is_signer, meta.is_writable); } let max_sol_cost_with_slippage = calculate_max_sol_cost_with_slippage(sol_amount, settings.slippage_bps); instruction_opt = Some(solana_program::instruction::Instruction { program_id, accounts: metas, data: build_data_for_buy(token_amount, max_sol_cost_with_slippage) }); }, Err(e) => last_err = Some(e.to_string()), }
        }
        let instruction = if let Some(instr) = instruction_opt { instr } else { if let Some(e) = last_err { debug!("IDL build failed for buy: {}", e); } let max_sol_cost_with_slippage = calculate_max_sol_cost_with_slippage(sol_amount, settings.slippage_bps); build_buy_instruction(&program_id, mint, token_amount, max_sol_cost_with_slippage, Some(false), &sim_payer_pubkey, &fee_recipient, creator_opt)? };
        let mut tx_instructions = Vec::new(); let mut pre_instructions: Vec<solana_program::instruction::Instruction> = Vec::new(); let ata = get_associated_token_address(&sim_payer_pubkey, &mint_pk); match fetch_with_fallback::<Value>(json!({ "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo", "params": [ ata.to_string(), { "encoding": "base64", "commitment": "confirmed" } ] }), "getAccountInfo", rpc_client, settings).await { Ok(info) => { if info.result.is_none() { pre_instructions.push(create_associated_token_account(&sim_payer_pubkey, &sim_payer_pubkey, &mint_pk, &spl_token::id())); } else if let Some(result_val) = info.result { let val = if let Some(v) = result_val.get("value") { v.clone() } else { result_val.clone() }; if val.is_null() { pre_instructions.push(create_associated_token_account(&sim_payer_pubkey, &sim_payer_pubkey, &mint_pk, &spl_token::id())); } } }, Err(e) => debug!("Failed to check ATA existence for {}: {}", ata, e), }
        for pi in pre_instructions.into_iter() { tx_instructions.push(pi); } tx_instructions.push(instruction.clone()); debug!("DRY RUN buy simulation for {}: program_id={}", mint, instruction.program_id); for (i, acc) in instruction.accounts.iter().enumerate() { debug!("    [{}] {} (signer={}, writable={})", i, acc.pubkey, acc.is_signer, acc.is_writable); }
        let mut tx = Transaction::new_with_payer(&tx_instructions, Some(&sim_payer_pubkey));
        match rpc_client.get_latest_blockhash().await {
            Ok(blockhash) => {
                tx.message.recent_blockhash = blockhash;
                let config_json = json!({
                    "sig_verify": false,
                    "replace_recent_blockhash": true,
                    "commitment": "confirmed",
                    "encoding": serde_json::Value::Null,
                    "accounts": serde_json::Value::Null,
                    "min_context_slot": serde_json::Value::Null,
                    "inner_instructions": false
                });
                match rpc_client.simulate_transaction_with_config(&tx, config_json).await {
                    Ok(simulation) => {
                        if let Some(value) = simulation.get("value") {
                            if let Some(err_val) = value.get("err") {
                                if !err_val.is_null() {
                                    let err_str = format!("{:?}", err_val);
                                    if err_str.contains("AccountNotFound") {
                                        info!("DRY RUN buy: tx built correctly for {} (simulation AccountNotFound is expected - ephemeral keypair has no SOL/accounts)", mint);
                                    } else if err_str.contains("IncorrectProgramId") {
                                        warn!("DRY RUN buy simulation: IncorrectProgramId for {}", mint);
                                    } else {
                                        warn!("DRY RUN buy simulation error for {}: {:?}", mint, err_val);
                                    }
                                } else {
                                    info!("DRY RUN buy simulation SUCCESS for {}: compute_units={:?}", mint, value.get("units_consumed"));
                                }
                            } else {
                                info!("DRY RUN: no err field present for {} simulation", mint);
                            }
                        } else {
                            info!("DRY RUN: no value present in simulation for {}", mint);
                        }
                    }
                    Err(e) => warn!("DRY RUN buy simulation RPC failed for {}: {}", mint, e),
                }
            }
            Err(e) => warn!("DRY RUN cannot get latest blockhash for {}: {}", mint, e),
        }
    }
    Ok(Holding { mint: mint.to_string(), amount: token_amount, buy_price: buy_price_sol, buy_time: Utc::now(), metadata: None, onchain_raw: None, onchain: None })
}
