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
  The anchoring pipeline (ADR-0002) commits a Merkle-aggregated root in a
  single Bitcoin transaction (per-head inclusion proofs preserve individual
  verifiability), gates "anchored" on a required confirmation depth, and
  flags any submitted-but-unconfirmed anchor as overdue so it cannot become
  a silent coverage gap. Only hashes are published — never treasury data.
- Dual control as a structural primitive (`treasury-core::dual_control`):
  match confirmations, leg designations, and scope assessments all require
  a preparer and a distinct approver; self-confirmation is a typed error.
- GL posting protocol (`treasury-posting`): the only outbound write path
  is a state machine with no guessing transitions — a lost acknowledgment
  resolves exclusively through read-back evidence, and retries reuse the
  same content-derived idempotency key.
  The GL adapter contract (`treasury-gl`, ADR-0003) makes read-back a
  mandatory trait method, so an adapter that can post but cannot verify
  what it posted cannot exist — the "verification-complete or not shipped"
  rule is enforced by the type system, not by review.
- Chain ingestion uses two independent in-house sources per chain
  (`treasury-chainsource`, ADR-0004): their settled histories must hash-match
  or the address blocks close — a divergence is surfaced for human resolution,
  never auto-reconciled. The independence axis sits where silent bugs live
  (the indexer for Bitcoin, the whole execution client for Ethereum), and a
  reproducibility gate rejects any source whose re-query does not reproduce
  its history hash.
- Disclosure packs (`treasury-disclosure`) are content-addressed and carry
  an evidence-reproduction manifest: the sorted hash closure of everything
  a number depends on. Citing a pack hash cites the entire close; auditing
  it is fetch-recompute-compare, with no trusted intermediary.
- Durable logs do not trust their files: on open, the ledger replays every
  record through full validation (the replayed event id must equal the
  recorded one) and the evidence store re-verifies every blob against its
  recorded hash. Tampered or truncated logs refuse to load; torn tails
  (crash mid-write) recover only through an explicit call.
- Whole-close determinism is tested end to end (`treasury-e2e`): the
  complete pipeline — ingestion through disclosure pack — runs twice and
  must produce identical artifact hashes.
- Fail-closed defaults throughout: unassessed assets block valuation,
  missing prices block valuation, missing materiality thresholds queue
  everything, empty venue scope reports reject, unbalanced journal entries
  are unconstructible, overdrawn lots are typed errors that mutate nothing.
