//! Lot / cost-basis engine (spec v2 §2.4, REQ-23).
//!
//! Per-lot acquisition tracking — needed for tax, comparatives, and the
//! roll-forward even under fair value. Structural guarantees:
//!
//! - **Fees are decomposed from basis** (remediation G-3): ASU 2023-08
//!   is deliberately silent on acquisition costs, so capitalize-vs-
//!   expense is a per-tenant policy election applied at the policy layer
//!   (L4). The lot stores cost basis and acquisition fee separately so
//!   *either* election replays from the same lots.
//! - **Integer arithmetic only**, checked everywhere. Partial relief
//!   allocates basis pro-rata with floor division and keeps the
//!   remainder in the residual lot — conservation is exact: relieved
//!   basis plus residual basis always equals the original, to the minor
//!   unit. A property test asserts it.
//! - **Relief order is an election, not a default**: every disposal
//!   names its [`ReliefMethod`] and the content hash of the policy
//!   artifact electing it (REQ-9); both are recorded in the result.
//! - **Disposing more than held is a typed error** (fail closed), never
//!   a negative lot.
//! - **Internal transfers never realize**: quantity moves between venues
//!   preserving pro-rata basis, fee, and — critically — the original
//!   acquisition timestamp, with lineage back to the source lot.
//! - **`lots_hash()`** commits to the open lot set — the `lots` input of
//!   the `(lots, price-snapshot, policy)` valuation key (spec v2 §3.5).

#![forbid(unsafe_code)]

pub mod book;
pub mod lot;

pub use book::{DisposalResult, LotBook, LotError, LotRelief, ReliefMethod};
pub use lot::{Lot, LotId};
