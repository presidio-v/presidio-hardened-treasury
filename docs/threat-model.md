# Threat Model — presidio-hardened-treasury

**Status:** Phase 0 deliverable (REQ-34, spec v2 §3.8) · authored v0.3.0, last reviewed v0.20.0
**Method:** per-asset STRIDE, with explicit trust-boundary enumeration. The
operator (us) is **inside** the threat model — controls that assume an honest
operator are listed as residual risk, not as mitigations.

-----

## 1. What an attacker gets

This system holds no keys that move funds — privilege rejection and the
egress allowlist make fund theft impossible *through us* by construction.
The assets are therefore informational and reputational:

- **A1 — Treasury posture.** Full wallet clusters, custody relationships,
  transaction cadence, OTC activity, liquidity patterns of a public company.
  Market-sensitive; disclosure is a securities-law event for the client.
- **A2 — xpubs / view keys.** Leak the entire address tree, past and future.
  Worse than A1: permanent, unrevokable visibility.
- **A3 — The books.** Ledger events, judgments, policies, checkpoints. An
  attacker who can *modify* these can manufacture a restatement; one who can
  *read* them gets A1.
- **A4 — Evidence integrity.** The product's entire value is "this number
  reproduces byte-for-byte." Silent corruption of the evidence store or its
  history is an existential attack even with no data exfiltrated.
- **A5 — Venue API keys (read-scoped).** Bounded by scope rejection + egress
  allowlist, but still grant A1 directly at the venue.

## 2. Trust boundaries

| # | Boundary | Crosses it |
|---|----------|-----------|
| B1 | Venue APIs / chain nodes → ingestion | All external data (untrusted input) |
| B2 | Ingestion → ledger (L1 observations) | Canonicalization, evidence hashing |
| B3 | Human → ledger (L3 judgments) | Approvals, dual-control confirmation |
| B4 | Ledger → client GL (feed-GL writer) | The only outbound write path |
| B5 | Operator/infrastructure → everything | Insider, supply chain, hosting |
| B6 | Tenant ↔ tenant | Isolation (per-tenant streams/keys) |
| B7 | Us → external anchor (chain / TSA) | Trust exit point for tamper-evidence |

## 3. STRIDE by boundary

### B1 — Untrusted external data

| Threat | Scenario | Control (structural) | Residual |
|--------|----------|----------------------|----------|
| Spoofing | Compromised chain endpoint serves forged history | Two independent sources per chain must hash-match (`treasury-chainsource`, ADR-0004): BTC = Core + electrs/Fulcrum, ETH = reth + Erigon; a divergence names both sources and **blocks close**, never auto-reconciled. The independence axis sits where silent bugs live — at the indexer for Bitcoin, the whole execution client for Ethereum | Single-source venues (exchange internal ledgers) — surfaced as a lower-corroboration tier in reconciliation (§5 Tier 2) |
| Tampering / silent omission | An indexer drops or revises settled movements (forgery *or* an ordinary indexing bug) | Settled history is content-addressed and compared by hash under a per-chain `FinalityPolicy` that excludes reorg churn; only a two-source-**agreed** history maps into an L1 observation (`book::draft_history_observation`); a divergence books nothing. Provider revisions land as new observations and trigger the checkpoint supersession workflow | Detection latency = re-fetch cadence; a bug *correlated across both* independent implementations (shared upstream) — bounded by choosing implementations that share no indexing code (ADR-0004) |
| Reproducibility | A source returns different history for the same query (nondeterminism), so "reproduce byte-for-byte" silently breaks | The reproducibility gate re-queries each source and rejects any whose settled-history hash is not byte-identical; before any live shim is trusted it must pass the `treasury-conformance` chain-source contract (identity, reproducibility, settled-history stability as the tip advances, two-source agreement) — the same assertions the fixtures pass today | Live-only failure modes (reorg-time races, process-restart nondeterminism) covered by the `--ignored` integration job against a real node, not the in-memory fixture |
| DoS / depth-bombs | Hostile JSON (floats, 1000-deep nesting) poisons hashing | Canonicalization rejects floats and caps depth (`treasury-evidence::canon`); rejected input never enters the ledger | — |
| Elevation | "Read-only" key actually permits trading (venue scope bug) | **Egress allowlist** (`treasury-ingest`): non-matching requests cannot leave the network regardless of key capability; scope gate fails closed, incl. empty scope reports | Justified-POST entries are the audit-review hotspot; each carries a written justification in the hashed artifact |

### B2 — Ingestion → ledger

| Threat | Scenario | Control | Residual |
|--------|----------|---------|----------|
| Tampering | Mutation of booked events | Append-only + per-tenant hash chain; `verify_chain` recomputes every link | In-process attacker who can also recompute the chain → covered by B7 anchoring |
| Repudiation | "We never saw that payload" | Every L1 observation carries an evidence-store hash of the raw payload | — |
| Integrity drift | Same payload hashing differently across re-fetches | Canonicalization spec per provider; golden vectors cross-verified against an independent implementation (REQ-7) | Provider-side nondeterminism (pagination reshuffles) handled per-provider in canonicalization configs |

### B3 — Human judgments

| Threat | Scenario | Control | Residual |
|--------|----------|---------|----------|
| Spoofing / elevation | Single insider books a self-serving classification | L3 judgments structurally require approver identities + policy hash; reconciliation queue is dual-control (preparer ≠ approver); vendor never classifies | Collusion of two client actors — visible in the audit trail, auditor-testable; this is management's fraud risk under the AS 1105 posture, not silently ours |
| Tampering | Retroactive edit of a judgment | Judgments supersede, never mutate; supersession cannot race, cross tenants, or cross layers | — |
| Repudiation | "I never approved that" | Approver identity inside the hashed event envelope | Identity binding strength = authn implementation (Phase 1: SSO + step-up for approvals) |

### B4 — Feed-GL writer (the only outbound write)

| Threat | Scenario | Control | Residual |
|--------|----------|---------|----------|
| Elevation | Ingestion compromise pivots to writing the client's GL | GL writer is a **separate egress service with distinct credentials** (§3.7); write capability never coexists with ingestion credentials | — |
| Tampering / replay | Duplicate or partial postings corrupt the client GL | Posting protocol: idempotency keys + read-back verification; the reconciliation read-back is itself L1 evidence | Target-GL semantics differ; per-GL failure modes documented per adapter (Phase 1) |

### B5 — Operator / supply chain (we are in scope)

| Threat | Scenario | Control | Residual |
|--------|----------|---------|----------|
| Insider tampering | Operator rewrites history, re-hashes the chain | **External anchoring** (`treasury-anchor`): RFC 6962 heads committed outside our trust boundary; `verify_against` detects any anchored-prefix rewrite; coverage-monotonic log prevents quiet head shrinkage | Window between anchors; cadence is a disclosed audit parameter |
| Insider exfiltration | Operator reads A1/A2 | Per-tenant encryption keys; dual-control prod data access; tenant-visible support-access transparency log (Phase 2, REQ-33) | Phase 0/1: hosting-provider trust documented in design-partner contracts |
| Supply chain | Malicious dependency exfiltrates or corrupts | `cargo-deny` (registry pinning, advisories, license gate); `Cargo.lock` committed; minimal third-party surface (the domain crates pull only serde/serde_json/sha2/hex/thiserror — no networking or I/O libraries in the accounting core; live integrations sit behind traits as out-of-core shims); `#![forbid(unsafe_code)]` workspace-wide | Build-infrastructure compromise → reproducible builds + SLSA provenance (Phase 2, REQ-33) |
| xpub theft | A2 from process memory or storage | xpubs never persisted; enclave derivation with per-session attestation (REQ-11, Phase 1; vendor open §9) | Until the enclave lands, design partners onboard with **per-address watch lists, not xpubs** — A2 never exists in the system in Phase 0 |

### B6 — Tenant isolation

| Threat | Scenario | Control | Residual |
|--------|----------|---------|----------|
| Information disclosure | Tenant A queries tenant B | No cross-tenant query path exists in the data layer; streams, checkpoints, policy timelines, and anchor logs are all keyed by tenant; supersession is structurally tenant-bound | Side channels (timing, shared caches) assessed when the serving layer exists (Phase 1) |

### B7 — External anchor

| Threat | Scenario | Control | Residual |
|--------|----------|---------|----------|
| Spoofing | Fake "anchored" receipt never actually committed | Receipt carries the chain tx ref / TSA token hash; anyone (auditor included) verifies the commitment independently — that is the point of anchoring | — |
| Rollback | Operator presents an older, shorter anchor log | Coverage-monotonic receipts; the *external* venue holds the newer commitment; auditor checks the chain/TSA, not us | Requires auditor procedure to actually check — documented in the evidence-reproduction runbook (Phase 1) |

## 4. Explicit non-goals (Phase 0)

Fund custody attacks (no keys that move funds exist here) · availability SLOs
(no serving layer yet) · side-channel hardening (no multi-tenant serving
process yet) · DDoS (infrastructure concern, Phase 1 deployment).

## 5. Standing review triggers

Re-review this document when: a new crate crosses a trust boundary · any
allowlist gains a justified-POST entry · the enclave vendor is chosen (§9) ·
the GL writer ships · SoR mode activates (Phase 2 trust inversion: we become
the external authority).
