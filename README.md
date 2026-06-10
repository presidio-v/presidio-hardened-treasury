# presidio-hardened-treasury

Audit-grade treasury close for crypto-first organizations. Flagship of the
[presidio-hardened](https://github.com/presidio-v) family.

The deliverable is not a dashboard: it is a **defensible quarterly close an
external auditor will sign**. See [`docs/treasury-suite-spec-v2.md`](docs/treasury-suite-spec-v2.md)
(active spec; v1, the two adversarial reviews, and the remediation analysis
that produced v2 are in [`docs/`](docs/)).

## Workspace

| Crate | Purpose |
|-------|---------|
| `treasury-core` | Domain primitives: identifier newtypes, integer money (no floats anywhere in the accounting path), bitemporal timestamps, SHA-256 content hashes |
| `treasury-evidence` | Content-addressed evidence store: float-rejecting canonical JSON, SHA-256 addressing, RFC 6962 Merkle tree heads for external anchoring; durable append-only file backend, hash-verified on open |
| `treasury-ledger` | The core: claim-layered (observations → derived facts → judgments → policy outputs), bitemporal, append-only, hash-chained event ledger; durable file backend, replay-verified on open |
| `treasury-policy` | Policy-as-code: content-addressed, approval-signed policy artifacts; per-tenant activation timelines; the `(lots, price-snapshot, policy)` valuation key |
| `treasury-close` | Checkpoint lineage: closed periods as immutable DAG nodes; supersession requires a reason code + materiality memo; as-filed vs as-corrected as pointers |
| `treasury-anchor` | External anchoring: content-addressed receipts in a coverage-monotonic log; prefix verification detects post-anchor tampering without trusting the operator; Merkle aggregation + Bitcoin submission-lifecycle pipeline (ADR-0002) with inclusion proofs, depth gating, and overdue-liveness |
| `treasury-ingest` | Read-only ingestion boundary: content-addressed egress allowlists (deny by default, no regex), fail-closed venue key-scope validation |
| `treasury-reconcile` | Internal-transfer reconciliation: deterministic tiered matcher (no numeric confidence — discrete corroboration classes), materiality-gated auto-netting, dual-control confirmation queue, close blockers |
| `treasury-scope` | ASU 2023-08 scope gate: six-criteria assessment under dual control; unassessed or out-of-scope assets hard-block before valuation |
| `treasury-lots` | Lot/cost-basis engine: integer-only lots, fees decomposed from basis, relief order as a recorded policy election, basis-preserving internal transfers |
| `treasury-fairvalue` | Fair-value engine: integer-exact valuation as a pure function of `(lots, price-snapshot, policy)`; fail-closed missing prices; content-addressed valuation reports |
| `treasury-gaap` | GAAP policy module (L4): structurally balanced journal entries with typed statement-line routing (ASU 2023-08 marks → net income); fee election applied; entries book as policy outputs |
| `treasury-posting` | GL posting protocol: content-addressed batches as idempotency keys, dual-control release, unknown-outcome retry semantics, two-way read-back verification |
| `treasury-gl` | Vendor-agnostic GL adapter contract (read-back mandatory by type — a post-only adapter cannot exist) and the lifecycle orchestration driving the posting protocol against any adapter; fixture GL for end-to-end tests (ADR-0003) |
| `treasury-chainsource` | Per-chain node+indexer abstraction (ADR-0004): chain-agnostic content-addressed address history; finality-gated two-source reconciliation (the §3.3 completeness control, divergence blocks close); reproducibility gate; **agreed-reconciliation → L1-observation mapping** (golden-vectored, ADR-0004 action item 5); fixture source for tests |
| `treasury-disclosure` | The disclosure pack: roll-forwards that structurally cannot fail to roll, two-way valuation tie-out, content-addressed packs, evidence-reproduction manifest — the product |
| `treasury-e2e` | The golden close: every stage of the pipeline in one quarter, asserting whole-close determinism (identical pack hash on rerun) |
| `treasury-conformance` | Reusable contract suites the live I/O shims must pass: one parameterized `verify_*_contract` per seam (chain source, anchor submitter, GL adapter), run against the fixtures today and against real endpoints when they land — the same assertions, no live service required to define them |

## Structural guarantees (Phase 0)

Enforced at append time, not by review:

- Append-only with per-tenant hash chaining; `verify_chain` detects any
  post-hoc mutation, insertion, or deletion.
- Bitemporal: "what did the books say as of the 10-Q filing" is a query
  (`as_of`), not an archaeology project. Corrections supersede; they never
  mutate, and supersession cannot race, cross tenants, or cross claim layers.
- Layer-specific mandatory provenance: a judgment without an approver and a
  content-addressed policy hash cannot enter the ledger.
- Floats reject at the canonicalization boundary: a value that cannot hash
  identically on every toolchain is not evidence.
- Event identity hashes are cross-verified against an independent Python
  implementation (golden vectors in the test suite).

## Development

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D clippy::all
cargo test --workspace
```

Toolchain is pinned in `rust-toolchain.toml`. All first-party crates are
`#![forbid(unsafe_code)]`; `cargo-deny` gates licenses, advisories, and
registry sources in CI.

Run `cargo fmt --all` before pushing: CI's `cargo fmt --all --check` is the
only formatting gate (there is no auto-formatting bot — a bot commit pushed
with the default token does not re-trigger required checks and leaves a PR
stuck "waiting for status"). Every CI job carries a `timeout-minutes` so a
hung step fails fast instead of sitting for hours.

## Requirements & SDLC

- **Architecture overview:** [`ARCHITECTURE.md`](ARCHITECTURE.md) — the
  claim-layer model, crate layering, the I/O seam/conformance boundary, and
  how the pieces compose into a reproducible close.
- **Requirements baseline:** [`PRESIDIO-REQ.md`](PRESIDIO-REQ.md) — the
  requirements view of the active spec, with delivery status per phase.
- **Active spec (source of truth):** [`docs/treasury-suite-spec-v2.md`](docs/treasury-suite-spec-v2.md).
- **Architecture Decision Records:** [`docs/adr/`](docs/adr/) — significant, hard-to-reverse decisions resolving the spec's §9 open questions.
- Developed under the **presidio-hardened SDLC** (security posture is a design
  constraint, not a backlog). Family SDLC documentation:
  <https://github.com/presidio-v/presidio-hardened-docs>.
- See also [`SECURITY.md`](SECURITY.md) for the hardening baseline and
  vulnerability reporting.
- **Auditor evidence guide:** [`docs/auditor-evidence-guide.md`](docs/auditor-evidence-guide.md)
  — how an external auditor reconstructs and verifies a close from hashed inputs.

## Working with us

The system is complete and verified end to end against deterministic fixtures: the full close runs twice and produces an identical disclosure-pack hash, every hash envelope is cross-checked against an independent implementation, and each live integration point already has a written conformance contract (`treasury-conformance`) a real endpoint must pass. What remains is wiring those seams to production infrastructure — and that is deliberately done with a partner's stack, not simulated in isolation.

We are now selecting a small number of initial design partners for that step. We are specifically looking to work with institutional treasury operators and crypto-native funds carrying digital assets on their balance sheet; regulated financial institutions with crypto exposure and a real audit obligation; the external auditors and accounting firms who will sign the resulting close; the GL vendors whose systems receive the journal entries (NetSuite, SAP, QuickBooks); and the Bitcoin and Ethereum node and indexer infrastructure providers (Core, electrs, Fulcrum, reth, Erigon) who serve the on-chain evidence. Each maps to a defined adapter contract already in the codebase, so engagement is concrete from day one rather than open-ended.

The use cases we want to validate with partners are equally specific: the first live quarterly close run end to end against real holdings and a real GL; an external auditor reconstructing and signing off on the evidence chain from a disclosure-pack hash alone, using the auditor evidence guide; and a multi-entity arrangement where independent operators reconcile against a shared, anchored evidence base. Partners bring the production stack, the holdings, and the audit relationship; we bring an audit-grade close architecture whose integrity properties are enforced by construction and already documented for the people who will scrutinise them.

If your organisation fits one of these and you have a concrete close or audit you want to run against it, reach out to **vladimir@presidio-group.eu**.

## Versioning

[Semantic Versioning](https://semver.org/). The authoritative version is the
workspace `[workspace.package].version` in `Cargo.toml` — single source of
truth, deliberately not repeated here.
Pre-1.0 (`0.x`): minor versions may carry breaking changes while the ledger and
evidence formats stabilize through Phase 0–1; releases are gated by audit reality,
not feature count (roadmap: spec v2 §7). Event-identity hashing and canonical-JSON
rules are a compatibility surface — any change to them is breaking and ships with
new golden vectors. Per-version history is in [`CHANGELOG.md`](CHANGELOG.md).

## License

MIT — see [LICENSE](LICENSE).
