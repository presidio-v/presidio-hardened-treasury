//! The product, end to end: lots → valuation → roll-forward → pack →
//! tie-out → manifest (REQ-26).

use std::collections::BTreeMap;
use treasury_core::{AssetAmount, AssetId, ContentHash, TenantId, TimestampNs, VenueId};
use treasury_disclosure::{DisclosurePack, RollForwardRow, TieBreak};
use treasury_fairvalue::{value_book, Price, PriceSnapshot, Valuation};
use treasury_lots::LotBook;

fn usd(minor: i128) -> AssetAmount {
    AssetAmount::new(AssetId::new("USD"), minor)
}

fn btc() -> AssetId {
    AssetId::new("BTC")
}

fn ts(n: i64) -> TimestampNs {
    TimestampNs::from_nanos(n)
}

fn policy() -> ContentHash {
    ContentHash([3; 32])
}

/// One period for tenant "g-t": acquire 7 atoms at basis 70, value at
/// 100 → remeasurement +30. Reuses the cross-verified golden chain.
fn close() -> (Valuation, RollForwardRow) {
    let mut book = LotBook::new(TenantId::new("g-t"));
    let Ok(_) = book.acquire(
        btc(),
        VenueId::new("v"),
        7,
        usd(70),
        usd(1),
        ts(10),
        ContentHash([1; 32]),
    ) else {
        unreachable!("acquire must succeed");
    };
    let mut prices = BTreeMap::new();
    prices.insert(
        btc(),
        Price {
            minor_per_unit: 100,
            atoms_per_unit: 7,
        },
    );
    let snapshot = PriceSnapshot {
        currency: AssetId::new("USD"),
        as_of: ts(1_000),
        prices,
    };
    let Ok(valuation) = value_book(&book, &snapshot, policy()) else {
        unreachable!("valuation must succeed");
    };
    let Ok(valuation_hash) = valuation.valuation_hash() else {
        unreachable!("hash must compute");
    };

    let Ok(row) = RollForwardRow::new(
        btc(),
        usd(0),
        usd(70),
        usd(0),
        usd(30),
        usd(100),
        vec![valuation_hash],
    ) else {
        unreachable!("row rolls: 0 + 70 - 0 + 30 == 100");
    };
    (valuation, row)
}

fn pack(valuation: &Valuation, rows: Vec<RollForwardRow>) -> DisclosurePack {
    let Ok(valuation_hash) = valuation.valuation_hash() else {
        unreachable!("hash must compute");
    };
    DisclosurePack {
        tenant: TenantId::new("g-t"),
        period: "2026Q2".to_owned(),
        checkpoint: ContentHash([7; 32]),
        valuation: valuation_hash,
        policies: vec![("principal-market/v1".to_owned(), policy())],
        anchor_receipt: None,
        rows,
    }
}

#[test]
fn pack_ties_to_valuation() {
    let (valuation, row) = close();
    let pack = pack(&valuation, vec![row]);
    assert_eq!(pack.tie_to_valuation(&valuation), Vec::new());
}

#[test]
fn closing_mismatch_is_named() {
    let (valuation, _) = close();
    // A row that rolls internally but to the wrong closing balance.
    let Ok(bad_row) = RollForwardRow::new(
        btc(),
        usd(0),
        usd(70),
        usd(0),
        usd(29),
        usd(99),
        vec![],
    ) else {
        unreachable!("row rolls: 0 + 70 - 0 + 29 == 99");
    };
    let pack = pack(&valuation, vec![bad_row]);
    assert_eq!(
        pack.tie_to_valuation(&valuation),
        vec![TieBreak::ClosingMismatch {
            asset: btc(),
            closing: 99,
            fair_value: 100,
        }]
    );
}

#[test]
fn missing_and_extra_rows_are_named() {
    let (valuation, _) = close();
    // No BTC row; an ETH row instead (rolls trivially: all zero).
    let Ok(eth_row) = RollForwardRow::new(
        AssetId::new("ETH"),
        usd(0),
        usd(0),
        usd(0),
        usd(0),
        usd(0),
        vec![],
    ) else {
        unreachable!("zero row rolls");
    };
    let pack = pack(&valuation, vec![eth_row]);
    let breaks = pack.tie_to_valuation(&valuation);
    assert!(breaks.contains(&TieBreak::MissingRow { asset: btc() }));
    assert!(breaks.contains(&TieBreak::ExtraRow {
        asset: AssetId::new("ETH"),
    }));
}

#[test]
fn manifest_is_the_sorted_closure_of_referenced_hashes() {
    let (valuation, row) = close();
    let Ok(valuation_hash) = valuation.valuation_hash() else {
        unreachable!("hash must compute");
    };
    let pack = pack(&valuation, vec![row]);
    let manifest = pack.manifest();
    // checkpoint, valuation, policy, and the row's evidence (the
    // valuation hash again — deduplicated).
    assert!(manifest.contains(&ContentHash([7; 32])));
    assert!(manifest.contains(&valuation_hash));
    assert!(manifest.contains(&policy()));
    assert_eq!(manifest.len(), 3);
    let mut sorted = manifest.clone();
    sorted.sort_unstable();
    assert_eq!(manifest, sorted);
}

/// Golden vector — pack hash independently recomputed in Python over
/// the full serde shape, on top of the already-golden valuation chain.
#[test]
fn golden_pack_hash_matches_independent_implementation() {
    let (valuation, row) = close();
    let disclosure = pack(&valuation, vec![row]);
    let hash = disclosure.pack_hash().map(|h| h.to_hex());
    assert_eq!(
        hash.as_deref(),
        Ok("f3931f76f9d08d70e0310042cd95191ffd220048314be76e20eabe2abb9a90a1")
    );
}
