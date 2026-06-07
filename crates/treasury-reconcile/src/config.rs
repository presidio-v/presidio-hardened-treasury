//! Matcher configuration — a content-addressed audit artifact, same
//! discipline as policy artifacts (REQ-9): the decision envelope commits
//! to the config hash, so an auditor replays exactly the matcher version
//! that ran.

use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use treasury_core::{AssetId, ContentHash};
use treasury_evidence::{canonical_bytes, sha256, CanonError};

/// Schema tag committed into every config hash; bump on change.
pub const CONFIG_SCHEMA: &str = "treasury-reconcile/matcher-config/v1";

/// Deterministic matcher parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatcherConfig {
    /// Tier-1 time window: an inflow must occur within this many
    /// nanoseconds **after** the outflow.
    pub time_window_ns: i64,
    /// Per-asset materiality thresholds in atoms: a tier-1 match with an
    /// outflow amount at or above its asset's threshold queues for human
    /// confirmation instead of auto-netting. An asset with no entry has
    /// threshold zero — **everything queues** (fail closed).
    pub materiality_atoms: BTreeMap<AssetId, i128>,
}

impl MatcherConfig {
    /// The config's content hash, committed into every match decision.
    ///
    /// # Errors
    /// [`CanonError`] is structurally unreachable for this envelope but
    /// propagated rather than swallowed.
    pub fn config_hash(&self) -> Result<ContentHash, CanonError> {
        let mut materiality = Map::new();
        for (asset, atoms) in &self.materiality_atoms {
            materiality.insert(asset.as_str().to_owned(), Value::String(atoms.to_string()));
        }
        let envelope = json!({
            "schema": CONFIG_SCHEMA,
            "time_window_ns": self.time_window_ns.to_string(),
            "materiality_atoms": materiality,
        });
        let bytes = canonical_bytes(&envelope)?;
        Ok(sha256(&bytes))
    }

    /// The materiality threshold for an asset; missing entries are zero
    /// (fail closed: everything queues).
    #[must_use]
    pub fn threshold_for(&self, asset: &AssetId) -> i128 {
        self.materiality_atoms.get(asset).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> MatcherConfig {
        let mut materiality = BTreeMap::new();
        materiality.insert(AssetId::new("BTC"), 100_000_000_i128);
        MatcherConfig {
            time_window_ns: 3_600_000_000_000,
            materiality_atoms: materiality,
        }
    }

    #[test]
    fn hash_commits_to_thresholds() {
        let a = config();
        let mut b = config();
        b.materiality_atoms.insert(AssetId::new("BTC"), 1);
        assert_ne!(a.config_hash(), b.config_hash());
    }

    #[test]
    fn missing_asset_fails_closed_to_zero() {
        assert_eq!(config().threshold_for(&AssetId::new("ETH")), 0);
    }

    /// Golden vector — independently recomputed in Python.
    #[test]
    fn golden_hash_matches_independent_implementation() {
        let hash = config().config_hash().map(|h| h.to_hex());
        assert_eq!(
            hash.as_deref(),
            Ok("c6f889098d4f7b8da633b73bcc7b686a2bffee3e1c517faf28f492e0a885b509")
        );
    }
}
