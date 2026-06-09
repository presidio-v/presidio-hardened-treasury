//! Mapping an agreed reconciliation into an L1 observation (ADR-0004
//! action item 5: the minimal, golden-vectored indexer-output →
//! observation mapping — the residual single point across both sources,
//! kept small and cross-verified).
//!
//! Only an **agreed** reconciliation books: a divergence blocks close, so
//! there is nothing to observe until a human resolves it. The booked
//! event is an L1 observation whose payload commits to the
//! two-source-agreed history hash, and whose provenance references the
//! evidence-store hash of the raw indexer payloads (the caller stores the
//! raw bytes; storing them is the I/O layer's job).

use crate::reconcile::Reconciliation;
use serde_json::json;
use treasury_core::{ContentHash, SourceId, TenantId, TimestampNs};
use treasury_ledger::{ClaimLayer, EventDraft, Provenance};

/// Schema tag committed into every observation payload; bump on change.
pub const OBSERVATION_SCHEMA: &str = "treasury-chainsource/observation/v1";

/// Draft the L1 observation for an agreed address-history reconciliation.
///
/// The payload commits to the agreed (two-source-confirmed) history hash,
/// the chain, the address, the settled height, and the movement count;
/// the full history is recoverable from the evidence store. Provenance is
/// an [`Provenance::Observation`] referencing `raw_evidence` — the
/// evidence-store hash of the raw indexer payloads behind the agreement.
///
/// # Errors
/// [`BookError::Diverged`] when the reconciliation is not agreed (a
/// divergence blocks close and books nothing).
pub fn draft_history_observation(
    reconciliation: &Reconciliation,
    tenant: TenantId,
    source_id: impl Into<String>,
    raw_evidence: ContentHash,
    observed_at: TimestampNs,
) -> Result<EventDraft, BookError> {
    let Reconciliation::Agreed {
        history,
        history_hash,
    } = reconciliation
    else {
        return Err(BookError::Diverged);
    };
    let movement_count = u64::try_from(history.movements.len()).unwrap_or(u64::MAX);
    let payload = json!({
        "schema": OBSERVATION_SCHEMA,
        "booking": "chain_address_history",
        "chain": history.chain,
        "address": history.address.clone(),
        "settled_to_height": history.settled_to_height.to_string(),
        "history": history_hash.to_hex(),
        "movement_count": movement_count.to_string(),
    });
    Ok(EventDraft {
        tenant,
        layer: ClaimLayer::Observation,
        event_time: observed_at,
        supersedes: None,
        provenance: Provenance::Observation {
            source: SourceId::new(source_id),
            evidence: raw_evidence,
        },
        payload,
    })
}

/// Errors from booking.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BookError {
    /// The reconciliation diverged; close is blocked and nothing books
    /// until a human resolves the divergence.
    #[error("reconciliation diverged; nothing to book until resolved")]
    Diverged,
}
