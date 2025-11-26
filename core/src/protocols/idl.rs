use serde_json::Value;
use std::collections::HashMap;
use solana_program::pubkey::Pubkey;
use solana_program::instruction::AccountMeta;
use std::str::FromStr;
use std::vec::Vec;
use crate::core::error::CoreError;

const SYSTEM_PROGRAM_PUBKEY: &str = "11111111111111111111111111111111";
const TOKEN_PROGRAM_PUBKEY: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const ASSOCIATED_PROGRAM_PUBKEY: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";

#[derive(Debug, Clone)]
pub struct SimpleIdl {
    pub address: Pubkey,
    pub raw: Value,
}

impl SimpleIdl {
    pub fn from_value(raw: Value) -> Result<Self, CoreError> {
        let addr_str = raw
            .get("address")
            .and_then(|v| v.as_str())
            .ok_or(CoreError::Config("IDL missing address field".to_string()))?;
        let address = Pubkey::from_str(addr_str).map_err(|e| CoreError::Parse(e.to_string()))?;
        Ok(SimpleIdl { address, raw })
    }

    pub fn from_str(s: &str) -> Result<Self, CoreError> {
        let raw: Value = serde_json::from_str(s).map_err(|e| CoreError::Serialization(e.to_string()))?;
        Self::from_value(raw)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_from(path: &str) -> Result<Self, CoreError> {
        let s = std::fs::read_to_string(path).map_err(|e| CoreError::Storage(e.to_string()))?;
        Self::from_str(&s)
    }

    /// Build the AccountMeta list for instruction `instr_name` using the provided
    /// context map (e.g. "mint" -> Pubkey, "user" -> Pubkey, "bonding_curve" -> Pubkey, etc.).
    /// As accounts are resolved (especially PDAs), they are added back to a mutable context
    /// so subsequent accounts can reference them.
    pub fn build_accounts_for(
        &self,
        instr_name: &str,
        context: &HashMap<String, Pubkey>,
    ) -> Result<Vec<AccountMeta>, CoreError> {
        let instructions = self
            .raw
            .get("instructions")
            .and_then(|v| v.as_array())
            .ok_or(CoreError::Config("IDL missing instructions array".to_string()))?;
        let mut instr_val_opt: Option<&Value> = None;
        for instr in instructions {
            if instr.get("name").and_then(|n| n.as_str()) == Some(instr_name) {
                instr_val_opt = Some(instr);
                break;
            }
        }
        let instr = instr_val_opt.ok_or(CoreError::Config(format!("Instruction {} not found in IDL", instr_name)))?;

        let accounts = instr.get("accounts").and_then(|v| v.as_array()).ok_or(CoreError::Config("accounts missing".to_string()))?;
        let mut metas: Vec<AccountMeta> = Vec::with_capacity(accounts.len());
        let mut working_context = context.clone();

        for account in accounts.iter() {
            let account_name = account.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let is_writable = account.get("writable").and_then(|v| v.as_bool()).unwrap_or(false);
            let is_signer = account.get("signer").and_then(|v| v.as_bool()).unwrap_or(false);
            if let Some(addr) = account.get("address").and_then(|a| a.as_str()) {
                let pk = Pubkey::from_str(addr).map_err(|e| CoreError::Parse(e.to_string()))?;
                if is_signer {
                    metas.push(AccountMeta::new(pk, true));
                } else if is_writable {
                    metas.push(AccountMeta::new(pk, false));
                } else {
                    metas.push(AccountMeta::new_readonly(pk, false));
                }
                if !account_name.is_empty() {
                    working_context.insert(account_name.to_string(), pk);
                }
                continue;
            }
            if let Some(pda_obj) = account.get("pda") {
                let program_id = if let Some(prog) = pda_obj.get("program") {
                    if let Some(addr) = prog.get("address").and_then(|a| a.as_str()) {
                        Pubkey::from_str(addr).map_err(|e| CoreError::Parse(e.to_string()))?
                    } else if let Some(kind) = prog.get("kind").and_then(|k| k.as_str()) {
                        match kind {
                            "const" => {
                                if let Some(arr) = prog.get("value").and_then(|v| v.as_array()) {
                                    let bytes: Vec<u8> = arr
                                        .iter()
                                        .filter_map(|n| n.as_u64().map(|x| x as u8))
                                        .collect();
                                    if bytes.len() == 32 {
                                        Pubkey::new_from_array(bytes.try_into().map_err(|_: Vec<u8>| CoreError::Parse("failed to convert vec to array".to_string()))?)
                                    } else {
                                        return Err(CoreError::Config(format!("Program const value has wrong length: {}", bytes.len())));
                                    }
                                } else {
                                    return Err(CoreError::Config("Program const missing value array".to_string()));
                                }
                            }
                            "account" => {
                                if let Some(path) = prog.get("path").and_then(|p| p.as_str()) {
                                    working_context.get(path).cloned().ok_or(CoreError::Config(format!("Context missing program account path: {}", path)))?
                                } else {
                                    return Err(CoreError::Config("Program account missing path".to_string()));
                                }
                            }
                            _ => self.address,
                        }
                    } else {
                        self.address
                    }
                } else {
                    self.address
                };

                let seeds_val = pda_obj.get("seeds").and_then(|s| s.as_array()).ok_or(CoreError::Config("pda.seeds missing".to_string()))?;
                let mut seeds_out: Vec<Vec<u8>> = Vec::new();
                for seed in seeds_val.iter() {
                    let kind = seed.get("kind").and_then(|k| k.as_str()).unwrap_or("const");
                    match kind {
                        "const" => {
                            if let Some(arr) = seed.get("value").and_then(|v| v.as_array()) {
                                let bytes: Vec<u8> = arr.iter().filter_map(|n| n.as_u64().map(|x| x as u8)).collect();
                                seeds_out.push(bytes);
                            } else {
                                return Err(CoreError::Config("const seed missing value".to_string()));
                            }
                        }
                        "account" => {
                            if let Some(path) = seed.get("path").and_then(|p| p.as_str()) {
                                let pk = working_context.get(path).cloned();
                                if let Some(found) = pk {
                                    seeds_out.push(found.to_bytes().to_vec());
                                } else {
                                    return Err(CoreError::Config(format!("Context missing key for seed path {} (account name: {})", path, account_name)));
                                }
                            } else {
                                return Err(CoreError::Config("account seed missing path".to_string()));
                            }
                        }
                        other => return Err(CoreError::Config(format!("Unsupported seed kind {}", other))),
                    }
                }
                let seed_refs: Vec<&[u8]> = seeds_out.iter().map(|v| v.as_slice()).collect();
                let (pda, _) = Pubkey::find_program_address(&seed_refs, &program_id);
                if is_signer {
                    metas.push(AccountMeta::new(pda, true));
                } else if is_writable {
                    metas.push(AccountMeta::new(pda, false));
                } else {
                    metas.push(AccountMeta::new_readonly(pda, false));
                }
                if !account_name.is_empty() {
                    working_context.insert(account_name.to_string(), pda);
                }
                continue;
            }

            if let Some(name) = account.get("name").and_then(|n| n.as_str()) {
                match name {
                    "system_program" => {
                        let pk = Pubkey::from_str(SYSTEM_PROGRAM_PUBKEY).map_err(|e| CoreError::Parse(e.to_string()))?;
                        metas.push(AccountMeta::new_readonly(pk, false));
                    }
                    "token_program" => {
                        let pk = Pubkey::from_str(TOKEN_PROGRAM_PUBKEY).map_err(|e| CoreError::Parse(e.to_string()))?;
                        metas.push(AccountMeta::new_readonly(pk, false));
                    }
                    "associated_token_program" => {
                        let pk = Pubkey::from_str(ASSOCIATED_PROGRAM_PUBKEY).map_err(|e| CoreError::Parse(e.to_string()))?;
                        metas.push(AccountMeta::new_readonly(pk, false));
                    }
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
                            return Err(CoreError::Config("Missing user or mint to derive associated_user".to_string()));
                        }
                    }
                    "fee_recipient" => {
                        if let Some(pk) = working_context.get("fee_recipient") {
                            if is_signer {
                                metas.push(AccountMeta::new(*pk, true));
                            } else if is_writable {
                                metas.push(AccountMeta::new(*pk, false));
                            } else {
                                metas.push(AccountMeta::new_readonly(*pk, false));
                            }
                        } else {
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
                        metas.push(AccountMeta::new_readonly(self.address, false));
                        working_context.insert(account_name.to_string(), self.address);
                    }
                    other_name => {
                        if let Some(pk) = working_context.get(other_name) {
                            if is_signer {
                                metas.push(AccountMeta::new(*pk, true));
                            } else if is_writable {
                                metas.push(AccountMeta::new(*pk, false));
                            } else {
                                metas.push(AccountMeta::new_readonly(*pk, false));
                            }
                        } else {
                            return Err(CoreError::Config(format!("Cannot resolve account name {} in IDL to a pubkey (missing in context)", other_name)));
                        }
                    }
                }
                continue;
            }
            return Err(CoreError::Config("Unhandled account entry in IDL".to_string()));
        }
        Ok(metas)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_all_idls() -> Result<HashMap<String, SimpleIdl>, CoreError> {
    use std::path::Path;
    let mut m = HashMap::new();
    let candidates = vec!["pumpfun.json", "pumpfunamm.json", "pumpfunfees.json"];
    for c in candidates {
        let candidate_paths = vec![
            c.to_string(),
            format!("./{}", c),
            format!("../{}", c),
            format!("pumpfun/{}", c),
            format!("src/protocols/pumpfun/{}", c),
            format!("sol_beast_protocols/pumpfun/{}", c),
            format!("../sol_beast_protocols/pumpfun/{}", c),
        ];
        let mut path = None;
        for p in candidate_paths.iter() {
            if Path::new(p).exists() {
                path = Some(p.clone());
                break;
            }
        }
        let path = match path {
            Some(p) => p,
            None => {
                log::debug!("Could not find candidate idl {} in default paths", c);
                c.to_string()
            }
        };
        if Path::new(&path).exists() {
            match SimpleIdl::load_from(&path) {
                Ok(idl) => {
                    let key = Path::new(c).file_stem().unwrap().to_str().unwrap().to_string();
                    m.insert(key, idl);
                }
                Err(e) => {
                    log::warn!("Failed to load IDL {}: {}", path, e);
                }
            }
        }
    }
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idls_load() {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let idls = load_all_idls().unwrap();
            assert!(!idls.is_empty());
        }
    }
}
