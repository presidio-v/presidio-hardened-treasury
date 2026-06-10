# Changelog

All notable changes to **presidio-hardened-treasury** are recorded here. Each
entry describes what changed and why it matters to an operator or an auditor —
not just which files moved.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
The project is pre-1.0 and follows [Semantic Versioning](https://semver.org/)
with the Phase-0 caveat that `0.x` minor versions may carry breaking changes to
the ledger and evidence formats while they stabilise; event-identity hashing and
canonical-JSON rules are a compatibility surface, and any change to them is
breaking and ships with new golden vectors. The authoritative version is the
workspace `[workspace.package].version` in `Cargo.toml`. Releases are not yet
git-tagged; each version below corresponds to the merged change that set it, in
the order it landed on `main`. Dates are omitted rather than fabricated — the
Phase-0 build ran as a continuous sequence, not dated releases.

## [Unreleased]

### Added
- ADR-0005 (authentication and approval identity binding), ADR-0006
  (enclave/HSM vendor for xpub derivation), ADR-0007 (multi-tenant serving layer
  and side-channel posture), and ADR-0008 (IFRS measurement model) added as
  Proposed ADRs under `docs/adr/`. **Why it matters:** the open decisions that
  gate live infrastructure now have named, reviewable decision records rather
  than living only in threat-model prose.
- Property-based tests (`proptest`, dev-only) over the integer invariants the
  whole system rests on: basis conservation under partial lot relief
  (`treasury-lots`), checked-arithmetic-never-panics and canonical-wire
  round-trips for money (`treasury-core`), and value-preserving canonicalization
  for arbitrary float-free JSON (`treasury-evidence`). **Why it matters:** the
  "no rounding leak, no overflow panic, reproduces byte-for-byte" guarantees are
  now checked across thousands of generated cases, not just hand-picked examples.

## [0.21.0] — Anchor confirmation depth as a content-addressed artifact

### Added
- `treasury-anchor::AnchorPolicy`: the anchor confirmation-depth threshold is now
  a content-addressed artifact (ADR-0002 action item 6). `AnchorPipeline::finalize`
  takes an `&AnchorPolicy` instead of a bare depth, and stamps the policy hash into
  every receipt. **Why it matters:** changing the "how many confirmations before
  anchored" threshold now changes the receipt hash, so the threshold an auditor
  relies on is in the audit trail rather than a call-site constant.

### Changed
- ADR action items (ADR-0001…0004) now carry honest status markers — shipped,
  domain-done-awaiting-live-infra, needs-ADR, or not-started — so the decision
  records reflect what is actually built versus what waits on a design partner's
  infrastructure. Added this changelog.

### Breaking
- The anchor receipt envelope gained a `confirmation_policy` field; its schema tag
  is bumped to `treasury-anchor/receipt/v2` and receipt hashes change accordingly
  (new golden vectors shipped). Anchor receipts written under v1 do not re-hash to
  the same value — expected pre-1.0 while the evidence format stabilises.

## [0.20.0] — Conformance harness for the live I/O seams

### Added
- `treasury-conformance`: reusable contract suites every live integration must
  pass before it is trusted. One parameterised `verify_*_contract` per seam —
  chain node+indexer, chain-wallet anchor submitter, GL adapter — encodes the
  invariants the pure core assumes. The same assertions run against the in-memory
  fixtures today and against a real endpoint the day it is wired, so a misbehaving
  shim (a non-deterministic indexer, a wallet reporting an unprovable
  confirmation, a GL that double-posts a retried key) cannot enter the evidence
  path. **Why it matters:** the integrity guarantees an auditor relies on are now
  enforced at the boundary where the system meets untrusted infrastructure.
- `ChainAnchorSubmitter` seam in `treasury-anchor` (broadcast / poll /
  calendar-proof) — names the chain-wallet boundary the live Bitcoin wallet will
  implement and the anchoring pipeline is driven from, with a deterministic fixture.
- Adversarial conformance tests that prove each contract *catches* a violator
  (not only that honest fixtures pass), exercising every failure variant.

## [0.19.0] — Agreed-reconciliation → L1 observation mapping

### Added
- `book::draft_history_observation`: only a two-source-**agreed** chain history
  maps into an L1 observation; a divergence books nothing. Golden-vectored, the
  residual single point across both sources kept minimal (ADR-0004 action item 5).
  **Why it matters:** a disagreement between independent chain sources can never
  silently enter the books — the completeness control reaches all the way to the
  ledger.

## [0.18.0] — Chain-source domain layer (ADR-0004)

### Added
- `treasury-chainsource`: chain-agnostic, integer-only, content-addressed
  `AddressHistory`; finality-gated **two-source reconciliation** (the §3.3
  completeness control — independent sources must hash-match or the address blocks
  close, never auto-reconciled); per-chain `FinalityPolicy` excluding reorg churn;
  and a reproducibility gate that rejects any source whose re-query is not
  byte-identical. Concrete electrs/Fulcrum/reth/Erigon clients remain live I/O
  behind the `ChainSource` trait. **Why it matters:** chain ingestion — the root
  of the entire evidence chain — is verifiable and tamper-evident by construction.

## [0.17.0] — GL adapter contract + orchestration (ADR-0003)

### Added
- `treasury-gl`: a vendor-agnostic `GlAdapter` trait where **read-back is
  mandatory by type** — an adapter that can post but cannot verify what it posted
  cannot exist — plus lifecycle orchestration that drives the posting protocol
  against any adapter (including honest lost-acknowledgement recovery), and a
  `FixtureGl` exercising the whole loop with fault injection. **Why it matters:**
  every journal entry pushed to a client's GL is verification-complete or it is
  not shipped; the rule is enforced by the compiler, not by review.

## [0.16.0] — Anchoring pipeline (ADR-0002)

### Added
- The anchoring pipeline in `treasury-anchor`: Merkle-aggregates many evidence
  tree heads into a single Bitcoin commitment (per-head inclusion proofs preserve
  individual verifiability), models the submission lifecycle as an evidence-driven
  state machine, gates "anchored" on a confirmation-depth threshold, and flags any
  submitted-but-unconfirmed anchor as overdue. **Why it matters:** tamper-evidence
  is committed outside the operator's trust boundary, and an anchor that never
  confirms cannot become a silent coverage gap.

## [0.15.0] — Durable, replay-verified backends

### Added
- `FileLedger` and `FileEvidenceStore`: durable append-only backends that do not
  trust their own files. On open, every ledger record re-runs full validation and
  must reproduce its recorded event id, and every evidence blob is re-verified
  against its recorded hash; a tampered or truncated log refuses to load, and a
  torn tail (crash mid-write) recovers only through an explicit call. **Why it
  matters:** persistence cannot weaken the integrity guarantees that hold
  in-memory.

## [0.14.0] — The golden close (end-to-end)

### Added
- `treasury-e2e`: the full pipeline — ingestion through disclosure pack — run for
  one quarter and asserted to be deterministic: a rerun produces an identical
  disclosure-pack hash. **Why it matters:** "this close reproduces byte-for-byte"
  is proven across every stage, not claimed per component.

## [0.13.0] — Disclosure pack (REQ-26)

### Added
- `treasury-disclosure`: roll-forward rows that structurally cannot fail to roll,
  two-way valuation tie-out that names every break, a content-addressed pack, and
  an evidence-reproduction manifest (the sorted hash closure of everything a number
  depends on). **Why it matters:** citing a pack hash cites the entire close;
  auditing it is fetch-recompute-compare, with no trusted intermediary.

## [0.12.0] — GL posting protocol (REQ-25)

### Added
- `treasury-posting`: content-addressed batches as idempotency keys, dual-control
  release, and a state machine whose only outbound write path resolves a lost
  acknowledgement exclusively through read-back evidence — never a guess. **Why it
  matters:** the client GL cannot be double-posted or left in an unknown state.

## [0.11.0] — GAAP policy module (REQ-25, L4)

### Added
- `treasury-gaap`: structurally balanced journal entries with typed statement-line
  routing (ASU 2023-08 marks → net income); an unbalanced entry is unconstructible.
  **Why it matters:** the accounting output is correct by construction at the type
  level.

## [0.10.0] — Fair-value engine (REQ-24)

### Added
- `treasury-fairvalue`: integer-exact valuation as a pure function of
  `(lots, price-snapshot, policy)`, fail-closed on any missing price. **Why it
  matters:** a valuation is reproducible and can never silently value a held asset
  at zero.

## [0.9.0] — Lot / cost-basis engine (REQ-23)

### Added
- `treasury-lots`: integer-only lots, fees decomposed from basis, relief order as
  a recorded policy election, basis- and holding-period-preserving transfers with
  lineage. **Why it matters:** cost basis is exact and auditable through every
  internal movement.

## [0.8.0] — Labelled corpus + float-free SLO harness (REQ-21)

### Added
- Content-addressed labelled corpora and SLO reports for the reconciliation
  matcher, with exact-rational metrics (no floats) where phantom auto-nets are the
  must-be-zero number. **Why it matters:** matcher quality is measured against a
  fixed, hashed benchmark rather than asserted.

## [0.7.0] — ASU 2023-08 scope gate (REQ-22)

### Added
- `treasury-scope`: a six-criteria scope assessment under dual control; an
  unassessed or out-of-scope asset hard-blocks before valuation. **Why it
  matters:** an out-of-scope asset can never be silently mis-valued into the books.

## [0.6.0] — Non-purchase-acquisition designation (REQ-21, gap G-1)

### Added
- Dual-control leg classification — disposal / acquisition / non-purchase
  acquisition (staking, airdrop, fork) — booked as L3 judgments; a rejected
  proposal books nothing and the leg stays a close blocker. **Why it matters:**
  judgment-laden classifications carry a preparer, a distinct approver, and a
  policy hash on the record.

## [0.5.0] — Reconciliation booking loop (REQ-21)

### Added
- Match decisions become ledger events: auto-nets book as L2 derived facts keyed
  by the matcher-config hash; human resolutions book as L3 judgments with
  dual-control approvers and the decision hash as evidence. **Why it matters:**
  every reconciliation outcome is replayable from hashed inputs.

## [0.4.0] — Reconciliation matcher core (REQ-21)

### Added
- `treasury-reconcile`: a deterministic tiered internal-transfer matcher (discrete
  corroboration classes, no numeric confidence), materiality-gated auto-netting, a
  dual-control confirmation queue, and close blockers for unclassified legs. **Why
  it matters:** unmatched value cannot quietly pass through the close.

## [0.3.0] — External anchoring, read-only ingestion boundary, threat model

### Added
- `treasury-anchor`: content-addressed anchor receipts in a coverage-monotonic log
  with anchored-prefix verification (REQ-8).
- `treasury-ingest`: a content-addressed egress allowlist (deny by default) and
  fail-closed venue key-scope validation, so ingestion is read-only by
  construction (REQ-10).
- The threat model (REQ-34): STRIDE per trust boundary, with the operator inside
  the threat model. **Why it matters:** tamper-evidence does not depend on trusting
  the operator, and the system holds no key that can move funds.

## [0.2.0] — Policy-as-code + checkpoint lineage

### Added
- `treasury-policy`: content-addressed, approval-signed policy artifacts with
  per-tenant activation timelines and the `(lots, price-snapshot-hash, policy-hash)`
  valuation key (REQ-9).
- `treasury-close`: closed periods as an immutable checkpoint DAG; supersession
  requires a reason code and a materiality memo; as-filed vs as-corrected are
  pointers (§3.6). **Why it matters:** "which policy governed at time T" and "what
  did the books say as of the filing" are queries, not reconstructions.

## [0.1.0] — Phase 0 foundations

### Added
- `treasury-core`: domain primitives — identifier newtypes, integer money (no
  floats anywhere in the accounting path), bitemporal timestamps, SHA-256 content
  hashes, and dual control as a structural primitive.
- `treasury-evidence`: a content-addressed evidence store with float- and
  depth-bomb-rejecting canonical JSON and RFC 6962 Merkle tree heads.
- `treasury-ledger`: the claim-layered (L1 observations → L2 derived facts → L3
  judgments → L4 policy outputs), bitemporal, append-only, per-tenant hash-chained
  event ledger; `verify_chain` detects any post-hoc mutation, insertion, or
  deletion, and corrections supersede rather than mutate.
- The hardened SDLC baseline: `#![forbid(unsafe_code)]` workspace-wide; floats,
  unchecked arithmetic, `unwrap`/`expect`/`panic` denied by lint; `cargo-deny`
  (licenses, advisories, registry pinning) and a pinned toolchain in CI; event
  identities cross-verified against an independent Python implementation via golden
  vectors. **Why it matters:** the integrity properties an auditor will scrutinise
  are enforced by construction from the first commit.

[Unreleased]: https://github.com/presidio-v/presidio-hardened-treasury/compare/main...HEAD
