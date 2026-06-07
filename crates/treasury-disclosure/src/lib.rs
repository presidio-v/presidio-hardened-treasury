//! The disclosure pack (spec v2 §2.7, REQ-26). *This is the product.*
//!
//! A quarterly close an auditor can sign is, concretely: a roll-forward
//! whose arithmetic cannot be wrong, positions that provably tie to the
//! valuation report, and a manifest enumerating every hash an auditor
//! needs to reproduce every number byte-for-byte.
//!
//! Structural guarantees:
//! - **A roll-forward that does not roll cannot be constructed**:
//!   `opening + additions − disposals + remeasurement == closing` is
//!   enforced at row construction with checked integer arithmetic, in
//!   one currency.
//! - **Tie-out is a function, not a review step**: every valuation
//!   position must have a row whose closing balance equals the
//!   position's fair value, both directions — extra rows and missing
//!   rows are named.
//! - **The pack is content-addressed** and commits to the as-filed
//!   checkpoint, the valuation key hash, the policies in force, and the
//!   anchor receipt: citing a pack hash cites the entire close.
//! - **The manifest is the audit surface**: the sorted, deduplicated
//!   closure of every artifact hash the pack references. Fetch each
//!   from the evidence store, recompute, compare — that is the whole
//!   auditor procedure (AS 1105 posture, R1).

#![forbid(unsafe_code)]

pub mod pack;
pub mod rollforward;

pub use pack::{DisclosurePack, PackError, TieBreak};
pub use rollforward::{RollForwardRow, RollError};
