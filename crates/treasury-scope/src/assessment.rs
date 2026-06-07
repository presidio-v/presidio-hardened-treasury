//! Six-criteria scope assessments, content-addressed.

use serde::{Deserialize, Serialize};
use serde_json::json;
use treasury_core::{AssetId, ContentHash, TenantId};
use treasury_evidence::{canonical_bytes, sha256, CanonError};

/// Schema tag committed into every assessment hash; bump on change.
pub const ASSESSMENT_SCHEMA: &str = "treasury-scope/assessment/v1";

/// The six ASU 2023-08 (ASC 350-60-15-1) criteria.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Criterion {
    /// Meets the definition of an intangible asset.
    IntangibleAsset,
    /// Does not provide enforceable rights to underlying goods, services,
    /// or other assets.
    NoEnforceableClaim,
    /// Created or resides on a distributed ledger / blockchain.
    OnDistributedLedger,
    /// Secured through cryptography.
    CryptographicallySecured,
    /// Fungible.
    Fungible,
    /// Not created or issued by the reporting entity or related parties.
    NotSelfIssued,
}

/// Status of one criterion. `Undetermined` is never in-scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CriterionStatus {
    /// Positively established.
    Met,
    /// Positively failed.
    NotMet,
    /// Could not be established — fails closed.
    Undetermined,
}

/// All six criteria, mandatory by construction: an assessment that
/// "forgot" one cannot exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CriteriaAssessment {
    /// Intangible asset.
    pub intangible_asset: CriterionStatus,
    /// No enforceable claim on other assets.
    pub no_enforceable_claim: CriterionStatus,
    /// Resides on a distributed ledger.
    pub on_distributed_ledger: CriterionStatus,
    /// Secured through cryptography.
    pub cryptographically_secured: CriterionStatus,
    /// Fungible.
    pub fungible: CriterionStatus,
    /// Not self-issued.
    pub not_self_issued: CriterionStatus,
}

impl CriteriaAssessment {
    fn entries(self) -> [(Criterion, CriterionStatus); 6] {
        [
            (Criterion::IntangibleAsset, self.intangible_asset),
            (Criterion::NoEnforceableClaim, self.no_enforceable_claim),
            (Criterion::OnDistributedLedger, self.on_distributed_ledger),
            (
                Criterion::CryptographicallySecured,
                self.cryptographically_secured,
            ),
            (Criterion::Fungible, self.fungible),
            (Criterion::NotSelfIssued, self.not_self_issued),
        ]
    }
}

/// Verdict derived from an assessment. Derived, never stored — the
/// assessment is the fact, the verdict is arithmetic over it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum ScopeVerdict {
    /// All six criteria positively met.
    InScope,
    /// Anything else; the asset hard-blocks before valuation.
    OutOfScope {
        /// Criteria positively failed.
        not_met: Vec<Criterion>,
        /// Criteria that could not be established (fail closed).
        undetermined: Vec<Criterion>,
    },
}

/// A content-addressed scope assessment for one asset of one tenant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeAssessment {
    /// Tenant whose books the assessment governs.
    pub tenant: TenantId,
    /// Asset under assessment.
    pub asset: AssetId,
    /// The six criteria.
    pub criteria: CriteriaAssessment,
    /// Content hash of the tenant's scope policy artifact (REQ-9) — the
    /// document defining how each criterion is evaluated.
    pub policy_hash: ContentHash,
}

impl ScopeAssessment {
    /// The assessment's content hash — queue id and judgment evidence.
    ///
    /// # Errors
    /// [`ScopeError::Canon`] on envelope failure (structurally
    /// unreachable).
    pub fn assessment_hash(&self) -> Result<ContentHash, ScopeError> {
        let envelope = json!({
            "schema": ASSESSMENT_SCHEMA,
            "tenant": self.tenant.clone(),
            "asset": self.asset.clone(),
            "criteria": self.criteria,
            "policy": self.policy_hash.to_hex(),
        });
        let bytes = canonical_bytes(&envelope)?;
        Ok(sha256(&bytes))
    }

    /// Derive the verdict: in scope iff all six criteria are `Met`.
    #[must_use]
    pub fn verdict(&self) -> ScopeVerdict {
        let mut not_met = Vec::new();
        let mut undetermined = Vec::new();
        for (criterion, status) in self.criteria.entries() {
            match status {
                CriterionStatus::Met => {}
                CriterionStatus::NotMet => not_met.push(criterion),
                CriterionStatus::Undetermined => undetermined.push(criterion),
            }
        }
        if not_met.is_empty() && undetermined.is_empty() {
            ScopeVerdict::InScope
        } else {
            ScopeVerdict::OutOfScope {
                not_met,
                undetermined,
            }
        }
    }
}

/// Errors from assessment operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ScopeError {
    /// Only confirmed assessments book.
    #[error("assessment is not confirmed; nothing to book")]
    NotConfirmed,
    /// Envelope canonicalization failure.
    #[error(transparent)]
    Canon(#[from] CanonError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_met() -> CriteriaAssessment {
        CriteriaAssessment {
            intangible_asset: CriterionStatus::Met,
            no_enforceable_claim: CriterionStatus::Met,
            on_distributed_ledger: CriterionStatus::Met,
            cryptographically_secured: CriterionStatus::Met,
            fungible: CriterionStatus::Met,
            not_self_issued: CriterionStatus::Met,
        }
    }

    fn assessment(criteria: CriteriaAssessment) -> ScopeAssessment {
        ScopeAssessment {
            tenant: TenantId::new("acme"),
            asset: AssetId::new("BTC"),
            criteria,
            policy_hash: ContentHash([9; 32]),
        }
    }

    #[test]
    fn all_met_is_in_scope() {
        assert_eq!(assessment(all_met()).verdict(), ScopeVerdict::InScope);
    }

    #[test]
    fn stablecoin_with_enforceable_claim_is_out() {
        let mut criteria = all_met();
        criteria.no_enforceable_claim = CriterionStatus::NotMet;
        let ScopeVerdict::OutOfScope { not_met, .. } = assessment(criteria).verdict() else {
            unreachable!("must be out of scope");
        };
        assert_eq!(not_met, vec![Criterion::NoEnforceableClaim]);
    }

    #[test]
    fn undetermined_fails_closed() {
        let mut criteria = all_met();
        criteria.fungible = CriterionStatus::Undetermined;
        let ScopeVerdict::OutOfScope { undetermined, .. } = assessment(criteria).verdict() else {
            unreachable!("undetermined must fail closed");
        };
        assert_eq!(undetermined, vec![Criterion::Fungible]);
    }

    #[test]
    fn hash_commits_to_criteria() {
        let a = assessment(all_met());
        let mut weaker = all_met();
        weaker.not_self_issued = CriterionStatus::Undetermined;
        let b = assessment(weaker);
        assert_ne!(a.assessment_hash(), b.assessment_hash());
    }

    /// Golden vector — independently recomputed in Python.
    #[test]
    fn golden_hash_matches_independent_implementation() {
        let hash = assessment(all_met()).assessment_hash().map(|h| h.to_hex());
        assert_eq!(
            hash.as_deref(),
            Ok("216f37371ea503b94c319fe1da102e66cf0d9c56f2a26378c3c50a617e037e8c")
        );
    }
}
