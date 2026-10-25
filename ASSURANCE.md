# Security Assurance Case

This document is the assurance case for `presidio-hardened-treasury`: an explicit argument
for why the project's security requirements are met. It has four parts, as
required by the OpenSSF Best Practices silver criterion `assurance_case`:

1. the threat model,
2. the trust boundaries,
3. the argument that secure design principles are applied, and
4. the argument that common implementation weaknesses are countered.

It is a summary that links to the authoritative detail in
[`docs/threat-model.md`](docs/threat-model.md) (per-boundary STRIDE tables and
residual risk), [`SECURITY.md`](SECURITY.md) (controls, reporting) and
[`ARCHITECTURE.md`](ARCHITECTURE.md) (components, flow, boundaries) for
`presidio-v/presidio-hardened-treasury`.

**Scope of the claim.** The repository is a Rust library workspace that
implements the pure core of a quarterly treasury close. It performs no network
I/O, holds no keys, and runs no service. The live I/O shims (chain
nodes/indexers, the egress proxy process, GL adapters, the anchor wallet) are
Phase 1 work behind trait seams. The argument below covers the code in this
repository. Where a control belongs to a component that does not exist yet,
it is marked as Phase 1 or Phase 2 rather than claimed.

## 1. Threat model

**Assets** (full list and rationale: [threat model §1](docs/threat-model.md)):

- **A3 — the books:** ledger events, judgments, policies, checkpoints. Write
  access lets an attacker manufacture a restatement.
- **A4 — evidence integrity:** every number in the disclosure pack must
  reproduce byte-for-byte from hashed inputs. Silent corruption is the
  existential threat even with nothing exfiltrated.
- **A1/A2 — treasury posture and xpubs**, and **A5 — read-scoped venue keys**:
  confidentiality assets. In this repository they matter mainly as data that
  must not be collected: Phase 0 onboards with per-address watch lists, so no
  xpub exists in the system.

**The operator is in scope.** Controls that only work if the maintainers are
honest are listed as residual risk, not as mitigations.

| Threat | Control in this repository |
|---|---|
| Forged or silently incomplete chain history (compromised or buggy source) | Two independent sources per chain must hash-match under a finality policy; a divergence blocks close and books nothing (`treasury-chainsource`, ADR-0004); the reproducibility gate rejects a source whose re-query differs |
| Hostile input poisoning hashing (floats, deep nesting) | Canonical JSON rejects every non-integer number and caps nesting at `MAX_DEPTH = 128` (`treasury-evidence::canon`); rejected input never enters the ledger |
| Mutation, insertion or deletion of booked events | Append-only, per-tenant hash-chained ledger; `verify_chain` recomputes every link; durable backends replay-verify on open and refuse a tampered log (`treasury-ledger`, `treasury-evidence`) |
| Insider rewrites history and recomputes the chain | RFC 6962 tree heads anchored outside the operator's boundary; coverage-monotonic receipt log; `verify_against` detects an anchored-prefix rewrite (`treasury-anchor`) |
| A single insider books a self-serving judgment | L3 judgments require a preparer, a *distinct* approver and a content-addressed policy hash; self-approval is a typed error (`treasury-core::dual_control`) |
| "Read-only" venue key that can trade | Content-addressed egress allowlist, deny by default, no regex; fail-closed key-scope gate including empty scope reports (`treasury-ingest`) |
| Duplicate or partial GL postings | Content-derived idempotency keys, dual-control release, no guessing transitions; read-back is a mandatory trait method, so a post-only adapter cannot be written (`treasury-posting`, `treasury-gl`) |
| Cross-tenant disclosure | Streams, checkpoints, policy timelines and anchor logs are keyed by tenant; no cross-tenant query path in the data layer |
| Malicious or vulnerable dependency | `cargo-deny` (advisories, yanked crates, license allowlist, registry pinning); committed `Cargo.lock`; small third-party surface; Dependabot |

**Out of scope, stated rather than assumed** ([threat model §4](docs/threat-model.md)):
fund custody (no key that moves funds exists here); availability and DDoS (no
serving layer); side channels (no multi-tenant serving process yet); xpub
derivation in an enclave (REQ-11, Phase 1, vendor open); build-infrastructure
compromise, which needs reproducible builds and SLSA provenance (REQ-33,
Phase 2); and collusion of two client approvers, which stays visible in the
audit trail but is management's fraud risk.

## 2. Trust boundaries

Names follow [threat model §2](docs/threat-model.md) and
[ARCHITECTURE.md#trust-boundaries](ARCHITECTURE.md#trust-boundaries).

| Boundary | Kind | Control |
|---|---|---|
| B1 — venue APIs / chain nodes → ingestion | Untrusted input | Two-source agreement and reproducibility gate (chain); egress allowlist and scope gate (venues). The live shims are Phase 1 and must pass the `treasury-conformance` contract before they are trusted |
| B2 — ingestion → ledger (L1) | Input validation | Canonicalization (float rejection, depth cap); every L1 observation carries the evidence-store hash of its raw payload |
| B3 — human → ledger (L3) | Authorization | Dual control as a type; approver identity inside the hashed envelope. Identity binding strength depends on authentication (ADR-0005, Proposed) |
| B4 — ledger → client GL | The only outbound write | Posting protocol state machine; mandatory read-back; GL writer intended as a separate egress service with distinct credentials (Phase 1) |
| B5 — operator / supply chain → everything | Insider | External anchoring; `cargo-deny`; `#![forbid(unsafe_code)]`; two-person review on `main` |
| B6 — tenant ↔ tenant | Isolation | Tenant-keyed data model; no cross-tenant path |
| B7 — us → external anchor | Trust exit | Receipts carry the chain transaction reference; auditors verify against the chain, not against us |

## 3. Secure design principles applied

- **Fail-safe defaults.** Every gate fails closed: unassessed or out-of-scope
  assets block valuation, missing prices block valuation, missing materiality
  thresholds queue everything, empty venue scope reports are rejected, an
  unclassified reconciliation leg blocks close, a two-source divergence blocks
  close, and the allowlist denies anything it does not match.
- **Complete mediation.** Data enters the ledger only through the
  canonicalization boundary and the layer-specific provenance checks applied at
  append time. Durable logs are not trusted on reopen: every record is replayed
  through full validation and every blob is re-hashed.
- **Least privilege.** The core holds no keys and no network capability. The
  domain crates depend only on `serde`, `serde_json`, `sha2`, `hex` and
  `thiserror`. Write capability towards the client GL is confined to one
  protocol and is separated from ingestion credentials by design.
- **Defense in depth.** Hash chaining detects tampering by anyone who cannot
  recompute the chain; external anchoring covers the operator who can; dual
  control covers the single insider; replay-on-open covers on-disk corruption;
  golden vectors cross-checked against an independent Python implementation and
  the whole-close determinism test (`treasury-e2e`) cover silent drift in
  hashing rules.
- **Economy of mechanism.** One canonical encoding, one hash function
  (SHA-256 from the RustCrypto `sha2` crate), RFC 6962 for Merkle trees. No
  bespoke cryptography, no floating point in the accounting path, and dangerous
  states (an unbalanced journal entry, a post-only GL adapter) are made
  unconstructible by the type system rather than checked after the fact.

## 4. Common implementation weaknesses countered

| Weakness class | How it is countered |
|---|---|
| Improper input validation / injection (CWE-20, CWE-74) | Canonical JSON is the only path into evidence: non-integer numbers and excess nesting are rejected; the allowlist matches exact endpoint and method pairs with no regex. No SQL, shell or template layer exists in the codebase |
| Memory safety (CWE-119 family) | Rust, with `unsafe_code = "forbid"` in every first-party crate (workspace lint) |
| Integer overflow and numeric error (CWE-190, CWE-681) | Integer base units only; `clippy::arithmetic_side_effects` and `clippy::float_arithmetic` denied workspace-wide; `overflow-checks = true` in release builds; property tests that checked arithmetic never panics |
| Uncontrolled resource consumption (CWE-400, CWE-674) | Recursion depth capped in canonicalization; `unwrap`, `expect`, `panic!` and unchecked indexing denied by clippy, so malformed input yields a typed error, not a crash. A cargo-fuzz target exercises the canonicalizer in CI |
| Cryptographic misuse (CWE-327, CWE-328) | SHA-256 only, via a vetted crate, used for content addressing; no encryption, signing or key handling in this repository |
| Hard-coded or exposed secrets (CWE-798, CWE-532) | The library takes no credentials; GitHub secret scanning with push protection is enabled on the repository |
| Insecure network / certificate validation (CWE-319, CWE-295) | Not applicable to this repository: it has no network code. TLS requirements apply to the Phase 1 shims |
| Unsafe deserialization (CWE-502) | `serde` into typed structs; evidence is re-hashed and replay-validated before use |
| Vulnerable dependencies (CWE-1104) | `cargo-deny check all` (RustSec advisories, yanked crates) is a required CI check; Dependabot for Cargo and GitHub Actions; committed lockfile |

These classes are checked continuously on every push and pull request by
**CodeQL** (Rust, `security-and-quality` queries), **clippy** with the deny-list
above, and **cargo-deny**, and weekly by **OpenSSF Scorecard**. The
specification went through two independent adversarial model reviews
([`docs/adversarial-review-spec-v1-gpt5.5.md`](docs/adversarial-review-spec-v1-gpt5.5.md),
[`docs/adversarial-review-spec-v1-grok.md`](docs/adversarial-review-spec-v1-grok.md))
whose remediations produced spec v2. No independent human security review of
the code has been performed yet, and this case does not claim one.

## Conclusion

The threats above are each matched to a control; the controls sit at explicit
trust boundaries; the design follows fail-safe, least-privilege, complete-
mediation, defense-in-depth, and economy-of-mechanism principles; and the common
implementation weakness classes are countered by design and checked by automated
analysis. The project's stated security requirements are therefore met for the
code in this repository, subject to the documented out-of-scope assumptions and
the Phase 1 and Phase 2 items named above.
