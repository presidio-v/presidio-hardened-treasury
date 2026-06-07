//! Event model: claim layers, layer-specific provenance, and content-derived
//! event identity (spec v2 §3.1).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use treasury_core::{ActorId, ContentHash, SourceId, TenantId, TimestampNs};
use treasury_evidence::{canonical_bytes, sha256, CanonError};

/// Schema tag committed into every event hash; bump on any envelope change.
pub const EVENT_SCHEMA: &str = "treasury-ledger/event/v1";

/// Identity of a sealed event: the SHA-256 of its canonical envelope,
/// which includes the previous event's hash (chain commitment).
pub type EventId = ContentHash;

/// The four claim layers (spec v2 §3.1). Layer determines which provenance
/// is mandatory — the type system enforces the pairing via [`Provenance`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimLayer {
    /// L1 — raw venue/chain payloads, hashed in the evidence store.
    Observation,
    /// L2 — deterministic computations over observations.
    DerivedFact,
    /// L3 — decisions: transfer confirmation, scope designation, elections.
    Judgment,
    /// L4 — policy-module outputs (entries, disclosures).
    PolicyOutput,
}

/// Layer-specific mandatory provenance. The variant must match the claim
/// layer; the ledger rejects mismatches at append time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Provenance {
    /// L1: where the raw payload came from and its evidence-store hash.
    Observation {
        /// Ingestion source (venue API, chain node, price feed).
        source: SourceId,
        /// Evidence-store hash of the raw payload.
        evidence: ContentHash,
    },
    /// L2: the code version that derived this fact and its input events.
    Derived {
        /// Hash of the code version that produced the derivation.
        code_version: ContentHash,
        /// Input events the derivation consumed.
        inputs: Vec<EventId>,
    },
    /// L3: the policy in force, who approved, and supporting evidence.
    Judgment {
        /// Content hash of the policy artifact in force (spec v2 §3.5).
        policy_hash: ContentHash,
        /// Human/service actors who approved (dual-control records two).
        approvers: Vec<ActorId>,
        /// Evidence-store hashes supporting the decision.
        evidence: Vec<ContentHash>,
    },
    /// L4: the policy module version and the events it consumed.
    PolicyOutput {
        /// Content hash of the policy module version.
        policy_hash: ContentHash,
        /// Input events (L1–L3) the output was computed from.
        inputs: Vec<EventId>,
    },
}

impl Provenance {
    /// The claim layer this provenance belongs to.
    #[must_use]
    pub fn layer(&self) -> ClaimLayer {
        match self {
            Self::Observation { .. } => ClaimLayer::Observation,
            Self::Derived { .. } => ClaimLayer::DerivedFact,
            Self::Judgment { .. } => ClaimLayer::Judgment,
            Self::PolicyOutput { .. } => ClaimLayer::PolicyOutput,
        }
    }
}

/// An event proposed for appending — everything except chain position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventDraft {
    /// Tenant whose stream this event joins.
    pub tenant: TenantId,
    /// Claim layer; must match `provenance.layer()`.
    pub layer: ClaimLayer,
    /// When it happened in the world (on-chain time, venue time).
    pub event_time: TimestampNs,
    /// Event this one supersedes (correction), if any.
    pub supersedes: Option<EventId>,
    /// Layer-specific mandatory provenance.
    pub provenance: Provenance,
    /// Domain payload. Floats reject at append (canonicalization rule).
    pub payload: Value,
}

/// An event sealed into the chain: draft + knowledge time + chain commitment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedEvent {
    /// Content-derived identity (includes `prev` — chain commitment).
    pub event_id: EventId,
    /// Hash of the previous event in this tenant's stream (or genesis).
    pub prev: ContentHash,
    /// When the ledger booked it; strictly monotonic per tenant.
    pub knowledge_time: TimestampNs,
    /// The proposed event content.
    pub draft: EventDraft,
}

impl SealedEvent {
    /// Canonical envelope used for hashing. Key order is fixed by
    /// canonicalization; the schema tag is committed so an envelope change
    /// can never collide with v1 hashes.
    ///
    /// # Errors
    /// [`CanonError`] when the payload contains floats or nests too deeply.
    pub fn canonical_envelope(
        prev: &ContentHash,
        knowledge_time: TimestampNs,
        draft: &EventDraft,
    ) -> Result<Vec<u8>, CanonError> {
        let supersedes = draft.supersedes.map(|s| s.to_hex());
        let envelope = json!({
            "schema": EVENT_SCHEMA,
            "prev": prev.to_hex(),
            "tenant": draft.tenant.clone(),
            "layer": draft.layer,
            "event_time": draft.event_time,
            "knowledge_time": knowledge_time,
            "supersedes": supersedes,
            "provenance": draft.provenance.clone(),
            "payload": draft.payload.clone(),
        });
        canonical_bytes(&envelope)
    }

    /// Compute the event id for a draft at a given chain position.
    ///
    /// # Errors
    /// [`CanonError`] when the payload cannot canonicalize.
    pub fn compute_id(
        prev: &ContentHash,
        knowledge_time: TimestampNs,
        draft: &EventDraft,
    ) -> Result<EventId, CanonError> {
        let bytes = Self::canonical_envelope(prev, knowledge_time, draft)?;
        Ok(sha256(&bytes))
    }
}

/// Genesis predecessor hash for a tenant's stream: domain-separated so no
/// real event hash can collide with a genesis marker.
#[must_use]
pub fn genesis_hash(tenant: &TenantId) -> ContentHash {
    let tag = format!("presidio-treasury:genesis:v1:{tenant}");
    sha256(tag.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(payload: Value) -> EventDraft {
        EventDraft {
            tenant: TenantId::new("acme"),
            layer: ClaimLayer::Observation,
            event_time: TimestampNs::from_nanos(1),
            supersedes: None,
            provenance: Provenance::Observation {
                source: SourceId::new("coinbase-prime"),
                evidence: ContentHash([2; 32]),
            },
            payload,
        }
    }

    #[test]
    fn event_id_commits_to_prev() {
        let d = draft(json!({"k": "v"}));
        let t = TimestampNs::from_nanos(10);
        let a = SealedEvent::compute_id(&ContentHash([0; 32]), t, &d);
        let b = SealedEvent::compute_id(&ContentHash([1; 32]), t, &d);
        assert_ne!(a, b, "identity must commit to chain position");
    }

    #[test]
    fn float_payload_rejected() {
        let d = draft(json!({"price": 0.5}));
        let result = SealedEvent::compute_id(&ContentHash([0; 32]), TimestampNs::from_nanos(1), &d);
        assert!(matches!(result, Err(CanonError::FloatRejected(_))));
    }

    #[test]
    fn genesis_differs_per_tenant() {
        assert_ne!(
            genesis_hash(&TenantId::new("a")),
            genesis_hash(&TenantId::new("b"))
        );
    }
}
