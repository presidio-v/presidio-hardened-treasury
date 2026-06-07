//! The valuation itself: positions, totals, and the content-addressed
//! report keyed by `(lots, price-snapshot, policy)`.

use crate::snapshot::PriceSnapshot;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use treasury_core::{AmountError, AssetAmount, AssetId, ContentHash, TimestampNs};
use treasury_evidence::{canonical_bytes, sha256, CanonError};
use treasury_lots::{LotBook, LotError};
use treasury_policy::ValuationKey;

/// Schema tag committed into every valuation hash; bump on change.
pub const VALUATION_SCHEMA: &str = "treasury-fairvalue/valuation/v1";

/// One asset's valued position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    /// The asset.
    pub asset: AssetId,
    /// Total atoms held (all venues).
    pub atoms: i128,
    /// Sum of unrelieved cost basis across open lots.
    pub cost_basis: AssetAmount,
    /// Fair value at the snapshot prices (floor division).
    pub fair_value: AssetAmount,
    /// `fair_value − cost_basis` — the mark the GAAP module routes to
    /// net income (spec v2 §4).
    pub unrealized: AssetAmount,
}

/// A content-addressed valuation report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Valuation {
    /// The R3 key: `(lots, price-snapshot, policy)`.
    pub key: ValuationKey,
    /// `key.key_hash()` — the memoization index a disclosure cites.
    pub key_hash: ContentHash,
    /// The reporting instant (from the snapshot).
    pub as_of: TimestampNs,
    /// Per-asset positions in deterministic (asset) order.
    pub positions: Vec<Position>,
    /// Sum of fair values.
    pub total_fair_value: AssetAmount,
    /// Sum of unrealized marks.
    pub total_unrealized: AssetAmount,
}

impl Valuation {
    /// The report's content hash.
    ///
    /// # Errors
    /// [`FvError::Canon`] on envelope failure (structurally unreachable).
    pub fn valuation_hash(&self) -> Result<ContentHash, FvError> {
        let mut positions: Vec<Value> = Vec::new();
        for p in &self.positions {
            positions.push(json!({
                "asset": p.asset.clone(),
                "atoms": p.atoms.to_string(),
                "cost_basis": p.cost_basis.clone(),
                "fair_value": p.fair_value.clone(),
                "unrealized": p.unrealized.clone(),
            }));
        }
        let envelope = json!({
            "schema": VALUATION_SCHEMA,
            "key": self.key_hash.to_hex(),
            "as_of": self.as_of,
            "positions": positions,
            "total_fair_value": self.total_fair_value.clone(),
            "total_unrealized": self.total_unrealized.clone(),
        });
        let bytes = canonical_bytes(&envelope)?;
        Ok(sha256(&bytes))
    }
}

/// Value a lot book at a snapshot under a governing policy.
///
/// Pure: the result (and its hash) is fully determined by the book's
/// open lots, the snapshot, and the policy hash.
///
/// # Errors
/// [`FvError::MissingPrice`] for a held asset absent from the snapshot
/// (fail closed); [`FvError::CurrencyMismatch`] when basis currency
/// differs from snapshot currency; [`FvError::BadPrice`] for
/// non-positive `atoms_per_unit`; [`FvError::Overflow`] on 128-bit
/// overflow; propagated lot/canon/money errors.
pub fn value_book(
    book: &LotBook,
    snapshot: &PriceSnapshot,
    policy_hash: ContentHash,
) -> Result<Valuation, FvError> {
    let lots_hash = book.lots_hash()?;
    let snapshot_hash = snapshot.snapshot_hash()?;
    let key = ValuationKey {
        lots_hash,
        price_snapshot_hash: snapshot_hash,
        policy_hash,
    };
    let key_hash = key.key_hash()?;
    let currency = snapshot.currency.clone();

    // Aggregate open lots per asset, deterministically.
    let mut by_asset: BTreeMap<AssetId, (i128, AssetAmount)> = BTreeMap::new();
    for lot in book.open_lots() {
        if lot.cost_basis.asset() != &currency {
            return Err(FvError::CurrencyMismatch);
        }
        match by_asset.get(&lot.asset) {
            None => {
                by_asset.insert(lot.asset.clone(), (lot.atoms, lot.cost_basis.clone()));
            }
            Some((atoms, basis)) => {
                let new_atoms = atoms.checked_add(lot.atoms).ok_or(FvError::Overflow)?;
                let new_basis = basis.checked_add(&lot.cost_basis)?;
                by_asset.insert(lot.asset.clone(), (new_atoms, new_basis));
            }
        }
    }

    let mut positions: Vec<Position> = Vec::new();
    let mut total_fair_value = AssetAmount::new(currency.clone(), 0);
    let mut total_unrealized = AssetAmount::new(currency.clone(), 0);
    for (asset, (atoms, cost_basis)) in &by_asset {
        let Some(price) = snapshot.prices.get(asset) else {
            return Err(FvError::MissingPrice(asset.clone()));
        };
        if price.atoms_per_unit <= 0 {
            return Err(FvError::BadPrice(asset.clone()));
        }
        let scaled = atoms
            .checked_mul(price.minor_per_unit)
            .ok_or(FvError::Overflow)?;
        let value_minor = scaled
            .checked_div(price.atoms_per_unit)
            .ok_or(FvError::Overflow)?;
        let fair_value = AssetAmount::new(currency.clone(), value_minor);
        let unrealized = fair_value.checked_sub(cost_basis)?;

        total_fair_value = total_fair_value.checked_add(&fair_value)?;
        total_unrealized = total_unrealized.checked_add(&unrealized)?;
        positions.push(Position {
            asset: asset.clone(),
            atoms: *atoms,
            cost_basis: cost_basis.clone(),
            fair_value,
            unrealized,
        });
    }

    Ok(Valuation {
        key,
        key_hash,
        as_of: snapshot.as_of,
        positions,
        total_fair_value,
        total_unrealized,
    })
}

/// Errors from valuation.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FvError {
    /// A held asset has no price in the snapshot — fail closed; the
    /// principal-market policy's fallback hierarchy must resolve prices
    /// *before* the snapshot is sealed.
    #[error("no price for held asset: {0}")]
    MissingPrice(AssetId),
    /// `atoms_per_unit` must be positive.
    #[error("bad price scale for asset: {0}")]
    BadPrice(AssetId),
    /// Basis currency differs from snapshot currency.
    #[error("currency mismatch between lots and snapshot")]
    CurrencyMismatch,
    /// 128-bit arithmetic overflow.
    #[error("arithmetic overflow")]
    Overflow,
    /// Lot book failure.
    #[error(transparent)]
    Lots(#[from] LotError),
    /// Money arithmetic failure.
    #[error(transparent)]
    Amount(#[from] AmountError),
    /// Envelope canonicalization failure.
    #[error(transparent)]
    Canon(#[from] CanonError),
}
