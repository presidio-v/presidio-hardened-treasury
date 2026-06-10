//! Negative close: each fail-closed guarantee, proven to compose across
//! crate boundaries. A clean close is the golden test; here every test
//! makes exactly one thing go wrong and asserts the pipeline refuses to
//! proceed for that specific reason — never "errors somewhere."

use std::cell::Cell;
use std::collections::BTreeMap;

use treasury_chainsource::{
    draft_history_observation, reconcile, reproducibility_gate, AddressHistory, BookError, Chain,
    ChainMovement, ChainSource, Direction, FinalityPolicy, FinalityRule, FixtureSource, ReproError,
    SourceError,
};
use treasury_core::{ActorId, AssetAmount, AssetId, ContentHash, TenantId, TimestampNs, VenueId};
use treasury_fairvalue::{value_book, FvError, PriceSnapshot};
use treasury_gaap::{EntryError, JournalEntry, JournalLine, Side, StatementLine};
use treasury_lots::LotBook;
use treasury_scope::{CriteriaAssessment, CriterionStatus, GateDecision, ScopeAssessment, ScopeGate};

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

fn btc_policy() -> FinalityPolicy {
    FinalityPolicy {
        chain: Chain::Bitcoin,
        rule: FinalityRule::ConfirmationDepth { depth: 6 },
    }
}

fn movement(atoms: i128) -> ChainMovement {
    ChainMovement {
        tx_ref: "t1".to_owned(),
        direction: Direction::Inflow,
        amount: AssetAmount::new(btc(), atoms),
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

fn source(id: &str, movements: Vec<ChainMovement>) -> FixtureSource {
    FixtureSource::new(Chain::Bitcoin, id).with_history(history(movements))
}

/// A source that returns different settled history on consecutive calls.
struct FlakySource {
    flip: Cell<bool>,
}

impl ChainSource for FlakySource {
    fn chain(&self) -> Chain {
        Chain::Bitcoin
    }

    fn source_id(&self) -> &str {
        "flaky"
    }

    fn address_history(
        &self,
        _address: &str,
        _observed_height: u64,
    ) -> Result<AddressHistory, SourceError> {
        let first = self.flip.get();
        self.flip.set(!first);
        let atoms = if first { 500 } else { 999 };
        Ok(history(vec![movement(atoms)]))
    }
}

/// Two independent sources that disagree must block close: the §3.3
/// completeness gate refuses to book anything until a human resolves it.
#[test]
fn a_chain_divergence_blocks_the_completeness_gate() {
    let electrs = source("core+electrs", vec![movement(500)]);
    // Fulcrum is missing the settled movement — a real indexing bug.
    let fulcrum = source("core+fulcrum", vec![]);
    let Ok(reconciliation) = reconcile(&electrs, &fulcrum, &btc_policy(), "bc1q-acme", 100) else {
        unreachable!("reconcile runs against both sources");
    };
    assert!(!reconciliation.agreed());

    // A divergence cannot be booked: no L1 observation enters the ledger,
    // so nothing downstream can close over forged or incomplete history.
    let booked = draft_history_observation(
        &reconciliation,
        tenant(),
        "core+electrs|core+fulcrum",
        ContentHash([7; 32]),
        ts(10),
    );
    assert!(matches!(booked, Err(BookError::Diverged)));
}

/// A non-deterministic source breaks "reproduces byte-for-byte" — the
/// reproducibility gate rejects it before it can become a system of record.
#[test]
fn a_non_deterministic_source_is_rejected_before_booking() {
    let flaky = FlakySource {
        flip: Cell::new(true),
    };
    let result = reproducibility_gate(&flaky, &btc_policy(), "bc1q-acme", 100);
    assert!(matches!(result, Err(ReproError::NotReproducible { .. })));
}

/// An in-scope asset with no price must fail valuation closed, not value
/// at zero or skip the asset.
#[test]
fn a_missing_price_fails_valuation_closed() {
    let mut book = LotBook::new(tenant());
    let acquired = book.acquire(
        btc(),
        VenueId::new("exchange"),
        7,
        usd(70),
        usd(1),
        ts(10),
        ContentHash([1; 32]),
    );
    assert!(acquired.is_ok());

    let snapshot = PriceSnapshot {
        currency: AssetId::new("USD"),
        as_of: ts(1_000),
        prices: BTreeMap::new(),
    };
    let result = value_book(&book, &snapshot, ContentHash([2; 32]));
    let Err(FvError::MissingPrice(asset)) = result else {
        unreachable!("a held asset with no price must fail closed");
    };
    assert_eq!(asset, btc());
}

/// An out-of-scope asset hard-blocks at the ASU 2023-08 gate, before any
/// accumulation or valuation can touch it.
#[test]
fn an_out_of_scope_asset_hard_blocks() {
    let criteria = CriteriaAssessment {
        intangible_asset: CriterionStatus::Met,
        no_enforceable_claim: CriterionStatus::NotMet,
        on_distributed_ledger: CriterionStatus::Met,
        cryptographically_secured: CriterionStatus::Met,
        fungible: CriterionStatus::Met,
        not_self_issued: CriterionStatus::Met,
    };
    let assessment = ScopeAssessment {
        tenant: tenant(),
        asset: btc(),
        criteria,
        policy_hash: ContentHash([2; 32]),
    };

    let mut gate = ScopeGate::new();
    let Ok(item) = gate.propose(assessment.clone(), ActorId::new("preparer")) else {
        unreachable!("proposal is well-formed");
    };
    assert!(gate.confirm(&item, ActorId::new("approver")).is_ok());
    assert!(matches!(
        gate.check(&tenant(), &btc()),
        GateDecision::BlockedOutOfScope { .. }
    ));
}

/// An unbalanced journal entry is unconstructible: the invariant is
/// enforced at the boundary, so no caller can build one to post.
#[test]
fn an_unbalanced_entry_is_unconstructible() {
    let lines = vec![
        JournalLine {
            side: Side::Debit,
            line: StatementLine::CryptoAssets,
            amount: usd(100),
        },
        JournalLine {
            side: Side::Credit,
            line: StatementLine::UnrealizedCryptoGainLoss,
            amount: usd(50),
        },
    ];
    let result = JournalEntry::new("manual", lines, ContentHash([3; 32]));
    assert!(matches!(
        result,
        Err(EntryError::Unbalanced {
            debits: 100,
            credits: 50
        })
    ));
}
