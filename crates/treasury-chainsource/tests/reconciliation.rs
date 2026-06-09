//! The two-source completeness control end to end (ADR-0004, §3.3):
//! agreement, divergence-blocks-close, finality gating across sources,
//! reproducibility gate, and the per-chain asymmetry (BTC depth vs ETH
//! external finalized).

use treasury_chainsource::{
    reconcile, reproducibility_gate, AddressHistory, Chain, ChainMovement, Direction,
    FinalityPolicy, FinalityRule, FixtureSource, ReconcileError, ReproError,
};
use treasury_core::{AssetAmount, AssetId};

fn btc(atoms: i128) -> AssetAmount {
    AssetAmount::new(AssetId::new("BTC"), atoms)
}

fn movement(tx: &str, dir: Direction, atoms: i128, height: u64) -> ChainMovement {
    ChainMovement {
        tx_ref: tx.to_owned(),
        direction: dir,
        amount: btc(atoms),
        block_height: height,
    }
}

fn history(address: &str, movements: Vec<ChainMovement>) -> AddressHistory {
    AddressHistory {
        chain: Chain::Bitcoin,
        address: address.to_owned(),
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
fn matching_sources_agree() {
    let addr = "bc1q-acme";
    let movements = vec![
        movement("t1", Direction::Inflow, 500, 10),
        movement("t2", Direction::Outflow, 200, 20),
    ];
    // electrs and Fulcrum over the same Core: same history, possibly in a
    // different order — must still agree.
    let electrs = FixtureSource::new(Chain::Bitcoin, "core+electrs")
        .with_history(history(addr, movements.clone()));
    let mut reordered = movements.clone();
    reordered.reverse();
    let fulcrum =
        FixtureSource::new(Chain::Bitcoin, "core+fulcrum").with_history(history(addr, reordered));

    let Ok(outcome) = reconcile(&electrs, &fulcrum, &btc_depth_6(), addr, 100) else {
        unreachable!("reconcile must succeed");
    };
    assert!(outcome.agreed());
}

#[test]
fn a_divergence_blocks_close_and_names_both_sources() {
    let addr = "bc1q-acme";
    let electrs = FixtureSource::new(Chain::Bitcoin, "core+electrs").with_history(history(
        addr,
        vec![movement("t1", Direction::Inflow, 500, 10)],
    ));
    // Fulcrum is missing a settled movement (a real indexing bug).
    let fulcrum = FixtureSource::new(Chain::Bitcoin, "core+fulcrum").with_history(history(
        addr,
        vec![
            movement("t1", Direction::Inflow, 500, 10),
            movement("t2", Direction::Outflow, 200, 20),
        ],
    ));

    let Ok(outcome) = reconcile(&electrs, &fulcrum, &btc_depth_6(), addr, 100) else {
        unreachable!("reconcile must succeed");
    };
    assert!(
        !outcome.agreed(),
        "a settled-history divergence must block close"
    );
    let treasury_chainsource::Reconciliation::Diverged { a, b, .. } = outcome else {
        unreachable!("must be a divergence");
    };
    assert_eq!(a.0, "core+electrs");
    assert_eq!(b.0, "core+fulcrum");
    assert_ne!(a.1, b.1);
}

#[test]
fn unsettled_movements_do_not_cause_false_divergence() {
    let addr = "bc1q-acme";
    // Both agree on settled history; they differ only above the settled
    // height (one source saw an extra unconfirmed tx). With depth 6 and
    // tip 100, settled height is 94 — the height-95 movement is excluded.
    let settled = vec![movement("t1", Direction::Inflow, 500, 10)];
    let a = FixtureSource::new(Chain::Bitcoin, "a").with_history(history(addr, settled.clone()));
    let mut with_unconfirmed = settled.clone();
    with_unconfirmed.push(movement("t-pending", Direction::Inflow, 1, 95));
    let b = FixtureSource::new(Chain::Bitcoin, "b").with_history(history(addr, with_unconfirmed));

    let Ok(outcome) = reconcile(&a, &b, &btc_depth_6(), addr, 100) else {
        unreachable!("reconcile must succeed");
    };
    assert!(
        outcome.agreed(),
        "unsettled-only differences must not diverge"
    );
}

#[test]
fn chain_mismatch_is_rejected() {
    let btc_source = FixtureSource::new(Chain::Bitcoin, "btc");
    let eth_source = FixtureSource::new(Chain::Ethereum, "eth");
    let policy = btc_depth_6();
    assert_eq!(
        reconcile(&btc_source, &eth_source, &policy, "addr", 100),
        Err(ReconcileError::ChainMismatch)
    );
}

#[test]
fn ethereum_uses_external_finalized_height() {
    // ETH: the caller supplies the consensus-finalized height as the
    // observed value; everything at or below it is settled.
    let addr = "0xacme";
    let eth_history = AddressHistory {
        chain: Chain::Ethereum,
        address: addr.to_owned(),
        settled_to_height: 0,
        movements: vec![ChainMovement {
            tx_ref: "0xabc".to_owned(),
            direction: Direction::Inflow,
            amount: AssetAmount::new(AssetId::new("ETH"), 1_000_000),
            block_height: 21_000_000,
        }],
    };
    let reth = FixtureSource::new(Chain::Ethereum, "reth").with_history(eth_history.clone());
    let erigon = FixtureSource::new(Chain::Ethereum, "erigon").with_history(eth_history);
    let policy = FinalityPolicy {
        chain: Chain::Ethereum,
        rule: FinalityRule::ExternalFinalized,
    };
    let Ok(outcome) = reconcile(&reth, &erigon, &policy, addr, 21_000_000) else {
        unreachable!("reconcile must succeed");
    };
    assert!(outcome.agreed());
}

#[test]
fn reproducibility_gate_passes_for_a_deterministic_source() {
    let addr = "bc1q-acme";
    let source = FixtureSource::new(Chain::Bitcoin, "core+electrs").with_history(history(
        addr,
        vec![movement("t1", Direction::Inflow, 500, 10)],
    ));
    assert!(reproducibility_gate(&source, &btc_depth_6(), addr, 100).is_ok());
}

#[test]
fn reproducibility_gate_rejects_wrong_chain() {
    let source = FixtureSource::new(Chain::Ethereum, "erigon");
    let policy = btc_depth_6();
    assert!(matches!(
        reproducibility_gate(&source, &policy, "addr", 100),
        Err(ReproError::Source(_))
    ));
}
