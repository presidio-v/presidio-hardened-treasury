//! Chain-history → L1 observation mapping (ADR-0004 action item 5):
//! an agreed reconciliation books as an L1 observation that appends to
//! the ledger and verifies; a divergence books nothing.

use treasury_chainsource::{
    draft_history_observation, reconcile, AddressHistory, BookError, Chain, ChainMovement,
    Direction, FinalityPolicy, FinalityRule, FixtureSource,
};
use treasury_core::{AssetAmount, AssetId, ContentHash, TenantId, TimestampNs};
use treasury_evidence::{canonical_bytes, sha256};
use treasury_ledger::{ClaimLayer, InMemoryLedger, Ledger};

fn tenant() -> TenantId {
    TenantId::new("acme")
}

fn movement() -> ChainMovement {
    ChainMovement {
        tx_ref: "t1".to_owned(),
        direction: Direction::Inflow,
        amount: AssetAmount::new(AssetId::new("BTC"), 500),
        block_height: 10,
    }
}

fn history(movements: Vec<ChainMovement>) -> AddressHistory {
    AddressHistory {
        chain: Chain::Bitcoin,
        address: "bc1q-acme".to_owned(),
        settled_to_height: 0,
        movements,
    }
}

fn btc_depth_6() -> FinalityPolicy {
    FinalityPolicy {
        chain: Chain::Bitcoin,
        rule: FinalityRule::ConfirmationDepth { depth: 6 },
    }
}

#[test]
fn agreed_reconciliation_books_an_l1_observation_into_the_ledger() {
    let a =
        FixtureSource::new(Chain::Bitcoin, "core+electrs").with_history(history(vec![movement()]));
    let b =
        FixtureSource::new(Chain::Bitcoin, "core+fulcrum").with_history(history(vec![movement()]));
    let Ok(reconciliation) = reconcile(&a, &b, &btc_depth_6(), "bc1q-acme", 100) else {
        unreachable!("reconcile must succeed");
    };
    assert!(reconciliation.agreed());

    let raw_evidence = ContentHash([7; 32]);
    let Ok(draft) = draft_history_observation(
        &reconciliation,
        tenant(),
        "core+electrs|core+fulcrum",
        raw_evidence,
        TimestampNs::from_nanos(1_000),
    ) else {
        unreachable!("agreed reconciliation must book");
    };
    assert_eq!(draft.layer, ClaimLayer::Observation);

    let mut ledger = InMemoryLedger::new();
    assert!(ledger.append(draft, TimestampNs::from_nanos(2_000)).is_ok());
    assert_eq!(ledger.verify_chain(&tenant()), Ok(()));
}

#[test]
fn a_divergence_books_nothing() {
    let a =
        FixtureSource::new(Chain::Bitcoin, "core+electrs").with_history(history(vec![movement()]));
    // Fulcrum is missing the settled movement — a real indexing bug.
    let b = FixtureSource::new(Chain::Bitcoin, "core+fulcrum").with_history(history(vec![]));
    let Ok(reconciliation) = reconcile(&a, &b, &btc_depth_6(), "bc1q-acme", 100) else {
        unreachable!("reconcile must succeed");
    };
    assert!(!reconciliation.agreed());

    assert_eq!(
        draft_history_observation(
            &reconciliation,
            tenant(),
            "x",
            ContentHash([7; 32]),
            TimestampNs::from_nanos(1_000),
        ),
        Err(BookError::Diverged)
    );
}

/// Golden vector — the mapping's payload hash, independently recomputed
/// in Python over the full canonical shape (history hash → payload hash).
#[test]
fn golden_observation_payload_matches_independent_implementation() {
    let a = FixtureSource::new(Chain::Bitcoin, "a").with_history(history(vec![movement()]));
    let b = FixtureSource::new(Chain::Bitcoin, "b").with_history(history(vec![movement()]));
    let Ok(reconciliation) = reconcile(&a, &b, &btc_depth_6(), "bc1q-acme", 100) else {
        unreachable!("reconcile must succeed");
    };
    let Ok(draft) = draft_history_observation(
        &reconciliation,
        tenant(),
        "src",
        ContentHash([7; 32]),
        TimestampNs::from_nanos(1_000),
    ) else {
        unreachable!("agreed reconciliation must book");
    };
    let Ok(bytes) = canonical_bytes(&draft.payload) else {
        unreachable!("payload canonicalizes");
    };
    assert_eq!(
        sha256(&bytes).to_hex(),
        "d69bbaafb2d307b48cc557037e30ce60b1114cb8bc7b26be3a730bacd533b532"
    );
}
