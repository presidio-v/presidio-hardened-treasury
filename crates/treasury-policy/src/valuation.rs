//! The valuation memoization key (spec v2 §3.5, remediation R3).
//!
//! Valuation is a pure function of `(lots, price-snapshot, policy)`. The
//! policy hash is part of the key — the v1-spec omission both adversarial
//! reviews quoted but neither caught. Two valuations under different
//! policies can therefore never share a memo entry, and a restatement
//! under a corrected policy is a clean re-run keyed by a different hash.

use serde_json::json;
use treasury_core::ContentHash;
use treasury_evidence::{canonical_bytes, sha256, CanonError};

/// Schema tag committed into every valuation key hash.
pub const VALUATION_KEY_SCHEMA: &str = "treasury-policy/valuation-key/v1";

/// The complete input identity of one valuation run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValuationKey {
    /// Content hash of the canonical lot set being valued.
    pub lots_hash: ContentHash,
    /// Evidence-store hash of the price snapshot used.
    pub price_snapshot_hash: ContentHash,
    /// Content hash of the governing principal-market policy artifact.
    pub policy_hash: ContentHash,
}

impl ValuationKey {
    /// The key's own content hash — the memoization index and the value
    /// a disclosure references to make the valuation reproducible.
    ///
    /// # Errors
    /// [`CanonError`] is structurally unreachable for this envelope (it
    /// contains only strings) but propagated rather than swallowed.
    pub fn key_hash(&self) -> Result<ContentHash, CanonError> {
        let envelope = json!({
            "schema": VALUATION_KEY_SCHEMA,
            "lots": self.lots_hash.to_hex(),
            "price_snapshot": self.price_snapshot_hash.to_hex(),
            "policy": self.policy_hash.to_hex(),
        });
        let bytes = canonical_bytes(&envelope)?;
        Ok(sha256(&bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> ValuationKey {
        ValuationKey {
            lots_hash: ContentHash([1; 32]),
            price_snapshot_hash: ContentHash([2; 32]),
            policy_hash: ContentHash([3; 32]),
        }
    }

    #[test]
    fn key_commits_to_policy() {
        let a = key();
        let mut b = key();
        b.policy_hash = ContentHash([4; 32]);
        assert_ne!(a.key_hash(), b.key_hash());
    }

    #[test]
    fn key_commits_to_all_three_inputs() {
        let base = key().key_hash();
        let mut lots = key();
        lots.lots_hash = ContentHash([5; 32]);
        let mut snap = key();
        snap.price_snapshot_hash = ContentHash([5; 32]);
        assert_ne!(base, lots.key_hash());
        assert_ne!(base, snap.key_hash());
    }

    /// Golden vector — independently recomputed in Python.
    #[test]
    fn golden_hash_matches_independent_implementation() {
        let hash = key().key_hash().map(|h| h.to_hex());
        assert_eq!(
            hash.as_deref(),
            Ok("05f919303bd5ee4e96cba2403c5685316c4413179eecaeb36c46593f40cf0d7b")
        );
    }
}
