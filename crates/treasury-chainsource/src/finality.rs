//! The per-chain finality / confirmation-depth policy (gap G-5, §3.5).
//!
//! Reorg churn must be excluded from the §3.3 two-source comparison by a
//! *documented* rule, not by guesswork. The policy is content-addressed
//! so the settled-height rule itself is an audit artifact.

use serde::{Deserialize, Serialize};
use serde_json::json;
use treasury_core::ContentHash;
use treasury_evidence::{canonical_bytes, sha256, CanonError};

use crate::history::Chain;

/// Schema tag committed into every policy hash; bump on change.
pub const FINALITY_SCHEMA: &str = "treasury-chainsource/finality-policy/v1";

/// How a chain decides a block is settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FinalityRule {
    /// Settled = chain tip minus a fixed confirmation depth (Bitcoin).
    ConfirmationDepth {
        /// Required confirmations below the tip.
        depth: u64,
    },
    /// Settled = the height the consensus layer reports finalized
    /// (Ethereum); the caller supplies that height as the observed value.
    ExternalFinalized,
}

/// A per-chain finality policy. Content-addressed: the rule is itself an
/// audit artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinalityPolicy {
    /// The chain this policy governs.
    pub chain: Chain,
    /// The settled-height rule.
    pub rule: FinalityRule,
}

impl FinalityPolicy {
    /// The settled height given an `observed` height: the chain tip for
    /// [`FinalityRule::ConfirmationDepth`], or the consensus-finalized
    /// height for [`FinalityRule::ExternalFinalized`].
    #[must_use]
    pub fn settled_height(&self, observed: u64) -> u64 {
        match self.rule {
            FinalityRule::ConfirmationDepth { depth } => observed.saturating_sub(depth),
            FinalityRule::ExternalFinalized => observed,
        }
    }

    /// The policy's content hash.
    ///
    /// # Errors
    /// [`CanonError`] is structurally unreachable for this envelope but
    /// propagated rather than swallowed.
    pub fn policy_hash(&self) -> Result<ContentHash, CanonError> {
        let envelope = json!({
            "schema": FINALITY_SCHEMA,
            "chain": self.chain,
            "rule": self.rule,
        });
        let bytes = canonical_bytes(&envelope)?;
        Ok(sha256(&bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmation_depth_subtracts_from_tip() {
        let policy = FinalityPolicy {
            chain: Chain::Bitcoin,
            rule: FinalityRule::ConfirmationDepth { depth: 6 },
        };
        assert_eq!(policy.settled_height(100), 94);
        // Never underflows past genesis.
        assert_eq!(policy.settled_height(3), 0);
    }

    #[test]
    fn external_finalized_passes_through() {
        let policy = FinalityPolicy {
            chain: Chain::Ethereum,
            rule: FinalityRule::ExternalFinalized,
        };
        assert_eq!(policy.settled_height(21_000_000), 21_000_000);
    }

    #[test]
    fn hash_distinguishes_chain_and_rule() {
        let btc = FinalityPolicy {
            chain: Chain::Bitcoin,
            rule: FinalityRule::ConfirmationDepth { depth: 6 },
        };
        let eth = FinalityPolicy {
            chain: Chain::Ethereum,
            rule: FinalityRule::ExternalFinalized,
        };
        assert_ne!(btc.policy_hash(), eth.policy_hash());
    }

    /// Golden vector — independently recomputed in Python.
    #[test]
    fn golden_policy_hash_matches_independent_implementation() {
        let policy = FinalityPolicy {
            chain: Chain::Bitcoin,
            rule: FinalityRule::ConfirmationDepth { depth: 6 },
        };
        let hash = policy.policy_hash().map(|h| h.to_hex());
        assert_eq!(
            hash.as_deref(),
            Ok("f953957858193437d33ea6be90c1cf0b777a9a882daddb722c71fc2eb350a7c2"),
        );
    }
}
