//! The pack: roll-forward + tie-out + manifest, content-addressed.

use crate::rollforward::RollForwardRow;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use treasury_core::{AssetId, ContentHash, TenantId};
use treasury_evidence::{canonical_bytes, sha256, CanonError};
use treasury_fairvalue::Valuation;

/// Schema tag committed into every pack hash; bump on change.
pub const PACK_SCHEMA: &str = "treasury-disclosure/pack/v1";

/// A named tie-out failure between roll-forward and valuation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TieBreak {
    /// A valuation position has no roll-forward row.
    MissingRow {
        /// The asset without a row.
        asset: AssetId,
    },
    /// A roll-forward row has no valuation position.
    ExtraRow {
        /// The asset without a position.
        asset: AssetId,
    },
    /// Closing balance differs from the position's fair value.
    ClosingMismatch {
        /// The asset.
        asset: AssetId,
        /// Roll-forward closing (minor units).
        closing: i128,
        /// Valuation fair value (minor units).
        fair_value: i128,
    },
}

/// The quarterly disclosure pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisclosurePack {
    /// Tenant whose close this is.
    pub tenant: TenantId,
    /// Period tag (e.g. `"2026Q2"`).
    pub period: String,
    /// The as-filed checkpoint this pack discloses (treasury-close).
    pub checkpoint: ContentHash,
    /// The valuation report's hash (treasury-fairvalue).
    pub valuation: ContentHash,
    /// Policies in force, as (kind, artifact hash) pairs in kind order.
    pub policies: Vec<(String, ContentHash)>,
    /// Anchor receipt covering the period's evidence (treasury-anchor),
    /// when anchoring has run.
    pub anchor_receipt: Option<ContentHash>,
    /// Per-asset roll-forward rows.
    pub rows: Vec<RollForwardRow>,
}

impl DisclosurePack {
    /// Tie the roll-forward to the valuation, both directions. Empty
    /// result means tied; failures name the asset and the numbers.
    #[must_use]
    pub fn tie_to_valuation(&self, valuation: &Valuation) -> Vec<TieBreak> {
        let mut breaks = Vec::new();
        let row_assets: BTreeSet<&AssetId> = self.rows.iter().map(RollForwardRow::asset).collect();
        let position_assets: BTreeSet<&AssetId> =
            valuation.positions.iter().map(|p| &p.asset).collect();

        for position in &valuation.positions {
            let Some(row) = self.rows.iter().find(|r| r.asset() == &position.asset) else {
                breaks.push(TieBreak::MissingRow {
                    asset: position.asset.clone(),
                });
                continue;
            };
            if row.closing() != &position.fair_value {
                breaks.push(TieBreak::ClosingMismatch {
                    asset: position.asset.clone(),
                    closing: row.closing().atoms(),
                    fair_value: position.fair_value.atoms(),
                });
            }
        }
        for asset in row_assets {
            if !position_assets.contains(asset) {
                breaks.push(TieBreak::ExtraRow {
                    asset: asset.clone(),
                });
            }
        }
        breaks
    }

    /// The evidence-reproduction manifest: the sorted, deduplicated
    /// closure of every hash this pack references. Fetch each from the
    /// evidence store, recompute, compare — the whole auditor procedure.
    #[must_use]
    pub fn manifest(&self) -> Vec<ContentHash> {
        let mut hashes: Vec<ContentHash> = Vec::new();
        hashes.push(self.checkpoint);
        hashes.push(self.valuation);
        for (_, policy) in &self.policies {
            hashes.push(*policy);
        }
        if let Some(receipt) = self.anchor_receipt {
            hashes.push(receipt);
        }
        for row in &self.rows {
            hashes.extend_from_slice(row.evidence());
        }
        hashes.sort_unstable();
        hashes.dedup();
        hashes
    }

    /// The pack's content hash — citing it cites the entire close.
    ///
    /// # Errors
    /// [`PackError::Canon`] on envelope failure (structurally
    /// unreachable: rows serialize float-free by construction).
    pub fn pack_hash(&self) -> Result<ContentHash, PackError> {
        let mut policies: Vec<Value> = Vec::new();
        for (kind, hash) in &self.policies {
            policies.push(json!({
                "kind": kind.clone(),
                "policy": hash.to_hex(),
            }));
        }
        let envelope = json!({
            "schema": PACK_SCHEMA,
            "tenant": self.tenant.clone(),
            "period": self.period.clone(),
            "checkpoint": self.checkpoint.to_hex(),
            "valuation": self.valuation.to_hex(),
            "policies": policies,
            "anchor_receipt": self.anchor_receipt.map(|r| r.to_hex()),
            "rows": self.rows.clone(),
        });
        let bytes = canonical_bytes(&envelope)?;
        Ok(sha256(&bytes))
    }
}

/// Errors from pack operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PackError {
    /// Envelope canonicalization failure.
    #[error(transparent)]
    Canon(#[from] CanonError),
}
