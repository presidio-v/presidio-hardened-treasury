//! Anchoring pipeline end to end (ADR-0002): aggregate several heads,
//! submit, confirm, gate on depth, finalize to receipts, append to the
//! log, and verify the log against the live store. Plus liveness and
//! no-guessing-transition discipline.

use treasury_anchor::{
    aggregate, verify_inclusion, AnchorLog, AnchorPipeline, AnchorTarget, PipelineError,
    PipelineState,
};
use treasury_core::{ContentHash, TimestampNs};
use treasury_evidence::{sha256, EvidenceStore, InMemoryEvidenceStore};

fn ts(n: i64) -> TimestampNs {
    TimestampNs::from_nanos(n)
}

fn targets() -> Vec<AnchorTarget> {
    // Three independent stores' heads, anchored together in one tx.
    vec![
        AnchorTarget {
            tree_head: sha256(b"tenant-a-head"),
            entry_count: 12,
        },
        AnchorTarget {
            tree_head: sha256(b"tenant-b-head"),
            entry_count: 4,
        },
        AnchorTarget {
            tree_head: sha256(b"tenant-c-head"),
            entry_count: 99,
        },
    ]
}

#[test]
fn full_lifecycle_aggregates_submits_confirms_finalizes() {
    let Ok(mut pipeline) = AnchorPipeline::start(targets()) else {
        unreachable!("non-empty targets");
    };
    assert_eq!(pipeline.state(), &PipelineState::Pending);

    // The chain adapter broadcasts the root at height 1000.
    let Ok(()) = pipeline.submitted("btc-tx-abc".to_owned(), 1_000) else {
        unreachable!("submit from pending");
    };
    // Not yet confirmed → not overdue within the window.
    assert!(!pipeline.is_overdue(1_003, 6));

    // Included at 1001; chain now at 1006 → depth 6.
    let Ok(()) = pipeline.confirmed(1_001, 1_006) else {
        unreachable!("confirm from submitted");
    };

    // Require depth 6; finalize emits one receipt per target.
    let proof_evidence = sha256(b"calendar-independent-proof");
    let Ok(receipts) = pipeline.finalize(6, proof_evidence, ts(2_000)) else {
        unreachable!("finalize at sufficient depth");
    };
    assert_eq!(receipts.len(), 3);
    assert!(matches!(pipeline.state(), PipelineState::Anchored { .. }));

    // Each receipt carries its target's coverage and shares the one tx.
    let counts: Vec<u64> = receipts.iter().map(|r| r.entry_count).collect();
    assert_eq!(counts, vec![12, 4, 99]);
}

#[test]
fn depth_below_threshold_refuses_to_finalize() {
    let Ok(mut pipeline) = AnchorPipeline::start(targets()) else {
        unreachable!("non-empty");
    };
    let Ok(()) = pipeline.submitted("tx".to_owned(), 1_000) else {
        unreachable!("submit");
    };
    let Ok(()) = pipeline.confirmed(1_001, 1_003) else {
        unreachable!("confirm"); // depth 3
    };
    assert_eq!(
        pipeline.finalize(6, sha256(b"p"), ts(1)),
        Err(PipelineError::InsufficientDepth {
            depth: 3,
            required: 6,
        })
    );
}

#[test]
fn guessing_is_not_a_transition() {
    let Ok(mut pipeline) = AnchorPipeline::start(targets()) else {
        unreachable!("non-empty");
    };
    // Cannot finalize before confirmation.
    assert_eq!(
        pipeline.finalize(1, sha256(b"p"), ts(1)),
        Err(PipelineError::InvalidTransition)
    );
    // Cannot confirm before submit.
    assert_eq!(
        pipeline.confirmed(1_001, 1_006),
        Err(PipelineError::InvalidTransition)
    );
}

#[test]
fn overdue_anchor_is_flagged_not_silent() {
    let Ok(mut pipeline) = AnchorPipeline::start(targets()) else {
        unreachable!("non-empty");
    };
    let Ok(()) = pipeline.submitted("tx".to_owned(), 1_000) else {
        unreachable!("submit");
    };
    // Within window: fine. Beyond window without confirmation: overdue.
    assert!(!pipeline.is_overdue(1_006, 6));
    assert!(pipeline.is_overdue(1_007, 6));
}

#[test]
fn every_anchored_head_has_a_verifiable_inclusion_proof() {
    // The aggregation underlying the pipeline yields a proof per head; a
    // disclosure can prove a specific head was in the committed root.
    let t = targets();
    let heads: Vec<ContentHash> = t.iter().map(|x| x.tree_head).collect();
    let Some(agg) = aggregate(&heads) else {
        unreachable!("non-empty");
    };
    for (index, head) in heads.iter().enumerate() {
        let Some(proof) = agg.proofs.get(index) else {
            unreachable!("proof per head");
        };
        assert!(verify_inclusion(head, proof, &agg.root));
    }
}

#[test]
fn empty_targets_have_nothing_to_anchor() {
    assert_eq!(
        AnchorPipeline::start(Vec::new()).err(),
        Some(PipelineError::NothingToAnchor)
    );
}

#[test]
fn receipts_append_to_log_in_coverage_order() {
    // Single-target pipeline whose head matches a real store, so the log
    // verifies against it.
    let mut store = InMemoryEvidenceStore::new();
    let _ = store.put(b"a");
    let _ = store.put(b"b");
    let head = store.tree_head();
    let count = store.len() as u64;

    let Ok(mut pipeline) = AnchorPipeline::start(vec![AnchorTarget {
        tree_head: head,
        entry_count: count,
    }]) else {
        unreachable!("non-empty");
    };
    let Ok(()) = pipeline.submitted("tx".to_owned(), 10) else {
        unreachable!("submit");
    };
    let Ok(()) = pipeline.confirmed(11, 16) else {
        unreachable!("confirm"); // depth 6
    };
    let Ok(receipts) = pipeline.finalize(6, sha256(b"proof"), ts(100)) else {
        unreachable!("finalize");
    };

    let mut log = AnchorLog::new();
    for receipt in receipts {
        let Ok(()) = log.append(receipt) else {
            unreachable!("append");
        };
    }
    assert_eq!(log.verify_against(&store), Ok(()));
}
