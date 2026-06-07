//! Designation lifecycle: an unmatched leg (e.g. a staking reward) is
//! classified under dual control and books as an L3 judgment (gap G-1).

use treasury_core::{ActorId, ContentHash, TenantId, TimestampNs};
use treasury_ledger::{ClaimLayer, InMemoryLedger, Ledger};
use treasury_reconcile::{
    draft_designation, DesignateError, DesignationProposal, DesignationQueue, DesignationState,
    LegClassification, NonPurchaseKind,
};

fn tenant() -> TenantId {
    TenantId::new("acme")
}

fn ts(n: i64) -> TimestampNs {
    TimestampNs::from_nanos(n)
}

fn staking_proposal() -> DesignationProposal {
    DesignationProposal {
        leg: ContentHash([7; 32]),
        classification: LegClassification::NonPurchaseAcquisition(NonPurchaseKind::StakingReward),
        policy_hash: ContentHash([9; 32]),
    }
}

#[test]
fn confirmed_designation_books_as_l3_and_clears_the_leg() {
    let mut queue = DesignationQueue::new();
    let proposal = staking_proposal();
    let Ok(id) = queue.propose(proposal.clone(), ActorId::new("alice")) else {
        unreachable!("propose must succeed");
    };
    assert_eq!(queue.confirm(&id, ActorId::new("bob")), Ok(()));
    assert_eq!(queue.designated_legs(), vec![ContentHash([7; 32])]);

    let Some(state) = queue.state(&id) else {
        unreachable!("item exists");
    };
    let Ok(draft) = draft_designation(&proposal, state, tenant(), ts(50)) else {
        unreachable!("confirmed designation must build a draft");
    };
    assert_eq!(draft.layer, ClaimLayer::Judgment);

    let mut ledger = InMemoryLedger::new();
    assert!(ledger.append(draft, ts(100)).is_ok());
    assert_eq!(ledger.verify_chain(&tenant()), Ok(()));
}

#[test]
fn preparer_cannot_self_confirm() {
    let mut queue = DesignationQueue::new();
    let Ok(id) = queue.propose(staking_proposal(), ActorId::new("alice")) else {
        unreachable!("propose must succeed");
    };
    assert_eq!(
        queue.confirm(&id, ActorId::new("alice")),
        Err(DesignateError::DualControlViolation)
    );
}

#[test]
fn rejected_designation_books_nothing_and_leg_stays_blocked() {
    let mut queue = DesignationQueue::new();
    let proposal = staking_proposal();
    let Ok(id) = queue.propose(proposal.clone(), ActorId::new("alice")) else {
        unreachable!("propose must succeed");
    };
    let reason = "this was a vendor refund, not a staking reward".to_owned();
    assert_eq!(queue.reject(&id, ActorId::new("bob"), reason), Ok(()));
    assert!(queue.designated_legs().is_empty());

    let Some(state) = queue.state(&id) else {
        unreachable!("item exists");
    };
    assert_eq!(
        draft_designation(&proposal, state, tenant(), ts(50)),
        Err(DesignateError::NotConfirmed)
    );
}

#[test]
fn identical_rejected_proposal_cannot_be_reproposed() {
    let mut queue = DesignationQueue::new();
    let proposal = staking_proposal();
    let Ok(id) = queue.propose(proposal.clone(), ActorId::new("alice")) else {
        unreachable!("propose must succeed");
    };
    let _ = queue.reject(&id, ActorId::new("bob"), "wrong".to_owned());

    // Same content → same hash → state untouched (still terminal).
    let Ok(id2) = queue.propose(proposal, ActorId::new("carol")) else {
        unreachable!("idempotent propose must succeed");
    };
    assert_eq!(id, id2);
    assert!(queue.state(&id2).is_some_and(DesignationState::is_terminal));

    // A different classification is a different proposal — new item.
    let mut different = staking_proposal();
    different.classification =
        LegClassification::NonPurchaseAcquisition(NonPurchaseKind::Airdrop);
    let Ok(id3) = queue.propose(different, ActorId::new("carol")) else {
        unreachable!("propose must succeed");
    };
    assert_ne!(id, id3);
}

#[test]
fn vague_other_kind_fails_closed() {
    let mut proposal = staking_proposal();
    proposal.classification =
        LegClassification::NonPurchaseAcquisition(NonPurchaseKind::Other("  ".to_owned()));
    assert_eq!(proposal.proposal_hash(), Err(DesignateError::VagueOther));
}

#[test]
fn disposal_and_acquisition_classes_are_expressible() {
    // Rejected match legs route here: a leg can be designated a disposal
    // or a purchase, not only a non-purchase acquisition.
    let mut queue = DesignationQueue::new();
    let mut disposal = staking_proposal();
    disposal.classification = LegClassification::Disposal;
    let Ok(id) = queue.propose(disposal, ActorId::new("alice")) else {
        unreachable!("propose must succeed");
    };
    assert_eq!(queue.confirm(&id, ActorId::new("bob")), Ok(()));
    assert_eq!(queue.designated_legs().len(), 1);
}

/// Golden vector — independently recomputed in Python.
#[test]
fn golden_hash_matches_independent_implementation() {
    let hash = staking_proposal().proposal_hash().map(|h| h.to_hex());
    assert_eq!(
        hash.as_deref(),
        Ok("9e706f309c6fd25e04a1a1b68fe6df3b34fe1bd0cbc5a7691bced68d225638fe")
    );
}
