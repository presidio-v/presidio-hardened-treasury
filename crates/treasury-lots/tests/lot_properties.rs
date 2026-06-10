//! Property tests (proptest): the lot engine's integer invariants hold for
//! any acquisition and any valid partial-relief quantity — not just the
//! hand-picked examples in the unit tests. The headline invariant is basis
//! conservation: under floor-division pro-rata relief, nothing leaks.

use proptest::prelude::*;
use treasury_core::{AssetAmount, AssetId, ContentHash, TenantId, TimestampNs, VenueId};
use treasury_lots::{LotBook, LotError, ReliefMethod};

fn usd(minor: i128) -> AssetAmount {
    AssetAmount::new(AssetId::new("USD"), minor)
}

fn book_with(qty: i128, basis: i128) -> LotBook {
    let mut book = LotBook::new(TenantId::new("acme"));
    let acquired = book.acquire(
        AssetId::new("BTC"),
        VenueId::new("v"),
        qty,
        usd(basis),
        usd(0),
        TimestampNs::from_nanos(1),
        ContentHash([1; 32]),
    );
    assert!(acquired.is_ok());
    book
}

proptest! {
    /// Partial relief conserves basis exactly: the residual stays in the
    /// lot, so `relieved + remaining == original basis` with no rounding
    /// leak under floor-division pro-rata.
    #[test]
    fn partial_relief_conserves_basis(
        (qty, dispose_qty) in (2_i128..100_000).prop_flat_map(|q| (Just(q), 1_i128..q)),
        basis in 0_i128..1_000_000_000_000,
    ) {
        let mut book = book_with(qty, basis);
        let Ok(disposal) = book.dispose(
            &AssetId::new("BTC"),
            &VenueId::new("v"),
            dispose_qty,
            ReliefMethod::Fifo,
            ContentHash([2; 32]),
            usd(0),
        ) else {
            unreachable!("a partial disposal of less than held must succeed");
        };
        let relieved = disposal.total_basis_relieved.atoms();
        let remaining = book
            .open_lots()
            .first()
            .map_or(0, |lot| lot.cost_basis.atoms());
        prop_assert_eq!(basis, relieved.saturating_add(remaining));
    }

    /// Relieving more than is held fails closed — never a negative lot.
    #[test]
    fn over_relief_is_rejected(qty in 1_i128..100_000, excess in 1_i128..100_000) {
        let mut book = book_with(qty, 1_000);
        let outcome = book.dispose(
            &AssetId::new("BTC"),
            &VenueId::new("v"),
            qty.saturating_add(excess),
            ReliefMethod::Fifo,
            ContentHash([2; 32]),
            usd(0),
        );
        let is_insufficient = matches!(outcome, Err(LotError::InsufficientQuantity { .. }));
        prop_assert!(is_insufficient);
    }

    /// Relieving zero is rejected — a disposal must move something.
    #[test]
    fn zero_relief_is_rejected(qty in 1_i128..100_000) {
        let mut book = book_with(qty, 1_000);
        let outcome = book.dispose(
            &AssetId::new("BTC"),
            &VenueId::new("v"),
            0,
            ReliefMethod::Fifo,
            ContentHash([2; 32]),
            usd(0),
        );
        prop_assert!(matches!(outcome, Err(LotError::NonPositiveQuantity(0))));
    }
}
