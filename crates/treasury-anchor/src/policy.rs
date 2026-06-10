//! The anchoring confirmation policy (ADR-0002 action item 6).
//!
//! The depth a Bitcoin anchor transaction must reach before it counts as
//! "anchored" is itself a documented, content-addressed artifact. Changing
//! the threshold changes this policy's hash, which every receipt the policy
//! finalizes commits to — so the change is visible in the audit trail like
//! any other policy, not buried in a call-site constant.
//!
//! This is the anchor-side sibling of `treasury-chainsource`'s
//! `FinalityPolicy`: both turn a confirmation threshold into a hashed
//! artifact. It lives here rather than on the per-chain finality policy
//! because the anchoring threshold is a *single* Bitcoin-anchoring
//! parameter — independent of which chains are ingested — so attaching it
//! to a per-chain policy (e.g. an Ethereum one) would be meaningless, and
//! the anchoring layer must not depend on the chain-ingestion layer.

use serde_json::json;
use treasury_core::ContentHash;
use treasury_evidence::{canonical_bytes, sha256, CanonError};

/// Schema tag committed into every anchor-policy hash; bump on change.
pub const ANCHOR_POLICY_SCHEMA: &str = "treasury-anchor/anchor-policy/v1";

/// The anchoring confirmation policy: how many confirmations the Bitcoin
/// anchor transaction must reach before [`crate::AnchorPipeline::finalize`]
/// will seal it. Content-addressed, so the threshold is an audit artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnchorPolicy {
    /// Required confirmation depth for "anchored".
    pub required_depth: u64,
}

impl AnchorPolicy {
    /// A policy requiring `required_depth` confirmations.
    #[must_use]
    pub fn new(required_depth: u64) -> Self {
        Self { required_depth }
    }

    /// The policy's content hash — recorded in every receipt this policy
    /// finalizes, so a change to the threshold is visible in the audit
    /// trail rather than silent.
    ///
    /// # Errors
    /// [`CanonError`] is structurally unreachable for this envelope but
    /// propagated rather than swallowed.
    pub fn policy_hash(&self) -> Result<ContentHash, CanonError> {
        let envelope = json!({
            "schema": ANCHOR_POLICY_SCHEMA,
            "required_depth": self.required_depth,
        });
        let bytes = canonical_bytes(&envelope)?;
        Ok(sha256(&bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_changes_the_hash() {
        assert_ne!(
            AnchorPolicy::new(6).policy_hash(),
            AnchorPolicy::new(3).policy_hash()
        );
    }

    /// Golden vector — independently recomputed in Python.
    #[test]
    fn golden_policy_hash_matches_independent_implementation() {
        let hash = AnchorPolicy::new(6).policy_hash().map(|h| h.to_hex());
        assert_eq!(
            hash.as_deref(),
            Ok("c55da46991dafa849b21eddafa0eed1e2cc33a27be85e535815030aa81417a15"),
        );
    }
}
