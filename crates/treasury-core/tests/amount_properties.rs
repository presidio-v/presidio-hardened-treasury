//! Property tests (proptest): integer-money invariants hold for arbitrary
//! amounts — checked arithmetic never panics (it returns `Ok` or `Err`,
//! never an overflow panic) and matches `i128` checked semantics, and the
//! canonical wire form round-trips.

use proptest::prelude::*;
use treasury_core::{AmountError, AssetAmount, AssetId};

fn small_string() -> impl Strategy<Value = String> {
    prop::collection::vec(any::<char>(), 0..8).prop_map(|cs| cs.into_iter().collect())
}

proptest! {
    /// Checked addition never panics and matches `i128` checked semantics:
    /// `Ok(x + y)` when it fits, `Err(Overflow)` when it does not.
    #[test]
    fn checked_add_matches_i128_and_never_panics(x in any::<i128>(), y in any::<i128>()) {
        let asset = AssetId::new("BTC");
        let a = AssetAmount::new(asset.clone(), x);
        let b = AssetAmount::new(asset, y);
        match (a.checked_add(&b), x.checked_add(y)) {
            (Ok(sum), Some(expected)) => prop_assert_eq!(sum.atoms(), expected),
            (Err(AmountError::Overflow), None) => {}
            (got, exp) => prop_assert!(false, "add diverged from i128: {:?} vs {:?}", got, exp),
        }
    }

    /// Checked subtraction never panics and matches `i128` checked semantics.
    #[test]
    fn checked_sub_matches_i128_and_never_panics(x in any::<i128>(), y in any::<i128>()) {
        let asset = AssetId::new("BTC");
        let a = AssetAmount::new(asset.clone(), x);
        let b = AssetAmount::new(asset, y);
        match (a.checked_sub(&b), x.checked_sub(y)) {
            (Ok(diff), Some(expected)) => prop_assert_eq!(diff.atoms(), expected),
            (Err(AmountError::Overflow), None) => {}
            (got, exp) => prop_assert!(false, "sub diverged from i128: {:?} vs {:?}", got, exp),
        }
    }

    /// Checked negation never panics and matches `i128` checked semantics
    /// (`i128::MIN` has no positive counterpart).
    #[test]
    fn checked_neg_matches_i128_and_never_panics(x in any::<i128>()) {
        let a = AssetAmount::new(AssetId::new("BTC"), x);
        match (a.checked_neg(), x.checked_neg()) {
            (Ok(neg), Some(expected)) => prop_assert_eq!(neg.atoms(), expected),
            (Err(AmountError::Overflow), None) => {}
            (got, exp) => prop_assert!(false, "neg diverged from i128: {:?} vs {:?}", got, exp),
        }
    }

    /// Mismatched assets are always rejected, never silently combined.
    #[test]
    fn cross_asset_arithmetic_is_rejected(x in any::<i128>(), y in any::<i128>()) {
        let a = AssetAmount::new(AssetId::new("BTC"), x);
        let b = AssetAmount::new(AssetId::new("ETH"), y);
        let add_mismatch = matches!(a.checked_add(&b), Err(AmountError::AssetMismatch { .. }));
        let sub_mismatch = matches!(a.checked_sub(&b), Err(AmountError::AssetMismatch { .. }));
        prop_assert!(add_mismatch);
        prop_assert!(sub_mismatch);
    }

    /// The canonical wire form round-trips: `deserialize(serialize(x)) == x`.
    #[test]
    fn amount_round_trips_through_canonical_json(asset in small_string(), atoms in any::<i128>()) {
        let amount = AssetAmount::new(AssetId::new(asset), atoms);
        let Ok(json) = serde_json::to_string(&amount) else {
            unreachable!("AssetAmount always serializes");
        };
        let Ok(back) = serde_json::from_str::<AssetAmount>(&json) else {
            unreachable!("its own canonical output always deserializes");
        };
        prop_assert_eq!(amount, back);
    }
}
