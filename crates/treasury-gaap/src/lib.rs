//! GAAP policy module — L4 (spec v2 §3.1, §4; remediation R11).
//!
//! The module contract emits *(measurement basis, statement-line target,
//! amount)*, not labels: GAAP (ASU 2023-08) routes fair-value changes to
//! **net income**, IFRS routes revaluation surplus to OCI — the same
//! gain lands in a different financial statement, so the routing is in
//! the type system ([`Statement`]), and the IFRS module reuses the
//! identical entry model with different targets. That is what keeps the
//! IFRS module an activation instead of a rewrite.
//!
//! Structural guarantees:
//! - **An unbalanced journal entry cannot be constructed** — debits must
//!   equal credits in a single currency, enforced at `JournalEntry::new`.
//! - Entries are content-addressed and commit to the policy-module
//!   version hash; they book as L4 policy outputs whose provenance names
//!   the input events — pure functions over the ledger, per the claim
//!   layering (R4).
//! - Fee treatment is applied here from the tenant's election (G-3):
//!   the lot engine kept fees decomposed; this module routes them to
//!   expense or into the asset's carrying amount.

#![forbid(unsafe_code)]

pub mod entry;
pub mod module;

pub use entry::{EntryError, JournalEntry, JournalLine, Side, Statement, StatementLine};
pub use module::{
    acquisition_entry, disposal_entry, draft_policy_output, remeasurement_entry, FeeTreatment,
};
