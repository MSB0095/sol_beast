// High-level buy service coordinating purchase flow
// Uses platform-agnostic traits for RPC, HTTP, and signing

use crate::buyer::evaluate_buy_heuristics;
use crate::error::CoreError;
use crate::idl::load_all_idls;
use crate::models::BondingCurveState;
use crate::rpc_client::{fetch_bonding_curve_creator, fetch_global_fee_recipient, RpcClient};
use crate::settings::Settings;
use crate::transaction_signer::TransactionSigner;
use crate::tx_builder::{build_buy_instruction, BuyArgs, BUY_DISCRIMINATOR};
use borsh::to_vec;
use solana_program::instruction::Instruction;
use solana_program::pubkey::Pubkey;
use spl_associated_token_account::get_associated_token_address;
use spl_associated_token_account::instruction::create_associated_token_account;
use std::collections::HashMap;
use std::str::FromStr;

pub type BuyServiceResult<T> = Result<T, CoreError>;

/// Configuration for a buy operation
#[derive(Debug, Clone)]
pub struct BuyConfig {
    pub mint: String,
    pub sol_amount: f64,
    pub current_price_sol: f64,
    pub bonding_curve_state: Option<BondingCurveState>,
}

/// Result of a buy operation
#[derive(Debug, Clone)]
pub struct BuyResult {
    pub mint: String,
    pub token_amount: u64,
    pub sol_spent: f64,
    pub transaction_signature: String,
    pub timestamp: i64,
}

/// High-level buy service
pub struct BuyService;

impl BuyService {
    /// Execute a token purchase with full validation and error handling
    pub async fn execute_buy(
        config: BuyConfig,
        rpc_client: &(dyn RpcClient + '_),
        signer: &(dyn TransactionSigner + '_),
        settings: &Settings,
    ) -> BuyServiceResult<BuyResult> {
        // 1. Validate heuristics first (fail fast)
        let heuristic = evaluate_buy_heuristics(
            &config.mint,
            config.sol_amount,
            config.current_price_sol,
            config.bonding_curve_state.as_ref(),
            settings,
        );

        if !heuristic.should_buy {
            return Err(CoreError::Validation(heuristic.reason));
        }

        // 2. Fetch blockhash for transaction
        let blockhash = rpc_client
            .get_latest_blockhash()
            .await
            .map_err(|_| CoreError::Validation("Failed to get latest blockhash".to_string()))?;

        // 3. Build buy instruction (placeholder - real implementation in CLI/WASM-specific code)
        let instructions = Self::build_buy_instructions(
            &config,
            heuristic.token_amount,
            signer.public_key(),
            rpc_client,
            settings,
        )
        .await?;

        // 4. Sign the transaction
        let signed_tx = signer
            .sign_instructions(instructions, &blockhash)
            .await?;

        // 5. Send the transaction
        let tx_sig = rpc_client
            .send_transaction(&signed_tx)
            .await
            .map_err(|_| CoreError::Validation("Failed to send transaction".to_string()))?;

        // 6. Return result
        Ok(BuyResult {
            mint: config.mint,
            token_amount: heuristic.token_amount,
            sol_spent: config.sol_amount,
            transaction_signature: tx_sig,
            timestamp: chrono::Utc::now().timestamp(),
        })
    }

    /// Build buy instructions (to be implemented by platform-specific code)
    /// This is a placeholder - actual implementation should be in CLI/WASM modules
    async fn build_buy_instructions(
        config: &BuyConfig,
        token_amount: u64,
        payer: Pubkey,
        rpc_client: &(dyn RpcClient + '_),
        settings: &Settings,
    ) -> BuyServiceResult<Vec<Instruction>> {
        let mint_pk = Pubkey::from_str(&config.mint)
            .map_err(|e| CoreError::Validation(format!("Invalid mint: {}", e)))?;

        // Discover creator and fee recipient using core RPC helpers
        let creator_opt = fetch_bonding_curve_creator(&config.mint, &settings.pump_fun_program, rpc_client)
            .await
            .ok()
            .flatten();
        let fee_recipient = fetch_global_fee_recipient(&settings.pump_fun_program, rpc_client)
            .await?;

        // Build account context common to IDL and fallback builders
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

        // Try IDL-based account resolution first
        let mut instruction_opt: Option<Instruction> = None;
        let mut last_err: Option<String> = None;
        let mut idls = load_all_idls();
        if let Some(idl) = idls.remove("pumpfun") {
            match idl.build_accounts_for("buy", &context) {
                Ok(metas) => {
                    let base_cost_lamports = (config.sol_amount * 1_000_000_000.0) as u64;
                    let slippage_multiplier = 1.0 + (settings.slippage_bps as f64 / 10000.0);
                    let max_sol_cost_with_slippage = (base_cost_lamports as f64 * slippage_multiplier) as u64;
                    let mut d = BUY_DISCRIMINATOR.to_vec();
                    d.extend(to_vec(&BuyArgs {
                        amount: token_amount,
                        max_sol_cost: max_sol_cost_with_slippage,
                        track_volume: Some(false),
                    }).map_err(|e| CoreError::Validation(format!("Encode buy args failed: {}", e)))?);
                    instruction_opt = Some(Instruction {
                        program_id: idl.address,
                        accounts: metas,
                        data: d,
                    });
                }
                Err(e) => last_err = Some(e.to_string()),
            }
        }

        let instruction = if let Some(i) = instruction_opt {
            i
        } else {
            if let Some(e) = last_err { log::debug!("IDL build_accounts_for(buy) failed: {}", e); }
            let base_cost_lamports = (config.sol_amount * 1_000_000_000.0) as u64;
            let slippage_multiplier = 1.0 + (settings.slippage_bps as f64 / 10000.0);
            let max_sol_cost_with_slippage = (base_cost_lamports as f64 * slippage_multiplier) as u64;
            build_buy_instruction(
                &pump_program_pk,
                &config.mint,
                token_amount,
                max_sol_cost_with_slippage,
                Some(false),
                &payer,
                &fee_recipient,
                creator_opt,
                settings,
            )
            .map_err(|e| CoreError::Validation(format!("Failed to build buy instruction: {}", e)))?
        };

        // Always ensure payer ATA exists before buy
        let ata = get_associated_token_address(&payer, &mint_pk);
        let mut instrs = Vec::new();
        instrs.push(create_associated_token_account(&payer, &payer, &mint_pk, &spl_token::id()));
        instrs.push(instruction);

        // Add dev tip if enabled
        if settings.has_dev_tips() {
            let transaction_lamports = (config.sol_amount * 1_000_000_000.0) as u64;
            crate::dev_fee::add_dev_tip_to_instructions(
                &mut instrs,
                &payer,
                transaction_lamports,
                settings.dev_tip_percent,
                settings.dev_tip_fixed_sol,
                0,
            )
            .map_err(|e| CoreError::Validation(format!("Failed to add dev tip: {}", e)))?;
        }

        log::debug!("Built buy instructions for mint {} (ATA {}, token_amount {})", config.mint, ata, token_amount);
        Ok(instrs)
    }

    /// Validate buy configuration before execution
    pub fn validate_config(config: &BuyConfig, settings: &Settings) -> BuyServiceResult<()> {
        // Validate mint is valid Pubkey
        Pubkey::from_str(&config.mint)
            .map_err(|e| CoreError::Validation(format!("Invalid mint: {}", e)))?;

        // Validate sol amount is positive
        if config.sol_amount <= 0.0 {
            return Err(CoreError::Validation(
                "SOL amount must be positive".to_string(),
            ));
        }

        // Validate price is positive
        if config.current_price_sol <= 0.0 {
            return Err(CoreError::Validation(
                "Price must be positive".to_string(),
            ));
        }

        // Validate settings constraints
        if config.sol_amount > settings.max_sol_per_token {
            return Err(CoreError::Validation(format!(
                "SOL amount {} exceeds maximum spend per token {}",
                config.sol_amount, settings.max_sol_per_token
            )));
        }

        Ok(())
    }
}

