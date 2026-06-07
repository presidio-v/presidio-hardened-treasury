# Security Policy

This project is part of the presidio-hardened family: security posture is a
design constraint, not a feature backlog.

## Supported versions

Pre-1.0: only the latest `0.x` release line receives security fixes. The project
is in Phase 0 (foundations); ledger and evidence formats are not yet stable.

| Version | Supported |
|---------|-----------|
| 0.1.x   | ✓ (current) |
| < 0.1   | ✗ |

## Reporting a vulnerability

Preferred: open a private **GitHub Security Advisory** in this repository
(**Security** tab → **Report a vulnerability**). Alternatively, email
**vladimir@presidio-group.eu** with reproduction details. Please do not open
public issues for suspected vulnerabilities. You will receive an acknowledgment
within 72 hours.

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
- Threat model maintained at [`docs/threat-model.md`](docs/threat-model.md)
  (STRIDE per trust boundary, operator in scope, standing review triggers);
  read-only ingestion is enforced at the network layer (`treasury-ingest`
  egress allowlists, deny by default, fail-closed key-scope validation),
  not by venue scope flags (spec v2 §3.4).
- External anchoring (`treasury-anchor`): RFC 6962 tree heads committed
  outside the operator's trust boundary; coverage-monotonic receipts detect
  post-anchor history rewrites.
- Dual control as a structural primitive (`treasury-core::dual_control`):
  match confirmations, leg designations, and scope assessments all require
  a preparer and a distinct approver; self-confirmation is a typed error.
- Fail-closed defaults throughout: unassessed assets block valuation,
  missing prices block valuation, missing materiality thresholds queue
  everything, empty venue scope reports reject, unbalanced journal entries
  are unconstructible, overdrawn lots are typed errors that mutate nothing.
