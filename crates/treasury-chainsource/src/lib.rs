//! Per-chain node + indexer abstraction (ADR-0004).
//!
//! ADR-0004 selects the concrete clients per chain (Bitcoin Core +
//! electrs / Fulcrum; reth + Erigon for Ethereum) and places the
//! independence axis where silent bugs live — at the indexer for
//! Bitcoin, at the whole execution client for Ethereum. The concrete
//! clients are I/O shims; this crate is the **pure-domain** layer they
//! plug into:
//!
//! - [`ChainSource`] — the trait each node+indexer implements, yielding a
//!   normalized [`AddressHistory`] for an address up to a height.
//! - [`AddressHistory`] — chain-agnostic, integer-only (no floats),
//!   canonically ordered and content-addressed, so two sources'
//!   histories are compared by hash.
//! - [`FinalityPolicy`] (gap G-5, §3.5) — the settled-height rule that
//!   excludes reorg-churn from the comparison: confirmation depth for
//!   Bitcoin, externally supplied finalized height for Ethereum.
//! - [`reconcile`] — the §3.3 completeness control: two independent
//!   sources' settled histories must hash-match; a divergence is named
//!   and **blocks close**, never auto-reconciled.
//! - [`reproducibility_gate`] — ADR-0001/0004 acceptance test: a source
//!   re-queried for the same range must reproduce its history hash
//!   byte-for-byte.

#![forbid(unsafe_code)]

pub mod finality;
pub mod history;
pub mod reconcile;
pub mod source;

pub use finality::{FinalityPolicy, FinalityRule};
pub use history::{AddressHistory, Chain, ChainMovement, Direction, HistoryError};
pub use reconcile::{reconcile, reproducibility_gate, ReconcileError, Reconciliation, ReproError};
pub use source::{ChainSource, FixtureSource, SourceError};
