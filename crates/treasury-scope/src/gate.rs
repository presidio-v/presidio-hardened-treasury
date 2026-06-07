//! The gate itself: only a confirmed, all-six-met assessment lets an
//! asset proceed to valuation. Everything else blocks, loudly.

use crate::assessment::{ScopeAssessment, ScopeError, ScopeVerdict};
use serde_json::json;
use std::collections::HashMap;
use treasury_core::{
    ActorId, AssetId, ContentHash, DualControlError, DualControlQueue, DualControlState, TenantId,
    TimestampNs,
};
use treasury_ledger::{ClaimLayer, EventDraft, Provenance};

/// What the gate says about an asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateDecision {
    /// A confirmed assessment found all six criteria met.
    Proceed {
        /// The governing assessment.
        assessment: ContentHash,
    },
    /// A confirmed assessment found the asset out of scope.
    BlockedOutOfScope {
        /// The governing assessment.
        assessment: ContentHash,
        /// The derived verdict (with failing criteria).
        verdict: ScopeVerdict,
    },
    /// No confirmed assessment exists — fail closed.
    BlockedNoAssessment,
}

/// Dual-controlled scope gate, one verdict per (tenant, asset).
#[derive(Debug, Default)]
pub struct ScopeGate {
    queue: DualControlQueue<ScopeAssessment>,
    confirmed: HashMap<(TenantId, AssetId), ContentHash>,
}

impl ScopeGate {
    /// Create an empty gate.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Propose an assessment; the proposer is the preparer.
    ///
    /// # Errors
    /// Propagates [`ScopeAssessment::assessment_hash`] errors.
    pub fn propose(
        &mut self,
        assessment: ScopeAssessment,
        preparer: ActorId,
    ) -> Result<ContentHash, ScopeError> {
        let id = assessment.assessment_hash()?;
        self.queue.propose(id, assessment, preparer);
        Ok(id)
    }

    /// A different approver confirms; the assessment becomes the
    /// governing one for its (tenant, asset). Later confirmations
    /// supersede earlier ones (reassessment under a revised policy).
    ///
    /// # Errors
    /// Propagates [`DualControlError`] (unknown id, bad transition,
    /// dual-control violation).
    pub fn confirm(&mut self, id: &ContentHash, approver: ActorId) -> Result<(), DualControlError> {
        self.queue.confirm(id, approver)?;
        if let Some(assessment) = self.queue.payload(id) {
            let key = (assessment.tenant.clone(), assessment.asset.clone());
            self.confirmed.insert(key, *id);
        }
        Ok(())
    }

    /// Reject a proposed assessment.
    ///
    /// # Errors
    /// Propagates [`DualControlError`].
    pub fn reject(
        &mut self,
        id: &ContentHash,
        actor: ActorId,
        reason: String,
    ) -> Result<(), DualControlError> {
        self.queue.reject(id, actor, reason)
    }

    /// The gate decision for an asset. Fail closed: no confirmed
    /// assessment means blocked.
    #[must_use]
    pub fn check(&self, tenant: &TenantId, asset: &AssetId) -> GateDecision {
        let key = (tenant.clone(), asset.clone());
        let Some(id) = self.confirmed.get(&key) else {
            return GateDecision::BlockedNoAssessment;
        };
        let Some(assessment) = self.queue.payload(id) else {
            return GateDecision::BlockedNoAssessment;
        };
        match assessment.verdict() {
            ScopeVerdict::InScope => GateDecision::Proceed { assessment: *id },
            verdict @ ScopeVerdict::OutOfScope { .. } => GateDecision::BlockedOutOfScope {
                assessment: *id,
                verdict,
            },
        }
    }

    /// State of a queue item (for UI / audit surfaces).
    #[must_use]
    pub fn state(&self, id: &ContentHash) -> Option<&DualControlState> {
        self.queue.state(id)
    }
}

/// Build the L3 judgment draft for a confirmed assessment. Both in-scope
/// and out-of-scope verdicts book — an explicit out-of-scope designation
/// is itself an audit artifact (spec v2 §2.3).
///
/// # Errors
/// [`ScopeError::NotConfirmed`] unless the state is `Confirmed`;
/// propagates assessment-hash errors.
pub fn draft_scope_judgment(
    assessment: &ScopeAssessment,
    state: &DualControlState,
    event_time: TimestampNs,
) -> Result<EventDraft, ScopeError> {
    let DualControlState::Confirmed { preparer, approver } = state else {
        return Err(ScopeError::NotConfirmed);
    };
    let id = assessment.assessment_hash()?;
    let payload = json!({
        "schema": crate::assessment::ASSESSMENT_SCHEMA,
        "booking": "scope_designation",
        "asset": assessment.asset.clone(),
        "criteria": assessment.criteria,
        "verdict": assessment.verdict(),
        "assessment": id.to_hex(),
    });
    Ok(EventDraft {
        tenant: assessment.tenant.clone(),
        layer: ClaimLayer::Judgment,
        event_time,
        supersedes: None,
        provenance: Provenance::Judgment {
            policy_hash: assessment.policy_hash,
            approvers: vec![preparer.clone(), approver.clone()],
            evidence: vec![id],
        },
        payload,
    })
}
