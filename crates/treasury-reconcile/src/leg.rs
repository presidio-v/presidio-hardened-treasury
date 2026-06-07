//! Transfer legs: the matcher's input shape.

use serde::{Deserialize, Serialize};
use treasury_core::{AssetAmount, ContentHash, TenantId, TimestampNs, VenueId};

/// Identity of a leg — the ledger event id (L1/L2) it was derived from.
pub type LegId = ContentHash;

/// Direction of a movement relative to the venue it was observed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Assets left the venue.
    Outflow,
    /// Assets arrived at the venue.
    Inflow,
}

/// One observed movement, normalized for matching. Serializable so that
/// labeled corpora (SLO harness) are content-addressable audit artifacts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferLeg {
    /// Ledger event this leg derives from.
    pub leg_id: LegId,
    /// Tenant whose books this belongs to.
    pub tenant: TenantId,
    /// Venue the movement was observed at.
    pub venue: VenueId,
    /// Outflow or inflow at that venue.
    pub direction: Direction,
    /// Gross amount moved (always positive atoms).
    pub amount: AssetAmount,
    /// Fee taken mid-flight, in the same asset, when known. `None` means
    /// the venue reported no fee — treated as zero for tier-1 arithmetic.
    pub fee: Option<AssetAmount>,
    /// On-chain transaction hash, when the venue exposes it.
    pub tx_hash: Option<String>,
    /// For outflows: the destination address. For inflows: the receiving
    /// address. Tier-1 corroboration requires both present and equal.
    pub address: Option<String>,
    /// When the movement happened (event time).
    pub event_time: TimestampNs,
}
