// Token metadata fetching - platform agnostic interface
// Provides traits and helpers for fetching on-chain and off-chain token metadata

use crate::error::CoreError;
use crate::models::OffchainTokenMetadata;
use async_trait::async_trait;
use mpl_token_metadata::accounts::Metadata;
use solana_program::pubkey::Pubkey;
use std::str::FromStr;
use log::{debug, warn, error};
use base64::{Engine as _, engine::general_purpose::STANDARD as Base64Engine};

/// Result type for metadata operations
pub type MetadataResult<T> = Result<T, CoreError>;

/// Abstract HTTP client trait for fetching off-chain metadata
/// Implementations should handle platform-specific HTTP requests
#[async_trait(?Send)]
pub trait HttpClient {
    /// Fetch text content from a URL
    async fn fetch_text(&self, url: &str) -> MetadataResult<String>;
}

/// Compute metadata PDA for a mint
pub fn compute_metadata_pda(mint: &str, metadata_program: &str) -> MetadataResult<String> {
    let metadata_program_pk = Pubkey::from_str(metadata_program)
        .map_err(|e| CoreError::ParseError(format!("Invalid metadata program: {}", e)))?;
    let mint_pk = Pubkey::from_str(mint)
        .map_err(|e| CoreError::ParseError(format!("Invalid mint: {}", e)))?;
    
    let (metadata_pda, _) = Pubkey::find_program_address(
        &[b"metadata", metadata_program_pk.as_ref(), mint_pk.as_ref()],
        &metadata_program_pk,
    );
    
    Ok(metadata_pda.to_string())
}

/// Parse on-chain metadata from account data
pub fn parse_onchain_metadata(account_data: &[u8]) -> MetadataResult<Metadata> {
    Metadata::safe_deserialize(account_data)
        .map_err(|e| CoreError::ParseError(format!("Failed to deserialize metadata: {}", e)))
}

/// Extract string value from JSON, trying multiple possible keys and formats
fn extract_first_string(v: &serde_json::Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(field) = v.get(*key) {
            match field {
                serde_json::Value::String(s) => return Some(s.clone()),
                serde_json::Value::Object(map) => {
                    // Try `en` locale or first string value
                    if let Some(serde_json::Value::String(s2)) = map.get("en") {
                        return Some(s2.clone());
                    }
                    for (_k, val) in map.iter() {
                        if let serde_json::Value::String(s3) = val {
                            return Some(s3.clone());
                        }
                    }
                }
                serde_json::Value::Array(arr) => {
                    if let Some(serde_json::Value::String(s4)) = arr.first() {
                        return Some(s4.clone());
                    }
                }
                other => {
                    // Fallback: use string representation for numbers or bools
                    return Some(other.to_string());
                }
            }
        }
    }
    None
}

/// Parse off-chain metadata from JSON
pub fn parse_offchain_metadata(json_str: &str) -> MetadataResult<OffchainTokenMetadata> {
    let body_val: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| CoreError::ParseError(format!("Failed to parse metadata JSON: {}", e)))?;
    
    let mut metadata = OffchainTokenMetadata {
        name: extract_first_string(&body_val, &["name", "title", "token_name"]),
        symbol: extract_first_string(&body_val, &["symbol", "ticker"]),
        description: body_val.get("description").and_then(|d| d.as_str().map(|s| s.to_string())),
        image: extract_first_string(&body_val, &["image", "image_url", "imageUri"]),
        extras: Some(body_val),
    };
    
    // Normalize and extract fields from extras
    metadata.normalize();
    
    Ok(metadata)
}

/// Fetch and parse off-chain metadata from URI
pub async fn fetch_offchain_metadata<H: HttpClient>(
    uri: &str,
    http_client: &H,
) -> MetadataResult<OffchainTokenMetadata> {
    // Validate URI
    if uri.is_empty() || (!uri.starts_with("http://") && !uri.starts_with("https://")) {
        return Err(CoreError::InvalidInput(format!("Invalid metadata URI: {}", uri)));
    }
    
    debug!("Fetching off-chain metadata from: {}", uri);
    
    // Fetch the JSON content
    let body = http_client.fetch_text(uri).await?;
    
    // Parse the JSON
    parse_offchain_metadata(&body)
}

/// Complete metadata fetching result
#[derive(Debug, Clone)]
pub struct TokenMetadata {
    pub onchain: Option<Metadata>,
    pub offchain: Option<OffchainTokenMetadata>,
    pub raw_account_data: Option<Vec<u8>>,
}

/// Decode account data from RPC response
pub fn decode_account_data(account_info: &serde_json::Value) -> MetadataResult<Vec<u8>> {
    // Normalize: some RPC implementations put the account under result.value
    let account_obj = if let Some(v) = account_info.get("value") {
        v
    } else {
        account_info
    };
    
    let base64_str = account_obj
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .ok_or_else(|| CoreError::ParseError("No data field in account info".to_string()))?;
    
    Base64Engine.decode(base64_str)
        .map_err(|e| CoreError::ParseError(format!("Failed to decode base64 account data: {}", e)))
}

/// Fetch complete token metadata (on-chain + off-chain)
/// 
/// This function orchestrates fetching both on-chain metadata from Solana
/// and off-chain metadata from the URI specified in the on-chain data.
pub async fn fetch_token_metadata<H: HttpClient>(
    mint: &str,
    _metadata_program: &str,
    account_data: Option<Vec<u8>>,
    http_client: &H,
) -> MetadataResult<TokenMetadata> {
    debug!("Fetching token metadata for mint: {}", mint);
    
    // If no account data provided, we can't proceed
    let decoded = match account_data {
        Some(data) => data,
        None => {
            debug!("No account data available for mint {}", mint);
            return Ok(TokenMetadata {
                onchain: None,
                offchain: None,
                raw_account_data: None,
            });
        }
    };
    
    // Parse on-chain metadata
    let onchain = match parse_onchain_metadata(&decoded) {
        Ok(meta) => Some(meta),
        Err(e) => {
            error!("Failed to deserialize metadata for mint {}: {:?}", mint, e);
            return Ok(TokenMetadata {
                onchain: None,
                offchain: None,
                raw_account_data: Some(decoded),
            });
        }
    };
    
    // Try to fetch off-chain metadata if URI is present
    let offchain = if let Some(ref meta) = onchain {
        let uri = meta.uri.trim_end_matches('\u{0}');
        if !uri.is_empty() && (uri.starts_with("http://") || uri.starts_with("https://")) {
            match fetch_offchain_metadata(uri, http_client).await {
                Ok(off_meta) => {
                    debug!("Fetched off-chain metadata for {}: {:?}", mint, off_meta);
                    Some(off_meta)
                }
                Err(e) => {
                    warn!("Failed to fetch off-chain metadata for {} from {}: {:?}", mint, uri, e);
                    None
                }
            }
        } else {
            debug!("No valid URI in on-chain metadata for {}", mint);
            None
        }
    } else {
        None
    };
    
    Ok(TokenMetadata {
        onchain,
        offchain,
        raw_account_data: Some(decoded),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_offchain_metadata_basic() {
        let json = r#"{"name": "Test Token", "symbol": "TEST", "description": "A test token"}"#;
        let result = parse_offchain_metadata(json).unwrap();
        assert_eq!(result.name, Some("Test Token".to_string()));
        assert_eq!(result.symbol, Some("TEST".to_string()));
        assert_eq!(result.description, Some("A test token".to_string()));
    }
    
    #[test]
    fn test_parse_offchain_metadata_with_image() {
        let json = r#"{"name": "NFT", "image": "https://example.com/image.png"}"#;
        let result = parse_offchain_metadata(json).unwrap();
        assert_eq!(result.name, Some("NFT".to_string()));
        assert_eq!(result.image, Some("https://example.com/image.png".to_string()));
    }
    
    #[test]
    fn test_extract_first_string_variants() {
        let json: serde_json::Value = serde_json::from_str(r#"{
            "title": "Title Field",
            "name": "Name Field"
        }"#).unwrap();
        
        // Should try "name" first, then "title"
        assert_eq!(extract_first_string(&json, &["name", "title"]), Some("Name Field".to_string()));
        assert_eq!(extract_first_string(&json, &["title", "name"]), Some("Title Field".to_string()));
    }
}
