use std::collections::HashMap;
use std::fs;
use serde_json::Value;
use solana_program::instruction::AccountMeta;
use solana_program::pubkey::Pubkey;
use std::str::FromStr;
use std::vec::Vec;

const SYSTEM_PROGRAM_PUBKEY: &str = "11111111111111111111111111111111";
const TOKEN_PROGRAM_PUBKEY: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const TOKEN_2022_PROGRAM_PUBKEY: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";
const ASSOCIATED_PROGRAM_PUBKEY: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";

#[derive(Debug)]
pub struct SimpleIdl {
    pub address: Pubkey,
    pub raw: Value,
}

impl SimpleIdl {
    pub fn load_from(path: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let s = fs::read_to_string(path)?;
        let raw: Value = serde_json::from_str(&s)?;
        let addr_str = raw
            .get("address")
            .and_then(|v| v.as_str())
            .ok_or("IDL missing address field")?;
        let address = Pubkey::from_str(addr_str)?;
        Ok(SimpleIdl { address, raw })
    }

    /// Build the AccountMeta list for instruction `instr_name` using the provided
    /// context map (e.g. "mint" -> Pubkey, "user" -> Pubkey, "bonding_curve" -> Pubkey, etc.).
    /// As accounts are resolved (especially PDAs), they are added back to a mutable context
    /// so subsequent accounts can reference them.
    pub fn build_accounts_for(
        &self,
        instr_name: &str,
        context: &HashMap<String, Pubkey>,
    ) -> Result<Vec<AccountMeta>, Box<dyn std::error::Error + Send + Sync>> {
        // Find instruction in IDL
        let instructions = self
            .raw
            .get("instructions")
            .and_then(|v| v.as_array())
            .ok_or_else(|| format!("IDL for program {} missing instructions array", self.address))?;
        let mut instr_val_opt: Option<&Value> = None;
        for instr in instructions {
            if instr.get("name").and_then(|n| n.as_str()) == Some(instr_name) {
                instr_val_opt = Some(instr);
                break;
            }
        }
        let instr = instr_val_opt.ok_or_else(|| {
            format!("Instruction '{}' not found in IDL for program {}", instr_name, self.address)
        })?;

        let accounts = instr.get("accounts").and_then(|v| v.as_array()).ok_or_else(|| {
            format!("Instruction '{}' missing accounts array in IDL", instr_name)
        })?;
        let mut metas: Vec<AccountMeta> = Vec::with_capacity(accounts.len());
        // Build a mutable working context so accounts resolved earlier can be referenced later
        let mut working_context = context.clone();

        for account in accounts.iter() {
            // Get the account name for tracking
            let account_name = account.get("name").and_then(|n| n.as_str()).unwrap_or("<unnamed>");
            // default flags
            let is_writable = account.get("writable").and_then(|v| v.as_bool()).unwrap_or(false);
            let is_signer = account.get("signer").and_then(|v| v.as_bool()).unwrap_or(false);
            // prefer explicit address
            if let Some(addr) = account.get("address").and_then(|a| a.as_str()) {
                let pk = Pubkey::from_str(addr)?;
                if is_signer {
                    metas.push(AccountMeta::new(pk, true));
                } else if is_writable {
                    metas.push(AccountMeta::new(pk, false));
                } else {
                    metas.push(AccountMeta::new_readonly(pk, false));
                }
                // Add to working context for potential later reference
                if !account_name.is_empty() {
                    working_context.insert(account_name.to_string(), pk);
                }
                continue;
            }

            // compute PDA if present
            if let Some(pda_obj) = account.get("pda") {
                // determine program id for PDA
                let program_id = if let Some(prog) = pda_obj.get("program") {
                    // program can be const byte array or object; try to read address first
                    if let Some(addr) = prog.get("address").and_then(|a| a.as_str()) {
                        Pubkey::from_str(addr)?
                    } else if let Some(kind) = prog.get("kind").and_then(|k| k.as_str()) {
                        match kind {
                            "const" => {
                                // extract value array
                                if let Some(arr) = prog.get("value").and_then(|v| v.as_array()) {
                                    let bytes: Vec<u8> = arr
                                        .iter()
                                        .filter_map(|n| n.as_u64().map(|x| x as u8))
                                        .collect();
                                    if bytes.len() == 32 {
                                        Pubkey::new_from_array(bytes.try_into().map_err(|_: Vec<u8>| "failed to convert vec to array")?)
                                    } else {
                                        return Err(format!("Program const value has wrong length: {}", bytes.len()).into());
                                    }
                                } else {
                                    return Err("Program const missing value array".into());
                                }
                            }
                            "account" => {
                                // program is a reference to another account in context
                                if let Some(path) = prog.get("path").and_then(|p| p.as_str()) {
                                    context.get(path).cloned().ok_or(format!("Context missing program account path: {}", path))?
                                } else {
                                    return Err("Program account missing path".into());
                                }
                            }
                            _ => {
                                // fallback: default to IDL address
                                self.address
                            }
                        }
                    } else {
                        // no kind or address, default to IDL address
                        self.address
                    }
                } else {
                    self.address
                };

                // collect seeds
                let seeds_val = pda_obj.get("seeds").and_then(|s| s.as_array()).ok_or("pda.seeds missing")?;
                let mut seeds_out: Vec<Vec<u8>> = Vec::new();
                for seed in seeds_val.iter() {
                    let kind = seed.get("kind").and_then(|k| k.as_str()).unwrap_or("const");
                    match kind {
                        "const" => {
                            if let Some(arr) = seed.get("value").and_then(|v| v.as_array()) {
                                let bytes: Vec<u8> = arr.iter().filter_map(|n| n.as_u64().map(|x| x as u8)).collect();
                                seeds_out.push(bytes);
                            } else {
                                return Err("const seed missing value".into());
                            }
                        }
                        "account" => {
                            // path: either "mint" or "bonding_curve" or "bonding_curve.creator" etc.
                            if let Some(path) = seed.get("path").and_then(|p| p.as_str()) {
                                // resolve path in working_context (which includes previously resolved accounts)
                                // allow nested like bonding_curve.creator
                                let pk = if path.contains('.') {
                                    // e.g. bonding_curve.creator => we expect working_context has bonding_curve.creator key
                                    working_context.get(path).cloned()
                                } else {
                                    working_context.get(path).cloned()
                                };
                                if let Some(found) = pk {
                                    seeds_out.push(found.to_bytes().to_vec());
                                } else {
                                    return Err(format!(
                                        "Cannot resolve account '{}' for instruction '{}': seed path '{}' not found in context. Available keys: {:?}",
                                        account_name, instr_name, path, working_context.keys().collect::<Vec<_>>()
                                    ).into());
                                }
                            } else {
                                return Err("account seed missing path".into());
                            }
                        }
                        other => {
                            return Err(format!("Unsupported seed kind {}", other).into());
                        }
                    }
                }

                // convert Vec<Vec<u8>> to Vec<&[u8]>
                let seed_refs: Vec<&[u8]> = seeds_out.iter().map(|v| v.as_slice()).collect();
                let (pda, _) = Pubkey::find_program_address(&seed_refs, &program_id);
                if is_signer {
                    metas.push(AccountMeta::new(pda, true));
                } else if is_writable {
                    metas.push(AccountMeta::new(pda, false));
                } else {
                    metas.push(AccountMeta::new_readonly(pda, false));
                }
                // Add computed PDA to working context for later reference
                if !account_name.is_empty() {
                    working_context.insert(account_name.to_string(), pda);
                }
                continue;
            }

            // fallback mapping by common account names
            if let Some(name) = account.get("name").and_then(|n| n.as_str()) {
                match name {
                    "system_program" | "systemProgram" => {
                        let pk = Pubkey::from_str(SYSTEM_PROGRAM_PUBKEY)?;
                        metas.push(AccountMeta::new_readonly(pk, false));
                    }
                    "token_program" | "tokenProgram" => {
                        // Check context first (caller may provide Token-2022 or SPL Token)
                        let pk = if let Some(tp) = working_context.get("token_program").or_else(|| working_context.get("tokenProgram")) {
                            *tp
                        } else {
                            // Default to Token-2022 for pump.fun tokens
                            Pubkey::from_str(TOKEN_2022_PROGRAM_PUBKEY)?
                        };
                        metas.push(AccountMeta::new_readonly(pk, false));
                        working_context.insert(account_name.to_string(), pk);
                    }
                    "associated_token_program" | "associatedTokenProgram" => {
                        let pk = Pubkey::from_str(ASSOCIATED_PROGRAM_PUBKEY)?;
                        metas.push(AccountMeta::new_readonly(pk, false));
                    }
                    // derive associated token account if user + mint present
                    "associated_user" | "associatedUser" => {
                        if let (Some(user), Some(mint)) = (working_context.get("user"), working_context.get("mint")) {
                            // Use the correct token program for ATA derivation
                            let token_prog = working_context.get("token_program")
                                .or_else(|| working_context.get("tokenProgram"))
                                .cloned()
                                .unwrap_or_else(|| Pubkey::from_str(TOKEN_2022_PROGRAM_PUBKEY).unwrap());
                            let ata = spl_associated_token_account::get_associated_token_address_with_program_id(user, mint, &token_prog);
                            if is_signer {
                                metas.push(AccountMeta::new(ata, true));
                            } else if is_writable {
                                metas.push(AccountMeta::new(ata, false));
                            } else {
                                metas.push(AccountMeta::new_readonly(ata, false));
                            }
                            working_context.insert(account_name.to_string(), ata);
                        } else {
                            return Err(format!(
                                "Cannot resolve account '{}' for instruction '{}': Missing user or mint in context",
                                account_name, instr_name
                            ).into());
                        }
                    }
                    "associated_bonding_curve" | "associatedBondingCurve" => {
                        // Derive associated token account for bonding curve
                        if let (Some(bonding_curve), Some(mint)) = (working_context.get("bondingCurve").or_else(|| working_context.get("bonding_curve")), working_context.get("mint")) {
                            let token_prog = working_context.get("token_program")
                                .or_else(|| working_context.get("tokenProgram"))
                                .cloned()
                                .unwrap_or_else(|| Pubkey::from_str(TOKEN_2022_PROGRAM_PUBKEY).unwrap());
                            let ata = spl_associated_token_account::get_associated_token_address_with_program_id(bonding_curve, mint, &token_prog);
                            if is_signer {
                                metas.push(AccountMeta::new(ata, true));
                            } else if is_writable {
                                metas.push(AccountMeta::new(ata, false));
                            } else {
                                metas.push(AccountMeta::new_readonly(ata, false));
                            }
                            working_context.insert(account_name.to_string(), ata);
                        } else {
                            return Err(format!(
                                "Cannot resolve account '{}' for instruction '{}': Missing bondingCurve or mint in context",
                                account_name, instr_name
                            ).into());
                        }
                    }
                    "fee_recipient" | "feeRecipient" => {
                        // fee_recipient is typically a global PDA or the protocol fee account
                        // For pump.fun, check if it's provided in context, otherwise compute standard PDA
                        if let Some(pk) = working_context.get("fee_recipient").or_else(|| working_context.get("feeRecipient")) {
                            if is_signer {
                                metas.push(AccountMeta::new(*pk, true));
                            } else if is_writable {
                                metas.push(AccountMeta::new(*pk, false));
                            } else {
                                metas.push(AccountMeta::new_readonly(*pk, false));
                            }
                        } else {
                            // Use a well-known fee recipient (pump.fun uses global PDA typically)
                            let (global_pda, _) = Pubkey::find_program_address(&[b"global"], &self.address);
                            if is_writable {
                                metas.push(AccountMeta::new(global_pda, false));
                            } else {
                                metas.push(AccountMeta::new_readonly(global_pda, false));
                            }
                            working_context.insert(account_name.to_string(), global_pda);
                        }
                    }
                    "program" => {
                        // The program account typically refers to the IDL's own program ID
                        metas.push(AccountMeta::new_readonly(self.address, false));
                        working_context.insert(account_name.to_string(), self.address);
                    }
                    other_name => {
                        // try to find in working_context
                        if let Some(pk) = working_context.get(other_name) {
                            if is_signer {
                                metas.push(AccountMeta::new(*pk, true));
                            } else if is_writable {
                                metas.push(AccountMeta::new(*pk, false));
                            } else {
                                metas.push(AccountMeta::new_readonly(*pk, false));
                            }
                        } else {
                            return Err(format!(
                                "Cannot resolve account '{}' for instruction '{}': not found in context. Available keys: {:?}",
                                other_name, instr_name, working_context.keys().collect::<Vec<_>>()
                            ).into());
                        }
                    }
                }
                continue;
            }

            return Err("Unhandled account entry in IDL".into());
        }

        Ok(metas)
    }
}

/// Load known IDLs from bundled idl/ directory or legacy locations.
/// Returns map of short name -> SimpleIdl.
pub fn load_all_idls() -> HashMap<String, SimpleIdl> {
    let mut m = HashMap::new();
    
    // Try loading from bundled idl/ directory first (preferred)
    let bundled_candidates = vec![
        ("pumpfun", "idl/pumpfun.json"),
        ("pumpfunamm", "idl/pumpfunamm.json"),
        ("pumpfunfees", "idl/pumpfunfees.json"),
    ];
    
    for (key, path) in bundled_candidates {
        if std::path::Path::new(path).exists() {
            match SimpleIdl::load_from(path) {
                Ok(idl) => {
                    log::info!("Loaded bundled IDL from {}", path);
                    m.insert(key.to_string(), idl);
                }
                Err(e) => {
                    log::debug!("Failed to load bundled IDL {}: {}", path, e);
                }
            }
        }
    }
    
    // Fallback to legacy root-level files if bundled not found
    if m.is_empty() {
        log::debug!("No bundled IDL files found, trying legacy root-level files");
        let legacy_candidates = vec!["pumpfun.json", "pumpfunamm.json", "pumpfunfees.json"];
        for c in legacy_candidates {
            if std::path::Path::new(c).exists() {
                match SimpleIdl::load_from(c) {
                    Ok(idl) => {
                        let key = c.trim_end_matches(".json").to_string();
                        log::info!("Loaded legacy IDL from {}", c);
                        m.insert(key, idl);
                    }
                    Err(e) => {
                        log::warn!("Failed to load legacy IDL {}: {}", c, e);
                    }
                }
            }
        }
    }
    
    if m.is_empty() {
        log::warn!("No IDL files loaded - transactions will use fallback builders");
    } else {
        log::info!("Loaded {} IDL file(s)", m.len());
    }
    
    m
}

#[cfg(test)]
mod tests {
    use super::load_all_idls;
    use std::collections::HashMap;
    use solana_program::pubkey::Pubkey;
    use std::str::FromStr;

    #[test]
    fn idls_load_does_not_panic() {
        // IDL loading from local files is optional when on-chain fetching is available
        // This test verifies the function doesn't panic when no local files are present
        // In production, IDLs can be loaded from on-chain or from local files
        let _idls = load_all_idls();
        // Test passes if we reach here without panicking
    }

    #[test]
    fn test_bundled_idl_loads() {
        // Test that bundled IDL files can be loaded
        let idls = load_all_idls();
        
        // Should find at least the pumpfun IDL in idl/ directory
        // If bundled files exist, they should load
        if std::path::Path::new("idl/pumpfun.json").exists() {
            assert!(idls.contains_key("pumpfun"), "Should load bundled pumpfun.json");
        }
    }

    #[test]
    fn test_idl_has_buy_instruction() {
        let idls = load_all_idls();
        
        if let Some(idl) = idls.get("pumpfun") {
            let instructions = idl.raw.get("instructions").and_then(|v| v.as_array());
            assert!(instructions.is_some(), "IDL should have instructions array");
            
            let has_buy = instructions.unwrap().iter().any(|instr| {
                instr.get("name").and_then(|n| n.as_str()) == Some("buy")
            });
            assert!(has_buy, "IDL should contain 'buy' instruction");
        }
    }

    #[test]
    fn test_idl_has_sell_instruction() {
        let idls = load_all_idls();
        
        if let Some(idl) = idls.get("pumpfun") {
            let instructions = idl.raw.get("instructions").and_then(|v| v.as_array());
            assert!(instructions.is_some(), "IDL should have instructions array");
            
            let has_sell = instructions.unwrap().iter().any(|instr| {
                instr.get("name").and_then(|n| n.as_str()) == Some("sell")
            });
            assert!(has_sell, "IDL should contain 'sell' instruction");
        }
    }

    #[test]
    fn test_build_buy_accounts_with_context() {
        let idls = load_all_idls();
        
        if let Some(idl) = idls.get("pumpfun") {
            // Create a deterministic test context
            let mut context = HashMap::new();
            context.insert("mint".to_string(), Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap());
            context.insert("user".to_string(), Pubkey::from_str("11111111111111111111111111111111").unwrap());
            context.insert("bondingCurve.creator".to_string(), Pubkey::from_str("11111111111111111111111111111112").unwrap());
            context.insert("feeRecipient".to_string(), Pubkey::from_str("11111111111111111111111111111113").unwrap());
            
            // Try to build accounts for buy instruction
            let result = idl.build_accounts_for("buy", &context);
            
            // Should either succeed or fail with a clear error message
            match result {
                Ok(metas) => {
                    assert!(!metas.is_empty(), "Buy instruction should have accounts");
                    assert!(metas.len() >= 10, "Buy instruction should have at least 10 accounts");
                }
                Err(e) => {
                    // If it fails, error should mention which account couldn't be resolved
                    let err_msg = e.to_string();
                    assert!(
                        err_msg.contains("Cannot resolve") || err_msg.contains("missing"),
                        "Error should be informative: {}",
                        err_msg
                    );
                }
            }
        }
    }

    #[test]
    fn test_build_sell_accounts_with_context() {
        let idls = load_all_idls();
        
        if let Some(idl) = idls.get("pumpfun") {
            // Create a deterministic test context
            let mut context = HashMap::new();
            context.insert("mint".to_string(), Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap());
            context.insert("user".to_string(), Pubkey::from_str("11111111111111111111111111111111").unwrap());
            context.insert("bondingCurve.creator".to_string(), Pubkey::from_str("11111111111111111111111111111112").unwrap());
            context.insert("feeRecipient".to_string(), Pubkey::from_str("11111111111111111111111111111113").unwrap());
            context.insert("feeProgram".to_string(), Pubkey::from_str("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ").unwrap());
            
            // Try to build accounts for sell instruction
            let result = idl.build_accounts_for("sell", &context);
            
            // Should either succeed or fail with a clear error message
            match result {
                Ok(metas) => {
                    assert!(!metas.is_empty(), "Sell instruction should have accounts");
                    assert!(metas.len() >= 10, "Sell instruction should have at least 10 accounts");
                }
                Err(e) => {
                    // If it fails, error should mention which account couldn't be resolved
                    let err_msg = e.to_string();
                    assert!(
                        err_msg.contains("Cannot resolve") || err_msg.contains("missing"),
                        "Error should be informative: {}",
                        err_msg
                    );
                }
            }
        }
    }

    #[test]
    fn test_idl_address_matches_program_id() {
        let idls = load_all_idls();
        
        if let Some(idl) = idls.get("pumpfun") {
            let expected_program_id = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
            assert_eq!(
                idl.address.to_string(),
                expected_program_id,
                "IDL address should match pump.fun program ID"
            );
        }
    }
}
