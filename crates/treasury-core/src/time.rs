//! Bitemporal timestamps (spec v2 §3.2).
//!
//! The same physical type carries both *event time* (when it happened
//! on-chain / in the world) and *knowledge time* (when the ledger booked
//! it); the ledger keeps the two axes apart by position, the type keeps
//! the encoding honest. Serialized as a decimal string of nanoseconds
//! since the Unix epoch — never a JSON float.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Nanoseconds since the Unix epoch (UTC). Range covers years 1677–2262.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TimestampNs(
    /// Raw nanoseconds since epoch.
    pub i64,
);

impl TimestampNs {
    /// Construct from raw nanoseconds since epoch.
    #[must_use]
    pub fn from_nanos(nanos: i64) -> Self {
        Self(nanos)
    }

    /// Raw nanoseconds since epoch.
    #[must_use]
    pub fn as_nanos(&self) -> i64 {
        self.0
    }
}

impl Serialize for TimestampNs {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for TimestampNs {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse::<i64>()
            .map(Self)
            .map_err(|_| D::Error::custom("timestamp must be a decimal string of nanoseconds"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_string() {
        let t = TimestampNs::from_nanos(1_700_000_000_000_000_000);
        let json = serde_json::to_string(&t).unwrap_or_default();
        assert_eq!(json, "\"1700000000000000000\"");
    }
}
