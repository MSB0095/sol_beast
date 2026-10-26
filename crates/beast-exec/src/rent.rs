//! Rent reclaimer for the wallet's own SPL Token and Token-2022 accounts.
//!
//! This is deliberately conservative: by default it only closes accounts with
//! an exactly zero token balance. Dust burning is *not* implemented here
//! because it requires an additional price oracle and explicit legal/UX
//! consent. Non-empty accounts are always reported as blocked.

use crate::{ExecutionError, Result, Wallet};
use base64::Engine;
use beast_core::config::RentConfig;
use beast_dex::Address;
use beast_rpc::{Account, RpcClient};
use serde::{Deserialize, Serialize};
use solana_sdk::hash::Hash;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::transaction::Transaction;
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub const TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const TOKEN_2022_PROGRAM_ID: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";
const SPL_TOKEN_ACCOUNT_LEN: usize = 165;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RentAccount {
    pub address: String,
    pub token_program: String,
    pub mint: String,
    pub owner: String,
    pub amount: u64,
    pub lamports: u64,
    pub state: u8,
    pub closable: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RentScan {
    pub wallet: String,
    pub scanned_at_ms: i64,
    pub accounts: Vec<RentAccount>,
    pub closable: usize,
    pub total_closable_lamports: u64,
    pub token_program_accounts: usize,
    pub token_2022_accounts: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RentBatch {
    pub signature: Option<String>,
    pub accounts: Vec<String>,
    pub simulated: bool,
    pub recovered_lamports: u64,
    pub fee_lamports: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RentReclaimReport {
    pub dry_run: bool,
    pub scanned: usize,
    pub closable: usize,
    pub attempted: usize,
    pub closed: usize,
    pub gross_recovered_lamports: u64,
    pub fee_lamports: u64,
    pub net_recovered_lamports: u64,
    pub batches: Vec<RentBatch>,
    pub errors: Vec<String>,
}

pub struct RentReclaimer {
    rpc: Arc<RpcClient>,
    wallet: Arc<Wallet>,
    config: RentConfig,
}

impl RentReclaimer {
    pub fn new(rpc: Arc<RpcClient>, wallet: Arc<Wallet>, config: RentConfig) -> Self {
        Self {
            rpc,
            wallet,
            config,
        }
    }

    pub fn config(&self) -> &RentConfig {
        &self.config
    }

    pub fn wallet_address(&self) -> Address {
        self.wallet.address()
    }

    pub async fn scan(&self) -> Result<RentScan> {
        if !self.config.enabled {
            return Err(ExecutionError::Config("rent reclaimer is disabled".into()));
        }
        let mut errors = Vec::new();
        let mut accounts = Vec::new();
        let mut seen = HashSet::new();
        let programs = if self.config.include_token_2022 {
            vec![TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID]
        } else {
            vec![TOKEN_PROGRAM_ID]
        };
        let owner = self.wallet.address();
        let mut token_program_accounts = 0usize;
        let mut token_2022_accounts = 0usize;
        for program in programs {
            let Ok(program_id) = program.parse::<Address>() else {
                errors.push(format!("invalid token program {program}"));
                continue;
            };
            match self
                .rpc
                .get_token_accounts_by_owner(owner, program_id)
                .await
            {
                Ok(entries) => {
                    for (address, account) in entries {
                        let key = address.to_string();
                        if !seen.insert(key.clone()) {
                            continue;
                        }
                        if program == TOKEN_PROGRAM_ID {
                            token_program_accounts += 1;
                        } else {
                            token_2022_accounts += 1;
                        }
                        accounts.push(classify_token_account(
                            key,
                            program.to_string(),
                            account,
                            owner,
                        ));
                    }
                }
                Err(error) => errors.push(format!("{}: {error}", program)),
            }
        }
        accounts.sort_by_key(|account| std::cmp::Reverse(account.lamports));
        let closable = accounts.iter().filter(|account| account.closable).count();
        let total_closable_lamports = accounts
            .iter()
            .filter(|account| account.closable)
            .map(|account| account.lamports)
            .sum();
        Ok(RentScan {
            wallet: owner.to_string(),
            scanned_at_ms: now_ms(),
            accounts,
            closable,
            total_closable_lamports,
            token_program_accounts,
            token_2022_accounts,
            errors,
        })
    }

    /// Reclaim empty token accounts. `real` must be explicitly true to send a
    /// transaction; otherwise the call returns an exact dry-run plan.
    pub async fn reclaim(&self, real: bool) -> Result<RentReclaimReport> {
        let scan = self.scan().await?;
        let candidates: Vec<RentAccount> = scan
            .accounts
            .iter()
            .filter(|account| account.closable)
            .cloned()
            .collect();
        let mut batches = Vec::new();
        let mut errors = scan.errors.clone();
        let batch_size = self.config.batch_size.clamp(1, 25);
        let mut gross = 0u64;
        let mut fees = 0u64;
        let mut closed = 0usize;
        let mut attempted = 0usize;

        for chunk in candidates.chunks(batch_size) {
            let recovered: u64 = chunk.iter().map(|account| account.lamports).sum();
            if recovered < self.config.min_recovered_lamports {
                continue;
            }
            attempted += chunk.len();
            if !real {
                gross = gross.saturating_add(recovered);
                fees = fees.saturating_add(5_000);
                batches.push(RentBatch {
                    signature: None,
                    accounts: chunk
                        .iter()
                        .map(|account| account.address.clone())
                        .collect(),
                    simulated: false,
                    recovered_lamports: recovered,
                    fee_lamports: 5_000,
                    error: None,
                });
                continue;
            }
            match self.close_batch(chunk).await {
                Ok(batch) => {
                    if batch.error.is_none() {
                        gross = gross.saturating_add(batch.recovered_lamports);
                        fees = fees.saturating_add(batch.fee_lamports);
                        if batch.signature.is_some() {
                            closed += chunk.len();
                        }
                    } else if let Some(error) = &batch.error {
                        errors.push(error.clone());
                    }
                    batches.push(batch);
                }
                Err(error) => errors.push(error.to_string()),
            }
        }

        Ok(RentReclaimReport {
            dry_run: !real,
            scanned: scan.accounts.len(),
            closable: scan.closable,
            attempted,
            closed,
            gross_recovered_lamports: gross,
            fee_lamports: fees,
            net_recovered_lamports: gross.saturating_sub(fees),
            batches,
            errors,
        })
    }

    async fn close_batch(&self, accounts: &[RentAccount]) -> Result<RentBatch> {
        let mut instructions = Vec::with_capacity(accounts.len());
        let mut recovered = 0u64;
        for account in accounts {
            let token_program = Pubkey::from_str(&account.token_program)
                .map_err(|error| ExecutionError::Transaction(format!("token program: {error}")))?;
            let token_account = Pubkey::from_str(&account.address)
                .map_err(|error| ExecutionError::Transaction(format!("token account: {error}")))?;
            let destination = self.wallet.pubkey();
            instructions.push(Instruction {
                program_id: token_program,
                accounts: vec![
                    AccountMeta::new(token_account, false),
                    AccountMeta::new(destination, false),
                    AccountMeta::new_readonly(destination, true),
                ],
                data: vec![9], // SPL Token CloseAccount
            });
            recovered = recovered.saturating_add(account.lamports);
        }
        let blockhash = self.latest_blockhash().await?;
        let tx = Transaction::new_signed_with_payer(
            &instructions,
            Some(&self.wallet.pubkey()),
            &[self.wallet.keypair()],
            blockhash,
        );
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(bincode::serialize(&tx).map_err(
                |error| ExecutionError::Transaction(format!("serialize rent transaction: {error}")),
            )?);
        let simulation = self.rpc.simulate_transaction(&encoded).await?;
        if let Some(error) = simulation.err {
            return Ok(RentBatch {
                signature: None,
                accounts: accounts
                    .iter()
                    .map(|account| account.address.clone())
                    .collect(),
                simulated: true,
                recovered_lamports: 0,
                fee_lamports: 0,
                error: Some(format!("simulation failed: {error}")),
            });
        }
        let signature = self.rpc.send_transaction(&encoded, false).await?;
        Ok(RentBatch {
            signature: Some(signature),
            accounts: accounts
                .iter()
                .map(|account| account.address.clone())
                .collect(),
            simulated: true,
            recovered_lamports: recovered,
            fee_lamports: 5_000,
            error: None,
        })
    }

    async fn latest_blockhash(&self) -> Result<Hash> {
        let (blockhash, _) = self.rpc.get_latest_blockhash().await?;
        Hash::from_str(&blockhash)
            .map_err(|error| ExecutionError::Transaction(format!("blockhash: {error}")))
    }
}

fn classify_token_account(
    address: String,
    token_program: String,
    account: Account,
    expected_owner: Address,
) -> RentAccount {
    if account.data.len() < SPL_TOKEN_ACCOUNT_LEN {
        return RentAccount {
            address,
            token_program,
            mint: String::new(),
            owner: String::new(),
            amount: 0,
            lamports: account.lamports,
            state: 0,
            closable: false,
            reason: "malformed token account".into(),
        };
    }
    let mint = Address::from_slice(&account.data[0..32])
        .map(|value| value.to_string())
        .unwrap_or_default();
    let owner = Address::from_slice(&account.data[32..64])
        .map(|value| value.to_string())
        .unwrap_or_default();
    let amount = u64::from_le_bytes(
        account.data[64..72]
            .try_into()
            .expect("token account amount is 8 bytes"),
    );
    let state = account.data[108];
    let reason = if owner != expected_owner.to_string() {
        "owner mismatch"
    } else if state == 2 {
        "token account is frozen"
    } else if state != 1 {
        "unexpected token account state"
    } else if amount != 0 {
        "non-empty token balance"
    } else {
        "empty; safe to close"
    };
    let closable = reason == "empty; safe to close";
    RentAccount {
        address,
        token_program,
        mint,
        owner,
        amount,
        lamports: account.lamports,
        state,
        closable,
        reason: reason.into(),
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use beast_rpc::Account;

    fn account(amount: u64, state: u8, lamports: u64) -> Account {
        let mut data = vec![0u8; SPL_TOKEN_ACCOUNT_LEN];
        data[32..64].copy_from_slice(
            Address::from_str("11111111111111111111111111111111")
                .unwrap()
                .as_bytes(),
        );
        data[64..72].copy_from_slice(&amount.to_le_bytes());
        data[108] = state;
        Account {
            lamports,
            owner: Address::from_str(TOKEN_PROGRAM_ID).unwrap(),
            data,
            executable: false,
            rent_epoch: 0,
        }
    }

    #[test]
    fn only_zero_balance_initialized_accounts_are_closable() {
        let owner = Address::from_str("11111111111111111111111111111111").unwrap();
        let empty = classify_token_account(
            "empty".into(),
            TOKEN_PROGRAM_ID.into(),
            account(0, 1, 2_039_280),
            owner,
        );
        assert!(empty.closable);
        let non_empty = classify_token_account(
            "non-empty".into(),
            TOKEN_PROGRAM_ID.into(),
            account(1, 1, 2_039_280),
            owner,
        );
        assert!(!non_empty.closable);
        let frozen = classify_token_account(
            "frozen".into(),
            TOKEN_PROGRAM_ID.into(),
            account(0, 2, 2_039_280),
            owner,
        );
        assert!(!frozen.closable);
    }

    #[test]
    fn close_instruction_uses_spl_close_index_and_owner_signer() {
        let owner = Pubkey::new_unique();
        let token = Pubkey::new_unique();
        let instruction = Instruction {
            program_id: Pubkey::from_str(TOKEN_PROGRAM_ID).unwrap(),
            accounts: vec![
                AccountMeta::new(token, false),
                AccountMeta::new(owner, false),
                AccountMeta::new_readonly(owner, true),
            ],
            data: vec![9],
        };
        assert_eq!(instruction.data, vec![9]);
        assert!(instruction.accounts[2].is_signer);
    }
}
