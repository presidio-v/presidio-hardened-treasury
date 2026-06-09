//! External anchoring (spec v2 §3.3, REQ-8).
//!
//! The evidence store's RFC 6962 tree head is periodically committed to a
//! venue **outside our trust boundary** — a public chain transaction or an
//! RFC 3161 timestamp authority. Tamper-evidence then does not depend on
//! trusting the operator: the insider threat model includes us.
//!
//! This crate is the domain machinery: content-addressed anchor receipts
//! and an append-only, coverage-monotonic anchor log per evidence store.
//! The submission integrations (chain wallet, TSA client) are deliberately
//! *not* here — they are I/O adapters that produce the [`AnchorMethod`]
//! reference this crate records and verifies against.
//!
//! Structural guarantees:
//! - A receipt commits to `(tree head, entry count, method reference,
//!   anchored-at)`; its identity is the SHA-256 of its canonical envelope.
//! - The log is append-only with strictly monotonic anchor times **and
//!   non-decreasing entry counts** — an anchor claiming to cover fewer
//!   entries than its predecessor is structurally rejected (evidence
//!   stores are append-only, so coverage can only grow).
//! - `verify_against` recomputes the live store's head and confirms the
//!   latest receipt matches it at the recorded entry count, detecting
//!   post-anchor tampering of any already-anchored prefix.

#![forbid(unsafe_code)]

pub mod aggregation;
pub mod log;
pub mod pipeline;
pub mod receipt;

pub use aggregation::{aggregate, verify_inclusion, Aggregation, InclusionProof};
pub use log::{AnchorError, AnchorLog};
pub use pipeline::{AnchorPipeline, AnchorTarget, PipelineError, PipelineState};
pub use receipt::{AnchorMethod, AnchorReceipt};
