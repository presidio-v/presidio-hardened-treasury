//! Content-addressed anchor receipts (spec v2 §3.3).

use serde::{Deserialize, Serialize};
use serde_json::json;
use treasury_core::{ContentHash, TimestampNs};
use treasury_evidence::{canonical_bytes, sha256, CanonError};

/// Schema tag committed into every receipt hash; bump on envelope change.
pub const RECEIPT_SCHEMA: &str = "treasury-anchor/receipt/v2";

/// Where the tree head was committed, outside our trust boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnchorMethod {
    /// A transaction on a public chain carrying the tree head.
    PublicChain {
        /// Chain identifier, e.g. `"bitcoin"` or `"ethereum"`.
        chain: String,
        /// Transaction reference (txid / hash) that embeds the head.
        tx_ref: String,
    },
    /// An RFC 3161 timestamp authority token over the tree head.
    Rfc3161Tsa {
        /// Authority identifier (URL or name).
        authority: String,
        /// Evidence-store hash of the DER-encoded timestamp token.
        token_hash: ContentHash,
    },
}

/// One anchoring event: at `anchored_at`, the evidence store's first
/// `entry_count` entries had RFC 6962 head `tree_head`, committed via
/// `method`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnchorReceipt {
    /// The anchored RFC 6962 tree head.
    pub tree_head: ContentHash,
    /// Number of evidence entries the head covers.
    pub entry_count: u64,
    /// External commitment reference.
    pub method: AnchorMethod,
    /// When the anchor was made (knowledge time).
    pub anchored_at: TimestampNs,
    /// Content hash of the [`crate::AnchorPolicy`] whose confirmation-depth
    /// threshold gated this anchor — so the threshold used is itself in the
    /// audit trail, not a call-site constant.
    pub confirmation_policy: ContentHash,
}

impl AnchorReceipt {
    /// The receipt's content hash — what a disclosure references to prove
    /// the evidence set existed, untampered, at the anchor time.
    ///
    /// # Errors
    /// [`CanonError`] is structurally unreachable for this envelope but
    /// propagated rather than swallowed.
    pub fn receipt_hash(&self) -> Result<ContentHash, CanonError> {
        let envelope = json!({
            "schema": RECEIPT_SCHEMA,
            "tree_head": self.tree_head.to_hex(),
            "entry_count": self.entry_count,
            "method": self.method.clone(),
            "anchored_at": self.anchored_at,
            "confirmation_policy": self.confirmation_policy.to_hex(),
        });
        let bytes = canonical_bytes(&envelope)?;
        Ok(sha256(&bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt() -> AnchorReceipt {
        AnchorReceipt {
            tree_head: ContentHash([1; 32]),
            entry_count: 42,
            method: AnchorMethod::PublicChain {
                chain: "bitcoin".to_owned(),
                tx_ref: "f00d".to_owned(),
            },
            anchored_at: TimestampNs::from_nanos(1_000),
            confirmation_policy: ContentHash([2; 32]),
        }
    }

    #[test]
    fn hash_commits_to_head_count_and_policy() {
        let a = receipt();
        let mut b = receipt();
        b.entry_count = 43;
        let mut c = receipt();
        c.tree_head = ContentHash([2; 32]);
        let mut d = receipt();
        d.confirmation_policy = ContentHash([9; 32]);
        assert_ne!(a.receipt_hash(), b.receipt_hash());
        assert_ne!(a.receipt_hash(), c.receipt_hash());
        // The confirmation-depth threshold is in the audit trail: change
        // the policy, change the receipt.
        assert_ne!(a.receipt_hash(), d.receipt_hash());
    }

    /// Golden vector — independently recomputed in Python from the
    /// documented envelope (sorted-key canonical JSON, SHA-256).
    #[test]
    fn golden_hash_matches_independent_implementation() {
        let hash = receipt().receipt_hash().map(|h| h.to_hex());
        assert_eq!(
            hash.as_deref(),
            Ok("5dbd2632810803c6a9b4f0b827a87f59b834e387f1e06cbc9a48e1b8b14c7528")
        );
    }
}
