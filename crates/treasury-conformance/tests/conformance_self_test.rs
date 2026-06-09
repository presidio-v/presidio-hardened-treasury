//! The conformance suites run against the in-memory fixtures. This proves
//! two things at once: the harness wiring is correct, and the fixtures
//! (the stand-ins the rest of the test suite relies on) actually satisfy
//! the same contract the live shims will be held to. When a real endpoint
//! lands, its shim crate calls these same `verify_*` functions.

use treasury_anchor::{AnchorTarget, FixtureChainSubmitter};
use treasury_chainsource::{
    AddressHistory, Chain, ChainMovement, Direction, FinalityPolicy, FinalityRule, FixtureSource,
};
use treasury_conformance::{anchor_submitter, chain_source, gl_adapter};
use treasury_core::{ActorId, AssetAmount, AssetId, ContentHash, TenantId, TimestampNs};
use treasury_gaap::{JournalEntry, JournalLine, Side, StatementLine};
use treasury_gl::FixtureGl;
use treasury_posting::PostingBatch;

// --- chain source -------------------------------------------------------

fn btc_movement() -> ChainMovement {
    ChainMovement {
        tx_ref: "t1".to_owned(),
        direction: Direction::Inflow,
        amount: AssetAmount::new(AssetId::new("BTC"), 500),
        block_height: 10,
    }
}

fn btc_history() -> AddressHistory {
    AddressHistory {
        chain: Chain::Bitcoin,
        address: "bc1q-acme".to_owned(),
        settled_to_height: 0,
        movements: vec![btc_movement()],
    }
}

fn btc_policy() -> FinalityPolicy {
    FinalityPolicy {
        chain: Chain::Bitcoin,
        rule: FinalityRule::ConfirmationDepth { depth: 6 },
    }
}

#[test]
fn bitcoin_core_electrs_and_fulcrum_satisfy_the_chain_source_contract() {
    let electrs = FixtureSource::new(Chain::Bitcoin, "core+electrs").with_history(btc_history());
    let fulcrum = FixtureSource::new(Chain::Bitcoin, "core+fulcrum").with_history(btc_history());

    assert_eq!(
        chain_source::verify_chain_source_contract(
            &electrs,
            Chain::Bitcoin,
            &btc_policy(),
            "bc1q-acme",
            90,
            100,
        ),
        Ok(())
    );
    assert!(chain_source::verify_sources_agree(
        &electrs,
        &fulcrum,
        &btc_policy(),
        "bc1q-acme",
        100,
    )
    .is_ok());
}

#[test]
fn ethereum_reth_and_erigon_satisfy_the_chain_source_contract() {
    let history = AddressHistory {
        chain: Chain::Ethereum,
        address: "0xacme".to_owned(),
        settled_to_height: 0,
        movements: vec![ChainMovement {
            tx_ref: "0xdead".to_owned(),
            direction: Direction::Inflow,
            amount: AssetAmount::new(AssetId::new("ETH"), 1_000_000),
            block_height: 21_000_000,
        }],
    };
    let reth = FixtureSource::new(Chain::Ethereum, "reth").with_history(history.clone());
    let erigon = FixtureSource::new(Chain::Ethereum, "erigon").with_history(history);
    let policy = FinalityPolicy {
        chain: Chain::Ethereum,
        rule: FinalityRule::ExternalFinalized,
    };

    assert_eq!(
        chain_source::verify_identity(&reth, Chain::Ethereum),
        Ok(())
    );
    assert!(chain_source::verify_sources_agree(
        &reth,
        &erigon,
        &policy,
        "0xacme",
        21_000_001,
    )
    .is_ok());
}

#[test]
fn a_diverging_indexer_fails_the_agreement_contract() {
    let electrs = FixtureSource::new(Chain::Bitcoin, "core+electrs").with_history(btc_history());
    // Fulcrum is missing the settled movement — a real indexing bug.
    let fulcrum = FixtureSource::new(Chain::Bitcoin, "core+fulcrum").with_history(AddressHistory {
        movements: vec![],
        ..btc_history()
    });
    assert_eq!(
        chain_source::verify_sources_agree(&electrs, &fulcrum, &btc_policy(), "bc1q-acme", 100),
        Err(treasury_conformance::ContractViolation::SourcesDisagree)
    );
}

// --- anchor submitter ---------------------------------------------------

fn target(byte: u8, entry_count: u64) -> AnchorTarget {
    AnchorTarget {
        tree_head: ContentHash([byte; 32]),
        entry_count,
    }
}

#[test]
fn fixture_chain_submitter_satisfies_the_anchor_contract() {
    let mut submitter = FixtureChainSubmitter::new(800_000, 1);
    let Ok(receipts) = anchor_submitter::verify_anchor_submitter_contract(
        &mut submitter,
        vec![target(1, 10), target(2, 5)],
        3,
        16,
        TimestampNs::from_nanos(1),
    ) else {
        unreachable!("the fixture submitter confirms within budget");
    };
    assert_eq!(receipts.len(), 2);
}

#[test]
fn too_small_a_poll_budget_surfaces_a_liveness_failure() {
    let mut submitter = FixtureChainSubmitter::new(800_000, 50);
    let outcome = anchor_submitter::verify_anchor_submitter_contract(
        &mut submitter,
        vec![target(1, 1)],
        3,
        2,
        TimestampNs::from_nanos(1),
    );
    assert!(matches!(
        outcome,
        Err(treasury_conformance::ContractViolation::AnchorNeverConfirmed { .. })
    ));
}

// --- gl adapter ---------------------------------------------------------

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
fn fixture_gl_satisfies_the_adapter_contract() {
    let mut gl = FixtureGl::new();
    assert_eq!(
        gl_adapter::verify_gl_adapter_contract(
            &mut gl,
            &batch(),
            ActorId::new("alice"),
            ActorId::new("bob"),
        ),
        Ok(())
    );
}
