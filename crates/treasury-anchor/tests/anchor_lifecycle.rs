//! Anchor lifecycle: evidence grows → heads anchored → prefix
//! verification detects tampering (spec v2 §3.3, REQ-8).

use treasury_anchor::{AnchorError, AnchorLog, AnchorMethod, AnchorReceipt};
use treasury_core::{ContentHash, TimestampNs};
use treasury_evidence::{EvidenceStore, InMemoryEvidenceStore};

fn ts(n: i64) -> TimestampNs {
    TimestampNs::from_nanos(n)
}

fn tsa(token: u8) -> AnchorMethod {
    AnchorMethod::Rfc3161Tsa {
        authority: "tsa.example".to_owned(),
        token_hash: ContentHash([token; 32]),
    }
}

fn receipt_for<S: EvidenceStore>(store: &S, at: i64, token: u8) -> AnchorReceipt {
    AnchorReceipt {
        tree_head: store.tree_head(),
        entry_count: store.len() as u64,
        method: tsa(token),
        anchored_at: ts(at),
        confirmation_policy: ContentHash([6; 32]),
    }
}

#[test]
fn growing_store_anchors_and_verifies() {
    let mut store = InMemoryEvidenceStore::new();
    let mut log = AnchorLog::new();

    let _ = store.put(b"q1-payload");
    assert_eq!(log.append(receipt_for(&store, 100, 1)), Ok(()));

    let _ = store.put(b"q2-payload");
    let _ = store.put(b"q2-prices");
    assert_eq!(log.append(receipt_for(&store, 200, 2)), Ok(()));

    // Both receipts verify: each anchored prefix head reproduces.
    assert_eq!(log.verify_against(&store), Ok(()));
    assert_eq!(log.receipts().len(), 2);
}

#[test]
fn anchored_head_mismatch_is_detected() {
    let mut store = InMemoryEvidenceStore::new();
    let _ = store.put(b"original");

    let mut log = AnchorLog::new();
    let mut forged = receipt_for(&store, 100, 1);
    forged.tree_head = ContentHash([0xEE; 32]); // claims a different past
    let _ = log.append(forged);

    assert!(matches!(
        log.verify_against(&store),
        Err(AnchorError::HeadMismatch { .. })
    ));
}

#[test]
fn receipt_beyond_store_is_detected() {
    let store = InMemoryEvidenceStore::new();
    let mut log = AnchorLog::new();
    let _ = log.append(AnchorReceipt {
        tree_head: ContentHash([1; 32]),
        entry_count: 5,
        method: tsa(1),
        anchored_at: ts(100),
        confirmation_policy: ContentHash([6; 32]),
    });
    assert!(matches!(
        log.verify_against(&store),
        Err(AnchorError::EntryCountExceedsStore { .. })
    ));
}

#[test]
fn anchor_time_is_strictly_monotonic() {
    let mut store = InMemoryEvidenceStore::new();
    let _ = store.put(b"x");
    let mut log = AnchorLog::new();
    let _ = log.append(receipt_for(&store, 100, 1));
    assert!(matches!(
        log.append(receipt_for(&store, 100, 2)),
        Err(AnchorError::NonMonotonicAnchorTime { .. })
    ));
}

#[test]
fn coverage_cannot_regress() {
    let mut store = InMemoryEvidenceStore::new();
    let _ = store.put(b"a");
    let _ = store.put(b"b");
    let mut log = AnchorLog::new();
    let _ = log.append(receipt_for(&store, 100, 1));

    let mut smaller = receipt_for(&store, 200, 2);
    smaller.entry_count = 1;
    assert!(matches!(
        log.append(smaller),
        Err(AnchorError::CoverageRegression { .. })
    ));
}
