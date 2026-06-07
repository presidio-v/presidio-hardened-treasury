//! The ASU 2023-08 roll-forward row: opening → activity → closing,
//! with the equation enforced at construction.

use serde::{Deserialize, Serialize};
use treasury_core::{AssetAmount, AssetId, ContentHash};

/// One asset's period roll-forward. Constructible only through
/// [`RollForwardRow::new`], which enforces the balance equation —
/// a roll-forward that does not roll cannot exist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollForwardRow {
    asset: AssetId,
    opening: AssetAmount,
    additions: AssetAmount,
    disposals: AssetAmount,
    remeasurement: AssetAmount,
    closing: AssetAmount,
    /// Evidence behind the row's numbers (valuation hashes, entry
    /// hashes, disposal results) — flows into the pack manifest.
    evidence: Vec<ContentHash>,
}

impl RollForwardRow {
    /// Construct, enforcing `opening + additions − disposals +
    /// remeasurement == closing` in a single currency, with
    /// non-negative opening/additions/disposals/closing (remeasurement
    /// is signed).
    ///
    /// # Errors
    /// See [`RollError`].
    pub fn new(
        asset: AssetId,
        opening: AssetAmount,
        additions: AssetAmount,
        disposals: AssetAmount,
        remeasurement: AssetAmount,
        closing: AssetAmount,
        mut evidence: Vec<ContentHash>,
    ) -> Result<Self, RollError> {
        let currency = opening.asset().clone();
        let same_currency = additions.asset() == &currency
            && disposals.asset() == &currency
            && remeasurement.asset() == &currency
            && closing.asset() == &currency;
        if !same_currency {
            return Err(RollError::MixedCurrencies);
        }
        if opening.atoms() < 0
            || additions.atoms() < 0
            || disposals.atoms() < 0
            || closing.atoms() < 0
        {
            return Err(RollError::NegativeComponent);
        }
        let rolled = opening
            .atoms()
            .checked_add(additions.atoms())
            .and_then(|v| v.checked_sub(disposals.atoms()))
            .and_then(|v| v.checked_add(remeasurement.atoms()))
            .ok_or(RollError::Overflow)?;
        if rolled != closing.atoms() {
            return Err(RollError::DoesNotRoll {
                rolled,
                closing: closing.atoms(),
            });
        }
        evidence.sort_unstable();
        evidence.dedup();
        Ok(Self {
            asset,
            opening,
            additions,
            disposals,
            remeasurement,
            closing,
            evidence,
        })
    }

    /// The asset.
    #[must_use]
    pub fn asset(&self) -> &AssetId {
        &self.asset
    }

    /// Opening balance.
    #[must_use]
    pub fn opening(&self) -> &AssetAmount {
        &self.opening
    }

    /// Additions (purchases + non-purchase acquisitions).
    #[must_use]
    pub fn additions(&self) -> &AssetAmount {
        &self.additions
    }

    /// Carrying amount derecognized by disposals.
    #[must_use]
    pub fn disposals(&self) -> &AssetAmount {
        &self.disposals
    }

    /// Signed fair-value remeasurement for the period.
    #[must_use]
    pub fn remeasurement(&self) -> &AssetAmount {
        &self.remeasurement
    }

    /// Closing balance.
    #[must_use]
    pub fn closing(&self) -> &AssetAmount {
        &self.closing
    }

    /// Evidence hashes behind the row.
    #[must_use]
    pub fn evidence(&self) -> &[ContentHash] {
        &self.evidence
    }
}

/// Errors constructing a roll-forward row.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RollError {
    /// All components must share one currency.
    #[error("mixed currencies in roll-forward")]
    MixedCurrencies,
    /// Balances and gross activity are non-negative.
    #[error("negative roll-forward component")]
    NegativeComponent,
    /// The equation must hold — structurally.
    #[error("roll-forward does not roll: computed {rolled}, stated closing {closing}")]
    DoesNotRoll {
        /// `opening + additions − disposals + remeasurement`.
        rolled: i128,
        /// The stated closing balance.
        closing: i128,
    },
    /// 128-bit overflow.
    #[error("arithmetic overflow")]
    Overflow,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usd(minor: i128) -> AssetAmount {
        AssetAmount::new(AssetId::new("USD"), minor)
    }

    #[test]
    fn row_that_rolls_constructs() {
        let row = RollForwardRow::new(
            AssetId::new("BTC"),
            usd(1_000),
            usd(500),
            usd(200),
            usd(-100),
            usd(1_200),
            vec![ContentHash([1; 32])],
        );
        assert!(row.is_ok());
    }

    #[test]
    fn row_that_does_not_roll_cannot_exist() {
        let row = RollForwardRow::new(
            AssetId::new("BTC"),
            usd(1_000),
            usd(500),
            usd(200),
            usd(-100),
            usd(1_201),
            vec![],
        );
        assert_eq!(
            row,
            Err(RollError::DoesNotRoll {
                rolled: 1_200,
                closing: 1_201,
            })
        );
    }
}
