//! Leg designation: classifying what a movement *is* when it is not an
//! internal transfer (spec v2 §2.2, §5; remediation gap G-1).
//!
//! Staking rewards, airdrops, and fork proceeds arrive in a design
//! partner's wallets whether or not "yield" is in scope — without a
//! designation path they would be unclassifiable observations that block
//! close forever. Disposals and purchases reach this flow too, via
//! rejected match proposals.
//!
//! Discipline (same as match confirmation):
//! - A designation proposal is content-addressed and commits to the
//!   tenant's **designation policy artifact** (REQ-9) — the document an
//!   auditor reads to see what each class means and how it is treated.
//! - Dual control: the proposing preparer cannot self-confirm.
//! - Only **confirmed** designations book — as L3 judgments with both
//!   actors as approvers. A rejected proposal asserts nothing about what
//!   the leg *is*, so it books nothing; the leg stays a close blocker
//!   until a correct proposal is confirmed. Re-proposing an identical
//!   rejected classification is structurally impossible (same content
//!   hash, already terminal) — a new proposal must differ.

use crate::leg::LegId;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use treasury_core::{ActorId, ContentHash, TenantId, TimestampNs};
use treasury_evidence::{canonical_bytes, sha256, CanonError};
use treasury_ledger::{ClaimLayer, EventDraft, Provenance};

/// Schema tag committed into every designation hash; bump on change.
pub const DESIGNATION_SCHEMA: &str = "treasury-reconcile/designation/v1";

/// How a non-purchase acquisition arose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum NonPurchaseKind {
    /// Staking or validation reward.
    StakingReward,
    /// Airdropped tokens.
    Airdrop,
    /// Proceeds of a chain fork.
    ForkProceeds,
    /// Mining reward.
    MiningReward,
    /// Anything else — requires a written description (fail closed on
    /// vagueness: an empty description rejects at proposal time).
    Other(String),
}

/// What the leg is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "class", content = "detail", rename_all = "snake_case")]
pub enum LegClassification {
    /// A sale or exchange — triggers lot relief and gain/loss.
    Disposal,
    /// A purchase — opens a lot at cost.
    Acquisition,
    /// Received without purchase; income-recognition treatment is the
    /// policy modules' concern (L4), not this layer's.
    NonPurchaseAcquisition(NonPurchaseKind),
}

/// A content-addressed designation proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignationProposal {
    /// The leg being classified.
    pub leg: LegId,
    /// Proposed classification.
    pub classification: LegClassification,
    /// Content hash of the tenant's designation policy artifact (REQ-9).
    pub policy_hash: ContentHash,
}

impl DesignationProposal {
    /// The proposal's content hash — queue item id and judgment evidence.
    ///
    /// # Errors
    /// [`DesignateError::VagueOther`] when an `Other` kind carries an
    /// empty description;
    /// [`DesignateError::Canon`] on envelope failure (structurally
    /// unreachable).
    pub fn proposal_hash(&self) -> Result<ContentHash, DesignateError> {
        if let LegClassification::NonPurchaseAcquisition(NonPurchaseKind::Other(d)) =
            &self.classification
        {
            if d.trim().is_empty() {
                return Err(DesignateError::VagueOther);
            }
        }
        let envelope = json!({
            "schema": DESIGNATION_SCHEMA,
            "leg": self.leg.to_hex(),
            "classification": self.classification.clone(),
            "policy": self.policy_hash.to_hex(),
        });
        let bytes = canonical_bytes(&envelope)?;
        Ok(sha256(&bytes))
    }
}

/// Lifecycle of a designation proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesignationState {
    /// Proposed by a preparer; awaiting a different approver.
    Asserted {
        /// The proposing preparer.
        preparer: ActorId,
    },
    /// Confirmed under dual control. Terminal; books as L3.
    Confirmed {
        /// The proposing preparer.
        preparer: ActorId,
        /// The confirming approver (≠ preparer).
        approver: ActorId,
    },
    /// Rejected. Terminal; books nothing — the leg remains a blocker.
    Rejected {
        /// The rejecting actor.
        actor: ActorId,
        /// Why (recorded for the audit trail of the queue, not the ledger).
        reason: String,
    },
}

impl DesignationState {
    /// Whether the state is terminal.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Confirmed { .. } | Self::Rejected { .. })
    }
}

/// Dual-control designation queue, keyed by proposal hash.
#[derive(Debug, Default)]
pub struct DesignationQueue {
    items: HashMap<ContentHash, (DesignationProposal, DesignationState)>,
}

impl DesignationQueue {
    /// Create an empty queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Propose a classification. The proposer is the preparer; the item
    /// enters `Asserted`. Re-proposing identical content is idempotent
    /// and never resets state.
    ///
    /// # Errors
    /// Propagates [`DesignationProposal::proposal_hash`] errors.
    pub fn propose(
        &mut self,
        proposal: DesignationProposal,
        preparer: ActorId,
    ) -> Result<ContentHash, DesignateError> {
        let id = proposal.proposal_hash()?;
        self.items
            .entry(id)
            .or_insert((proposal, DesignationState::Asserted { preparer }));
        Ok(id)
    }

    /// A second approver confirms. Dual control enforced.
    ///
    /// # Errors
    /// [`DesignateError::UnknownItem`] for unknown ids;
    /// [`DesignateError::InvalidTransition`] unless `Asserted`;
    /// [`DesignateError::DualControlViolation`] when approver == preparer.
    pub fn confirm(&mut self, id: &ContentHash, approver: ActorId) -> Result<(), DesignateError> {
        let (_, state) = self
            .items
            .get_mut(id)
            .ok_or(DesignateError::UnknownItem(*id))?;
        let DesignationState::Asserted { preparer } = state.clone() else {
            return Err(DesignateError::InvalidTransition);
        };
        if preparer == approver {
            return Err(DesignateError::DualControlViolation);
        }
        *state = DesignationState::Confirmed { preparer, approver };
        Ok(())
    }

    /// Reject a proposal. Terminal; the leg remains a close blocker.
    ///
    /// # Errors
    /// [`DesignateError::UnknownItem`] for unknown ids;
    /// [`DesignateError::InvalidTransition`] when already terminal.
    pub fn reject(
        &mut self,
        id: &ContentHash,
        actor: ActorId,
        reason: String,
    ) -> Result<(), DesignateError> {
        let (_, state) = self
            .items
            .get_mut(id)
            .ok_or(DesignateError::UnknownItem(*id))?;
        if state.is_terminal() {
            return Err(DesignateError::InvalidTransition);
        }
        *state = DesignationState::Rejected { actor, reason };
        Ok(())
    }

    /// State of an item.
    #[must_use]
    pub fn state(&self, id: &ContentHash) -> Option<&DesignationState> {
        self.items.get(id).map(|(_, s)| s)
    }

    /// Leg ids with a confirmed designation — the legs no longer blocking
    /// close, in deterministic (sorted) order.
    #[must_use]
    pub fn designated_legs(&self) -> Vec<LegId> {
        let mut legs: Vec<LegId> = Vec::new();
        for (proposal, state) in self.items.values() {
            if matches!(state, DesignationState::Confirmed { .. }) {
                legs.push(proposal.leg);
            }
        }
        legs.sort_unstable();
        legs.dedup();
        legs
    }
}

/// Build the L3 judgment draft for a confirmed designation.
///
/// # Errors
/// [`DesignateError::NotConfirmed`] unless the state is `Confirmed`;
/// propagates proposal-hash errors.
pub fn draft_designation(
    proposal: &DesignationProposal,
    state: &DesignationState,
    tenant: TenantId,
    event_time: TimestampNs,
) -> Result<EventDraft, DesignateError> {
    let DesignationState::Confirmed { preparer, approver } = state else {
        return Err(DesignateError::NotConfirmed);
    };
    let id = proposal.proposal_hash()?;
    let payload = json!({
        "schema": DESIGNATION_SCHEMA,
        "booking": "leg_designation",
        "leg": proposal.leg.to_hex(),
        "classification": proposal.classification.clone(),
        "proposal": id.to_hex(),
    });
    Ok(EventDraft {
        tenant,
        layer: ClaimLayer::Judgment,
        event_time,
        supersedes: None,
        provenance: Provenance::Judgment {
            policy_hash: proposal.policy_hash,
            approvers: vec![preparer.clone(), approver.clone()],
            evidence: vec![id],
        },
        payload,
    })
}

/// Errors from designation operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DesignateError {
    /// `Other` non-purchase kinds require a non-empty description.
    #[error("'other' non-purchase kind requires a written description")]
    VagueOther,
    /// No item under this proposal hash.
    #[error("unknown designation item: {0}")]
    UnknownItem(ContentHash),
    /// The requested transition is not legal from the current state.
    #[error("invalid state transition")]
    InvalidTransition,
    /// Approver and preparer must be different actors.
    #[error("dual-control violation: approver must differ from preparer")]
    DualControlViolation,
    /// Only confirmed designations book.
    #[error("designation is not confirmed; nothing to book")]
    NotConfirmed,
    /// Envelope canonicalization failure.
    #[error(transparent)]
    Canon(#[from] CanonError),
}
