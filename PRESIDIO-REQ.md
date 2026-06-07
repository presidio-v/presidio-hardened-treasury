# PRESIDIO-REQ — presidio-hardened-treasury

Requirements baseline and versioning rationale for the audit-grade crypto
treasury-close system. The **single source of truth for product scope and
architecture** is [`docs/treasury-suite-spec-v2.md`](docs/treasury-suite-spec-v2.md)
(active spec); this file is the requirements view of that spec, with delivery
status per phase. Requirement IDs cite spec sections (`§n`).

- **Active version:** 0.12.0 (workspace `[workspace.package].version`)
- **Phase:** 0 — Foundations (no UI)
- **Accounting surface:** GAAP + IFRS architecturally day one; IFRS *delivery* Phase 3
- **Audit posture:** Phase 1 = management's evidence-preparation tool (AS 1105);
  Phase 2+ = relied-upon service organization (AS 2601), gated on SOC 2 Type I

---

## Mandatory architectural guarantees (§3) — enforced at append time, not by review

| ID | Requirement | Spec | Status (v0.1.0 / Phase 0) |
|----|-------------|------|---------------------------|
| REQ-1 | **Claim-layered ledger** — L1 observations → L2 derived facts (code-version hash) → L3 judgments (policy hash + approver) → L4 policy outputs (pure fn of L1–L3) | §3.1 | **Implemented** (`treasury-ledger`) |
| REQ-2 | **Bitemporal, event-sourced, append-only** — event time + knowledge time; corrections supersede, never mutate; `as_of` is a query | §3.2 | **Implemented** (`treasury-ledger`) |
| REQ-3 | **Per-tenant hash chaining** — `verify_chain` detects any post-hoc mutation, insertion, or deletion; supersession cannot race, cross tenants, or cross claim layers | §3.2, §3.8 | **Implemented** (`treasury-ledger`) |
| REQ-4 | **Content-addressed evidence store** — float-rejecting canonical JSON, SHA-256 addressing, RFC 6962 Merkle tree heads for external anchoring | §3.3 | **Implemented** (`treasury-evidence`) |
| REQ-5 | **Layer-specific mandatory provenance** — a judgment (L3) without approver + content-addressed policy hash cannot enter the ledger | §3.1, §3.5 | **Implemented** (`treasury-ledger`) |
| REQ-6 | **No floats in the accounting path** — integer base-unit money with checked arithmetic; floats reject at the canonicalization boundary | §3.8 | **Implemented** (`treasury-core`, `treasury-evidence`) |
| REQ-7 | **Cross-implementation hash verification** — event identity hashes cross-verified against an independent implementation (golden vectors in the test suite) | §3.3 | **Implemented** (`treasury-ledger/tests`) |
| REQ-8 | **External anchoring** — evidence-store Merkle root periodically committed to a public chain and/or RFC 3161 TSA; tamper-evidence does not require trusting the operator | §3.3 | **Implemented** (`treasury-anchor`: content-addressed receipts, coverage-monotonic log, anchored-prefix verification; chain/TSA *submission adapters* are Phase 1 I/O) |
| REQ-9 | **Policy-as-code, content-addressed, approval-signed** — principal-market, fee-treatment, finality, FX policies as versioned artifacts; valuation key is `(lots, price-snapshot-hash, policy-hash)` | §3.5 | **Implemented** (`treasury-policy`: artifact hashing requires ≥1 approver, per-tenant activation timelines answer the bitemporal "which policy governed at T"; the actual principal-market policy *bodies* are a Phase 0 exit deliverable with the design partner's auditor, §7) |
| REQ-10 | **Read-only ingestion by construction** — all venue traffic via an egress proxy allowlisting read-only endpoint+method pairs; versioned allowlist is an audit artifact; onboarding rejects keys with trade/withdraw scope | §3.4 | **Implemented** (`treasury-ingest`: content-addressed approval-signed allowlists, deny-by-default decision core, fail-closed scope gate incl. empty scope reports; the proxy *process* wrapping this core is Phase 1 I/O) |
| REQ-11 | **xpub secrecy** — derivation in an enclave with per-session remote attestation; persist only derived addresses, never the master key | §3.4 | Phase 1 (enclave vendor open, §9) |
| REQ-12 | **Checkpoint lineage** — closed periods are immutable DAG nodes; supersession structurally requires a reason code + materiality memo evidence hash; "as filed" and "as corrected" are pointers; state root reproduces byte-for-byte from `as_of` | §3.6 | **Implemented** (`treasury-close`) |

## Close-pipeline requirements (§2) — seven ordered stages

| ID | Stage | Spec | Phase |
|----|-------|------|-------|
| REQ-20 | Read-only ingestion (on-chain xpubs/view keys + exchange/custodian APIs; full history) | §2, §3.4 | 1 |
| REQ-21 | Reconciliation — tiered internal-transfer matcher; non-purchase acquisitions as first-class L3 judgments; **unclassified legs block close**; dual-control client confirmation; replayable versioned matcher; precision/recall SLOs on a labeled corpus | §5 | 1 (critical path) — **matcher core implemented** (`treasury-reconcile` v0.4.0: tiers 0/1/2, content-addressed matcher config, materiality fail-closed, ambiguity demotion, dual-control queue, close blockers). **Complete at the harness level** (v0.8.0: content-addressed labeled corpora + SLO reports committing to corpus and config hashes; exact-rational metrics, no floats; error/abstention taxonomy where phantom auto-nets are the must-be-zero number; synthetic adversarial baseline incl. a deliberately visible batched-withdrawal miss). Remaining: design-partner labeled history to grow the corpus. **Designation flow implemented** (v0.6.0: dual-control leg classification — disposal / acquisition / non-purchase acquisition incl. staking, airdrop, fork — booking as L3 judgments against the tenant's designation policy artifact; rejected proposals book nothing and the leg stays a close blocker). **Ledger integration implemented** (v0.5.0 booking loop: auto-nets book as L2 derived facts keyed by config hash; human resolutions book as L3 judgments with dual-control approvers and the decision hash as evidence) |
| REQ-22 | Asset accounting designation — ASU 2023-08 six-criteria **scope gate** (in-scope proceeds; out-of-scope hard-blocks, never silently mis-valued) | §2.3 | **Implemented** (`treasury-scope` v0.7.0: six mandatory criteria by construction, Undetermined fails closed, unassessed assets block, dual control via generic `treasury-core::dual_control`, both verdicts book as L3 judgments) |
| REQ-23 | Lot / cost-basis engine — per-lot tracking; fees decomposed from basis; capitalize-vs-expense as per-tenant policy election | §2.4 | **Implemented** (`treasury-lots` v0.9.0: integer-only checked arithmetic, exact basis conservation under floor-division partial relief, FIFO/specific-ID as recorded elections with policy hash, fail-closed overdraw, basis- and holding-period-preserving transfers with lineage, `lots_hash()` feeding the valuation key) |
| REQ-24 | Fair-value engine — pure function of `(lots, price-snapshot-hash, policy-hash)` under the tenant's content-addressed principal-market policy | §2.5, §3.5 | **Implemented** (`treasury-fairvalue` v0.10.0: integer-exact prices and floor-division valuation, fail-closed missing prices, currency-strict unrealized marks, content-addressed valuation reports keyed by the R3 triple) |
| REQ-25 | GL output — GAAP/IFRS journal entries via the posting protocol (idempotency keys + read-back verification; GL reconciliation is itself L1 evidence) | §2.6, §3.7 | 1 (GAAP); 3 (IFRS) — **entry generation implemented** (`treasury-gaap` v0.11.0: structurally balanced content-addressed journal entries; ASU 2023-08 remeasurement routed to net income via typed statement-line targets per R11; fee election applied from G-3 decomposition; entries book as L4 policy outputs). **Posting protocol implemented** (`treasury-posting` v0.12.0: batch content hash IS the idempotency key; dual-control release; lost-ack lands in Unknown with only evidence-driven exits — read-back finds it or proves absence for a same-key retry; verification is two-way content equality naming missing/unexpected entries; GL responses and read-backs referenced as L1 evidence). GL vendor adapters (NetSuite/QBO/SAP I/O shims) remaining, driven by design-partner stack (§9) |
| REQ-26 | Disclosure pack + audit trail — quarterly roll-forward, fair-value disclosures, reproducible evidence per number; auditor-facing evidence-reproduction UX | §2.7 | 1 |

## Security baseline (§3.8) — presidio-hardened

| ID | Requirement | Status |
|----|-------------|--------|
| REQ-30 | First-party crates `#![forbid(unsafe_code)]` | **Implemented** (workspace lint) |
| REQ-31 | `cargo-deny` gates licenses, advisories, registry sources in CI | **Implemented** (`deny.toml`) |
| REQ-32 | Tenant isolation by construction — per-tenant keys + event-stream partitions; no cross-tenant query path | Phase 0 ledger enforces per-tenant chains; key/partition isolation Phase 1+ |
| REQ-33 | Insider / supply-chain — signed reproducible builds, SLSA-style provenance, dual-control prod data access, tenant-visible support-access log | Phase 2 |
| REQ-34 | Threat model is a Phase 0 deliverable | **Implemented** ([`docs/threat-model.md`](docs/threat-model.md): STRIDE per trust boundary, operator in scope, standing review triggers) |

## Tenant configuration contract (§6)

- `accounting_standard` ∈ {`gaap`, `ifrs`}; `close_mode` ∈ {`system_of_record`, `feed_gl`}; set once per tenant.
- **Supported-configuration contract:** single legal entity · single standard · no dual reporters, FPIs, or transition-period parallel books. Checked at onboarding; violations rejected loudly. No per-transaction branching.

## Explicitly out of scope for v1 (§2)

Managed custody · payments / payroll · yield & DeFi position management · cross-chain
rebalancing · multi-entity consolidation · tax-lot optimization · accounting for
out-of-ASU-scope assets (designated and blocked, never silently mis-valued) · dual
reporters / FPIs / transition-period parallel books. All ride the same ledger later;
none is required to win the first auditor.

## Versioning

Semantic Versioning. Pre-1.0 (`0.x`): minor versions may carry breaking changes
while the ledger and evidence formats stabilize through Phase 0–1. The workspace
version in `Cargo.toml` is authoritative; releases are gated by **audit reality**,
not feature count (§7 roadmap). Ledger event-identity hashing and canonical-JSON
rules are a compatibility surface: any change to them is a breaking change and ships
with migration + new golden vectors (REQ-7).
