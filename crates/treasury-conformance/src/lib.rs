//! Conformance suites: the contract each live I/O shim must satisfy.
//!
//! ADR-0001/0002/0003/0004 all share one discipline — the *domain* logic
//! is pure and tested, and the concrete integration (a Bitcoin/Ethereum
//! node+indexer, a chain wallet, a NetSuite/QuickBooks/SAP GL) is an I/O
//! shim behind a trait. The risk is that a real shim quietly violates an
//! invariant the pure core assumes (a non-deterministic indexer, a wallet
//! that reports a confirmation it cannot prove, a GL that double-posts a
//! retried key). These are exactly the bugs that never show up against an
//! in-memory fixture.
//!
//! This crate closes that gap. Each seam has **one parameterized test
//! body** — a `pub fn verify_*_contract(...)` that takes a constructed
//! implementor and exercises the invariant. The same body runs against
//! the fixtures here (proving the harness and the fixtures agree) and,
//! when a real endpoint lands, against that endpoint from the shim
//! crate's own integration test:
//!
//! ```ignore
//! #[test]
//! #[ignore = "requires a live regtest bitcoind; run with --ignored in the integration job"]
//! fn live_electrs_satisfies_the_chain_source_contract() {
//!     let source = ElectrsSource::connect(&regtest_url())?;
//!     treasury_conformance::chain_source::verify_reproducible(
//!         &source, &btc_policy(), TEST_ADDR, tip,
//!     )?;
//! }
//! ```
//!
//! The suites never require a live service themselves; what they require
//! is an implementor of the seam trait. What they cannot exercise without
//! one — an indexer that is *actually* non-deterministic, a wallet that
//! *actually* drops a transaction — is documented per module as the
//! residual that only the live integration job covers.

#![forbid(unsafe_code)]

pub mod anchor_submitter;
pub mod chain_source;
pub mod gl_adapter;

use treasury_chainsource::Chain;
use treasury_core::ContentHash;

/// A way a shim failed its contract. Underlying library errors are
/// captured by their `Display` so this type stays comparable; the
/// semantic violations carry typed, hashable fields for precise asserts.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ContractViolation {
    /// A source reported a chain other than the one it was built for.
    #[error("wrong chain: expected {expected:?}, source reported {found:?}")]
    WrongChain {
        /// The chain the source was constructed for.
        expected: Chain,
        /// The chain the source reported.
        found: Chain,
    },
    /// A source returned an empty identifier (provenance needs a stable
    /// non-empty id).
    #[error("source id is empty")]
    EmptySourceId,
    /// The same query produced two different settled-history hashes — the
    /// source is not deterministic and cannot be a system of record.
    #[error("not reproducible: {first} != {second}")]
    NotReproducible {
        /// First query's hash.
        first: ContentHash,
        /// Second query's hash.
        second: ContentHash,
    },
    /// Advancing the observed tip rewrote already-settled history: the
    /// hash of the prefix settled at the lower height changed.
    #[error("settled history at height {settled_height} changed as the tip advanced: {low} != {high}")]
    SettledHistoryRewritten {
        /// The settled height whose prefix must be stable.
        settled_height: u64,
        /// Hash observed when queried at the lower tip.
        low: ContentHash,
        /// Hash observed when queried at the higher tip.
        high: ContentHash,
    },
    /// Two independent sources covering the same chain state disagreed
    /// when they were expected to agree.
    #[error("independent sources disagreed when agreement was expected")]
    SourcesDisagree,
    /// A reported chain height went backwards across observations.
    #[error("chain height went backwards: {previous} then {observed}")]
    HeightWentBackwards {
        /// The previously observed height.
        previous: u64,
        /// The lower height observed next.
        observed: u64,
    },
    /// A broadcast never reached the required confirmation depth within
    /// the poll budget — a liveness failure.
    #[error("anchor never confirmed to depth {required} within {polls} polls")]
    AnchorNeverConfirmed {
        /// Required confirmation depth.
        required: u64,
        /// Polls spent before giving up.
        polls: u32,
    },
    /// Finalization produced the wrong number of receipts.
    #[error("expected {expected} receipts, got {found}")]
    ReceiptCountMismatch {
        /// One receipt per target was expected.
        expected: usize,
        /// What finalization produced.
        found: usize,
    },
    /// A GL did not show a batch that the contract required it to hold.
    #[error("batch not present in the GL after a posted, verified submit")]
    NotPosted,
    /// Read-back did not round-trip the batch fingerprint exactly.
    #[error("read-back mismatch: {missing:?} missing, {unexpected:?} unexpected")]
    ReadbackMismatch {
        /// Batch entries the GL did not show.
        missing: Vec<ContentHash>,
        /// GL entries the batch did not contain.
        unexpected: Vec<ContentHash>,
    },
    /// Re-submitting the same idempotency key produced a second, distinct
    /// GL reference — the adapter is not idempotent.
    #[error("submit was not idempotent: key produced two references {first} and {second}")]
    NotIdempotent {
        /// Reference from the first submit.
        first: String,
        /// Reference from the second submit.
        second: String,
    },
    /// An underlying library or transport error, captured by `Display`.
    #[error("underlying error: {0}")]
    Underlying(String),
}
