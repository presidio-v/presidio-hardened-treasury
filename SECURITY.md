# Security Policy

This project is part of the presidio-hardened family: security posture is a
design constraint, not a feature backlog.

## Supported versions

Pre-1.0: only the latest `0.x` line receives security fixes, and fixes land on
`main`. The project is in Phase 0 (foundations); ledger and evidence formats are
not yet stable.

| Version | Supported |
|---------|-----------|
| 0.22.x (`main`) | ✓ (current) |
| < 0.22  | ✗ |

## Reporting a vulnerability

Preferred: open a private **GitHub Security Advisory** in this repository
([**Security** tab → **Report a vulnerability**](https://github.com/presidio-v/presidio-hardened-treasury/security/advisories/new)).
Alternatively, email **security@presidio-group.eu** with reproduction details.
Please do not open public issues for suspected vulnerabilities.

Include a description, steps to reproduce, the potential impact, and a
suggested fix if you have one.

**Response process.** You will receive an acknowledgment within 72 hours. We aim
to release a fix within 30 days of a confirmed vulnerability and will keep you
updated until the issue is resolved or dismissed with a rationale.

**Credit.** We credit reporters of valid vulnerabilities by name in the
published GitHub Security Advisory and in the `CHANGELOG.md` entry for the fix,
unless the reporter asks to remain anonymous.

## Threat model and assurance case

The threat model is [`docs/threat-model.md`](docs/threat-model.md): assets,
seven trust boundaries, STRIDE per boundary, explicit non-goals and standing
review triggers, with the operator in scope. The consolidated assurance case
(threat model, trust boundaries, secure-design argument, and how common
implementation weaknesses are countered) is [`ASSURANCE.md`](ASSURANCE.md). The
controls are listed below.

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
  its history hash. Only an *agreed* reconciliation maps into the evidence
  chain: the indexer-output → L1-observation mapping books a single
  golden-vectored event committing to the two-source-agreed history hash,
  while a divergence books nothing — keeping the residual single point
  across both sources minimal and audited (ADR-0004 action item 5).
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
- Every live I/O shim is held to a written contract before it is trusted
  (`treasury-conformance`): one parameterized suite per seam — chain
  node+indexer, chain-wallet anchor submitter, GL adapter — asserts the
  invariants the pure core assumes (a non-deterministic indexer, a wallet
  reporting an unproven confirmation, a GL double-posting a retried key
  all fail the suite). The same assertions run against the in-memory
  fixtures today and against a real endpoint the day it is wired, so a
  shim cannot enter the evidence path without first passing the contract.
- Fail-closed defaults throughout: unassessed assets block valuation,
  missing prices block valuation, missing materiality thresholds queue
  everything, empty venue scope reports reject, unbalanced journal entries
  are unconstructible, overdrawn lots are typed errors that mutate nothing.

## Automated checks

Every pull request runs `rustfmt`, `clippy` with the deny-list above, the full
test suite, a 90% line-coverage gate, `cargo-deny`, CodeQL (Rust) and a short
cargo-fuzz run of the canonicalizer. OpenSSF Scorecard runs weekly. GitHub
secret scanning with push protection and Dependabot security updates are
enabled. Changes to `main` need an approving review from a code owner other
than the author.

## Verifying Releases and Obtaining Public Signing Keys

The workspace is distributed as source from this repository; nothing is
published to a package registry. Releases are **SSH-signed git tags** made with
the `presidio-v` organisation's release signing key. That key is registered as a
**signing key** on the maintainer's GitHub account, so each release tag shows as
**Verified** on GitHub and via the API:

```bash
gh api repos/presidio-v/presidio-hardened-treasury/git/tags/<sha> --jq .verification.verified
```

The public key is published in the repository's
[`allowed_signers`](allowed_signers) file. To verify a tag locally:

```bash
git -c gpg.ssh.allowedSignersFile=allowed_signers verify-tag <tag>
```

Build provenance attestations and SBOMs are not produced yet; they are part of
REQ-33 (signed reproducible builds, SLSA-style provenance, Phase 2).
