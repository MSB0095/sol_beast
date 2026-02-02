use serde_json::{json, Value};
use base64::{engine::general_purpose::STANDARD as Base64Engine, Engine};
use crate::{
    models::{Holding, PriceCache},
    settings::Settings,
    rpc::{fetch_current_price, fetch_bonding_curve_state, fetch_global_fee_recipient, detect_idl_for_mint, fetch_bonding_curve_creator, build_missing_ata_preinstructions, fetch_with_fallback},
    tx_builder::{BUY_DISCRIMINATOR, build_buy_instruction},
    idl::load_all_idls,
};
use solana_client::rpc_client::RpcClient;
use std::{sync::Arc, collections::HashMap};
use tokio::sync::Mutex;
use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::Transaction, pubkey::Pubkey,
};
use log::{info, warn, debug};
use std::str::FromStr;
use spl_associated_token_account::instruction::create_associated_token_account;
use spl_associated_token_account::get_associated_token_address;
use chrono::Utc;

pub async fn buy_token(
    mint: &str,
    sol_amount: f64,
    is_real: bool,
    keypair: Option<&Keypair>,
    simulate_keypair: Option<&Keypair>,
    price_cache: Arc<Mutex<PriceCache>>,
    rpc_client: &Arc<RpcClient>,
    settings: &Arc<Settings>,
) -> Result<Holding, Box<dyn std::error::Error + Send + Sync>> {
    // fetch_current_price now returns SOL per token
    let buy_price_sol = fetch_current_price(mint, &price_cache, rpc_client, settings).await?;
    // Compute token amount as SOL amount divided by SOL per token, using actual mint decimals
    let decimals = match crate::rpc::fetch_mint_decimals(mint, rpc_client, settings).await {
        Ok(d) => d as i32,
        Err(e) => {
            warn!("Failed to fetch mint decimals for {}: {} -- falling back to {}", mint, e, settings.default_token_decimals);
            settings.default_token_decimals as i32
        }
    };
    let token_amount = ((sol_amount / buy_price_sol) * 10f64.powi(decimals)) as u64;
    
    // Safety checks when enabled
    if settings.enable_safer_sniping {
        // Check 1: Minimum tokens threshold
        if token_amount < settings.min_tokens_threshold {
            return Err(format!(
                "Token amount {} is below minimum threshold {} (price too high: {:.18} SOL/token)",
                token_amount, settings.min_tokens_threshold, buy_price_sol
            ).into());
        }
        
        // Check 2: Maximum SOL per token (price ceiling)
        if buy_price_sol > settings.max_sol_per_token {
            return Err(format!(
                "Token price {:.18} SOL/token exceeds maximum {:.18} SOL/token (already too expensive)",
                buy_price_sol, settings.max_sol_per_token
            ).into());
        }
        
        // Check 3: Liquidity checks (requires bonding curve data)
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

    // Fetch fee_recipient from Global PDA (needed for both real and simulate modes)
    let fee_recipient = fetch_global_fee_recipient(rpc_client, settings).await?;

    if is_real {
        let client = RpcClient::new(&settings.solana_rpc_urls[0]);
        let payer = keypair.ok_or("Keypair required")?;
        debug!("Preparing buy TX for mint {} amount {} SOL (real)", mint, sol_amount);
        
        // Determine best IDL for this mint (try detect by PDA existence)
        let detected_idl_opt = detect_idl_for_mint(mint, rpc_client, settings).await;
        let mut built_instr: Option<solana_program::instruction::Instruction> = None;
        let mut last_err: Option<String> = None;
        let mint_pk = Pubkey::from_str(mint)?;
        let creator_opt = fetch_bonding_curve_creator(mint, rpc_client, settings).await.ok().flatten();
        let payer_pubkey = payer.pubkey();
        // Build a rich context that IDLs commonly expect
        let mut context: HashMap<String, Pubkey> = HashMap::new();
        context.insert("mint".to_string(), mint_pk);
        context.insert("user".to_string(), payer_pubkey);
        if let Some(c) = creator_opt { context.insert("bonding_curve.creator".to_string(), c); }
        // bonding_curve PDA using configured pump program as fallback
        let pump_program_pk = Pubkey::from_str(&settings.pump_fun_program)?;
        let (curve_pda_fallback, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_program_pk);
        context.insert("bonding_curve".to_string(), curve_pda_fallback);
        if let Some(creator) = context.get("bonding_curve.creator") {
            let (creator_vault, _) = Pubkey::find_program_address(&[b"creator-vault", creator.as_ref()], &pump_program_pk);
            context.insert("creator_vault".to_string(), creator_vault);
        }
        // Use actual fee_recipient from bonding curve
        context.insert("fee_recipient".to_string(), fee_recipient);
        
        // NOTE: fee_program is invoked via CPI (Cross-Program Invocation) inside pump.fun program
        // It is NOT included in the main instruction accounts list - only in inner instructions
        // Do NOT add fee_program to context
        
        // Debug: log context before IDL building
        debug!("Context for buy instruction building:");
        for (k, v) in context.iter() {
            debug!("  {}: {}", k, v);
        }
        
        // detect_idl result preferred; otherwise fallback order
        let try_idls: Vec<crate::idl::SimpleIdl> = if let Some(idl) = detected_idl_opt { vec![idl] } else { load_all_idls().into_values().collect() };
        for idl in try_idls {
            debug!("Trying IDL with program_id: {}", idl.address);
            match idl.build_accounts_for("buy", &context) {
                Ok(metas) => {
                    warn!("IDL {} build_accounts_for(buy) succeeded with {} accounts - USING THIS IDL", idl.address, metas.len());
                    for (i, meta) in metas.iter().enumerate() {
                        debug!("  [{}] {} (signer={}, writable={})", i, meta.pubkey, meta.is_signer, meta.is_writable);
                    }
                    // Apply slippage to max_sol_cost: increase by slippage_bps basis points
                    // max_sol_cost should be the maximum SOL we're willing to spend (in lamports)
                    let base_cost_lamports = (sol_amount * 1_000_000_000.0) as u64;
                    let slippage_multiplier = 1.0 + (settings.slippage_bps as f64 / 10000.0);
                    let max_sol_cost_with_slippage = (base_cost_lamports as f64 * slippage_multiplier) as u64;
                    let mut d = BUY_DISCRIMINATOR.to_vec();
                    d.extend(borsh::to_vec(&crate::tx_builder::BuyArgs { 
                        amount: token_amount, 
                        max_sol_cost: max_sol_cost_with_slippage, 
                        track_volume: Some(false) 
                    }).unwrap());
                    built_instr = Some(solana_program::instruction::Instruction { program_id: idl.address, accounts: metas, data: d });
                    break;
                }
                Err(e) => last_err = Some(e.to_string()),
            }
        }
        let instruction = if let Some(instr) = built_instr { instr } else {
            if let Some(e) = last_err { debug!("IDL buy build errors: {}", e); }
            // fallback to legacy builder using configured pump program
            let program_id = Pubkey::from_str(&settings.pump_fun_program)?;
            // Calculate max_sol_cost with slippage
            let base_cost_lamports = (sol_amount * 1_000_000_000.0) as u64;
            let slippage_multiplier = 1.0 + (settings.slippage_bps as f64 / 10000.0);
            let max_sol_cost_with_slippage = (base_cost_lamports as f64 * slippage_multiplier) as u64;
            build_buy_instruction(
                &program_id,
                mint,
                token_amount,
                max_sol_cost_with_slippage,
                Some(false),
                &payer_pubkey,
                &fee_recipient,
                creator_opt,
                settings,
            )?
        };
        // ALWAYS create user's ATA when buying (even if exists, instruction will succeed idempotently)
        // This ensures the account exists and we can close it later when selling to reclaim rent
        let _ata = get_associated_token_address(&payer_pubkey, &mint_pk);
        let _pre_instructions: Vec<solana_program::instruction::Instruction> = vec![
            create_associated_token_account(&payer_pubkey, &payer_pubkey, &mint_pk, &spl_token::id()),
        ];        
        // prepare context with payer so ATA creation uses correct funding account
        let mut real_context: HashMap<String, Pubkey> = HashMap::new();
        real_context.insert("mint".to_string(), mint_pk);
        real_context.insert("user".to_string(), payer_pubkey);
        real_context.insert("payer".to_string(), payer_pubkey);
        if let Some(c) = creator_opt { real_context.insert("bonding_curve.creator".to_string(), c); }
        if let Some(bc) = context.get("bonding_curve") { real_context.insert("bonding_curve".to_string(), *bc); }
        if let Some(cv) = context.get("creator_vault") { real_context.insert("creator_vault".to_string(), *cv); }
        // compute missing ATA pre-instructions for accounts in the instruction
        let ata_pre = build_missing_ata_preinstructions(&real_context).await?;
        let mut all_instrs: Vec<solana_program::instruction::Instruction> = Vec::new();
        for pi in ata_pre.into_iter() { all_instrs.push(pi); }
        all_instrs.push(instruction);
        
        // Add dev fee if enabled
        if settings.dev_fee_enabled {
            // Calculate transaction amount in lamports (sol_amount is in SOL)
            let transaction_lamports = (sol_amount * 1_000_000_000.0) as u64;
            crate::dev_fee::add_dev_fee_to_instructions(&mut all_instrs, &payer.pubkey(), transaction_lamports, 0, &*settings)?;
            info!("Added {}% dev fee to buy transaction ({} SOL)", settings.dev_fee_percent, sol_amount);
        }
        
        // Choose transaction submission method
        let mut final_token_amount_u64: Option<u64> = None;
        if settings.helius_sender_enabled {
            info!("Using Helius Sender for buy transaction of mint {}", mint);
            let signature = crate::helius_sender::send_transaction_with_retry(
                all_instrs,
                payer,
                settings,
                &client,
                3, // max retries
            ).await?;
            info!("Buy transaction sent via Helius Sender: {}", signature);
            // Query on-chain token accounts to find exact token balance for payer
            let owner_str = payer_pubkey.to_string();
            if let Ok(Some(acc)) = crate::rpc::find_token_account_owned_by_owner(mint, &owner_str, rpc_client, settings).await {
                if let Ok(pk) = Pubkey::from_str(&acc) {
                    if let Ok(balance) = client.get_token_account_balance(&pk) {
                        if let Ok(amount_u64) = balance.amount.parse::<u64>() {
                            final_token_amount_u64 = Some(amount_u64);
                        }
                    }
                }
            }
        } else {
            let mut tx = Transaction::new_with_payer(&all_instrs, Some(&payer.pubkey()));
            let blockhash = client.get_latest_blockhash()?;
            tx.sign(&[payer], blockhash);
            client.send_and_confirm_transaction(&tx)?;
            // Query on-chain token accounts to find exact token balance for payer
            let owner_str = payer_pubkey.to_string();
            if let Ok(Some(acc)) = crate::rpc::find_token_account_owned_by_owner(mint, &owner_str, rpc_client, settings).await {
                if let Ok(pk) = Pubkey::from_str(&acc) {
                    if let Ok(balance) = client.get_token_account_balance(&pk) {
                        if let Ok(amount_u64) = balance.amount.parse::<u64>() {
                            final_token_amount_u64 = Some(amount_u64);
                        }
                    }
                }
            }
        }
        // If we fetched an on-chain amount, override token_amount returned to be exact
        if let Some(exact) = final_token_amount_u64 {
            info!("Buy complete: on-chain token amount for {} = {} (base units)", mint, exact);
            // Use this exact amount for returned holding
            return Ok(Holding {
                amount: exact,
                buy_price: buy_price_sol,
                buy_time: Utc::now(),
                metadata: None,
                onchain_raw: None,
                onchain: None,
            });
        }
    } else {
        // Dry-run simulation: construct same instruction and simulate it using
        // either the provided simulate_keypair or an ephemeral Keypair fallback.
        let client = RpcClient::new(&settings.solana_rpc_urls[0]);
        // keep an owned Keypair alive in this scope if we need to create one
        let mut _maybe_owned_sim: Option<Keypair> = None;
        let sim_payer_ref: &Keypair = if let Some(k) = simulate_keypair {
            k
        } else {
            let owned_sim = Keypair::new();
            _maybe_owned_sim = Some(owned_sim);
            _maybe_owned_sim.as_ref().ok_or_else(|| Box::<dyn std::error::Error + Send + Sync>::from("Failed to get sim keypair ref"))?
        };
        debug!("Preparing simulated buy TX for mint {} amount {} SOL (dry run)", mint, sol_amount);
        let program_id = Pubkey::from_str(&settings.pump_fun_program)?;
        let creator_opt = fetch_bonding_curve_creator(mint, rpc_client, settings).await.ok().flatten();
        let sim_payer_pubkey = sim_payer_ref.pubkey();
        // Try to build accounts via IDL-aware builder for exactness
        let idls = load_all_idls();
        let mut instruction_opt: Option<solana_program::instruction::Instruction> = None;
        let mut last_err: Option<String> = None;
        let mint_pk = Pubkey::from_str(mint)?;
        if let Some(idl) = idls.get("pumpfun") {
            // prepare context map
            let mut context: HashMap<String, Pubkey> = HashMap::new();
            context.insert("mint".to_string(), mint_pk);
            context.insert("user".to_string(), sim_payer_pubkey);
            if let Some(c) = creator_opt { context.insert("bonding_curve.creator".to_string(), c); }
            // Add bonding_curve PDA
            let pump_program_pk = Pubkey::from_str(&settings.pump_fun_program)?;
            let (curve_pda, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_program_pk);
            context.insert("bonding_curve".to_string(), curve_pda);
            // Add creator_vault PDA if creator exists
            if let Some(creator) = context.get("bonding_curve.creator").cloned() {
                let (creator_vault, _) = Pubkey::find_program_address(&[b"creator-vault", creator.as_ref()], &pump_program_pk);
                context.insert("creator_vault".to_string(), creator_vault);
            }
            // Add fee_recipient - pump.fun uses a fixed address
            let fee_recipient = Pubkey::from_str("39azUYFWPz3VHgKCf3VChUwbpURdCHRxjWVowf5jUJjg")?;
            context.insert("fee_recipient".to_string(), fee_recipient);
            // NOTE: fee_program is invoked via CPI, not included in main instruction accounts
            // Do NOT add fee_program to context
            match idl.build_accounts_for("buy", &context) {
                Ok(metas) => {
                    debug!("IDL build_accounts_for(buy) succeeded with {} accounts (dry-run)", metas.len());
                    for (i, meta) in metas.iter().enumerate() {
                        debug!("  [{}] {} (signer={}, writable={})", i, meta.pubkey, meta.is_signer, meta.is_writable);
                    }
                    // Apply slippage to max_sol_cost
                    let base_cost_lamports = (sol_amount * 1_000_000_000.0) as u64;
                    let slippage_multiplier = 1.0 + (settings.slippage_bps as f64 / 10000.0);
                    let max_sol_cost_with_slippage = (base_cost_lamports as f64 * slippage_multiplier) as u64;
                    instruction_opt = Some(solana_program::instruction::Instruction { program_id, accounts: metas, data: {
                        let mut d = BUY_DISCRIMINATOR.to_vec();
                        d.extend(borsh::to_vec(&crate::tx_builder::BuyArgs { 
                            amount: token_amount, 
                            max_sol_cost: max_sol_cost_with_slippage, 
                            track_volume: Some(false) 
                        }).unwrap());
                        d
                    }});
                }
                Err(e) => last_err = Some(e.to_string()),
            }
        }
        let instruction = if let Some(instr) = instruction_opt { instr } else {
            if let Some(e) = last_err { debug!("IDL build failed for buy: {}", e); }
            // fallback to legacy builder
            let base_cost_lamports = (sol_amount * 1_000_000_000.0) as u64;
            let slippage_multiplier = 1.0 + (settings.slippage_bps as f64 / 10000.0);
            let max_sol_cost_with_slippage = (base_cost_lamports as f64 * slippage_multiplier) as u64;
            build_buy_instruction(
                &program_id,
                mint,
                token_amount,
                max_sol_cost_with_slippage,
                Some(false),
                &sim_payer_pubkey,
                &fee_recipient,
                creator_opt,
                settings,
            )?
        };
    // include any pre_instructions in the simulated tx (e.g., ATA creation)
    let mut tx_instructions = Vec::new();
        // Build pre_instructions for dry-run (ensure ATA exists for sim payer)
        let mut pre_instructions: Vec<solana_program::instruction::Instruction> = Vec::new();
        let mint_pk = Pubkey::from_str(mint)?;
        let ata = get_associated_token_address(&sim_payer_pubkey, &mint_pk);
        match fetch_with_fallback::<Value>(json!({
            "jsonrpc": "2.0", "id": 1, "method": "getAccountInfo",
            "params": [ ata.to_string(), { "encoding": "base64", "commitment": "confirmed" } ]
        }), "getAccountInfo", rpc_client, settings).await {
            Ok(info) => {
                if info.result.is_none() {
                    pre_instructions.push(create_associated_token_account(&sim_payer_pubkey, &sim_payer_pubkey, &mint_pk, &spl_token::id()));
                } else if let Some(result_val) = info.result {
                    let val = if let Some(v) = result_val.get("value") { v.clone() } else { result_val.clone() };
                    if val.is_null() {
                        pre_instructions.push(create_associated_token_account(&sim_payer_pubkey, &sim_payer_pubkey, &mint_pk, &spl_token::id()));
                    }
                }
            }
            Err(e) => debug!("Failed to check ATA existence for {}: {}", ata, e),
        }
        // we can't easily create payer-signed ATA in dry-run when using ephemeral keypair,
        // but include the instruction so simulateTransaction can check behavior.
        // convert pre_instructions (created with payer_pubkey) to use sim_payer_pubkey when needed
        for pi in pre_instructions.into_iter() {
            // ensure the payer pubkey in create_associated_token_account matches sim_payer
            // the create_associated_token_account sets accounts; it's safe to just push as-is
            tx_instructions.push(pi);
        }
        tx_instructions.push(instruction.clone());
        
        // Debug: log instruction details before simulation
        debug!("DRY RUN buy simulation for {}: program_id={}", mint, instruction.program_id);
        debug!("  Instruction has {} accounts:", instruction.accounts.len());
        for (i, acc) in instruction.accounts.iter().enumerate() {
            debug!("    [{}] {} (signer={}, writable={})", i, acc.pubkey, acc.is_signer, acc.is_writable);
        }
        debug!("  Instruction data length: {} bytes", instruction.data.len());
        debug!("  Payer (sim wallet): {}", sim_payer_pubkey);
        
        let mut tx = Transaction::new_with_payer(&tx_instructions, Some(&sim_payer_pubkey));
        match client.get_latest_blockhash() {
            Ok(blockhash) => {
                // For dry-run simulation with ephemeral keypair:
                // We build the transaction correctly but cannot fully simulate because
                // the ephemeral keypair has no SOL and its ATAs don't exist on-chain.
                // This is expected - the transaction building itself validates the logic.
                tx.message.recent_blockhash = blockhash;
                // Sign the transaction with the simulate payer so remote simulate endpoints
                // which expect a signed transaction will behave more predictably.
                tx.sign(&[sim_payer_ref], blockhash);

                // Serialize and send to Helius simulate endpoint for consistent simulation
                match bincode::serialize(&tx) {
                    Ok(serialized) => {
                        let tx_base64 = Base64Engine.encode(&serialized);
                        match crate::helius_sender::simulate_transaction_via_helius(&tx_base64, &*settings).await {
                            Ok(json) => {
                                // Try to inspect error field inside result if present
                                if let Some(err) = json.get("error") {
                                    warn!("DRY RUN buy simulation error for {}: {}", mint, err);
                                } else {
                                    info!("DRY RUN buy simulation completed for {} (helius)", mint);
                                }
                            }
                            Err(e) => warn!("DRY RUN buy simulation (helius) failed for {}: {}", mint, e),
                        }
                    }
                    Err(e) => warn!("Failed to serialize TX for dry-run simulate: {}", e),
                }
            }
            Err(e) => warn!("DRY RUN cannot get latest blockhash for {}: {}", mint, e),
        }
    }
    // Return simulated holding for dry runs
    Ok(Holding {
        amount: token_amount,
        buy_price: buy_price_sol,
        buy_time: Utc::now(),
        metadata: None,
        onchain_raw: None,
        onchain: None,
    })
}