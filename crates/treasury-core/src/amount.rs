//! Integer money (spec v2 §3.8): amounts are `i128` base units ("atoms",
//! e.g. satoshi, wei) tagged with their asset. No floats exist in this
//! type or anywhere downstream of it; all arithmetic is checked.
//!
//! Serialization: atoms encode as a decimal **string** — JSON numbers are
//! not trusted to round-trip 128-bit integers across toolchains.

use crate::ids::AssetId;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A signed quantity of one asset in indivisible base units.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AssetAmount {
    asset: AssetId,
    atoms: i128,
}

impl AssetAmount {
    /// Construct an amount of `atoms` base units of `asset`.
    #[must_use]
    pub fn new(asset: AssetId, atoms: i128) -> Self {
        Self { asset, atoms }
    }

    /// The asset this amount denominates.
    #[must_use]
    pub fn asset(&self) -> &AssetId {
        &self.asset
    }

    /// Base units (may be negative for outflows).
    #[must_use]
    pub fn atoms(&self) -> i128 {
        self.atoms
    }

    /// Checked addition. Fails on overflow or asset mismatch.
    ///
    /// # Errors
    /// [`AmountError::AssetMismatch`] when assets differ;
    /// [`AmountError::Overflow`] when the sum exceeds `i128`.
    pub fn checked_add(&self, other: &Self) -> Result<Self, AmountError> {
        if self.asset != other.asset {
            return Err(AmountError::AssetMismatch {
                left: self.asset.clone(),
                right: other.asset.clone(),
            });
        }
        let atoms = self
            .atoms
            .checked_add(other.atoms)
            .ok_or(AmountError::Overflow)?;
        Ok(Self { asset: self.asset.clone(), atoms })
    }

    /// Checked subtraction. Fails on overflow or asset mismatch.
    ///
    /// # Errors
    /// [`AmountError::AssetMismatch`] when assets differ;
    /// [`AmountError::Overflow`] when the difference exceeds `i128`.
    pub fn checked_sub(&self, other: &Self) -> Result<Self, AmountError> {
        if self.asset != other.asset {
            return Err(AmountError::AssetMismatch {
                left: self.asset.clone(),
                right: other.asset.clone(),
            });
        }
        let atoms = self
            .atoms
            .checked_sub(other.atoms)
            .ok_or(AmountError::Overflow)?;
        Ok(Self { asset: self.asset.clone(), atoms })
    }

    /// Checked negation (fails only on `i128::MIN`).
    ///
    /// # Errors
    /// [`AmountError::Overflow`] when atoms is `i128::MIN`.
    pub fn checked_neg(&self) -> Result<Self, AmountError> {
        let atoms = self.atoms.checked_neg().ok_or(AmountError::Overflow)?;
        Ok(Self { asset: self.asset.clone(), atoms })
    }
}

/// Errors from checked amount arithmetic.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AmountError {
    /// Arithmetic across two different assets.
    #[error("asset mismatch: {left} vs {right}")]
    AssetMismatch {
        /// Left operand's asset.
        left: AssetId,
        /// Right operand's asset.
        right: AssetId,
    },
    /// `i128` overflow.
    #[error("amount overflow")]
    Overflow,
}

#[derive(Serialize, Deserialize)]
struct AmountWire {
    asset: AssetId,
    atoms: String,
}

impl Serialize for AssetAmount {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        AmountWire { asset: self.asset.clone(), atoms: self.atoms.to_string() }
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AssetAmount {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = AmountWire::deserialize(deserializer)?;
        let atoms = wire
            .atoms
            .parse::<i128>()
            .map_err(|_| D::Error::custom("atoms must be a decimal string"))?;
        Ok(Self { asset: wire.asset, atoms })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn btc(atoms: i128) -> AssetAmount {
        AssetAmount::new(AssetId::new("BTC"), atoms)
    }

    #[test]
    fn checked_add_works() {
        let sum = btc(10).checked_add(&btc(-3));
        assert_eq!(sum, Ok(btc(7)));
    }

    #[test]
    fn asset_mismatch_rejected() {
        let eth = AssetAmount::new(AssetId::new("ETH"), 1);
        assert!(matches!(
            btc(1).checked_add(&eth),
            Err(AmountError::AssetMismatch { .. })
        ));
    }

    #[test]
    fn overflow_is_an_error() {
        let max = btc(i128::MAX);
        assert_eq!(max.checked_add(&btc(1)), Err(AmountError::Overflow));
    }

    #[test]
    fn atoms_serialize_as_string() {
        let json = serde_json::to_string(&btc(21_000_000)).unwrap_or_default();
        assert_eq!(json, r#"{"asset":"BTC","atoms":"21000000"}"#);
    }
}
