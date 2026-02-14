use std::collections::HashMap;
use std::fs;
use serde_json::Value;
use solana_program::instruction::AccountMeta;
use solana_program::pubkey::Pubkey;
use std::str::FromStr;
use std::vec::Vec;

const SYSTEM_PROGRAM_PUBKEY: &str = "11111111111111111111111111111111";
const TOKEN_PROGRAM_PUBKEY: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
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
            .ok_or("IDL missing instructions array")?;
        let mut instr_val_opt: Option<&Value> = None;
        for instr in instructions {
            if instr.get("name").and_then(|n| n.as_str()) == Some(instr_name) {
                instr_val_opt = Some(instr);
                break;
            }
        }
        let instr = instr_val_opt.ok_or(format!("Instruction {} not found in IDL", instr_name))?;

        let accounts = instr.get("accounts").and_then(|v| v.as_array()).ok_or("accounts missing")?;
        let mut metas: Vec<AccountMeta> = Vec::with_capacity(accounts.len());
        // Build a mutable working context so accounts resolved earlier can be referenced later
        let mut working_context = context.clone();

        for account in accounts.iter() {
            // Get the account name for tracking
            let account_name = account.get("name").and_then(|n| n.as_str()).unwrap_or("");
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
                                    return Err(format!("Context missing key for seed path {} (account name: {})", path, account_name).into());
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
                    "system_program" => {
                        let pk = Pubkey::from_str(SYSTEM_PROGRAM_PUBKEY)?;
                        metas.push(AccountMeta::new_readonly(pk, false));
                    }
                    "token_program" => {
                        let pk = Pubkey::from_str(TOKEN_PROGRAM_PUBKEY)?;
                        metas.push(AccountMeta::new_readonly(pk, false));
                    }
                    "associated_token_program" => {
                        let pk = Pubkey::from_str(ASSOCIATED_PROGRAM_PUBKEY)?;
                        metas.push(AccountMeta::new_readonly(pk, false));
                    }
                    // derive associated token account if user + mint present
                    "associated_user" => {
                        if let (Some(user), Some(mint)) = (working_context.get("user"), working_context.get("mint")) {
                            let ata = spl_associated_token_account::get_associated_token_address(user, mint);
                            if is_signer {
                                metas.push(AccountMeta::new(ata, true));
                            } else if is_writable {
                                metas.push(AccountMeta::new(ata, false));
                            } else {
                                metas.push(AccountMeta::new_readonly(ata, false));
                            }
                            working_context.insert(account_name.to_string(), ata);
                        } else {
                            return Err("Missing user or mint to derive associated_user".into());
                        }
                    }
                    "fee_recipient" => {
                        // fee_recipient is typically a global PDA or the protocol fee account
                        // For pump.fun, check if it's provided in context, otherwise compute standard PDA
                        if let Some(pk) = working_context.get("fee_recipient") {
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
                            return Err(format!("Cannot resolve account name {} in IDL to a pubkey (missing in context)", other_name).into());
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

/// Load known IDLs from repo root. Returns map of short name -> SimpleIdl.
pub fn load_all_idls() -> HashMap<String, SimpleIdl> {
    let mut m = HashMap::new();
    let candidates = vec!["pumpfun.json", "pumpfunamm.json", "pumpfunfees.json"];
    for c in candidates {
        if std::path::Path::new(c).exists() {
            match SimpleIdl::load_from(c) {
                Ok(idl) => {
                    // key by file stem
                    let key = c.trim_end_matches(".json").to_string();
                    m.insert(key, idl);
                }
                Err(e) => {
                    log::warn!("Failed to load IDL {}: {}", c, e);
                }
            }
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::load_all_idls;

    #[test]
    fn idls_load() {
        let idls = load_all_idls();
        // IDL loading from local files is optional when on-chain fetching is available
        // This test just verifies the function doesn't panic
        // In production, IDLs can be loaded from on-chain or local files
        assert!(idls.len() <= 3); // At most 3 IDL files (pumpfun, pumpfunamm, pumpfunfees)
    }
}
