//! Lot engine lifecycle (REQ-23): FIFO and specific-ID relief, exact
//! basis conservation under partial relief, decomposed fees, fail-closed
//! sufficiency, and basis-preserving transfers.

use treasury_core::{AssetAmount, AssetId, ContentHash, TenantId, TimestampNs, VenueId};
use treasury_lots::{LotBook, LotError, ReliefMethod};

fn usd(minor: i128) -> AssetAmount {
    AssetAmount::new(AssetId::new("USD"), minor)
}

fn btc() -> AssetId {
    AssetId::new("BTC")
}

fn exchange() -> VenueId {
    VenueId::new("exchange")
}

fn cold() -> VenueId {
    VenueId::new("cold-wallet")
}

fn ts(n: i64) -> TimestampNs {
    TimestampNs::from_nanos(n)
}

fn ev(n: u8) -> ContentHash {
    ContentHash([n; 32])
}

fn book() -> LotBook {
    LotBook::new(TenantId::new("acme"))
}

#[test]
fn fifo_relieves_oldest_first() {
    let mut book = book();
    let Ok(first) = book.acquire(btc(), exchange(), 100, usd(1_000), usd(10), ts(10), ev(1))
    else {
        unreachable!("acquire must succeed");
    };
    let Ok(_second) = book.acquire(btc(), exchange(), 100, usd(2_000), usd(20), ts(20), ev(2))
    else {
        unreachable!("acquire must succeed");
    };

    let Ok(result) = book.dispose(
        &btc(),
        &exchange(),
        100,
        ReliefMethod::Fifo,
        ev(9),
        usd(1_500),
    ) else {
        unreachable!("dispose must succeed");
    };
    assert_eq!(result.reliefs.len(), 1);
    assert_eq!(result.reliefs.first().map(|r| r.lot_id), Some(first));
    assert_eq!(result.total_basis_relieved, usd(1_000));
    assert_eq!(result.realized, usd(500));
    assert_eq!(book.held(&btc(), &exchange()), 100);
}

#[test]
fn partial_relief_conserves_basis_exactly() {
    let mut book = book();
    // Basis 10 over 3 atoms: floor division must leak nothing.
    let Ok(_) = book.acquire(btc(), exchange(), 3, usd(10), usd(0), ts(10), ev(1)) else {
        unreachable!("acquire must succeed");
    };

    let Ok(first) = book.dispose(&btc(), &exchange(), 1, ReliefMethod::Fifo, ev(9), usd(0))
    else {
        unreachable!("dispose must succeed");
    };
    // floor(10 * 1 / 3) = 3; residual lot keeps 7 over 2 atoms.
    assert_eq!(first.total_basis_relieved, usd(3));

    let Ok(second) = book.dispose(&btc(), &exchange(), 2, ReliefMethod::Fifo, ev(9), usd(0))
    else {
        unreachable!("dispose must succeed");
    };
    assert_eq!(second.total_basis_relieved, usd(7));
    // 3 + 7 == 10: conservation to the minor unit.
    assert_eq!(book.held(&btc(), &exchange()), 0);
    assert!(book.open_lots().is_empty());
}

#[test]
fn fees_stay_decomposed_from_basis() {
    let mut book = book();
    let Ok(_) = book.acquire(btc(), exchange(), 10, usd(1_000), usd(50), ts(10), ev(1)) else {
        unreachable!("acquire must succeed");
    };
    let Ok(result) = book.dispose(&btc(), &exchange(), 5, ReliefMethod::Fifo, ev(9), usd(600))
    else {
        unreachable!("dispose must succeed");
    };
    // Realized gain is proceeds − basis; the fee rides along separately
    // for the policy layer to capitalize or expense (G-3).
    assert_eq!(result.total_basis_relieved, usd(500));
    assert_eq!(result.realized, usd(100));
    assert_eq!(result.reliefs.first().map(|r| r.fee_relieved.clone()), Some(usd(25)));
}

#[test]
fn specific_identification_relieves_named_lots() {
    let mut book = book();
    let Ok(_first) = book.acquire(btc(), exchange(), 100, usd(1_000), usd(0), ts(10), ev(1))
    else {
        unreachable!("acquire must succeed");
    };
    let Ok(second) = book.acquire(btc(), exchange(), 100, usd(4_000), usd(0), ts(20), ev(2))
    else {
        unreachable!("acquire must succeed");
    };

    // Elect the high-basis lot explicitly (e.g. loss harvesting).
    let Ok(result) = book.dispose(
        &btc(),
        &exchange(),
        100,
        ReliefMethod::SpecificLots(vec![second]),
        ev(9),
        usd(3_000),
    ) else {
        unreachable!("dispose must succeed");
    };
    assert_eq!(result.total_basis_relieved, usd(4_000));
    assert_eq!(result.realized, usd(-1_000));
}

#[test]
fn overdraw_fails_closed_and_mutates_nothing() {
    let mut book = book();
    let Ok(_) = book.acquire(btc(), exchange(), 100, usd(1_000), usd(0), ts(10), ev(1)) else {
        unreachable!("acquire must succeed");
    };
    let before = book.lots_hash();
    let result = book.dispose(&btc(), &exchange(), 101, ReliefMethod::Fifo, ev(9), usd(0));
    assert_eq!(
        result,
        Err(LotError::InsufficientQuantity {
            requested: 101,
            available: 100,
        })
    );
    assert_eq!(book.lots_hash(), before, "failed disposal must not mutate");
}

#[test]
fn transfer_preserves_basis_and_acquisition_date() {
    let mut book = book();
    let Ok(origin) = book.acquire(btc(), cold(), 100, usd(1_000), usd(10), ts(10), ev(1))
    else {
        unreachable!("acquire must succeed");
    };

    let Ok(new_ids) = book.transfer(&btc(), &cold(), &exchange(), 40, ev(2)) else {
        unreachable!("transfer must succeed");
    };
    assert_eq!(new_ids.len(), 1);
    assert_eq!(book.held(&btc(), &cold()), 60);
    assert_eq!(book.held(&btc(), &exchange()), 40);

    let lots = book.open_lots();
    let Some(moved) = lots.iter().find(|l| l.venue == exchange()) else {
        unreachable!("moved lot exists");
    };
    // Pro-rata basis moved; original acquisition date preserved (holding
    // period continuity); lineage recorded. Nothing realized.
    assert_eq!(moved.cost_basis, usd(400));
    assert_eq!(moved.acquisition_fee, usd(4));
    assert_eq!(moved.acquired_at, ts(10));
    assert_eq!(moved.moved_from, Some(origin));

    let Some(residual) = lots.iter().find(|l| l.venue == cold()) else {
        unreachable!("residual lot exists");
    };
    // Conservation across the transfer: 400 + 600 = 1000, 4 + 6 = 10.
    assert_eq!(residual.cost_basis, usd(600));
    assert_eq!(residual.acquisition_fee, usd(6));
}

#[test]
fn transferred_lot_relieves_with_original_date_under_fifo() {
    let mut book = book();
    // Old lot acquired at t=10, transferred later; a newer lot at t=20.
    let Ok(_) = book.acquire(btc(), cold(), 50, usd(500), usd(0), ts(10), ev(1)) else {
        unreachable!("acquire must succeed");
    };
    let Ok(_) = book.acquire(btc(), exchange(), 50, usd(5_000), usd(0), ts(20), ev(2)) else {
        unreachable!("acquire must succeed");
    };
    let Ok(_) = book.transfer(&btc(), &cold(), &exchange(), 50, ev(3)) else {
        unreachable!("transfer must succeed");
    };

    // FIFO at the exchange must pick the *transferred* lot first: its
    // acquisition date (t=10) predates the native lot (t=20).
    let Ok(result) = book.dispose(&btc(), &exchange(), 50, ReliefMethod::Fifo, ev(9), usd(0))
    else {
        unreachable!("dispose must succeed");
    };
    assert_eq!(result.total_basis_relieved, usd(500));
}

/// Golden vectors — lot id and open-lot-set hash independently
/// recomputed in Python over the full serde shape.
#[test]
fn golden_hashes_match_independent_implementation() {
    let mut book = LotBook::new(TenantId::new("g-t"));
    let acquired = book.acquire(
        btc(),
        VenueId::new("v"),
        7,
        usd(70),
        usd(1),
        ts(10),
        ev(1),
    );
    assert_eq!(
        acquired.map(|h| h.to_hex()).as_deref(),
        Ok("b0c75b7fc9812c7ecc74825fe5a43418c6c9d9f879ad036066d9db7cd5fa02cf")
    );
    assert_eq!(
        book.lots_hash().map(|h| h.to_hex()).as_deref(),
        Ok("c6d7c81944dacce0a2bc5b2c2e5ac4639bbbe3d3330acb9bfb2897114566b121")
    );
}

#[test]
fn lots_hash_is_deterministic_and_state_sensitive() {
    let mut book = book();
    let empty = book.lots_hash();
    let Ok(_) = book.acquire(btc(), exchange(), 10, usd(100), usd(1), ts(10), ev(1)) else {
        unreachable!("acquire must succeed");
    };
    let after_acquire = book.lots_hash();
    assert_ne!(empty, after_acquire);
    assert_eq!(book.lots_hash(), after_acquire, "hash is a pure function of state");
}
