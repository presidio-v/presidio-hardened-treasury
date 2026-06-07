//! Spec v2 §5 scenarios end to end: tiers, materiality, ambiguity,
//! false-negative bias, dual control, determinism.

use std::collections::BTreeMap;
use treasury_core::{ActorId, AssetAmount, AssetId, ContentHash, TenantId, TimestampNs, VenueId};
use treasury_reconcile::{
    match_legs, ConfirmationQueue, Direction, Disposition, MatchError, MatcherConfig,
    QueueError, QueueState, Tier, TransferLeg,
};

fn btc(atoms: i128) -> AssetAmount {
    AssetAmount::new(AssetId::new("BTC"), atoms)
}

fn leg(id: u8, dir: Direction, atoms: i128, time: i64) -> TransferLeg {
    TransferLeg {
        leg_id: ContentHash([id; 32]),
        tenant: TenantId::new("acme"),
        venue: VenueId::new(if dir == Direction::Outflow { "self-custody" } else { "exchange" }),
        direction: dir,
        amount: btc(atoms),
        fee: None,
        tx_hash: None,
        address: None,
        event_time: TimestampNs::from_nanos(time),
    }
}

fn config() -> MatcherConfig {
    let mut materiality = BTreeMap::new();
    materiality.insert(AssetId::new("BTC"), 1_000_000_i128);
    MatcherConfig {
        time_window_ns: 1_000,
        materiality_atoms: materiality,
    }
}

#[test]
fn tier0_same_tx_hash_auto_nets() {
    let mut out = leg(1, Direction::Outflow, 500, 10);
    let mut inn = leg(2, Direction::Inflow, 480, 20);
    out.tx_hash = Some("abc123".to_owned());
    inn.tx_hash = Some("abc123".to_owned());

    let Ok(outcome) = match_legs(&[out, inn], &config()) else {
        unreachable!("match must succeed");
    };
    assert_eq!(outcome.proposals.len(), 1);
    let Some(p) = outcome.proposals.first() else {
        unreachable!("one proposal asserted above");
    };
    assert_eq!(p.tier, Tier::Deterministic);
    assert_eq!(p.disposition, Disposition::AutoNet);
    assert!(outcome.blockers.close_permitted());
}

#[test]
fn tier1_fee_aware_below_materiality_auto_nets() {
    let mut out = leg(1, Direction::Outflow, 500, 10);
    out.fee = Some(btc(20));
    out.address = Some("bc1qdest".to_owned());
    let mut inn = leg(2, Direction::Inflow, 480, 500);
    inn.address = Some("bc1qdest".to_owned());

    let Ok(outcome) = match_legs(&[out, inn], &config()) else {
        unreachable!("match must succeed");
    };
    assert_eq!(outcome.proposals.len(), 1);
    let Some(p) = outcome.proposals.first() else {
        unreachable!("one proposal asserted above");
    };
    assert_eq!(p.tier, Tier::StrongCorroboration);
    assert_eq!(p.disposition, Disposition::AutoNet);
    assert!(outcome.blockers.close_permitted());
}

#[test]
fn tier1_at_or_above_materiality_queues() {
    let mut out = leg(1, Direction::Outflow, 2_000_000, 10);
    out.address = Some("bc1qdest".to_owned());
    let mut inn = leg(2, Direction::Inflow, 2_000_000, 500);
    inn.address = Some("bc1qdest".to_owned());

    let Ok(outcome) = match_legs(&[out, inn], &config()) else {
        unreachable!("match must succeed");
    };
    let Some(p) = outcome.proposals.first() else {
        unreachable!("a proposal must exist");
    };
    assert_eq!(p.tier, Tier::StrongCorroboration);
    assert_eq!(p.disposition, Disposition::Queue);
    assert!(!outcome.blockers.close_permitted());
    assert_eq!(outcome.blockers.queued_decisions.len(), 1);
}

#[test]
fn missing_materiality_entry_fails_closed_to_queue() {
    // ETH has no threshold entry → threshold 0 → everything queues.
    let eth = AssetAmount::new(AssetId::new("ETH"), 1);
    let mut out = leg(1, Direction::Outflow, 0, 10);
    out.amount = eth.clone();
    out.address = Some("0xdest".to_owned());
    let mut inn = leg(2, Direction::Inflow, 0, 500);
    inn.amount = eth;
    inn.address = Some("0xdest".to_owned());

    let Ok(outcome) = match_legs(&[out, inn], &config()) else {
        unreachable!("match must succeed");
    };
    let disposition = outcome.proposals.first().map(|p| p.disposition);
    assert_eq!(disposition, Some(Disposition::Queue));
}

#[test]
fn ambiguity_demotes_to_tier2_and_consumes_nothing() {
    let mut out = leg(1, Direction::Outflow, 500, 10);
    out.address = Some("bc1qdest".to_owned());
    let mut in_a = leg(2, Direction::Inflow, 500, 100);
    in_a.address = Some("bc1qdest".to_owned());
    let mut in_b = leg(3, Direction::Inflow, 500, 200);
    in_b.address = Some("bc1qdest".to_owned());

    let Ok(outcome) = match_legs(&[out, in_a, in_b], &config()) else {
        unreachable!("match must succeed");
    };
    // Two tier-2 proposals, both queued, nothing auto-netted.
    assert_eq!(outcome.proposals.len(), 2);
    for p in &outcome.proposals {
        assert_eq!(p.tier, Tier::Probabilistic);
        assert_eq!(p.disposition, Disposition::Queue);
    }
    assert!(!outcome.blockers.close_permitted());
}

#[test]
fn no_address_corroboration_is_tier2() {
    let out = leg(1, Direction::Outflow, 500, 10);
    let inn = leg(2, Direction::Inflow, 500, 500);
    // Amount and window match, but neither leg has an address.
    let Ok(outcome) = match_legs(&[out, inn], &config()) else {
        unreachable!("match must succeed");
    };
    assert_eq!(outcome.proposals.len(), 1);
    let tier = outcome.proposals.first().map(|p| p.tier);
    assert_eq!(tier, Some(Tier::Probabilistic));
    let disposition = outcome.proposals.first().map(|p| p.disposition);
    assert_eq!(disposition, Some(Disposition::Queue));
}

#[test]
fn unmatched_leg_blocks_close() {
    let out = leg(1, Direction::Outflow, 500, 10);
    // No inflow at all: false-negative bias — this cannot silently book.
    let Ok(outcome) = match_legs(&[out], &config()) else {
        unreachable!("match must succeed");
    };
    assert!(outcome.proposals.is_empty());
    assert_eq!(outcome.blockers.unmatched_legs.len(), 1);
    assert!(!outcome.blockers.close_permitted());
}

#[test]
fn out_of_window_inflow_does_not_match() {
    let mut out = leg(1, Direction::Outflow, 500, 10);
    out.address = Some("bc1qdest".to_owned());
    let mut inn = leg(2, Direction::Inflow, 500, 5_000); // window is 1_000
    inn.address = Some("bc1qdest".to_owned());

    let Ok(outcome) = match_legs(&[out, inn], &config()) else {
        unreachable!("match must succeed");
    };
    assert!(outcome.proposals.is_empty());
    assert_eq!(outcome.blockers.unmatched_legs.len(), 2);
}

#[test]
fn mixed_tenants_rejected() {
    let out = leg(1, Direction::Outflow, 500, 10);
    let mut inn = leg(2, Direction::Inflow, 500, 20);
    inn.tenant = TenantId::new("other");
    assert_eq!(match_legs(&[out, inn], &config()), Err(MatchError::MixedTenants));
}

#[test]
fn matching_is_deterministic_under_input_reordering() {
    let mut out = leg(1, Direction::Outflow, 500, 10);
    out.tx_hash = Some("t1".to_owned());
    let mut inn = leg(2, Direction::Inflow, 490, 20);
    inn.tx_hash = Some("t1".to_owned());
    let stray = leg(3, Direction::Inflow, 777, 30);

    let forward = match_legs(&[out.clone(), inn.clone(), stray.clone()], &config());
    let reversed = match_legs(&[stray, inn, out], &config());
    assert_eq!(forward, reversed);
}

#[test]
fn dual_control_queue_lifecycle() {
    let mut out = leg(1, Direction::Outflow, 2_000_000, 10);
    out.address = Some("bc1qdest".to_owned());
    let mut inn = leg(2, Direction::Inflow, 2_000_000, 500);
    inn.address = Some("bc1qdest".to_owned());
    let Ok(outcome) = match_legs(&[out, inn], &config()) else {
        unreachable!("match must succeed");
    };

    let mut queue = ConfirmationQueue::new();
    let Some(proposal) = outcome.proposals.first() else {
        unreachable!("a queued proposal must exist");
    };
    let Ok(id) = queue.enqueue(proposal.clone()) else {
        unreachable!("enqueue must succeed");
    };

    // Dual control: the preparer cannot self-confirm.
    let preparer = ActorId::new("controller-alice");
    assert_eq!(queue.assert_match(&id, preparer.clone()), Ok(()));
    assert_eq!(
        queue.confirm(&id, preparer.clone()),
        Err(QueueError::DualControlViolation)
    );

    // A different approver confirms; the state is terminal and immutable.
    let approver = ActorId::new("cfo-bob");
    assert_eq!(queue.confirm(&id, approver), Ok(()));
    assert!(queue.state(&id).is_some_and(QueueState::is_terminal));
    assert_eq!(
        queue.reject(&id, preparer, "too late".to_owned()),
        Err(QueueError::InvalidTransition)
    );
    assert!(queue.open_items().is_empty());
}
