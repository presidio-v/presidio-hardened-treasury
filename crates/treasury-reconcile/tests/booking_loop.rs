//! The full loop: legs → matcher → (auto-net | queue → dual control) →
//! ledger events, with append-time layer enforcement (spec v2 §5).

use std::collections::BTreeMap;
use treasury_core::{ActorId, AssetAmount, AssetId, ContentHash, TenantId, TimestampNs, VenueId};
use treasury_ledger::{ClaimLayer, InMemoryLedger, Ledger};
use treasury_reconcile::{
    draft_auto_net, draft_resolution, match_legs, BookError, ConfirmationQueue, Direction,
    MatcherConfig, QueueState, TransferLeg,
};

fn tenant() -> TenantId {
    TenantId::new("acme")
}

fn ts(n: i64) -> TimestampNs {
    TimestampNs::from_nanos(n)
}

fn leg(id: u8, dir: Direction, atoms: i128, time: i64) -> TransferLeg {
    TransferLeg {
        leg_id: ContentHash([id; 32]),
        tenant: tenant(),
        venue: VenueId::new("venue"),
        direction: dir,
        amount: AssetAmount::new(AssetId::new("BTC"), atoms),
        fee: None,
        tx_hash: None,
        address: None,
        event_time: ts(time),
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
fn auto_net_books_as_l2_derived_fact() {
    let mut out = leg(1, Direction::Outflow, 500, 10);
    let mut inn = leg(2, Direction::Inflow, 500, 20);
    out.tx_hash = Some("t1".to_owned());
    inn.tx_hash = Some("t1".to_owned());

    let Ok(outcome) = match_legs(&[out, inn], &config()) else {
        unreachable!("match must succeed");
    };
    let Some(proposal) = outcome.proposals.first() else {
        unreachable!("one auto-net proposal expected");
    };

    let Ok(draft) = draft_auto_net(proposal, tenant(), ts(30)) else {
        unreachable!("auto-net draft must build");
    };
    assert_eq!(draft.layer, ClaimLayer::DerivedFact);

    let mut ledger = InMemoryLedger::new();
    let appended = ledger.append(draft, ts(100));
    assert!(appended.is_ok(), "ledger must accept the derived fact");
    assert_eq!(ledger.verify_chain(&tenant()), Ok(()));
    assert_eq!(ledger.as_of(&tenant(), ts(100)).len(), 1);
}

#[test]
fn confirmed_queue_item_books_as_l3_judgment_with_both_actors() {
    let mut out = leg(1, Direction::Outflow, 2_000_000, 10);
    out.address = Some("bc1q".to_owned());
    let mut inn = leg(2, Direction::Inflow, 2_000_000, 500);
    inn.address = Some("bc1q".to_owned());

    let Ok(outcome) = match_legs(&[out, inn], &config()) else {
        unreachable!("match must succeed");
    };
    let Some(proposal) = outcome.proposals.first() else {
        unreachable!("one queued proposal expected");
    };

    let mut queue = ConfirmationQueue::new();
    let Ok(id) = queue.enqueue(proposal.clone()) else {
        unreachable!("enqueue must succeed");
    };
    let _ = queue.assert_match(&id, ActorId::new("alice"));
    let _ = queue.confirm(&id, ActorId::new("bob"));
    let Some(state) = queue.state(&id) else {
        unreachable!("item exists");
    };

    let Ok(draft) = draft_resolution(proposal, state, tenant(), ts(600)) else {
        unreachable!("resolution draft must build");
    };
    assert_eq!(draft.layer, ClaimLayer::Judgment);

    let mut ledger = InMemoryLedger::new();
    assert!(ledger.append(draft, ts(700)).is_ok());
    assert_eq!(ledger.verify_chain(&tenant()), Ok(()));
}

#[test]
fn rejected_queue_item_books_as_explicit_non_transfer() {
    let mut out = leg(1, Direction::Outflow, 2_000_000, 10);
    out.address = Some("bc1q".to_owned());
    let mut inn = leg(2, Direction::Inflow, 2_000_000, 500);
    inn.address = Some("bc1q".to_owned());

    let Ok(outcome) = match_legs(&[out, inn], &config()) else {
        unreachable!("match must succeed");
    };
    let Some(proposal) = outcome.proposals.first() else {
        unreachable!("one queued proposal expected");
    };

    let mut queue = ConfirmationQueue::new();
    let Ok(id) = queue.enqueue(proposal.clone()) else {
        unreachable!("enqueue must succeed");
    };
    let reason = "counterparty is an OTC desk, not our wallet".to_owned();
    let _ = queue.reject(&id, ActorId::new("alice"), reason);
    let Some(state) = queue.state(&id) else {
        unreachable!("item exists");
    };

    let Ok(draft) = draft_resolution(proposal, state, tenant(), ts(600)) else {
        unreachable!("rejection draft must build");
    };
    assert_eq!(draft.layer, ClaimLayer::Judgment);
    let mut ledger = InMemoryLedger::new();
    assert!(ledger.append(draft, ts(700)).is_ok());
}

#[test]
fn queued_proposal_cannot_book_as_auto_net() {
    let mut out = leg(1, Direction::Outflow, 2_000_000, 10);
    out.address = Some("bc1q".to_owned());
    let mut inn = leg(2, Direction::Inflow, 2_000_000, 500);
    inn.address = Some("bc1q".to_owned());

    let Ok(outcome) = match_legs(&[out, inn], &config()) else {
        unreachable!("match must succeed");
    };
    let Some(proposal) = outcome.proposals.first() else {
        unreachable!("one queued proposal expected");
    };
    assert_eq!(
        draft_auto_net(proposal, tenant(), ts(30)),
        Err(BookError::NotAutoNet)
    );
}

#[test]
fn non_terminal_state_cannot_book() {
    let mut out = leg(1, Direction::Outflow, 2_000_000, 10);
    out.address = Some("bc1q".to_owned());
    let mut inn = leg(2, Direction::Inflow, 2_000_000, 500);
    inn.address = Some("bc1q".to_owned());

    let Ok(outcome) = match_legs(&[out, inn], &config()) else {
        unreachable!("match must succeed");
    };
    let Some(proposal) = outcome.proposals.first() else {
        unreachable!("one queued proposal expected");
    };
    assert_eq!(
        draft_resolution(proposal, &QueueState::Pending, tenant(), ts(30)),
        Err(BookError::NotTerminal)
    );
}
