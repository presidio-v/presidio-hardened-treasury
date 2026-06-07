//! The golden close: one quarter, every crate, run twice — identical
//! pack hash. See this crate's `lib.rs` for the stage map.

use serde_json::json;
use std::collections::BTreeMap;
use treasury_anchor::{AnchorLog, AnchorMethod, AnchorReceipt};
use treasury_close::{state_root, CheckpointDag, CheckpointDraft, PeriodId};
use treasury_core::{
    ActorId, AssetAmount, AssetId, ContentHash, SourceId, TenantId, TimestampNs, VenueId,
};
use treasury_disclosure::{DisclosurePack, RollForwardRow};
use treasury_evidence::{EvidenceStore, InMemoryEvidenceStore};
use treasury_fairvalue::{value_book, Price, PriceSnapshot};
use treasury_gaap::{acquisition_entry, draft_policy_output, remeasurement_entry, FeeTreatment};
use treasury_ledger::{ClaimLayer, EventDraft, InMemoryLedger, Ledger, Provenance};
use treasury_lots::LotBook;
use treasury_policy::{PolicyArtifact, PolicyKind, PolicyRegistry};
use treasury_posting::{PostingBatch, PostingProtocol, PostingState};
use treasury_reconcile::{
    draft_auto_net, draft_designation, match_legs, DesignationProposal, DesignationQueue,
    Direction, LegClassification, MatcherConfig, NonPurchaseKind, TransferLeg,
};
use treasury_scope::{
    draft_scope_judgment, CriteriaAssessment, CriterionStatus, GateDecision, ScopeAssessment,
    ScopeGate,
};

macro_rules! ok {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(_) => unreachable!("golden close step failed"),
        }
    };
}

fn tenant() -> TenantId {
    TenantId::new("acme")
}

fn btc() -> AssetId {
    AssetId::new("BTC")
}

fn usd(minor: i128) -> AssetAmount {
    AssetAmount::new(AssetId::new("USD"), minor)
}

fn ts(n: i64) -> TimestampNs {
    TimestampNs::from_nanos(n)
}

fn observation(
    store: &mut InMemoryEvidenceStore,
    raw: &[u8],
    payload: serde_json::Value,
    event_time: i64,
) -> EventDraft {
    let evidence = ok!(store.put(raw));
    EventDraft {
        tenant: tenant(),
        layer: ClaimLayer::Observation,
        event_time: ts(event_time),
        supersedes: None,
        provenance: Provenance::Observation {
            source: SourceId::new("venue-api"),
            evidence,
        },
        payload,
    }
}

struct CloseArtifacts {
    pack_hash: ContentHash,
    checkpoint: ContentHash,
    valuation_hash: ContentHash,
    receipt_hash: ContentHash,
}

#[allow(clippy::too_many_lines)]
fn run_close() -> CloseArtifacts {
    let mut store = InMemoryEvidenceStore::new();
    let mut ledger = InMemoryLedger::new();
    let mut kt = 0_i64;
    let mut next_kt = || {
        kt = kt.saturating_add(10);
        ts(kt)
    };

    // ── Policies in force (REQ-9): principal market + designation.
    let mut registry = PolicyRegistry::new();
    let pm_kind = PolicyKind::new("principal-market/v1");
    let pm_artifact = PolicyArtifact {
        kind: pm_kind.clone(),
        body: json!({"asset": "BTC", "venues": ["exchange"], "tie_break": "volume"}),
        approvers: vec![ActorId::new("cfo"), ActorId::new("controller")],
        effective_from: ts(0),
    };
    let pm = ok!(registry.register(pm_artifact));
    ok!(registry.activate(tenant(), pm_kind.clone(), pm, ts(1)));
    let designation_artifact = PolicyArtifact {
        kind: PolicyKind::new("designation/v1"),
        body: json!({"staking_reward": "recognize at fair value on receipt"}),
        approvers: vec![ActorId::new("cfo"), ActorId::new("controller")],
        effective_from: ts(0),
    };
    let designation_policy = ok!(registry.register(designation_artifact));

    // ── Ingestion (L1): purchase, withdrawal, deposit, staking reward.
    let purchase = ok!(ledger.append(
        observation(
            &mut store,
            b"raw-purchase",
            json!({"kind": "buy", "atoms": "7", "cost_minor": "70", "fee_minor": "1"}),
            10,
        ),
        next_kt(),
    ));
    let withdrawal = ok!(ledger.append(
        observation(
            &mut store,
            b"raw-withdrawal",
            json!({"kind": "withdraw", "atoms": "7", "tx": "t-1"}),
            20,
        ),
        next_kt(),
    ));
    let deposit = ok!(ledger.append(
        observation(
            &mut store,
            b"raw-deposit",
            json!({"kind": "deposit", "atoms": "7", "tx": "t-1"}),
            30,
        ),
        next_kt(),
    ));
    let staking = ok!(ledger.append(
        observation(
            &mut store,
            b"raw-staking-reward",
            json!({"kind": "deposit", "atoms": "1"}),
            40,
        ),
        next_kt(),
    ));

    // ── Reconciliation (§5): tier-0 net of the withdrawal/deposit pair.
    let exchange = VenueId::new("exchange");
    let cold = VenueId::new("cold");
    let leg = |id: ContentHash, dir: Direction, venue: &VenueId, atoms: i128, t: i64| TransferLeg {
        leg_id: id,
        tenant: tenant(),
        venue: venue.clone(),
        direction: dir,
        amount: AssetAmount::new(btc(), atoms),
        fee: None,
        tx_hash: Some("t-1".to_owned()),
        address: None,
        event_time: ts(t),
    };
    let mut staking_leg = leg(staking, Direction::Inflow, &cold, 1, 40);
    staking_leg.tx_hash = None;
    let legs = vec![
        leg(withdrawal, Direction::Outflow, &exchange, 7, 20),
        leg(deposit, Direction::Inflow, &cold, 7, 30),
        staking_leg,
    ];
    let mut materiality = BTreeMap::new();
    materiality.insert(btc(), 1_000_000_i128);
    let config = MatcherConfig {
        time_window_ns: 1_000,
        materiality_atoms: materiality,
    };
    let outcome = ok!(match_legs(&legs, &config));
    assert_eq!(outcome.proposals.len(), 1, "tier-0 net expected");
    let Some(proposal) = outcome.proposals.first() else {
        unreachable!("asserted above");
    };
    let auto_net = ok!(ledger.append(ok!(draft_auto_net(proposal, tenant(), ts(30))), next_kt(),));

    // The staking leg blocks close until designated (false-negative bias).
    assert_eq!(outcome.blockers.unmatched_legs, vec![staking]);

    // ── Designation (G-1): staking reward under dual control, booked L3.
    let mut queue = DesignationQueue::new();
    let staking_proposal = DesignationProposal {
        leg: staking,
        classification: LegClassification::NonPurchaseAcquisition(NonPurchaseKind::StakingReward),
        policy_hash: designation_policy,
    };
    let item = ok!(queue.propose(staking_proposal.clone(), ActorId::new("preparer")));
    ok!(queue.confirm(&item, ActorId::new("approver")));
    let Some(state) = queue.state(&item) else {
        unreachable!("item exists");
    };
    let designation_event = ok!(ledger.append(
        ok!(draft_designation(
            &staking_proposal,
            state,
            tenant(),
            ts(40)
        )),
        next_kt(),
    ));
    // Every unmatched leg is now designated: reconciliation clears.
    let designated = queue.designated_legs();
    let cleared = outcome
        .blockers
        .unmatched_legs
        .iter()
        .all(|l| designated.contains(l));
    assert!(cleared && outcome.blockers.queued_decisions.is_empty());

    // ── Scope gate (REQ-22): BTC assessed in scope, booked L3.
    let mut gate = ScopeGate::new();
    let assessment = ScopeAssessment {
        tenant: tenant(),
        asset: btc(),
        criteria: CriteriaAssessment {
            intangible_asset: CriterionStatus::Met,
            no_enforceable_claim: CriterionStatus::Met,
            on_distributed_ledger: CriterionStatus::Met,
            cryptographically_secured: CriterionStatus::Met,
            fungible: CriterionStatus::Met,
            not_self_issued: CriterionStatus::Met,
        },
        policy_hash: designation_policy,
    };
    let scope_item = ok!(gate.propose(assessment.clone(), ActorId::new("preparer")));
    ok!(gate.confirm(&scope_item, ActorId::new("approver")));
    let Some(scope_state) = gate.state(&scope_item) else {
        unreachable!("item exists");
    };
    let _scope_event = ok!(ledger.append(
        ok!(draft_scope_judgment(&assessment, scope_state, ts(45))),
        next_kt(),
    ));
    assert!(matches!(
        gate.check(&tenant(), &btc()),
        GateDecision::Proceed { .. }
    ));

    // ── Lots (REQ-23): acquire, transfer along the netted movement,
    //    book the staking reward at receipt fair value.
    let mut book = LotBook::new(tenant());
    ok!(book.acquire(
        btc(),
        exchange.clone(),
        7,
        usd(70),
        usd(1),
        ts(10),
        purchase
    ));
    ok!(book.transfer(&btc(), &exchange, &cold, 7, auto_net));
    ok!(book.acquire(
        btc(),
        cold.clone(),
        1,
        usd(5),
        usd(0),
        ts(40),
        designation_event
    ));

    // ── Valuation (REQ-24): 8 atoms at 100/8 → fair value 100.
    let mut prices = BTreeMap::new();
    prices.insert(
        btc(),
        Price {
            minor_per_unit: 100,
            atoms_per_unit: 8,
        },
    );
    let snapshot = PriceSnapshot {
        currency: AssetId::new("USD"),
        as_of: ts(1_000),
        prices,
    };
    let Some(active_pm) = registry.active_at(&tenant(), &pm_kind, ts(1_000)) else {
        unreachable!("principal-market policy is active");
    };
    let valuation = ok!(value_book(&book, &snapshot, active_pm));
    assert_eq!(valuation.total_fair_value, usd(100));
    assert_eq!(valuation.total_unrealized, usd(25));
    let valuation_hash = ok!(valuation.valuation_hash());

    // ── GAAP (REQ-25): entries booked L4 — all four claim layers live.
    let e_buy = ok!(acquisition_entry(
        &usd(70),
        &usd(1),
        FeeTreatment::Expense,
        active_pm
    ));
    let e_reward = ok!(acquisition_entry(
        &usd(5),
        &usd(0),
        FeeTreatment::Expense,
        active_pm
    ));
    let Some(e_mark) = ok!(remeasurement_entry(&usd(75), &usd(100), active_pm)) else {
        unreachable!("non-zero mark");
    };
    for (entry, input) in [(&e_buy, purchase), (&e_reward, designation_event)] {
        let draft = ok!(draft_policy_output(entry, tenant(), ts(50), vec![input]));
        ok!(ledger.append(draft, next_kt()));
    }
    let mark_draft = ok!(draft_policy_output(
        &e_mark,
        tenant(),
        ts(50),
        vec![purchase]
    ));
    ok!(ledger.append(mark_draft, next_kt()));

    // ── Posting protocol (R9): released, posted, read-back verified.
    let batch = PostingBatch {
        tenant: tenant(),
        target_gl: "netsuite:prod".to_owned(),
        period: "2026Q2".to_owned(),
        entries: vec![e_buy.clone(), e_reward.clone(), e_mark.clone()],
    };
    let mut protocol = PostingProtocol::new();
    let batch_id = ok!(protocol.register(batch.clone(), ActorId::new("preparer")));
    ok!(protocol.release(&batch_id, ActorId::new("approver")));
    ok!(protocol.begin_submit(&batch_id));
    let response_evidence = ok!(store.put(b"raw-gl-response"));
    ok!(protocol.confirm_posted(&batch_id, "JE-1".to_owned(), response_evidence));
    let readback_evidence = ok!(store.put(b"raw-gl-readback"));
    let fingerprint = ok!(batch.entry_fingerprint());
    ok!(protocol.verify(&batch_id, &fingerprint, readback_evidence));
    assert!(matches!(
        protocol.state(&batch_id),
        Some(PostingState::Verified { .. })
    ));

    // ── Checkpoint (§3.6): seal the period over the as_of state root.
    let root = ok!(state_root(&ledger.as_of(&tenant(), ts(1_000))));
    let mut dag = CheckpointDag::new();
    let checkpoint = ok!(dag.seal(
        CheckpointDraft {
            tenant: tenant(),
            period: PeriodId::new("2026Q2"),
            as_of: ts(1_000),
            state_root: root,
            supersedes: None,
            reason: None,
            materiality_memo: None,
        },
        ts(1_000),
    ));

    // ── External anchor (REQ-8): head committed, verified against store.
    let receipt = AnchorReceipt {
        tree_head: store.tree_head(),
        entry_count: store.len() as u64,
        method: AnchorMethod::Rfc3161Tsa {
            authority: "tsa.example".to_owned(),
            token_hash: ContentHash([0xA1; 32]),
        },
        anchored_at: ts(1_100),
    };
    let receipt_hash = ok!(receipt.receipt_hash());
    let mut anchor_log = AnchorLog::new();
    ok!(anchor_log.append(receipt));
    ok!(anchor_log.verify_against(&store));

    // ── Disclosure pack (REQ-26): roll-forward, tie-out, manifest.
    let row_evidence = vec![
        valuation_hash,
        ok!(e_buy.entry_hash()),
        ok!(e_reward.entry_hash()),
        ok!(e_mark.entry_hash()),
    ];
    let row = ok!(RollForwardRow::new(
        btc(),
        usd(0),
        usd(75),
        usd(0),
        usd(25),
        usd(100),
        row_evidence,
    ));
    let pack = DisclosurePack {
        tenant: tenant(),
        period: "2026Q2".to_owned(),
        checkpoint,
        valuation: valuation_hash,
        policies: vec![("principal-market/v1".to_owned(), active_pm)],
        anchor_receipt: Some(receipt_hash),
        rows: vec![row],
    };
    assert_eq!(pack.tie_to_valuation(&valuation), Vec::new());
    let manifest = pack.manifest();
    for required in [checkpoint, valuation_hash, active_pm, receipt_hash] {
        assert!(
            manifest.contains(&required),
            "manifest must close over refs"
        );
    }

    // Phase 0 guarantee, full-system: the chain still verifies.
    assert_eq!(ledger.verify_chain(&tenant()), Ok(()));

    CloseArtifacts {
        pack_hash: ok!(pack.pack_hash()),
        checkpoint,
        valuation_hash,
        receipt_hash,
    }
}

#[test]
fn golden_close_runs_and_is_deterministic() {
    let first = run_close();
    let second = run_close();
    // Whole-close determinism: every artifact reproduces, byte-for-byte.
    assert_eq!(first.pack_hash, second.pack_hash);
    assert_eq!(first.checkpoint, second.checkpoint);
    assert_eq!(first.valuation_hash, second.valuation_hash);
    assert_eq!(first.receipt_hash, second.receipt_hash);
}
