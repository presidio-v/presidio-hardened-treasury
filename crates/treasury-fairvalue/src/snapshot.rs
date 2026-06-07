//! Content-addressed price snapshots.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use treasury_core::{AssetId, ContentHash, TimestampNs};
use treasury_evidence::{canonical_bytes, sha256, CanonError};

/// Schema tag committed into every snapshot hash; bump on change.
pub const SNAPSHOT_SCHEMA: &str = "treasury-fairvalue/price-snapshot/v1";

/// An integer-exact price: `minor_per_unit` currency minor units buy
/// `atoms_per_unit` atoms (one whole asset unit). E.g. BTC at
/// $65,000.00: `minor_per_unit = 6_500_000`, `atoms_per_unit = 10^8`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Price {
    /// Currency minor units per whole unit.
    pub minor_per_unit: i128,
    /// Atoms per whole unit (the asset's scale). Must be positive.
    pub atoms_per_unit: i128,
}

/// Prices for a reporting instant, sealed under the principal-market
/// policy *before* reaching this engine (spec v2 §3.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceSnapshot {
    /// Reporting currency all prices are quoted in.
    pub currency: AssetId,
    /// The reporting instant.
    pub as_of: TimestampNs,
    /// Price per asset. `BTreeMap` for deterministic iteration.
    pub prices: BTreeMap<AssetId, Price>,
}

impl PriceSnapshot {
    /// The snapshot's content hash — the `price-snapshot` input of the
    /// valuation key.
    ///
    /// # Errors
    /// [`CanonError`] is structurally unreachable for this envelope but
    /// propagated rather than swallowed.
    pub fn snapshot_hash(&self) -> Result<ContentHash, CanonError> {
        let mut prices = Map::new();
        for (asset, price) in &self.prices {
            prices.insert(
                asset.as_str().to_owned(),
                json!({
                    "minor_per_unit": price.minor_per_unit.to_string(),
                    "atoms_per_unit": price.atoms_per_unit.to_string(),
                }),
            );
        }
        let envelope = json!({
            "schema": SNAPSHOT_SCHEMA,
            "currency": self.currency.clone(),
            "as_of": self.as_of,
            "prices": Value::Object(prices),
        });
        let bytes = canonical_bytes(&envelope)?;
        Ok(sha256(&bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> PriceSnapshot {
        let mut prices = BTreeMap::new();
        prices.insert(
            AssetId::new("BTC"),
            Price {
                minor_per_unit: 6_500_000,
                atoms_per_unit: 100_000_000,
            },
        );
        PriceSnapshot {
            currency: AssetId::new("USD"),
            as_of: TimestampNs::from_nanos(1_000),
            prices,
        }
    }

    #[test]
    fn hash_commits_to_prices() {
        let a = snapshot();
        let mut b = snapshot();
        b.prices.insert(
            AssetId::new("BTC"),
            Price {
                minor_per_unit: 6_500_001,
                atoms_per_unit: 100_000_000,
            },
        );
        assert_ne!(a.snapshot_hash(), b.snapshot_hash());
    }

    /// Golden vector — independently recomputed in Python.
    #[test]
    fn golden_hash_matches_independent_implementation() {
        let hash = snapshot().snapshot_hash().map(|h| h.to_hex());
        assert_eq!(
            hash.as_deref(),
            Ok("2958718be3140ff23249e338f01ac59ba527375be8fabc8a491d91d972bf35d9")
        );
    }
}
