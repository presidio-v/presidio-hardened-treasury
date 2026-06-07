//! Reproducible state folding (spec v2 §3.6, Phase 0 exit criterion).
//!
//! The state root commits to the exact set of ledger events active at the
//! checkpoint's knowledge time. Recomputing `as_of` at the same knowledge
//! time on the same (tamper-verified) stream must reproduce the root
//! byte-for-byte — that is the Phase 0 exit test.

use serde_json::json;
use treasury_core::ContentHash;
use treasury_evidence::{canonical_bytes, sha256, CanonError};
use treasury_ledger::SealedEvent;

/// Schema tag committed into every state root.
pub const STATE_SCHEMA: &str = "treasury-close/state/v1";

/// Commitment to the active event set at a knowledge time, in stream
/// order (the order is part of the commitment).
///
/// # Errors
/// [`CanonError`] is structurally unreachable for this envelope but
/// propagated rather than swallowed.
pub fn state_root(active_events: &[&SealedEvent]) -> Result<ContentHash, CanonError> {
    let ids: Vec<String> = active_events.iter().map(|e| e.event_id.to_hex()).collect();
    let envelope = json!({
        "schema": STATE_SCHEMA,
        "events": ids,
    });
    let bytes = canonical_bytes(&envelope)?;
    Ok(sha256(&bytes))
}
