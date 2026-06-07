//! Domain primitives for the presidio-hardened treasury suite.
//!
//! Design rules enforced here (spec v2 §3.8):
//! - No floating point anywhere in the accounting path: money is integer
//!   base units ([`AssetAmount`]) and serializes as decimal strings.
//! - All arithmetic is checked; overflow is an error, never a wrap.
//! - Identifiers are newtypes — no bare strings cross subsystem boundaries.

#![forbid(unsafe_code)]

pub mod amount;
pub mod hash;
pub mod ids;
pub mod time;

pub use amount::{AmountError, AssetAmount};
pub use hash::ContentHash;
pub use ids::{ActorId, AssetId, SourceId, TenantId, VenueId};
pub use time::TimestampNs;
