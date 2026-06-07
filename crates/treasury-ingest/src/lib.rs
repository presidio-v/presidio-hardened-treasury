//! Read-only ingestion boundary (spec v2 §3.4, REQ-10).
//!
//! Venue scope flags are venue-implemented and venue-buggy; they are a
//! hygiene check, not the control. The control is an **egress allowlist**:
//! every venue request must match a listed `(venue, method, path)` entry
//! or it does not leave the network — deny by default, fail closed.
//!
//! This crate is the decision core the egress proxy consults; the proxy
//! itself is an I/O adapter. Structural guarantees:
//! - The allowlist is a content-addressed, approval-signed audit artifact
//!   (same discipline as policy artifacts, REQ-9): no approvers → no
//!   identity → nothing to deploy.
//! - Path patterns are exact or literal-prefix only — no regex, nothing an
//!   auditor cannot evaluate by eye, nothing an attacker can backtrack.
//! - A non-read HTTP method can only be listed with a written
//!   justification (some venues serve authenticated *reads* over POST);
//!   an unjustified non-read entry cannot be constructed.
//! - Key-scope validation **fails closed**: unknown scopes reject, and
//!   any trade/withdraw/transfer capability rejects the key outright.

#![forbid(unsafe_code)]

pub mod allowlist;
pub mod scope;

pub use allowlist::{
    AllowlistEntry, AllowlistError, EgressAllowlist, EgressDecision, HttpMethod, PathPattern,
};
pub use scope::{validate_scopes, ScopeClaim, ScopeDecision, ScopeViolation};
