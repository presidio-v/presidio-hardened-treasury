//! The lot: one acquisition's remaining quantity and unreleived basis.

use serde::{Deserialize, Serialize};
use serde_json::json;
use treasury_core::{AssetAmount, AssetId, ContentHash, TenantId, TimestampNs, VenueId};
use treasury_evidence::{canonical_bytes, sha256, CanonError};

/// Schema tag committed into every lot hash; bump on change.
pub const LOT_SCHEMA: &str = "treasury-lots/lot/v1";

/// Identity of a lot: SHA-256 of its canonical opening envelope.
pub type LotId = ContentHash;

/// One open lot. Quantities are asset atoms; basis and fee are integer
/// minor units of the tenant's reporting currency, carried as
/// [`AssetAmount`] tagged with the currency code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lot {
    /// Content-derived identity (from the opening envelope).
    pub lot_id: LotId,
    /// Tenant whose books this lot belongs to.
    pub tenant: TenantId,
    /// Asset held.
    pub asset: AssetId,
    /// Venue currently holding the quantity.
    pub venue: VenueId,
    /// Remaining quantity in atoms (always positive). Serializes as a
    /// decimal string — JSON numbers are not trusted with 128 bits.
    #[serde(with = "atoms_as_string")]
    pub atoms: i128,
    /// Unrelieved cost basis (reporting-currency minor units). Excludes
    /// the acquisition fee — see `acquisition_fee`.
    pub cost_basis: AssetAmount,
    /// Acquisition fee, decomposed from basis (G-3): whether it
    /// capitalizes or expenses is an L4 policy election.
    pub acquisition_fee: AssetAmount,
    /// When the quantity was originally acquired. Preserved across
    /// internal transfers.
    pub acquired_at: TimestampNs,
    /// Ledger event the acquisition derives from.
    pub source_event: ContentHash,
    /// For transfer-created lots: the lot the quantity moved from.
    pub moved_from: Option<LotId>,
}

mod atoms_as_string {
    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &i128, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&value.to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i128, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse::<i128>()
            .map_err(|_| D::Error::custom("atoms must be a decimal string"))
    }
}

/// Compute a lot id from its opening facts.
///
/// # Errors
/// [`CanonError`] is structurally unreachable for this envelope but
/// propagated rather than swallowed.
#[allow(clippy::too_many_arguments)]
pub fn lot_id(
    tenant: &TenantId,
    asset: &AssetId,
    venue: &VenueId,
    atoms: i128,
    cost_basis: &AssetAmount,
    acquisition_fee: &AssetAmount,
    acquired_at: TimestampNs,
    source_event: ContentHash,
    moved_from: Option<LotId>,
) -> Result<LotId, CanonError> {
    let envelope = json!({
        "schema": LOT_SCHEMA,
        "tenant": tenant.clone(),
        "asset": asset.clone(),
        "venue": venue.clone(),
        "atoms": atoms.to_string(),
        "cost_basis": cost_basis.clone(),
        "acquisition_fee": acquisition_fee.clone(),
        "acquired_at": acquired_at,
        "source_event": source_event.to_hex(),
        "moved_from": moved_from.map(|m| m.to_hex()),
    });
    let bytes = canonical_bytes(&envelope)?;
    Ok(sha256(&bytes))
}
