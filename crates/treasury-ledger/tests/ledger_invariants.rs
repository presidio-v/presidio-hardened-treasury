//! Integration tests for the Phase 0 structural guarantees (spec v2 §7
//! Phase 0 exit: any historical figure reproduces byte-for-byte from
//! hashed inputs).

use serde_json::json;
use treasury_core::{ActorId, ContentHash, SourceId, TenantId, TimestampNs};
use treasury_ledger::{ClaimLayer, EventDraft, InMemoryLedger, Ledger, LedgerError, Provenance};

fn tenant() -> TenantId {
    TenantId::new("acme")
}

fn observation(payload: serde_json::Value) -> EventDraft {
    EventDraft {
        tenant: tenant(),
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

fn judgment(supersedes: Option<treasury_ledger::EventId>) -> EventDraft {
    EventDraft {
        tenant: tenant(),
        layer: ClaimLayer::Judgment,
        event_time: TimestampNs::from_nanos(5),
        supersedes,
        provenance: Provenance::Judgment {
            policy_hash: ContentHash([9; 32]),
            approvers: vec![ActorId::new("preparer"), ActorId::new("approver")],
            evidence: vec![ContentHash([2; 32])],
        },
        payload: json!({"classification": "internal_transfer"}),
    }
}

fn ts(n: i64) -> TimestampNs {
    TimestampNs::from_nanos(n)
}

/// Golden vector — event ids independently recomputed in Python from the
/// documented envelope definition (sorted-key canonical JSON, SHA-256,
/// genesis = SHA-256("presidio-treasury:genesis:v1:acme")). If this test
/// breaks, the hash definition changed and EVENT_SCHEMA must be bumped.
#[test]
fn event_ids_match_independent_implementation() {
    let mut ledger = InMemoryLedger::new();
    let id1 = ledger
        .append(observation(json!({"k": "v"})), ts(10))
        .unwrap_or(ContentHash([0; 32]));
    assert_eq!(
        id1.to_hex(),
        "bc06fc114f0ed7629aa40fb3e581eca560b28f60e6bb1211fc99748993719261"
    );

    let mut correction = observation(json!({"k": "v2"}));
    correction.supersedes = Some(id1);
    let id2 = ledger
        .append(correction, ts(20))
        .unwrap_or(ContentHash([0; 32]));
    assert_eq!(
        id2.to_hex(),
        "ed4a35dcf746d2df178a76de954502ec9e7621f165e86721cff1dde7ecd60c3d"
    );

    assert_eq!(ledger.verify_chain(&tenant()), Ok(()));
}

#[test]
fn bitemporal_as_of_resolves_supersession() {
    let mut ledger = InMemoryLedger::new();
    let id1 = ledger
        .append(observation(json!({"qty": "10"})), ts(10))
        .unwrap_or(ContentHash([0; 32]));

    let mut correction = observation(json!({"qty": "9"}));
    correction.supersedes = Some(id1);
    let id2 = ledger
        .append(correction, ts(20))
        .unwrap_or(ContentHash([0; 32]));

    // As of knowledge time 15 (e.g. the 10-Q filing) the original governs.
    let at_filing = ledger.as_of(&tenant(), ts(15));
    assert_eq!(at_filing.len(), 1);
    assert_eq!(at_filing.first().map(|e| e.event_id), Some(id1));

    // As of now, the correction governs; the original is superseded but
    // still present in the stream (append-only).
    let now = ledger.as_of(&tenant(), ts(30));
    assert_eq!(now.len(), 1);
    assert_eq!(now.first().map(|e| e.event_id), Some(id2));
    assert_eq!(ledger.stream(&tenant()).len(), 2);
}

#[test]
fn knowledge_time_strictly_monotonic() {
    let mut ledger = InMemoryLedger::new();
    let _ = ledger.append(observation(json!({})), ts(10));
    let result = ledger.append(observation(json!({})), ts(10));
    assert!(matches!(
        result,
        Err(LedgerError::NonMonotonicKnowledgeTime { .. })
    ));
}

#[test]
fn layer_provenance_mismatch_rejected() {
    let mut ledger = InMemoryLedger::new();
    let mut bad = observation(json!({}));
    bad.layer = ClaimLayer::Judgment; // observation provenance, judgment layer
    assert!(matches!(
        ledger.append(bad, ts(10)),
        Err(LedgerError::LayerProvenanceMismatch { .. })
    ));
}

#[test]
fn double_supersession_rejected() {
    let mut ledger = InMemoryLedger::new();
    let id1 = ledger
        .append(judgment(None), ts(10))
        .unwrap_or(ContentHash([0; 32]));
    let _ = ledger.append(judgment(Some(id1)), ts(20));
    let result = ledger.append(judgment(Some(id1)), ts(30));
    assert_eq!(result, Err(LedgerError::AlreadySuperseded(id1)));
}

#[test]
fn cross_layer_supersession_rejected() {
    let mut ledger = InMemoryLedger::new();
    let obs_id = ledger
        .append(observation(json!({})), ts(10))
        .unwrap_or(ContentHash([0; 32]));
    let result = ledger.append(judgment(Some(obs_id)), ts(20));
    assert!(matches!(
        result,
        Err(LedgerError::SupersedeCrossLayer { .. })
    ));
}

#[test]
fn cross_tenant_supersession_rejected() {
    let mut ledger = InMemoryLedger::new();
    let id1 = ledger
        .append(observation(json!({})), ts(10))
        .unwrap_or(ContentHash([0; 32]));

    let mut foreign = observation(json!({}));
    foreign.tenant = TenantId::new("other");
    foreign.supersedes = Some(id1);
    assert_eq!(
        ledger.append(foreign, ts(20)),
        Err(LedgerError::SupersedeCrossTenant(id1))
    );
}

#[test]
fn float_payload_rejected_at_append() {
    let mut ledger = InMemoryLedger::new();
    let result = ledger.append(observation(json!({"price": 1.23})), ts(10));
    assert!(matches!(result, Err(LedgerError::Canon(_))));
}

#[test]
fn tenant_streams_are_independent_chains() {
    let mut ledger = InMemoryLedger::new();
    let _ = ledger.append(observation(json!({"n": 1})), ts(10));

    let mut other = observation(json!({"n": 1}));
    other.tenant = TenantId::new("other");
    // Same knowledge time is legal across tenants — monotonicity is per stream.
    let _ = ledger.append(other, ts(10));

    assert_eq!(ledger.verify_chain(&tenant()), Ok(()));
    assert_eq!(ledger.verify_chain(&TenantId::new("other")), Ok(()));
    assert_eq!(ledger.stream(&tenant()).len(), 1);
    assert_eq!(ledger.stream(&TenantId::new("other")).len(), 1);
}
