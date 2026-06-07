//! Gate lifecycle: assess under dual control, gate decisions fail
//! closed, confirmed assessments book as L3 judgments (REQ-22).

use treasury_core::{ActorId, AssetId, ContentHash, DualControlError, TenantId, TimestampNs};
use treasury_ledger::{ClaimLayer, InMemoryLedger, Ledger};
use treasury_scope::{
    draft_scope_judgment, CriteriaAssessment, CriterionStatus, GateDecision, ScopeAssessment,
    ScopeGate, ScopeVerdict,
};

fn tenant() -> TenantId {
    TenantId::new("acme")
}

fn ts(n: i64) -> TimestampNs {
    TimestampNs::from_nanos(n)
}

fn all_met() -> CriteriaAssessment {
    CriteriaAssessment {
        intangible_asset: CriterionStatus::Met,
        no_enforceable_claim: CriterionStatus::Met,
        on_distributed_ledger: CriterionStatus::Met,
        cryptographically_secured: CriterionStatus::Met,
        fungible: CriterionStatus::Met,
        not_self_issued: CriterionStatus::Met,
    }
}

fn btc_assessment(criteria: CriteriaAssessment) -> ScopeAssessment {
    ScopeAssessment {
        tenant: tenant(),
        asset: AssetId::new("BTC"),
        criteria,
        policy_hash: ContentHash([9; 32]),
    }
}

#[test]
fn unassessed_asset_blocks() {
    let gate = ScopeGate::new();
    assert_eq!(
        gate.check(&tenant(), &AssetId::new("BTC")),
        GateDecision::BlockedNoAssessment
    );
}

#[test]
fn proposed_but_unconfirmed_assessment_still_blocks() {
    let mut gate = ScopeGate::new();
    let Ok(_) = gate.propose(btc_assessment(all_met()), ActorId::new("alice")) else {
        unreachable!("propose must succeed");
    };
    assert_eq!(
        gate.check(&tenant(), &AssetId::new("BTC")),
        GateDecision::BlockedNoAssessment
    );
}

#[test]
fn confirmed_in_scope_assessment_proceeds() {
    let mut gate = ScopeGate::new();
    let Ok(id) = gate.propose(btc_assessment(all_met()), ActorId::new("alice")) else {
        unreachable!("propose must succeed");
    };
    assert_eq!(
        gate.confirm(&id, ActorId::new("alice")),
        Err(DualControlError::DualControlViolation)
    );
    assert_eq!(gate.confirm(&id, ActorId::new("bob")), Ok(()));
    assert_eq!(
        gate.check(&tenant(), &AssetId::new("BTC")),
        GateDecision::Proceed { assessment: id }
    );
}

#[test]
fn confirmed_out_of_scope_assessment_blocks_with_verdict() {
    let mut criteria = all_met();
    criteria.no_enforceable_claim = CriterionStatus::NotMet; // stablecoin
    let assessment = ScopeAssessment {
        asset: AssetId::new("USDX"),
        ..btc_assessment(criteria)
    };

    let mut gate = ScopeGate::new();
    let Ok(id) = gate.propose(assessment, ActorId::new("alice")) else {
        unreachable!("propose must succeed");
    };
    let _ = gate.confirm(&id, ActorId::new("bob"));

    let decision = gate.check(&tenant(), &AssetId::new("USDX"));
    let GateDecision::BlockedOutOfScope { verdict, .. } = decision else {
        unreachable!("must block out of scope");
    };
    assert!(matches!(verdict, ScopeVerdict::OutOfScope { .. }));
}

#[test]
fn reassessment_supersedes_after_confirmation() {
    let mut gate = ScopeGate::new();
    // First assessment: undetermined fungibility → blocked.
    let mut criteria = all_met();
    criteria.fungible = CriterionStatus::Undetermined;
    let Ok(first) = gate.propose(btc_assessment(criteria), ActorId::new("alice")) else {
        unreachable!("propose must succeed");
    };
    let _ = gate.confirm(&first, ActorId::new("bob"));
    assert!(matches!(
        gate.check(&tenant(), &AssetId::new("BTC")),
        GateDecision::BlockedOutOfScope { .. }
    ));

    // Research resolves it; a new assessment under a revised policy.
    let mut resolved = btc_assessment(all_met());
    resolved.policy_hash = ContentHash([10; 32]);
    let Ok(second) = gate.propose(resolved, ActorId::new("alice")) else {
        unreachable!("propose must succeed");
    };
    let _ = gate.confirm(&second, ActorId::new("carol"));
    assert_eq!(
        gate.check(&tenant(), &AssetId::new("BTC")),
        GateDecision::Proceed { assessment: second }
    );
}

#[test]
fn confirmed_assessment_books_as_l3_judgment() {
    let mut gate = ScopeGate::new();
    let assessment = btc_assessment(all_met());
    let Ok(id) = gate.propose(assessment.clone(), ActorId::new("alice")) else {
        unreachable!("propose must succeed");
    };
    let _ = gate.confirm(&id, ActorId::new("bob"));
    let Some(state) = gate.state(&id) else {
        unreachable!("item exists");
    };

    let Ok(draft) = draft_scope_judgment(&assessment, state, ts(50)) else {
        unreachable!("confirmed assessment must build a draft");
    };
    assert_eq!(draft.layer, ClaimLayer::Judgment);

    let mut ledger = InMemoryLedger::new();
    assert!(ledger.append(draft, ts(100)).is_ok());
    assert_eq!(ledger.verify_chain(&tenant()), Ok(()));
}
