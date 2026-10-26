use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

/// A Solana public key. Kept local so this crate can be fuzzed/tested without
/// pulling the entire Solana SDK and its native dependencies.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Address(pub [u8; 32]);

impl Address {
    pub const ZERO: Self = Self([0u8; 32]);

    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 32]
    }

    pub fn from_slice(bytes: &[u8]) -> Result<Self, crate::DexError> {
        let array: [u8; 32] = bytes
            .try_into()
            .map_err(|_| crate::DexError::InvalidAddress("expected 32 bytes".into()))?;
        Ok(Self(array))
    }
}

impl From<[u8; 32]> for Address {
    fn from(value: [u8; 32]) -> Self {
        Self(value)
    }
}

impl FromStr for Address {
    type Err = crate::DexError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = bs58::decode(s)
            .into_vec()
            .map_err(|e| crate::DexError::InvalidAddress(format!("{s:?}: {e}")))?;
        Self::from_slice(&bytes)
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&bs58::encode(self.0).into_string())
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Address({self})")
    }
}

impl Serialize for Address {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Address {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_sol_mint() {
        let text = "So11111111111111111111111111111111111111112";
        let address: Address = text.parse().unwrap();
        assert_eq!(address.to_string(), text);
        let json = serde_json::to_string(&address).unwrap();
        assert_eq!(json, format!("\"{text}\""));
        let back: Address = serde_json::from_str(&json).unwrap();
        assert_eq!(back, address);
    }

    #[test]
    fn rejects_wrong_length() {
        assert!("abc".parse::<Address>().is_err());
    }
}
