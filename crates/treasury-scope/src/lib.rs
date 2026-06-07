//! ASU 2023-08 scope gate (spec v2 §2.3, REQ-22).
//!
//! v1 ships a **scope gate, not a scope engine**: an asset proceeds to
//! valuation only when a dual-control-confirmed assessment finds all six
//! ASU 2023-08 criteria met. Everything else — not-met criteria,
//! undetermined criteria, or **no assessment at all** — hard-blocks with
//! an explicit verdict. Honest rejection is audit-defensible; silent
//! misclassification is not.
//!
//! The six criteria (ASC 350-60-15-1): intangible asset · does not
//! provide enforceable rights to underlying goods/services/other assets ·
//! resides on a distributed ledger · secured through cryptography ·
//! fungible · not created or issued by the reporting entity or its
//! related parties.
//!
//! Structural guarantees:
//! - The criteria struct has six mandatory fields — an assessment that
//!   "forgot" a criterion cannot be constructed.
//! - `Undetermined` is never in-scope (fail closed).
//! - The gate only honors **confirmed** assessments (dual control via
//!   `treasury-core::dual_control`); confirmed assessments book as L3
//!   judgments against the tenant's scope policy artifact (REQ-9).

#![forbid(unsafe_code)]

pub mod assessment;
pub mod gate;

pub use assessment::{
    CriteriaAssessment, Criterion, CriterionStatus, ScopeAssessment, ScopeError, ScopeVerdict,
};
pub use gate::{draft_scope_judgment, GateDecision, ScopeGate};
