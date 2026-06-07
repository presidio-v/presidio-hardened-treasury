//! Content hash primitive (SHA-256) used for evidence references, policy
//! hashes, code-version hashes, and ledger event identity.
//!
//! SHA-256 is the deliberate, conservative choice: every auditor toolchain
//! can independently verify it (spec v2 §3.3).

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A 32-byte SHA-256 digest, serialized as lowercase hex.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContentHash(
    /// Raw digest bytes.
    pub [u8; 32],
);

impl ContentHash {
    /// The digest bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Lowercase hex encoding.
    #[must_use]
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Parse from lowercase or uppercase hex.
    ///
    /// # Errors
    /// Returns an error when the input is not exactly 64 hex characters.
    pub fn from_hex(s: &str) -> Result<Self, HashParseError> {
        let bytes = hex::decode(s).map_err(|_| HashParseError::InvalidHex)?;
        let arr: [u8; 32] = bytes.try_into().map_err(|_| HashParseError::WrongLength)?;
        Ok(Self(arr))
    }
}

impl core::fmt::Display for ContentHash {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl Serialize for ContentHash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::from_hex(&s).map_err(D::Error::custom)
    }
}

/// Errors parsing a [`ContentHash`] from hex.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum HashParseError {
    /// Input was not valid hex.
    #[error("invalid hex")]
    InvalidHex,
    /// Decoded length was not 32 bytes.
    #[error("digest must be exactly 32 bytes")]
    WrongLength,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        let h = ContentHash([0xab; 32]);
        let s = h.to_hex();
        assert_eq!(s.len(), 64);
        assert_eq!(ContentHash::from_hex(&s), Ok(h));
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(ContentHash::from_hex("abcd"), Err(HashParseError::WrongLength));
        assert_eq!(ContentHash::from_hex("zz"), Err(HashParseError::InvalidHex));
    }
}
