//! Content-addressed policy artifacts (spec v2 §3.5).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use treasury_core::{ActorId, ContentHash, TimestampNs};
use treasury_evidence::{canonical_bytes, sha256, CanonError};

/// Schema tag committed into every policy hash; bump on envelope change.
pub const POLICY_SCHEMA: &str = "treasury-policy/artifact/v1";

/// A policy kind, e.g. `"principal-market/v1"`, `"fee-treatment/v1"`,
/// `"finality/v1"`, `"fx-translation/v1"`. The kind names the *contract*
/// the body must satisfy; versioning the kind is a breaking change to
/// that contract.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PolicyKind(String);

impl PolicyKind {
    /// Construct from any string-like value.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The kind as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for PolicyKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A versioned, approval-signed policy artifact. Identity is the SHA-256
/// of the canonical envelope — body, kind, approvers, and effectivity all
/// commit into the hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyArtifact {
    /// The contract this body satisfies.
    pub kind: PolicyKind,
    /// Policy body (float-free JSON; floats reject at hashing).
    pub body: Value,
    /// Approving actors; at least one is mandatory (spec v2 §3.5
    /// "approval-signed"). Dual-control supplies two.
    pub approvers: Vec<ActorId>,
    /// Earliest event time this policy may govern.
    pub effective_from: TimestampNs,
}

impl PolicyArtifact {
    /// Compute the artifact's content hash.
    ///
    /// # Errors
    /// [`PolicyError::NoApprovers`] when the approver list is empty;
    /// [`PolicyError::Canon`] when the body contains floats or nests too
    /// deeply.
    pub fn policy_hash(&self) -> Result<ContentHash, PolicyError> {
        if self.approvers.is_empty() {
            return Err(PolicyError::NoApprovers);
        }
        let envelope = json!({
            "schema": POLICY_SCHEMA,
            "kind": self.kind.clone(),
            "body": self.body.clone(),
            "approvers": self.approvers.clone(),
            "effective_from": self.effective_from,
        });
        let bytes = canonical_bytes(&envelope)?;
        Ok(sha256(&bytes))
    }
}

/// Errors constructing or hashing policy artifacts.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PolicyError {
    /// Approval is structural: an unapproved policy has no identity.
    #[error("policy artifact requires at least one approver")]
    NoApprovers,
    /// Body failed canonicalization (floats, depth).
    #[error(transparent)]
    Canon(#[from] CanonError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact() -> PolicyArtifact {
        PolicyArtifact {
            kind: PolicyKind::new("principal-market/v1"),
            body: json!({
                "asset": "BTC",
                "venues": ["coinbase-prime", "kraken"],
                "tie_break": "highest_30d_volume",
                "stale_after_seconds": 120,
            }),
            approvers: vec![ActorId::new("cfo"), ActorId::new("controller")],
            effective_from: TimestampNs::from_nanos(1_000),
        }
    }

    #[test]
    fn hash_commits_to_body() {
        let a = artifact();
        let mut b = artifact();
        b.body = json!({"asset": "BTC", "venues": ["kraken"]});
        assert_ne!(a.policy_hash(), b.policy_hash());
    }

    #[test]
    fn hash_commits_to_approvers() {
        let a = artifact();
        let mut b = artifact();
        b.approvers = vec![ActorId::new("cfo")];
        assert_ne!(a.policy_hash(), b.policy_hash());
    }

    #[test]
    fn unapproved_policy_has_no_identity() {
        let mut a = artifact();
        a.approvers.clear();
        assert_eq!(a.policy_hash(), Err(PolicyError::NoApprovers));
    }

    #[test]
    fn float_body_rejected() {
        let mut a = artifact();
        a.body = json!({"threshold": 0.05});
        assert!(matches!(a.policy_hash(), Err(PolicyError::Canon(_))));
    }

    /// Golden vector — independently recomputed in Python from the
    /// documented envelope (sorted-key canonical JSON, SHA-256).
    #[test]
    fn golden_hash_matches_independent_implementation() {
        let hash = artifact().policy_hash().map(|h| h.to_hex());
        assert_eq!(
            hash.as_deref(),
            Ok("56739b8e60742300f5b57b8ca6c7142d419558d87f2043ebfbde17388058a877")
        );
    }
}
