//! Policy-as-code (spec v2 §3.5, REQ-9).
//!
//! Every accounting-relevant policy — principal-market price sourcing,
//! fee-treatment election, per-chain finality, FX translation — is a
//! **versioned, content-addressed, approval-signed artifact**. An auditor
//! dispute then has an exact, dated object to dispute, and a restatement
//! under a corrected policy is a clean re-run keyed by a different hash.
//!
//! Structural guarantees:
//! - A policy artifact without at least one approver cannot be registered;
//!   its identity is the SHA-256 of its canonical envelope (float-free).
//! - Activation is append-only per `(tenant, kind)` with strictly monotonic
//!   knowledge time; `active_at` answers "which policy governed this
//!   valuation at the time it was performed" — the bitemporal question.
//! - The valuation memoization key is `(lots, price-snapshot, policy)` —
//!   the policy version is part of the function input, so two valuations
//!   under different policies can never collide (spec v2 §3.5 / R3).

#![forbid(unsafe_code)]

pub mod artifact;
pub mod registry;
pub mod valuation;

pub use artifact::{PolicyArtifact, PolicyError, PolicyKind};
pub use registry::{ActivationRecord, PolicyRegistry, RegistryError};
pub use valuation::ValuationKey;
