//! The lot book: acquisitions open lots, disposals relieve them under an
//! elected order, transfers move quantity without realizing.

use crate::lot::{lot_id, Lot, LotId, LOT_SCHEMA};
use serde_json::json;
use treasury_core::{
    AmountError, AssetAmount, AssetId, ContentHash, TenantId, TimestampNs, VenueId,
};
use treasury_evidence::{canonical_bytes, sha256, CanonError};

/// Schema tag committed into the open-lot-set hash; bump on change.
pub const LOTS_STATE_SCHEMA: &str = "treasury-lots/lots-state/v1";

/// How a disposal relieves lots. The *election* of a method is a policy
/// artifact (REQ-9); the method and the electing policy hash are both
/// recorded in the [`DisposalResult`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReliefMethod {
    /// First-in, first-out by (acquired-at, lot-id).
    Fifo,
    /// Specific identification: relieve exactly these lots, in order.
    SpecificLots(Vec<LotId>),
}

/// One lot's contribution to a disposal or transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LotRelief {
    /// The lot relieved.
    pub lot_id: LotId,
    /// Atoms taken from it.
    pub atoms: i128,
    /// Basis relieved (pro-rata, floor; residual stays in the lot).
    pub basis_relieved: AssetAmount,
    /// Fee relieved (same pro-rata rule, kept decomposed — G-3).
    pub fee_relieved: AssetAmount,
}

/// The outcome of a disposal — the audit record of basis relief.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisposalResult {
    /// Elected relief method.
    pub method: ReliefMethod,
    /// Policy artifact electing the method.
    pub policy_hash: ContentHash,
    /// Per-lot reliefs in application order.
    pub reliefs: Vec<LotRelief>,
    /// Sum of relieved basis.
    pub total_basis_relieved: AssetAmount,
    /// Disposal proceeds (reporting currency).
    pub proceeds: AssetAmount,
    /// `proceeds − total_basis_relieved`. Fee treatment is applied at
    /// the policy layer from the decomposed fee reliefs.
    pub realized: AssetAmount,
}

/// Per-tenant lot book. Holds open lots only; a fully relieved lot
/// leaves the book (its history lives in the ledger events that
/// relieved it).
#[derive(Debug)]
pub struct LotBook {
    tenant: TenantId,
    lots: Vec<Lot>,
}

impl LotBook {
    /// Create an empty book for a tenant.
    #[must_use]
    pub fn new(tenant: TenantId) -> Self {
        Self {
            tenant,
            lots: Vec::new(),
        }
    }

    /// Open a lot from an acquisition.
    ///
    /// # Errors
    /// [`LotError::NonPositiveQuantity`] for zero/negative atoms;
    /// [`LotError::NegativeMoney`] for negative basis or fee;
    /// [`LotError::CurrencyMismatch`] when fee and basis currencies
    /// differ; [`LotError::Canon`] on envelope failure.
    #[allow(clippy::too_many_arguments)]
    pub fn acquire(
        &mut self,
        asset: AssetId,
        venue: VenueId,
        atoms: i128,
        cost_basis: AssetAmount,
        acquisition_fee: AssetAmount,
        acquired_at: TimestampNs,
        source_event: ContentHash,
    ) -> Result<LotId, LotError> {
        if atoms <= 0 {
            return Err(LotError::NonPositiveQuantity(atoms));
        }
        if cost_basis.atoms() < 0 || acquisition_fee.atoms() < 0 {
            return Err(LotError::NegativeMoney);
        }
        if cost_basis.asset() != acquisition_fee.asset() {
            return Err(LotError::CurrencyMismatch);
        }
        let id = lot_id(
            &self.tenant,
            &asset,
            &venue,
            atoms,
            &cost_basis,
            &acquisition_fee,
            acquired_at,
            source_event,
            None,
        )?;
        self.lots.push(Lot {
            lot_id: id,
            tenant: self.tenant.clone(),
            asset,
            venue,
            atoms,
            cost_basis,
            acquisition_fee,
            acquired_at,
            source_event,
            moved_from: None,
        });
        Ok(id)
    }

    /// Total atoms held of an asset at a venue.
    #[must_use]
    pub fn held(&self, asset: &AssetId, venue: &VenueId) -> i128 {
        let mut total: i128 = 0;
        for lot in &self.lots {
            if lot.asset == *asset && lot.venue == *venue {
                total = total.saturating_add(lot.atoms);
            }
        }
        total
    }

    /// Open lots (current state), in (acquired-at, lot-id) order.
    #[must_use]
    pub fn open_lots(&self) -> Vec<&Lot> {
        let mut lots: Vec<&Lot> = self.lots.iter().collect();
        lots.sort_by_key(|l| (l.acquired_at, l.lot_id));
        lots
    }

    /// Commitment to the current open lot set — the `lots` input of the
    /// `(lots, price-snapshot, policy)` valuation key (spec v2 §3.5).
    ///
    /// # Errors
    /// [`LotError::Canon`] on envelope failure (structurally
    /// unreachable).
    pub fn lots_hash(&self) -> Result<ContentHash, LotError> {
        let mut snapshot: Vec<Lot> = self.lots.clone();
        snapshot.sort_by_key(|l| l.lot_id);
        let envelope = json!({
            "schema": LOTS_STATE_SCHEMA,
            "lot_schema": LOT_SCHEMA,
            "tenant": self.tenant.clone(),
            "lots": snapshot,
        });
        let bytes = canonical_bytes(&envelope)?;
        Ok(sha256(&bytes))
    }

    /// Dispose of quantity at a venue under an elected relief order.
    /// Validates sufficiency before mutating anything (a failed disposal
    /// leaves the book byte-identical).
    ///
    /// # Errors
    /// [`LotError::NonPositiveQuantity`]; [`LotError::InsufficientQuantity`]
    /// (fail closed — never a negative lot); [`LotError::UnknownLot`] /
    /// [`LotError::WrongLotTarget`] for bad specific identification;
    /// [`LotError::CurrencyMismatch`] when proceeds currency differs from
    /// basis currency; [`LotError::Overflow`] on arithmetic overflow.
    #[allow(clippy::too_many_arguments)]
    pub fn dispose(
        &mut self,
        asset: &AssetId,
        venue: &VenueId,
        atoms: i128,
        method: ReliefMethod,
        policy_hash: ContentHash,
        proceeds: AssetAmount,
    ) -> Result<DisposalResult, LotError> {
        let plan = self.plan_relief(asset, venue, atoms, &method)?;
        let currency = self.basis_currency(&plan)?;
        if proceeds.asset() != &currency {
            return Err(LotError::CurrencyMismatch);
        }

        let reliefs = self.apply_relief(&plan)?;
        let mut total_basis = AssetAmount::new(currency.clone(), 0);
        for relief in &reliefs {
            total_basis = total_basis.checked_add(&relief.basis_relieved)?;
        }
        let realized = proceeds.checked_sub(&total_basis)?;
        Ok(DisposalResult {
            method,
            policy_hash,
            reliefs,
            total_basis_relieved: total_basis,
            proceeds,
            realized,
        })
    }

    /// Move quantity between venues **without realizing**: pro-rata basis
    /// and fee move with it, and the original acquisition timestamp is
    /// preserved. New lots carry lineage (`moved_from`).
    ///
    /// # Errors
    /// Same sufficiency/quantity errors as [`LotBook::dispose`].
    pub fn transfer(
        &mut self,
        asset: &AssetId,
        from_venue: &VenueId,
        to_venue: &VenueId,
        atoms: i128,
        transfer_event: ContentHash,
    ) -> Result<Vec<LotId>, LotError> {
        let plan = self.plan_relief(asset, from_venue, atoms, &ReliefMethod::Fifo)?;
        // Capture origin timestamps before mutating.
        let mut origins: Vec<(LotId, TimestampNs)> = Vec::new();
        for (id, _) in &plan {
            let Some(lot) = self.lots.iter().find(|l| l.lot_id == *id) else {
                return Err(LotError::UnknownLot(*id));
            };
            origins.push((*id, lot.acquired_at));
        }
        let reliefs = self.apply_relief(&plan)?;

        let mut new_ids = Vec::new();
        for relief in &reliefs {
            let Some((_, acquired_at)) = origins.iter().find(|(id, _)| *id == relief.lot_id)
            else {
                return Err(LotError::UnknownLot(relief.lot_id));
            };
            let id = lot_id(
                &self.tenant,
                asset,
                to_venue,
                relief.atoms,
                &relief.basis_relieved,
                &relief.fee_relieved,
                *acquired_at,
                transfer_event,
                Some(relief.lot_id),
            )?;
            self.lots.push(Lot {
                lot_id: id,
                tenant: self.tenant.clone(),
                asset: asset.clone(),
                venue: to_venue.clone(),
                atoms: relief.atoms,
                cost_basis: relief.basis_relieved.clone(),
                acquisition_fee: relief.fee_relieved.clone(),
                acquired_at: *acquired_at,
                source_event: transfer_event,
                moved_from: Some(relief.lot_id),
            });
            new_ids.push(id);
        }
        Ok(new_ids)
    }

    /// Build the relief plan `(lot_id, atoms_to_take)` without mutating.
    fn plan_relief(
        &self,
        asset: &AssetId,
        venue: &VenueId,
        atoms: i128,
        method: &ReliefMethod,
    ) -> Result<Vec<(LotId, i128)>, LotError> {
        if atoms <= 0 {
            return Err(LotError::NonPositiveQuantity(atoms));
        }
        let ordered: Vec<&Lot> = match method {
            ReliefMethod::Fifo => {
                let mut candidates: Vec<&Lot> = self
                    .lots
                    .iter()
                    .filter(|l| l.asset == *asset && l.venue == *venue)
                    .collect();
                candidates.sort_by_key(|l| (l.acquired_at, l.lot_id));
                candidates
            }
            ReliefMethod::SpecificLots(ids) => {
                let mut candidates: Vec<&Lot> = Vec::new();
                for id in ids {
                    let Some(lot) = self.lots.iter().find(|l| l.lot_id == *id) else {
                        return Err(LotError::UnknownLot(*id));
                    };
                    if lot.asset != *asset || lot.venue != *venue {
                        return Err(LotError::WrongLotTarget(*id));
                    }
                    candidates.push(lot);
                }
                candidates
            }
        };

        let mut available: i128 = 0;
        for lot in &ordered {
            available = available.checked_add(lot.atoms).ok_or(LotError::Overflow)?;
        }
        if available < atoms {
            return Err(LotError::InsufficientQuantity {
                requested: atoms,
                available,
            });
        }

        let mut plan = Vec::new();
        let mut needed = atoms;
        for lot in &ordered {
            if needed == 0 {
                break;
            }
            let take = needed.min(lot.atoms);
            plan.push((lot.lot_id, take));
            needed = needed.checked_sub(take).ok_or(LotError::Overflow)?;
        }
        Ok(plan)
    }

    /// The (single) basis currency across the planned lots.
    fn basis_currency(&self, plan: &[(LotId, i128)]) -> Result<AssetId, LotError> {
        let Some((first_id, _)) = plan.first() else {
            return Err(LotError::NonPositiveQuantity(0));
        };
        let Some(first) = self.lots.iter().find(|l| l.lot_id == *first_id) else {
            return Err(LotError::UnknownLot(*first_id));
        };
        let currency = first.cost_basis.asset().clone();
        for (id, _) in plan {
            let Some(lot) = self.lots.iter().find(|l| l.lot_id == *id) else {
                return Err(LotError::UnknownLot(*id));
            };
            if lot.cost_basis.asset() != &currency {
                return Err(LotError::CurrencyMismatch);
            }
        }
        Ok(currency)
    }

    /// Apply a validated plan: mutate residuals, drop emptied lots,
    /// return per-lot reliefs with exact conservation (floor + residual).
    fn apply_relief(&mut self, plan: &[(LotId, i128)]) -> Result<Vec<LotRelief>, LotError> {
        let mut reliefs = Vec::new();
        for (id, take) in plan {
            let Some(lot) = self.lots.iter_mut().find(|l| l.lot_id == *id) else {
                return Err(LotError::UnknownLot(*id));
            };
            let basis_part = pro_rata(lot.cost_basis.atoms(), *take, lot.atoms)?;
            let fee_part = pro_rata(lot.acquisition_fee.atoms(), *take, lot.atoms)?;
            let currency = lot.cost_basis.asset().clone();

            lot.atoms = lot.atoms.checked_sub(*take).ok_or(LotError::Overflow)?;
            let basis_left = lot
                .cost_basis
                .atoms()
                .checked_sub(basis_part)
                .ok_or(LotError::Overflow)?;
            let fee_left = lot
                .acquisition_fee
                .atoms()
                .checked_sub(fee_part)
                .ok_or(LotError::Overflow)?;
            lot.cost_basis = AssetAmount::new(currency.clone(), basis_left);
            lot.acquisition_fee = AssetAmount::new(currency.clone(), fee_left);

            reliefs.push(LotRelief {
                lot_id: *id,
                atoms: *take,
                basis_relieved: AssetAmount::new(currency.clone(), basis_part),
                fee_relieved: AssetAmount::new(currency, fee_part),
            });
        }
        self.lots.retain(|l| l.atoms > 0);
        Ok(reliefs)
    }
}

/// `floor(total * part / whole)`; callers guarantee `whole > 0` and
/// `0 < part <= whole`, so the result is exact-conserving with the
/// residual kept in the lot.
fn pro_rata(total: i128, part: i128, whole: i128) -> Result<i128, LotError> {
    let scaled = total.checked_mul(part).ok_or(LotError::Overflow)?;
    scaled.checked_div(whole).ok_or(LotError::Overflow)
}

/// Errors from lot book operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LotError {
    /// Quantities must be strictly positive.
    #[error("non-positive quantity: {0}")]
    NonPositiveQuantity(i128),
    /// Basis and fees are non-negative money.
    #[error("negative basis or fee")]
    NegativeMoney,
    /// Mixed currencies (basis vs fee vs proceeds).
    #[error("currency mismatch")]
    CurrencyMismatch,
    /// Fail closed: never a negative lot.
    #[error("insufficient quantity: requested {requested}, available {available}")]
    InsufficientQuantity {
        /// Atoms requested.
        requested: i128,
        /// Atoms actually available.
        available: i128,
    },
    /// Specific identification named a lot that does not exist.
    #[error("unknown lot: {0}")]
    UnknownLot(LotId),
    /// Specific identification named a lot of another asset or venue.
    #[error("lot belongs to another asset or venue: {0}")]
    WrongLotTarget(LotId),
    /// 128-bit arithmetic overflow.
    #[error("arithmetic overflow")]
    Overflow,
    /// Money arithmetic failure.
    #[error(transparent)]
    Amount(#[from] AmountError),
    /// Envelope canonicalization failure.
    #[error(transparent)]
    Canon(#[from] CanonError),
}
