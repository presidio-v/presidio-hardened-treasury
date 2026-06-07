//! GL posting protocol (spec v2 §3.7, REQ-25; remediation R9).
//!
//! "Not an adapter — a protocol." This crate is the state machine every
//! GL adapter (`NetSuite`, `QuickBooks`, SAP) is driven by; the adapters
//! themselves are I/O shims that report outcomes back into it.
//!
//! The crux is retry safety. The adversarial scenario (review F-12 /
//! spec §8): the export posts correctly once and badly on retry. Here:
//! - A batch's identity **is** its idempotency key: the content hash of
//!   (tenant, target GL, period tag, entry hashes). A retry carries the
//!   same key, and the target GL's idempotency check (or our read-back)
//!   dedupes it.
//! - A submission whose acknowledgment is lost lands in `Unknown` — the
//!   protocol's honest state. The only exits from `Unknown` are
//!   evidence-driven: a read-back that finds the batch (→ `Posted`) or
//!   one that proves its absence (→ back to `ReadyToSubmit`, same key).
//!   Guessing is not a transition.
//! - Release requires dual control (client-side approval before
//!   anything leaves for the GL, §3.7).
//! - Verification is content-equality: the read-back entries' hashes
//!   must match the batch's, both directions; mismatches are terminal
//!   and name the missing/unexpected entries. Every GL response and
//!   read-back is referenced by evidence hash — the GL reconciliation
//!   is itself L1 evidence.

#![forbid(unsafe_code)]

pub mod batch;
pub mod protocol;

pub use batch::PostingBatch;
pub use protocol::{PostingError, PostingProtocol, PostingState};
