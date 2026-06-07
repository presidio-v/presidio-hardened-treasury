//! Valuation flow (REQ-24): purity, fail-closed prices, floor rounding,
//! and the (lots, price-snapshot, policy) key end to end.

use std::collections::BTreeMap;
use treasury_core::{AssetAmount, AssetId, ContentHash, TenantId, TimestampNs, VenueId};
use treasury_fairvalue::{value_book, FvError, Price, PriceSnapshot};
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

fn ev(n: u8) -> ContentHash {
    ContentHash([n; 32])
}

fn policy() -> ContentHash {
    ContentHash([3; 32])
}

fn snapshot_with(prices: &[(&str, i128, i128)]) -> PriceSnapshot {
    let mut map = BTreeMap::new();
    for (asset, minor, scale) in prices {
        map.insert(
            AssetId::new(*asset),
            Price {
                minor_per_unit: *minor,
                atoms_per_unit: *scale,
            },
        );
    }
    PriceSnapshot {
        currency: AssetId::new("USD"),
        as_of: ts(1_000),
        prices: map,
    }
}

#[test]
fn values_positions_and_unrealized_marks() {
    let mut book = LotBook::new(TenantId::new("acme"));
    // 1.5 BTC across two venues, total basis $90,000.00.
    let Ok(_) = book.acquire(
        btc(),
        VenueId::new("cold"),
        100_000_000,
        usd(6_000_000),
        usd(0),
        ts(10),
        ev(1),
    ) else {
        unreachable!("acquire must succeed");
    };
    let Ok(_) = book.acquire(
        btc(),
        VenueId::new("exchange"),
        50_000_000,
        usd(3_000_000),
        usd(0),
        ts(20),
        ev(2),
    ) else {
        unreachable!("acquire must succeed");
    };

    // BTC at $65,000.00 per unit.
    let snapshot = snapshot_with(&[("BTC", 6_500_000, 100_000_000)]);
    let Ok(valuation) = value_book(&book, &snapshot, policy()) else {
        unreachable!("valuation must succeed");
    };

    assert_eq!(valuation.positions.len(), 1);
    let Some(position) = valuation.positions.first() else {
        unreachable!("one position asserted above");
    };
    assert_eq!(position.atoms, 150_000_000);
    assert_eq!(position.cost_basis, usd(9_000_000));
    assert_eq!(position.fair_value, usd(9_750_000));
    assert_eq!(position.unrealized, usd(750_000));
    assert_eq!(valuation.total_fair_value, usd(9_750_000));
    assert_eq!(valuation.total_unrealized, usd(750_000));
}

#[test]
fn missing_price_fails_closed() {
    let mut book = LotBook::new(TenantId::new("acme"));
    let Ok(_) = book.acquire(
        AssetId::new("ETH"),
        VenueId::new("cold"),
        1_000,
        usd(100),
        usd(0),
        ts(10),
        ev(1),
    ) else {
        unreachable!("acquire must succeed");
    };
    let snapshot = snapshot_with(&[("BTC", 6_500_000, 100_000_000)]);
    assert_eq!(
        value_book(&book, &snapshot, policy()),
        Err(FvError::MissingPrice(AssetId::new("ETH")))
    );
}

#[test]
fn basis_currency_must_match_snapshot_currency() {
    let mut book = LotBook::new(TenantId::new("acme"));
    let eur = AssetAmount::new(AssetId::new("EUR"), 100);
    let eur_fee = AssetAmount::new(AssetId::new("EUR"), 0);
    let Ok(_) = book.acquire(btc(), VenueId::new("cold"), 10, eur, eur_fee, ts(10), ev(1))
    else {
        unreachable!("acquire must succeed");
    };
    let snapshot = snapshot_with(&[("BTC", 100, 10)]);
    assert_eq!(
        value_book(&book, &snapshot, policy()),
        Err(FvError::CurrencyMismatch)
    );
}

#[test]
fn floor_rounding_is_deterministic() {
    let mut book = LotBook::new(TenantId::new("acme"));
    let Ok(_) = book.acquire(btc(), VenueId::new("cold"), 1, usd(0), usd(0), ts(10), ev(1))
    else {
        unreachable!("acquire must succeed");
    };
    // 1 atom at 1 minor unit per 3 atoms: floor(1/3) = 0.
    let snapshot = snapshot_with(&[("BTC", 1, 3)]);
    let Ok(valuation) = value_book(&book, &snapshot, policy()) else {
        unreachable!("valuation must succeed");
    };
    assert_eq!(valuation.total_fair_value, usd(0));
}

#[test]
fn valuation_is_pure_and_key_commits_to_policy() {
    let mut book = LotBook::new(TenantId::new("acme"));
    let Ok(_) = book.acquire(btc(), VenueId::new("cold"), 7, usd(70), usd(1), ts(10), ev(1))
    else {
        unreachable!("acquire must succeed");
    };
    let snapshot = snapshot_with(&[("BTC", 100, 7)]);

    let a = value_book(&book, &snapshot, policy());
    let b = value_book(&book, &snapshot, policy());
    assert_eq!(a, b);
    let hash_a = a.and_then(|v| v.valuation_hash());
    let hash_b = b.and_then(|v| v.valuation_hash());
    assert_eq!(hash_a, hash_b);

    // Same lots, same prices, different governing policy → different key.
    let under_other_policy = value_book(&book, &snapshot, ContentHash([4; 32]));
    let Ok(other) = under_other_policy else {
        unreachable!("valuation must succeed");
    };
    let Ok(valuation) = value_book(&book, &snapshot, policy()) else {
        unreachable!("valuation must succeed");
    };
    assert_ne!(valuation.key_hash, other.key_hash);
    assert_eq!(valuation.positions, other.positions);
}

/// Golden vector — full chain (lot → lots-state → snapshot → key →
/// valuation) independently recomputed in Python.
#[test]
fn golden_valuation_hash_matches_independent_implementation() {
    let mut book = LotBook::new(TenantId::new("g-t"));
    let Ok(_) = book.acquire(btc(), VenueId::new("v"), 7, usd(70), usd(1), ts(10), ev(1))
    else {
        unreachable!("acquire must succeed");
    };
    let snapshot = snapshot_with(&[("BTC", 100, 7)]);
    let Ok(valuation) = value_book(&book, &snapshot, policy()) else {
        unreachable!("valuation must succeed");
    };
    assert_eq!(valuation.total_fair_value, usd(100));
    assert_eq!(valuation.total_unrealized, usd(30));
    let hash = valuation.valuation_hash().map(|h| h.to_hex());
    assert_eq!(
        hash.as_deref(),
        Ok("f410a6b1721cc8fe0ef0e6d5f687f4f12e667ddb6251261005250ee022e0d181")
    );
}
