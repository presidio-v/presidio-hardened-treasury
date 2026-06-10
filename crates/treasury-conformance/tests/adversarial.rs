//! Adversarial conformance: prove the suites *catch* a misbehaving shim,
//! not just that the honest fixtures pass. Each test builds a shim that
//! violates exactly one contract in the way a real integration could (a
//! non-deterministic indexer, a chain whose tip rewinds, a GL that
//! double-posts a retried key) and asserts the matching
//! [`ContractViolation`]. Together with the fixture self-tests, this shows
//! both directions of every contract.

use std::cell::Cell;

use treasury_anchor::{
    AnchorTarget, Broadcast, ChainAnchorSubmitter, Confirmation, FixtureChainSubmitter,
    SubmitterError,
};
use treasury_chainsource::{
    AddressHistory, Chain, ChainMovement, ChainSource, Direction, FinalityPolicy, FinalityRule,
    FixtureSource, SourceError,
};
use treasury_conformance::{anchor_submitter, chain_source, gl_adapter, ContractViolation};
use treasury_core::{AssetAmount, AssetId, ContentHash, TenantId, TimestampNs};
use treasury_gaap::{JournalEntry, JournalLine, Side, StatementLine};
use treasury_gl::{GlAdapter, GlError, GlReadback, SubmitOutcome};
use treasury_posting::PostingBatch;

fn btc_policy() -> FinalityPolicy {
    FinalityPolicy {
        chain: Chain::Bitcoin,
        rule: FinalityRule::ConfirmationDepth { depth: 6 },
    }
}

fn settled_movement(atoms: i128) -> ChainMovement {
    ChainMovement {
        tx_ref: "t1".to_owned(),
        direction: Direction::Inflow,
        amount: AssetAmount::new(AssetId::new("BTC"), atoms),
        block_height: 10,
    }
}

fn target(byte: u8, entry_count: u64) -> AnchorTarget {
    AnchorTarget {
        tree_head: ContentHash([byte; 32]),
        entry_count,
    }
}

// --- chain source adversaries -------------------------------------------

// WrongChain and EmptySourceId are exercised directly through
// `FixtureSource`, which lets the chain and id be set to anything.

/// Yields different settled history on consecutive identical queries.
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
        address: &str,
        _observed_height: u64,
    ) -> Result<AddressHistory, SourceError> {
        let first = self.flip.get();
        self.flip.set(!first);
        let atoms = if first { 500 } else { 999 };
        Ok(AddressHistory {
            chain: Chain::Bitcoin,
            address: address.to_owned(),
            settled_to_height: 0,
            movements: vec![settled_movement(atoms)],
        })
    }
}

/// Rewrites an already-settled block as the observed tip advances: the
/// settled movement's amount depends on `observed_height`.
struct RewriteSource;

impl ChainSource for RewriteSource {
    fn chain(&self) -> Chain {
        Chain::Bitcoin
    }

    fn source_id(&self) -> &str {
        "rewriter"
    }

    fn address_history(
        &self,
        address: &str,
        observed_height: u64,
    ) -> Result<AddressHistory, SourceError> {
        let atoms = if observed_height >= 100 { 999 } else { 500 };
        Ok(AddressHistory {
            chain: Chain::Bitcoin,
            address: address.to_owned(),
            settled_to_height: 0,
            movements: vec![settled_movement(atoms)],
        })
    }
}

/// A node that is simply unreachable — a generic transport failure.
struct DownSource;

impl ChainSource for DownSource {
    fn chain(&self) -> Chain {
        Chain::Bitcoin
    }

    fn source_id(&self) -> &str {
        "down"
    }

    fn address_history(
        &self,
        _address: &str,
        _observed_height: u64,
    ) -> Result<AddressHistory, SourceError> {
        Err(SourceError::Transport("node unreachable".to_owned()))
    }
}

#[test]
fn wrong_chain_is_caught() {
    let source = FixtureSource::new(Chain::Ethereum, "mislabeled");
    let result = chain_source::verify_identity(&source, Chain::Bitcoin);
    assert!(matches!(result, Err(ContractViolation::WrongChain { .. })));
}

#[test]
fn empty_source_id_is_caught() {
    let source = FixtureSource::new(Chain::Bitcoin, "");
    let result = chain_source::verify_identity(&source, Chain::Bitcoin);
    assert!(matches!(result, Err(ContractViolation::EmptySourceId)));
}

#[test]
fn a_non_deterministic_source_fails_reproducibility() {
    let source = FlakySource {
        flip: Cell::new(true),
    };
    let result = chain_source::verify_reproducible(&source, &btc_policy(), "bc1q-acme", 100);
    assert!(matches!(result, Err(ContractViolation::NotReproducible { .. })));
}

#[test]
fn rewriting_a_settled_block_is_caught() {
    let result = chain_source::verify_settled_history_stable(
        &RewriteSource,
        &btc_policy(),
        "bc1q-acme",
        90,
        100,
    );
    assert!(matches!(
        result,
        Err(ContractViolation::SettledHistoryRewritten { .. })
    ));
}

#[test]
fn a_transport_failure_propagates_as_underlying() {
    let result = chain_source::verify_reproducible(&DownSource, &btc_policy(), "bc1q-acme", 100);
    assert!(matches!(result, Err(ContractViolation::Underlying(_))));
}

// --- anchor submitter adversaries ---------------------------------------

/// Reports a higher chain tip first, then a lower one — a rewinding tip
/// that would corrupt every downstream depth computation.
struct RewindSubmitter {
    polls: Cell<u32>,
}

impl ChainAnchorSubmitter for RewindSubmitter {
    fn broadcast(&mut self, _root: ContentHash) -> Result<Broadcast, SubmitterError> {
        Ok(Broadcast {
            tx_ref: "adv-tx".to_owned(),
            submitted_height: 100,
        })
    }

    fn poll(&self, _tx_ref: &str) -> Result<Confirmation, SubmitterError> {
        let n = self.polls.get();
        self.polls.set(n.saturating_add(1));
        let current_height = if n == 0 { 105 } else { 101 };
        Ok(Confirmation {
            current_height,
            included_at: None,
        })
    }

    fn calendar_proof(&self, _tx_ref: &str) -> Result<ContentHash, SubmitterError> {
        Err(SubmitterError::NotConfirmed)
    }
}

#[test]
fn a_rewinding_tip_is_caught() {
    let mut submitter = RewindSubmitter {
        polls: Cell::new(0),
    };
    let result = anchor_submitter::verify_anchor_submitter_contract(
        &mut submitter,
        vec![target(1, 1)],
        3,
        4,
        TimestampNs::from_nanos(1),
    );
    assert!(matches!(result, Err(ContractViolation::HeightWentBackwards { .. })));
}

#[test]
fn a_never_confirming_broadcast_fails_liveness() {
    let mut submitter = FixtureChainSubmitter::never_confirming(800_000);
    let result = anchor_submitter::verify_anchor_submitter_contract(
        &mut submitter,
        vec![target(1, 1)],
        3,
        8,
        TimestampNs::from_nanos(1),
    );
    assert!(matches!(result, Err(ContractViolation::AnchorNeverConfirmed { .. })));
}

#[test]
fn a_short_receipt_set_is_caught() {
    // The pipeline upholds one-receipt-per-target by construction, so this
    // guard is exercised directly: two targets expected, none produced.
    let result = anchor_submitter::verify_receipt_coverage(2, &[]);
    assert_eq!(result, Err(ContractViolation::ReceiptCountMismatch { expected: 2, found: 0 }));
}

// --- gl adapter adversaries ---------------------------------------------

/// Read-back returns an entry the batch never contained.
struct MismatchGl;

impl GlAdapter for MismatchGl {
    fn submit(
        &mut self,
        _batch: &PostingBatch,
        _idempotency_key: ContentHash,
    ) -> Result<SubmitOutcome, GlError> {
        Ok(SubmitOutcome::Acknowledged {
            gl_ref: "JE-1".to_owned(),
            raw_response: Vec::new(),
        })
    }

    fn read_back(&self, _idempotency_key: ContentHash) -> Result<GlReadback, GlError> {
        Ok(GlReadback {
            gl_ref: Some("JE-1".to_owned()),
            entry_hashes: vec![ContentHash([0xAB; 32])],
            raw_payload: Vec::new(),
        })
    }
}

/// Acknowledges a second identical-key submit with a different reference.
struct NonIdempotentGl {
    posted: bool,
}

impl GlAdapter for NonIdempotentGl {
    fn submit(
        &mut self,
        _batch: &PostingBatch,
        _idempotency_key: ContentHash,
    ) -> Result<SubmitOutcome, GlError> {
        let gl_ref = if self.posted { "JE-B" } else { "JE-A" };
        self.posted = true;
        Ok(SubmitOutcome::Acknowledged {
            gl_ref: gl_ref.to_owned(),
            raw_response: Vec::new(),
        })
    }

    fn read_back(&self, _idempotency_key: ContentHash) -> Result<GlReadback, GlError> {
        Ok(GlReadback {
            gl_ref: Some("JE-A".to_owned()),
            entry_hashes: Vec::new(),
            raw_payload: Vec::new(),
        })
    }
}

fn usd(minor: i128) -> AssetAmount {
    AssetAmount::new(AssetId::new("USD"), minor)
}

fn entry(minor: i128) -> JournalEntry {
    let Ok(entry) = JournalEntry::new(
        "asu2023_08_remeasurement",
        vec![
            JournalLine {
                side: Side::Debit,
                line: StatementLine::CryptoAssets,
                amount: usd(minor),
            },
            JournalLine {
                side: Side::Credit,
                line: StatementLine::UnrealizedCryptoGainLoss,
                amount: usd(minor),
            },
        ],
        ContentHash([9; 32]),
    ) else {
        unreachable!("entry is balanced");
    };
    entry
}

fn batch() -> PostingBatch {
    PostingBatch {
        tenant: TenantId::new("acme"),
        target_gl: "netsuite:prod".to_owned(),
        period: "2026Q2".to_owned(),
        entries: vec![entry(30), entry(70)],
    }
}

#[test]
fn a_readback_that_disagrees_is_caught() {
    let mut gl = MismatchGl;
    let result = gl_adapter::verify_readback_fidelity(&mut gl, &batch());
    assert!(matches!(result, Err(ContractViolation::ReadbackMismatch { .. })));
}

#[test]
fn a_non_idempotent_submit_is_caught() {
    let mut gl = NonIdempotentGl { posted: false };
    let result = gl_adapter::verify_idempotent_submit(&mut gl, &batch());
    assert!(matches!(result, Err(ContractViolation::NotIdempotent { .. })));
}
