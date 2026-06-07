//! Match proposals and their content-addressed decision envelopes.

use crate::leg::LegId;
use serde::{Deserialize, Serialize};
use serde_json::json;
use treasury_core::ContentHash;
use treasury_evidence::{canonical_bytes, sha256, CanonError};

/// Schema tag committed into every decision hash; bump on change.
pub const DECISION_SCHEMA: &str = "treasury-reconcile/decision/v1";

/// Discrete corroboration class — never a numeric confidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// Same on-chain tx hash observed on both legs.
    Deterministic,
    /// Amount−fee + time window + address corroboration.
    StrongCorroboration,
    /// Ambiguous or under-corroborated; human classification required.
    Probabilistic,
}

/// What the matcher decided to do with a proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    /// Netted automatically (tier 0, or tier 1 below materiality).
    AutoNet,
    /// Surfaced for dual-control human confirmation.
    Queue,
}

/// A proposed pairing of one outflow with one inflow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchProposal {
    /// The outflow leg.
    pub out_leg: LegId,
    /// The inflow leg.
    pub in_leg: LegId,
    /// Corroboration class.
    pub tier: Tier,
    /// Auto-net or queue.
    pub disposition: Disposition,
    /// Hash of the matcher config that produced this proposal.
    pub config_hash: ContentHash,
}

impl MatchProposal {
    /// The proposal's content hash — the queue item id, and what the L3
    /// judgment references when a human confirms or rejects it.
    ///
    /// # Errors
    /// [`CanonError`] is structurally unreachable for this envelope but
    /// propagated rather than swallowed.
    pub fn decision_hash(&self) -> Result<ContentHash, CanonError> {
        let envelope = json!({
            "schema": DECISION_SCHEMA,
            "out_leg": self.out_leg.to_hex(),
            "in_leg": self.in_leg.to_hex(),
            "tier": self.tier,
            "disposition": self.disposition,
            "config": self.config_hash.to_hex(),
        });
        let bytes = canonical_bytes(&envelope)?;
        Ok(sha256(&bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proposal() -> MatchProposal {
        MatchProposal {
            out_leg: ContentHash([1; 32]),
            in_leg: ContentHash([2; 32]),
            tier: Tier::Deterministic,
            disposition: Disposition::AutoNet,
            config_hash: ContentHash([3; 32]),
        }
    }

    #[test]
    fn hash_commits_to_tier_and_disposition() {
        let a = proposal();
        let mut b = proposal();
        b.tier = Tier::StrongCorroboration;
        b.disposition = Disposition::Queue;
        assert_ne!(a.decision_hash(), b.decision_hash());
    }

    /// Golden vector — independently recomputed in Python.
    #[test]
    fn golden_hash_matches_independent_implementation() {
        let hash = proposal().decision_hash().map(|h| h.to_hex());
        assert_eq!(
            hash.as_deref(),
            Ok("d12b836656bb3b031dc854363046b450b249fd3622036179f003444705423577")
        );
    }
}
