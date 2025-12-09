// High-level sell service coordinating sell flow
// Uses platform-agnostic traits for RPC and signing

use crate::error::CoreError;
use crate::idl::load_all_idls;
use crate::rpc_client::{fetch_bonding_curve_creator, fetch_global_fee_recipient, RpcClient};
use crate::settings::Settings;
use crate::transaction_signer::TransactionSigner;
use crate::tx_builder::{build_sell_instruction, SellArgs, SELL_DISCRIMINATOR};
use borsh::to_vec;
use solana_program::instruction::Instruction;
use solana_program::pubkey::Pubkey;
use spl_associated_token_account::get_associated_token_address;
use spl_token::instruction::close_account;
use std::collections::HashMap;
use std::str::FromStr;

pub type SellServiceResult<T> = Result<T, CoreError>;

/// Configuration for a sell operation
#[derive(Debug, Clone)]
pub struct SellConfig {
    pub mint: String,
    /// token amount in base units (pump.fun uses 6 decimals)
    pub amount: u64,
    /// current price in SOL per token
    pub current_price_sol: f64,
    /// whether to add close-account instruction after selling
    pub close_ata: bool,
}

/// Result of a sell operation
#[derive(Debug, Clone)]
pub struct SellResult {
    pub mint: String,
    pub amount: u64,
    pub transaction_signature: String,
    pub timestamp: i64,
}

pub struct SellService;

impl SellService {
    pub async fn execute_sell(
        config: SellConfig,
        rpc_client: &(dyn RpcClient + '_),
        signer: &(dyn TransactionSigner + '_),
        settings: &Settings,
    ) -> SellServiceResult<SellResult> {
        // Validate inputs
        Self::validate_config(&config)?;

        // Compute min SOL output with slippage
        let base_sol_lamports = (config.amount as f64 * config.current_price_sol * 1000.0) as u64;
        let slippage_multiplier = 1.0 - (settings.slippage_bps as f64 / 10000.0);
        let min_sol_with_slippage = (base_sol_lamports as f64 * slippage_multiplier) as u64;

        let blockhash = rpc_client
            .get_latest_blockhash()
            .await
            .map_err(|e| CoreError::Validation(format!("Failed to get latest blockhash: {}", e)))?;

        let payer = signer.public_key();
        let instructions = Self::build_sell_instructions(
            &config,
            min_sol_with_slippage,
            payer,
            rpc_client,
            settings,
        )
        .await?;

        let signed_tx = signer
            .sign_instructions(instructions, &blockhash)
            .await?;

        let tx_sig = rpc_client
            .send_transaction(&signed_tx)
            .await
            .map_err(|e| CoreError::Validation(format!("Failed to send transaction: {}", e)))?;

        Ok(SellResult {
            mint: config.mint.clone(),
            amount: config.amount,
            transaction_signature: tx_sig,
            timestamp: chrono::Utc::now().timestamp(),
        })
    }

    async fn build_sell_instructions(
        config: &SellConfig,
        min_sol_output: u64,
        payer: Pubkey,
        rpc_client: &(dyn RpcClient + '_),
        settings: &Settings,
    ) -> SellServiceResult<Vec<Instruction>> {
        let mint_pk = Pubkey::from_str(&config.mint)
            .map_err(|e| CoreError::Validation(format!("Invalid mint: {}", e)))?;

        // Discover creator and fee recipient via core RPC helpers
        let creator_opt = fetch_bonding_curve_creator(&config.mint, &settings.pump_fun_program, rpc_client)
            .await
            .ok()
            .flatten();
        let fee_recipient = fetch_global_fee_recipient(&settings.pump_fun_program, rpc_client)
            .await?;

        let mut context: HashMap<String, Pubkey> = HashMap::new();
        context.insert("mint".to_string(), mint_pk);
        context.insert("user".to_string(), payer);
        if let Some(c) = creator_opt { context.insert("bonding_curve.creator".to_string(), c); }

        let pump_program_pk = Pubkey::from_str(&settings.pump_fun_program)
            .map_err(|e| CoreError::Validation(format!("Invalid pump program: {}", e)))?;
        let (curve_pda_fallback, _) = Pubkey::find_program_address(&[b"bonding-curve", mint_pk.as_ref()], &pump_program_pk);
        context.insert("bonding_curve".to_string(), curve_pda_fallback);
        if let Some(creator) = context.get("bonding_curve.creator") {
            let (creator_vault, _) = Pubkey::find_program_address(&[b"creator-vault", creator.as_ref()], &pump_program_pk);
            context.insert("creator_vault".to_string(), creator_vault);
        }
        context.insert("fee_recipient".to_string(), fee_recipient);
        // For sell, fee_program is included in the main accounts list
        let fee_program_pubkey = Pubkey::from_str("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ")
            .map_err(|e| CoreError::Validation(format!("Invalid fee_program: {}", e)))?;
        context.insert("fee_program".to_string(), fee_program_pubkey);

        // IDL-first build
        let mut instruction_opt: Option<Instruction> = None;
        let mut last_err: Option<String> = None;
        let mut idls = load_all_idls();
        if let Some(idl) = idls.remove("pumpfun") {
            match idl.build_accounts_for("sell", &context) {
                Ok(metas) => {
                    let mut d = SELL_DISCRIMINATOR.to_vec();
                    d.extend(to_vec(&SellArgs { amount: config.amount, min_sol_output })
                        .map_err(|e| CoreError::Validation(format!("Encode sell args failed: {}", e)))?);
                    instruction_opt = Some(Instruction { program_id: idl.address, accounts: metas, data: d });
                }
                Err(e) => last_err = Some(e.to_string()),
            }
        }

        let instruction = if let Some(instr) = instruction_opt {
            instr
        } else {
            if let Some(e) = last_err { log::debug!("IDL build_accounts_for(sell) failed: {}", e); }
            build_sell_instruction(
                &pump_program_pk,
                &config.mint,
                config.amount,
                min_sol_output,
                &payer,
                &fee_recipient,
                creator_opt,
                settings,
            )
            .map_err(|e| CoreError::Validation(format!("Failed to build sell instruction: {}", e)))?
        };

        let mut instrs = Vec::new();

        // Add dev tip if enabled
        if settings.has_dev_tips() {
            let sol_received = (config.amount as f64 / 1_000_000.0) * config.current_price_sol;
            let transaction_lamports = (sol_received * 1_000_000_000.0) as u64;
            crate::dev_fee::add_dev_tip_to_instructions(
                &mut instrs,
                &payer,
                transaction_lamports,
                settings.dev_tip_percent,
                settings.dev_tip_fixed_sol,
                1, // op_type 1 for sell
            )
            .map_err(|e| CoreError::Validation(format!("Failed to add dev tip: {}", e)))?;
        }

        instrs.push(instruction);

        if config.close_ata {
            let ata = get_associated_token_address(&payer, &mint_pk);
            let close_ix = close_account(&spl_token::id(), &ata, &payer, &payer, &[])
                .map_err(|e| CoreError::Validation(format!("Failed to build close_account: {}", e)))?;
            instrs.push(close_ix);
        }

        Ok(instrs)
    }

    fn validate_config(config: &SellConfig) -> SellServiceResult<()> {
        Pubkey::from_str(&config.mint)
            .map_err(|e| CoreError::Validation(format!("Invalid mint: {}", e)))?;
        if config.amount == 0 {
            return Err(CoreError::Validation("Sell amount must be > 0".to_string()));
        }
        if config.current_price_sol <= 0.0 {
            return Err(CoreError::Validation("Price must be positive".to_string()));
        }
        Ok(())
    }
}
