//! Anchor instruction discriminators.
//!
//! The legacy sniper loads pump.fun IDLs from disk and only needs the
//! discriminator helpers at runtime. IDL account discovery, decompression and
//! caching were removed because the executable never used them.

use crate::idl::SimpleIdl;
use sha2::{Digest, Sha256};

/// Compute an Anchor-style instruction discriminator: the first eight bytes of
/// `SHA256("global:<instruction_name>")`.
pub fn compute_anchor_discriminator(instruction_name: &str) -> [u8; 8] {
    let preimage = format!("global:{instruction_name}");
    let hash = Sha256::digest(preimage.as_bytes());
    let mut discriminator = [0u8; 8];
    discriminator.copy_from_slice(&hash[..8]);
    discriminator
}

/// Read an instruction discriminator from an IDL, honoring an explicit
/// `discriminator` array when the IDL provides one.
pub fn get_instruction_discriminator(
    idl: &SimpleIdl,
    instruction_name: &str,
) -> Result<[u8; 8], Box<dyn std::error::Error + Send + Sync>> {
    let instructions = idl
        .raw
        .get("instructions")
        .and_then(|value| value.as_array())
        .ok_or("IDL missing instructions array")?;

    for instruction in instructions {
        if instruction.get("name").and_then(|name| name.as_str()) != Some(instruction_name) {
            continue;
        }
        if let Some(values) = instruction
            .get("discriminator")
            .and_then(|value| value.as_array())
        {
            if values.len() != 8 {
                return Err(
                    format!("instruction {instruction_name} discriminator is not 8 bytes").into(),
                );
            }
            let mut discriminator = [0u8; 8];
            for (index, value) in values.iter().enumerate() {
                let byte = value.as_u64().ok_or_else(|| {
                    format!("instruction {instruction_name} discriminator is not numeric")
                })?;
                discriminator[index] = u8::try_from(byte).map_err(|_| {
                    format!("instruction {instruction_name} discriminator byte out of range")
                })?;
            }
            return Ok(discriminator);
        }
        return Ok(compute_anchor_discriminator(instruction_name));
    }

    Err(format!("Instruction {instruction_name} not found in IDL").into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_discriminators_match_pumpfun_idl() {
        assert_eq!(
            compute_anchor_discriminator("buy"),
            [102, 6, 61, 18, 1, 218, 235, 234]
        );
        assert_eq!(
            compute_anchor_discriminator("sell"),
            [51, 230, 133, 164, 1, 127, 131, 173]
        );
    }
}
