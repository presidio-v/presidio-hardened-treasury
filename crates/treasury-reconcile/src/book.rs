//! Booking: turning match decisions into ledger events (spec v2 §5).
//!
//! The layering is the point:
//! - **Auto-nets are L2 derived facts.** A tier-0 or below-materiality
//!   tier-1 net is a deterministic computation over observations — its
//!   provenance is the matcher config hash (as the code version) and the
//!   two leg events. No human exists to name, and pretending the matcher
//!   is an "approver" would corrupt the judgment layer.
//! - **Human resolutions are L3 judgments.** A confirmed or rejected
//!   queue item carries the actual approver identities (dual control:
//!   preparer + approver) and the decision hash as evidence.
//!
//! Either way the decision is replayable: the envelope commits to the
//! matcher config hash, and the ledger's append-time validation enforces
//! the layer/provenance pairing structurally.

use crate::decision::{Disposition, MatchProposal};
use crate::queue::QueueState;
use serde_json::json;
use treasury_core::{TenantId, TimestampNs};
use treasury_evidence::CanonError;
use treasury_ledger::{ClaimLayer, EventDraft, Provenance};

/// Schema tag inside every booking payload; bump on shape change.
pub const BOOKING_SCHEMA: &str = "treasury-reconcile/booking/v1";

/// Build the L2 derived-fact draft for an auto-netted proposal.
///
/// # Errors
/// [`BookError::NotAutoNet`] when the proposal was queued instead;
/// [`BookError::Canon`] on envelope failure (structurally unreachable).
pub fn draft_auto_net(
    proposal: &MatchProposal,
    tenant: TenantId,
    event_time: TimestampNs,
) -> Result<EventDraft, BookError> {
    if proposal.disposition != Disposition::AutoNet {
        return Err(BookError::NotAutoNet);
    }
    let payload = json!({
        "schema": BOOKING_SCHEMA,
        "booking": "internal_transfer_net",
        "resolution": "auto",
        "out_leg": proposal.out_leg.to_hex(),
        "in_leg": proposal.in_leg.to_hex(),
        "tier": proposal.tier,
        "decision": proposal.decision_hash()?.to_hex(),
    });
    Ok(EventDraft {
        tenant,
        layer: ClaimLayer::DerivedFact,
        event_time,
        supersedes: None,
        provenance: Provenance::Derived {
            code_version: proposal.config_hash,
            inputs: vec![proposal.out_leg, proposal.in_leg],
        },
        payload,
    })
}

/// Build the L3 judgment draft for a terminally resolved queue item.
///
/// Confirmed items book as internal-transfer nets with both dual-control
/// actors as approvers; rejected items book as explicit
/// not-internal-transfer judgments (the legs then await their own
/// classification — disposal, acquisition, or non-purchase acquisition).
///
/// # Errors
/// [`BookError::NotTerminal`] for pending/asserted items;
/// [`BookError::Canon`] on envelope failure (structurally unreachable).
pub fn draft_resolution(
    proposal: &MatchProposal,
    state: &QueueState,
    tenant: TenantId,
    event_time: TimestampNs,
) -> Result<EventDraft, BookError> {
    let decision = proposal.decision_hash()?;
    match state {
        QueueState::Confirmed { preparer, approver } => {
            let payload = json!({
                "schema": BOOKING_SCHEMA,
                "booking": "internal_transfer_net",
                "resolution": "confirmed",
                "out_leg": proposal.out_leg.to_hex(),
                "in_leg": proposal.in_leg.to_hex(),
                "tier": proposal.tier,
                "decision": decision.to_hex(),
            });
            Ok(EventDraft {
                tenant,
                layer: ClaimLayer::Judgment,
                event_time,
                supersedes: None,
                provenance: Provenance::Judgment {
                    policy_hash: proposal.config_hash,
                    approvers: vec![preparer.clone(), approver.clone()],
                    evidence: vec![decision],
                },
                payload,
            })
        }
        QueueState::Rejected { actor, reason } => {
            let payload = json!({
                "schema": BOOKING_SCHEMA,
                "booking": "not_internal_transfer",
                "resolution": "rejected",
                "out_leg": proposal.out_leg.to_hex(),
                "in_leg": proposal.in_leg.to_hex(),
                "tier": proposal.tier,
                "decision": decision.to_hex(),
                "reason": reason,
            });
            Ok(EventDraft {
                tenant,
                layer: ClaimLayer::Judgment,
                event_time,
                supersedes: None,
                provenance: Provenance::Judgment {
                    policy_hash: proposal.config_hash,
                    approvers: vec![actor.clone()],
                    evidence: vec![decision],
                },
                payload,
            })
        }
        QueueState::Pending | QueueState::Asserted { .. } => Err(BookError::NotTerminal),
    }
}

/// Errors building booking drafts.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BookError {
    /// Only auto-net proposals book as derived facts.
    #[error("proposal was queued; book it via its queue resolution")]
    NotAutoNet,
    /// Only terminal queue states book as judgments.
    #[error("queue item is not terminal; resolve it first")]
    NotTerminal,
    /// Envelope canonicalization failure.
    #[error(transparent)]
    Canon(#[from] CanonError),
}
