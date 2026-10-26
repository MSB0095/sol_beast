use thiserror::Error;

pub type Result<T> = std::result::Result<T, DexError>;

#[derive(Debug, Error)]
pub enum DexError {
    #[error("invalid address: {0}")]
    InvalidAddress(String),
    #[error("account too short: need {need} bytes, got {got}")]
    AccountTooShort { need: usize, got: usize },
    #[error("account has wrong discriminator for {venue}")]
    WrongDiscriminator { venue: &'static str },
    #[error("unsupported decode: {0}")]
    Unsupported(String),
    #[error("math error: {0}")]
    Math(String),
    #[error("route error: {0}")]
    Route(String),
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub(crate) fn read_u8(data: &[u8], offset: usize) -> Result<u8> {
    data.get(offset).copied().ok_or(DexError::AccountTooShort {
        need: offset + 1,
        got: data.len(),
    })
}

pub(crate) fn read_u16(data: &[u8], offset: usize) -> Result<u16> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or(DexError::AccountTooShort {
            need: offset + 2,
            got: data.len(),
        })?;
    Ok(u16::from_le_bytes(
        bytes.try_into().expect("slice length checked"),
    ))
}

pub(crate) fn read_u32(data: &[u8], offset: usize) -> Result<u32> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or(DexError::AccountTooShort {
            need: offset + 4,
            got: data.len(),
        })?;
    Ok(u32::from_le_bytes(
        bytes.try_into().expect("slice length checked"),
    ))
}

pub(crate) fn read_u64(data: &[u8], offset: usize) -> Result<u64> {
    let bytes = data
        .get(offset..offset + 8)
        .ok_or(DexError::AccountTooShort {
            need: offset + 8,
            got: data.len(),
        })?;
    Ok(u64::from_le_bytes(
        bytes.try_into().expect("slice length checked"),
    ))
}

pub(crate) fn read_u128(data: &[u8], offset: usize) -> Result<u128> {
    let bytes = data
        .get(offset..offset + 16)
        .ok_or(DexError::AccountTooShort {
            need: offset + 16,
            got: data.len(),
        })?;
    Ok(u128::from_le_bytes(
        bytes.try_into().expect("slice length checked"),
    ))
}

pub(crate) fn read_address(data: &[u8], offset: usize) -> Result<crate::Address> {
    let bytes = data
        .get(offset..offset + 32)
        .ok_or(DexError::AccountTooShort {
            need: offset + 32,
            got: data.len(),
        })?;
    crate::Address::from_slice(bytes)
}

pub(crate) fn check_discriminator(
    data: &[u8],
    expected: [u8; 8],
    venue: &'static str,
) -> Result<()> {
    if data.len() < 8 {
        return Err(DexError::AccountTooShort {
            need: 8,
            got: data.len(),
        });
    }
    if data[..8] != expected {
        return Err(DexError::WrongDiscriminator { venue });
    }
    Ok(())
}
