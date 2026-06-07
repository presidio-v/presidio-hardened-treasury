# Security Policy

This project is part of the presidio-hardened family: security posture is a
design constraint, not a feature backlog.

## Reporting a vulnerability

Email **vladimir@presidio-group.eu** with reproduction details. Please do not
open public issues for suspected vulnerabilities. You will receive an
acknowledgment within 72 hours.

## Hardening baseline

- `#![forbid(unsafe_code)]` in all first-party crates.
- No floating point in the accounting path; integer base units with checked
  arithmetic only (`clippy::float_arithmetic` and
  `clippy::arithmetic_side_effects` are denied workspace-wide).
- Canonicalization rejects floats and depth-bombs at the evidence boundary.
- Append-only, hash-chained ledger streams; RFC 6962 tree heads intended for
  external anchoring so tamper-evidence does not depend on trusting the
  operator (spec v2 §3.3).
- `cargo-deny` in CI: license allowlist, advisory database, registry pinning.
- Threat model is a Phase 0 deliverable (spec v2 §3.8); read-only ingestion
  is enforced at the network layer (egress allowlist), not by venue scope
  flags (spec v2 §3.4).
