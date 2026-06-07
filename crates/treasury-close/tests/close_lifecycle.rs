//! End-to-end close lifecycle: ledger → fold → checkpoint → late data →
//! reason-coded supersession → as-filed vs as-corrected (spec v2 §3.6).

use serde_json::json;
use treasury_close::{
    state_root, CheckpointDag, CheckpointDraft, CloseError, PeriodId, SupersessionReason,
};
use treasury_core::{ContentHash, SourceId, TenantId, TimestampNs};
use treasury_ledger::{ClaimLayer, EventDraft, InMemoryLedger, Ledger, Provenance};

fn tenant() -> TenantId {
    TenantId::new("acme")
}

fn period() -> PeriodId {
    PeriodId::new("2026Q2")
}

fn ts(n: i64) -> TimestampNs {
    TimestampNs::from_nanos(n)
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

fn initial_draft(root: ContentHash, as_of: TimestampNs) -> CheckpointDraft {
    CheckpointDraft {
        tenant: tenant(),
        period: period(),
        as_of,
        state_root: root,
        supersedes: None,
        reason: None,
        materiality_memo: None,
    }
}

#[test]
fn full_lifecycle_with_late_data_correction() {
    let mut ledger = InMemoryLedger::new();
    let id1 = ledger
        .append(observation(json!({"qty": "10"})), ts(10))
        .unwrap_or(ContentHash([0; 32]));

    // Close the quarter at knowledge time 100.
    let filed_root = state_root(&ledger.as_of(&tenant(), ts(100))).unwrap_or(ContentHash([0; 32]));
    let mut dag = CheckpointDag::new();
    let filed_id = dag
        .seal(initial_draft(filed_root, ts(100)), ts(100))
        .unwrap_or(ContentHash([0; 32]));

    // Late data: the venue revises the observation after filing.
    let mut correction = observation(json!({"qty": "9"}));
    correction.supersedes = Some(id1);
    let _ = ledger.append(correction, ts(200));

    // Re-fold and supersede with reason + materiality memo.
    let corrected_root =
        state_root(&ledger.as_of(&tenant(), ts(200))).unwrap_or(ContentHash([0; 32]));
    assert_ne!(filed_root, corrected_root);
    let corrected_id = dag
        .seal(
            CheckpointDraft {
                tenant: tenant(),
                period: period(),
                as_of: ts(200),
                state_root: corrected_root,
                supersedes: Some(filed_id),
                reason: Some(SupersessionReason::ProviderRevision),
                materiality_memo: Some(ContentHash([9; 32])),
            },
            ts(200),
        )
        .unwrap_or(ContentHash([0; 32]));

    // As-filed is permanent; head is the correction; lineage has both.
    assert_eq!(
        dag.as_filed(&tenant(), &period()).map(|c| c.checkpoint_id),
        Some(filed_id)
    );
    assert_eq!(
        dag.head(&tenant(), &period()).map(|c| c.checkpoint_id),
        Some(corrected_id)
    );
    assert_eq!(dag.lineage(&tenant(), &period()).len(), 2);

    // Phase 0 exit criterion: re-folding at the filing knowledge time
    // reproduces the as-filed state root byte-for-byte.
    let refold = state_root(&ledger.as_of(&tenant(), ts(100))).unwrap_or(ContentHash([1; 32]));
    assert_eq!(refold, filed_root);
    assert_eq!(ledger.verify_chain(&tenant()), Ok(()));
}

#[test]
fn supersession_without_justification_rejected() {
    let mut dag = CheckpointDag::new();
    let filed = dag
        .seal(initial_draft(ContentHash([1; 32]), ts(100)), ts(100))
        .unwrap_or(ContentHash([0; 32]));

    let mut missing_memo = initial_draft(ContentHash([2; 32]), ts(200));
    missing_memo.supersedes = Some(filed);
    missing_memo.reason = Some(SupersessionReason::LateData);
    assert_eq!(
        dag.seal(missing_memo, ts(200)),
        Err(CloseError::SupersessionWithoutJustification)
    );
}

#[test]
fn initial_checkpoint_with_correction_fields_rejected() {
    let mut dag = CheckpointDag::new();
    let mut bad = initial_draft(ContentHash([1; 32]), ts(100));
    bad.reason = Some(SupersessionReason::LateData);
    assert_eq!(
        dag.seal(bad, ts(100)),
        Err(CloseError::InitialWithCorrectionFields)
    );
}

#[test]
fn parallel_initial_checkpoints_rejected() {
    let mut dag = CheckpointDag::new();
    let _ = dag.seal(initial_draft(ContentHash([1; 32]), ts(100)), ts(100));
    assert_eq!(
        dag.seal(initial_draft(ContentHash([2; 32]), ts(200)), ts(200)),
        Err(CloseError::ParallelInitialCheckpoint)
    );
}

#[test]
fn double_supersession_rejected() {
    let mut dag = CheckpointDag::new();
    let filed = dag
        .seal(initial_draft(ContentHash([1; 32]), ts(100)), ts(100))
        .unwrap_or(ContentHash([0; 32]));

    let correction = |root: u8, at: i64| CheckpointDraft {
        tenant: tenant(),
        period: period(),
        as_of: ts(at),
        state_root: ContentHash([root; 32]),
        supersedes: Some(filed),
        reason: Some(SupersessionReason::LateData),
        materiality_memo: Some(ContentHash([9; 32])),
    };
    let _ = dag.seal(correction(2, 200), ts(200));
    assert_eq!(
        dag.seal(correction(3, 300), ts(300)),
        Err(CloseError::AlreadySuperseded(filed))
    );
}

#[test]
fn cross_period_supersession_rejected() {
    let mut dag = CheckpointDag::new();
    let q2 = dag
        .seal(initial_draft(ContentHash([1; 32]), ts(100)), ts(100))
        .unwrap_or(ContentHash([0; 32]));

    let cross = CheckpointDraft {
        tenant: tenant(),
        period: PeriodId::new("2026Q3"),
        as_of: ts(200),
        state_root: ContentHash([2; 32]),
        supersedes: Some(q2),
        reason: Some(SupersessionReason::LateData),
        materiality_memo: Some(ContentHash([9; 32])),
    };
    assert_eq!(
        dag.seal(cross, ts(200)),
        Err(CloseError::SupersedeAcrossLineage(q2))
    );
}

#[test]
fn state_root_commits_to_order_and_content() {
    let mut ledger = InMemoryLedger::new();
    let _ = ledger.append(observation(json!({"n": "1"})), ts(10));
    let _ = ledger.append(observation(json!({"n": "2"})), ts(20));

    let at_10 = state_root(&ledger.as_of(&tenant(), ts(10))).unwrap_or(ContentHash([0; 32]));
    let at_20 = state_root(&ledger.as_of(&tenant(), ts(20))).unwrap_or(ContentHash([0; 32]));
    assert_ne!(at_10, at_20);
}
