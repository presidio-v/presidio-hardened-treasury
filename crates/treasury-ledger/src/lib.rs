//! The claim-layered, bitemporal, append-only ledger (spec v2 §3.1–§3.2).
//!
//! Structural guarantees, enforced at append time rather than by review:
//! - **Append-only with hash chaining** — every event's identity commits to
//!   its full content *and* its predecessor; `verify_chain` detects any
//!   mutation, insertion, or deletion after the fact.
//! - **Bitemporal** — events carry *event time* (when it happened in the
//!   world) and *knowledge time* (when the ledger booked it, strictly
//!   monotonic per tenant). "What did the books say as of the 10-Q filing"
//!   is `as_of(filing_knowledge_time)`.
//! - **Claim layers** — observations, derived facts, judgments, and policy
//!   outputs are distinct layers with layer-specific mandatory provenance;
//!   a judgment without an approver and policy hash cannot enter the ledger.
//! - **Supersession, not mutation** — corrections append a superseding
//!   event; the superseded event remains, and bitemporal queries resolve
//!   which one was authoritative at any knowledge time.
//! - **Float-free payloads** — payloads canonicalize through
//!   `treasury-evidence`; a float anywhere in a payload rejects the append.

#![forbid(unsafe_code)]

pub mod event;
pub mod ledger;

pub use event::{ClaimLayer, EventDraft, EventId, Provenance, SealedEvent};
pub use ledger::{InMemoryLedger, Ledger, LedgerError};
