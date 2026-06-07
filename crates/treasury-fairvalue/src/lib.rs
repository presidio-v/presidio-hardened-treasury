//! Fair-value engine (spec v2 §2.5 + §3.5, REQ-24).
//!
//! Valuation is a **pure function of `(lots, price-snapshot, policy)`**
//! — the remediation-R3 key, realized: this crate consumes the lot
//! book's `lots_hash()`, a content-addressed [`PriceSnapshot`], and the
//! governing principal-market policy hash, and emits a content-addressed
//! [`Valuation`] whose key is exactly that triple. Same inputs, same
//! report, same hash — on any machine, forever.
//!
//! Discipline:
//! - **Integer-exact.** Prices are integer minor units per integer atoms
//!   per unit; position values use floor division with checked 128-bit
//!   arithmetic. No floats exist here, including in the prices.
//! - **Fail closed.** A held asset without a price in the snapshot is a
//!   typed error — stale/fallback handling is the *policy's* job before
//!   the snapshot is sealed (spec v2 §3.5), never an engine default.
//! - **Currency-strict.** Snapshot currency must match basis currency or
//!   the valuation refuses; unrealized = fair value − basis is computed
//!   with checked money arithmetic.

#![forbid(unsafe_code)]

pub mod snapshot;
pub mod valuation;

pub use snapshot::{Price, PriceSnapshot};
pub use valuation::{value_book, FvError, Position, Valuation};
