//! GL adapter contract + lifecycle orchestration (ADR-0003).
//!
//! `treasury-posting` is the vendor-agnostic posting *protocol* — the
//! state machine with the idempotency-key and read-back-verification
//! discipline. This crate is the **adapter contract** every concrete GL
//! (NetSuite, QuickBooks, SAP) implements, plus the orchestration that
//! drives the protocol against any adapter. The concrete vendors are
//! thin I/O shims implementing [`GlAdapter`]; this crate, and its
//! [`FixtureGl`], are pure domain and exercise the whole loop.
//!
//! The ADR-0003 acceptance rule — *"verification-complete or not
//! shipped: no adapter ships that can post but cannot read back"* — is
//! enforced in the **type system**: [`GlAdapter::read_back`] is a
//! required trait method, so an adapter that posts but cannot read back
//! cannot exist. A post-only "adapter" simply does not implement the
//! trait.
//!
//! The orchestration ([`post_batch`]) walks the full posting-protocol
//! lifecycle, including the honest `Unknown`/read-back recovery path: a
//! lost acknowledgment is resolved only by evidence (the GL is queried),
//! never by guessing, and a retry reuses the same content-derived
//! idempotency key.

#![forbid(unsafe_code)]

pub mod adapter;
pub mod drive;
pub mod fixture;

pub use adapter::{GlAdapter, GlError, GlReadback, SubmitOutcome};
pub use drive::{post_batch, DriveError, DriveOutcome};
pub use fixture::{FixtureGl, FixtureFault};
