use crate::{idl::SimpleIdl, settings::Settings};
use log::{debug, info, warn};
use serde_json::Value;
use solana_client::rpc_client::RpcClient;
use solana_program::pubkey::Pubkey;
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

/// In-memory cache for fetched IDLs (program_id -> SimpleIdl)
pub type IdlCache = Arc<Mutex<HashMap<String, SimpleIdl>>>;

/// Directory for disk-cached IDL files
const IDL_CACHE_DIR: &str = ".idl_cache";

/// Compute the Anchor-style IDL account address for a program
/// Anchor IDL accounts use the PDA: ["anchor:idl", program_id]
pub fn derive_idl_account(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"anchor:idl", program_id.as_ref()], program_id)
}

/// Fetch IDL account data from on-chain via RPC
/// Returns the raw account data bytes if the account exists
pub async fn fetch_idl_account_data(
    idl_account: &Pubkey,
    rpc_client: &RpcClient,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    debug!("Fetching IDL account data for {}", idl_account);
    
    match rpc_client.get_account_data(idl_account) {
        Ok(data) => {
            debug!("Fetched {} bytes from IDL account {}", data.len(), idl_account);
            Ok(data)
        }
        Err(e) => {
            Err(format!("Failed to fetch IDL account {}: {}", idl_account, e).into())
        }
    }
}

/// Decompress zlib-compressed IDL data
/// Anchor IDL accounts typically store the IDL as zlib-compressed JSON
/// The account data format is: [8 bytes authority | 4 bytes length | compressed data]
pub fn decompress_idl_data(
    data: &[u8],
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    // Anchor IDL account layout:
    // - First 8 bytes: authority pubkey (or marker)
    // - Next 4 bytes: compressed data length (u32 LE)
    // - Remaining: zlib-compressed JSON
    
    if data.len() < 12 {
        return Err("IDL account data too short (< 12 bytes)".into());
    }
    
    // Skip first 8 bytes (authority), read next 4 bytes as length
    let compressed_start = 12;
    let compressed_data = &data[compressed_start..];
    
    debug!("Decompressing {} bytes of IDL data", compressed_data.len());
    
    // Use flate2 to decompress zlib data
    let mut decoder = flate2::read::ZlibDecoder::new(compressed_data);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed)?;
    
    debug!("Decompressed to {} bytes", decompressed.len());
    Ok(decompressed)
}

/// Parse decompressed IDL JSON into SimpleIdl
pub fn parse_idl_json(
    json_bytes: &[u8],
    program_id: Pubkey,
) -> Result<SimpleIdl, Box<dyn std::error::Error + Send + Sync>> {
    let json_str = std::str::from_utf8(json_bytes)?;
    let mut raw: Value = serde_json::from_str(json_str)?;
    
    // Inject the program address if not present
    if raw.get("address").is_none() {
        if let Value::Object(ref mut map) = raw {
            map.insert("address".to_string(), Value::String(program_id.to_string()));
        }
    }
    
    Ok(SimpleIdl {
        address: program_id,
        raw,
    })
}

/// Fetch IDL from on-chain for a given program ID
pub async fn fetch_onchain_idl(
    program_id: &Pubkey,
    rpc_client: &RpcClient,
) -> Result<SimpleIdl, Box<dyn std::error::Error + Send + Sync>> {
    info!("Fetching on-chain IDL for program {}", program_id);
    
    let (idl_account, _bump) = derive_idl_account(program_id);
    debug!("Derived IDL account: {}", idl_account);
    
    let account_data = fetch_idl_account_data(&idl_account, rpc_client).await?;
    let decompressed = decompress_idl_data(&account_data)?;
    let idl = parse_idl_json(&decompressed, *program_id)?;
    
    info!("Successfully fetched on-chain IDL for program {}", program_id);
    Ok(idl)
}

/// Load IDL from disk cache
pub fn load_idl_from_disk_cache(
    program_id: &Pubkey,
) -> Result<SimpleIdl, Box<dyn std::error::Error + Send + Sync>> {
    let cache_path = format!("{}/{}.json", IDL_CACHE_DIR, program_id);
    
    if !Path::new(&cache_path).exists() {
        return Err(format!("IDL cache file not found: {}", cache_path).into());
    }
    
    debug!("Loading IDL from disk cache: {}", cache_path);
    SimpleIdl::load_from(&cache_path)
}

/// Save IDL to disk cache
pub fn save_idl_to_disk_cache(
    idl: &SimpleIdl,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Create cache directory if it doesn't exist
    std::fs::create_dir_all(IDL_CACHE_DIR)?;
    
    let cache_path = format!("{}/{}.json", IDL_CACHE_DIR, idl.address);
    debug!("Saving IDL to disk cache: {}", cache_path);
    
    let json_str = serde_json::to_string_pretty(&idl.raw)?;
    std::fs::write(&cache_path, json_str)?;
    
    Ok(())
}

/// Load IDL with multi-tier fallback:
/// 1. Check in-memory cache
/// 2. Try disk cache
/// 3. Fetch from on-chain
/// 4. Save to caches if fetched
pub async fn load_idl_with_cache(
    program_id: &Pubkey,
    rpc_client: &RpcClient,
    cache: &IdlCache,
    use_onchain: bool,
) -> Result<SimpleIdl, Box<dyn std::error::Error + Send + Sync>> {
    let program_str = program_id.to_string();
    
    // Check in-memory cache first
    {
        let cache_lock = cache.lock().unwrap();
        if let Some(idl) = cache_lock.get(&program_str) {
            debug!("IDL for {} found in memory cache", program_id);
            return Ok(SimpleIdl {
                address: idl.address,
                raw: idl.raw.clone(),
            });
        }
    }
    
    // Try disk cache
    if let Ok(idl) = load_idl_from_disk_cache(program_id) {
        debug!("IDL for {} loaded from disk cache", program_id);
        // Update in-memory cache
        let mut cache_lock = cache.lock().unwrap();
        cache_lock.insert(program_str.clone(), SimpleIdl {
            address: idl.address,
            raw: idl.raw.clone(),
        });
        return Ok(idl);
    }
    
    // Fetch from on-chain if enabled
    if use_onchain {
        match fetch_onchain_idl(program_id, rpc_client).await {
            Ok(idl) => {
                // Save to both caches
                if let Err(e) = save_idl_to_disk_cache(&idl) {
                    warn!("Failed to save IDL to disk cache: {}", e);
                }
                
                let mut cache_lock = cache.lock().unwrap();
                cache_lock.insert(program_str, SimpleIdl {
                    address: idl.address,
                    raw: idl.raw.clone(),
                });
                
                return Ok(idl);
            }
            Err(e) => {
                debug!("Failed to fetch on-chain IDL for {}: {}", program_id, e);
                return Err(e);
            }
        }
    }
    
    Err(format!("Could not load IDL for program {}", program_id).into())
}

/// Compute Anchor-style instruction discriminator from instruction name
/// Discriminator is the first 8 bytes of SHA256("global:{instruction_name}")
pub fn compute_anchor_discriminator(instruction_name: &str) -> [u8; 8] {
    use sha2::{Digest, Sha256};
    
    let preimage = format!("global:{}", instruction_name);
    let hash = Sha256::digest(preimage.as_bytes());
    let mut discriminator = [0u8; 8];
    discriminator.copy_from_slice(&hash[..8]);
    
    discriminator
}

/// Extract discriminator for an instruction from IDL
pub fn get_instruction_discriminator(
    idl: &SimpleIdl,
    instruction_name: &str,
) -> Result<[u8; 8], Box<dyn std::error::Error + Send + Sync>> {
    // Try to find explicit discriminator in IDL first
    let instructions = idl
        .raw
        .get("instructions")
        .and_then(|v| v.as_array())
        .ok_or("IDL missing instructions array")?;
    
    for instr in instructions {
        if instr.get("name").and_then(|n| n.as_str()) == Some(instruction_name) {
            // Check for explicit discriminator field
            if let Some(disc_array) = instr.get("discriminator").and_then(|d| d.as_array()) {
                if disc_array.len() == 8 {
                    let mut discriminator = [0u8; 8];
                    for (i, val) in disc_array.iter().enumerate() {
                        if let Some(byte) = val.as_u64() {
                            discriminator[i] = byte as u8;
                        }
                    }
                    return Ok(discriminator);
                }
            }
            
            // Fallback: compute discriminator from name
            return Ok(compute_anchor_discriminator(instruction_name));
        }
    }
    
    Err(format!("Instruction {} not found in IDL", instruction_name).into())
}

/// Helper to create a new IDL cache
pub fn new_idl_cache() -> IdlCache {
    Arc::new(Mutex::new(HashMap::new()))
}

/// Load pump.fun program IDL from multiple sources
pub async fn load_pumpfun_idl(
    rpc_client: &RpcClient,
    settings: &Settings,
    cache: &IdlCache,
) -> Result<SimpleIdl, Box<dyn std::error::Error + Send + Sync>> {
    let program_id = Pubkey::from_str(&settings.pump_fun_program)?;
    
    // Try local file first (legacy support)
    if let Ok(idl) = SimpleIdl::load_from("pumpfun.json") {
        debug!("Loaded pump.fun IDL from local file pumpfun.json");
        return Ok(idl);
    }
    
    // Fall back to on-chain fetch with caching
    load_idl_with_cache(&program_id, rpc_client, cache, true).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anchor_discriminator_computation() {
        // Test known Anchor discriminators
        // "initialize" -> should compute to specific bytes
        let disc = compute_anchor_discriminator("buy");
        assert_eq!(disc.len(), 8);
        
        // Verify deterministic computation
        let disc2 = compute_anchor_discriminator("buy");
        assert_eq!(disc, disc2);
        
        // Known discriminators from pump.fun IDL
        let buy_disc = compute_anchor_discriminator("buy");
        let expected_buy: [u8; 8] = [102, 6, 61, 18, 1, 218, 235, 234];
        assert_eq!(buy_disc, expected_buy, "Buy discriminator should match IDL");
        
        let sell_disc = compute_anchor_discriminator("sell");
        let expected_sell: [u8; 8] = [51, 230, 133, 164, 1, 127, 131, 173];
        assert_eq!(sell_disc, expected_sell, "Sell discriminator should match IDL");
    }

    #[test]
    fn test_idl_account_derivation() {
        // Test IDL account derivation for a known program
        let program_id = Pubkey::from_str("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P")
            .unwrap();
        let (idl_account, _bump) = derive_idl_account(&program_id);
        
        // Should derive a valid PDA
        assert_ne!(idl_account, program_id);
        assert_ne!(idl_account, Pubkey::default());
    }
}
