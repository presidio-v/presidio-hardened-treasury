# ADR-0002: Anchor submission target — public chain vs RFC 3161 TSA

**Status:** Accepted
**Date:** 2026-06-07
**Deciders:** CTO, security lead, first design-partner's audit firm (consulted)
**Resolves:** the concrete external-anchoring target for REQ-8 / §3.3 (the `AnchorMethod` choice already modeled in `treasury-anchor`)
**Related:** [ADR-0001](0001-chain-indexing-build-vs-buy.md) (self-hosted Bitcoin node) · threat model B7 (external anchor) · spec §3.3

-----

## Context

`treasury-anchor` (v0.3.0, REQ-8) commits the evidence store's RFC 6962 tree head to a venue **outside our trust boundary** so that tamper-evidence does not depend on trusting the operator — the threat model explicitly puts *us* inside the adversary set (B5 insider), and B7 is the anchor itself. The crate already models two `AnchorMethod` variants — `PublicChain { chain, tx_ref }` and `Rfc3161Tsa { authority, token_hash }` — and verifies a receipt by recomputing the anchored prefix head. What it does **not** yet decide is which target is the default and what each is for. This ADR fixes that.

The property the anchor must deliver (SECURITY.md, §3.3): an auditor verifies the commitment **independently** — "fetch, recompute, compare, with no trusted intermediary." The whole point of anchoring is undone if verifying it requires trusting another party we chose.

Two forces specific to this system shape the answer:

1. **Trust-minimization is the requirement, not a preference.** The reason to anchor at all is that the operator is in the threat model. A target that reintroduces a *new* trusted third party (whose records we'd have to trust for the anchor to mean anything) only moves the trust, it doesn't remove it.

2. **ADR-0001 already commits us to operating a self-hosted Bitcoin full node.** That same node is exactly what is needed to *submit* a Bitcoin-anchored commitment and, more importantly, to *verify* any Bitcoin anchor proof with no third party at all. The infrastructure cost of Bitcoin anchoring is therefore largely **already paid**, and the verification path is self-contained: once the anchoring transaction confirms, the proof upgrades to a calendar-independent form verifiable by anyone with a Bitcoin node ([OpenTimestamps](https://petertodd.org/2016/opentimestamps-announcement)).

Anchoring cadence matters to the trade-off: we anchor a tree head that is **already sealed** (per close, or daily), so anchoring is never on the close's critical path. Latency to confirmation is therefore nearly irrelevant — which neutralizes the main practical advantage of a TSA.

## Decision

**Default to public-chain (Bitcoin) anchoring, OpenTimestamps-style; offer an RFC 3161 TSA token as an optional, additive legal-recognition layer — never as the trust-minimization mechanism.**

- **Primary (trust-minimized): Bitcoin anchoring.** Periodically Merkle-aggregate the tree head(s) and commit the aggregate root in a single Bitcoin transaction, then upgrade the receipt to a **calendar-independent proof** (path to the block header). This is the only target that satisfies *both* "tamper-evidence does not depend on trusting the operator" *and* "verifiable with no trusted intermediary" — the auditor checks it against the public chain (their own node, or ours, or any), not against us. Submission and verification both run through the Bitcoin node ADR-0001 already requires; public OTS calendar servers may be used for aggregation efficiency and redundancy, but only as a *liveness* convenience — the final proof does not trust them.
- **Optional (legal recognition): RFC 3161 TSA token.** Where a tenant's jurisdiction or auditor specifically wants an eIDAS-style qualified timestamp with established PKI and faster formal legal standing, *also* obtain a TSA token over the same tree head and record it as a second `Rfc3161Tsa` receipt. It complements the chain anchor; it never replaces it. The two are not mutually exclusive — `AnchorLog` already accepts multiple receipts, and belt-and-suspenders is the recommended posture for a tenant who wants both trust-minimization and formal recognition.

Only a hash (the tree head / aggregate root) is ever published, never any data, so neither target leaks treasury posture — consistent with the ADR-0001 privacy thesis. The only residual public signal is anchoring *cadence* (that a close happened around a time), which is immaterial.

## Options Considered

### Option A — RFC 3161 TSA as primary

A trusted Time Stamping Authority issues a signed token over the tree head ([RFC 3161](https://en.wikipedia.org/wiki/Trusted_timestamping)).

| Dimension | Assessment |
|-----------|------------|
| Trust model | **Reintroduces a trusted third party** — the TSA is a single point of trust; the anchor means something only if we trust the TSA's records and continued existence |
| Independent verification | Requires the TSA's certificate/PKI chain and its ongoing operation — not self-contained |
| Latency | **Fast** — seconds; but irrelevant, we anchor already-sealed heads |
| Legal standing | **Strong** — clear recognition under eIDAS in some jurisdictions |
| Cost | Low per-stamp fee; established enterprise PKI |
| Longevity | Depends on the authority's survival and record-keeping |

**Pros:** fast, legally well-understood, easy enterprise fit.
**Cons:** structurally at odds with *why we anchor* — it puts trust back into a third party we selected, exactly the move the anchor exists to avoid; "TSAs operate as single points of trust and do not supply a tamper-evident record of event ordering" ([study](https://doi.org/10.3390/app152312722)).

### Option B — Public-chain (Bitcoin) anchoring as primary (**chosen for the default**)

Commit the tree head to Bitcoin via Merkle aggregation; upgrade to a calendar-independent proof ([OpenTimestamps](https://opentimestamps.org/)).

| Dimension | Assessment |
|-----------|------------|
| Trust model | **Trust-minimized** — once confirmed, the proof needs no third party; "outlives any single organization" |
| Independent verification | **Self-contained** — anyone with a Bitcoin node verifies it; we already run one (ADR-0001) |
| Latency | ~10+ min to confirm — but anchoring is off the critical path, so immaterial |
| Legal standing | Less formal statutory recognition than eIDAS-qualified TSAs today |
| Cost | Effectively free to us — calendar servers are free to clients, or one batched tx via our own node; Merkle aggregation amortizes one tx across many heads/tenants |
| Longevity | As durable as Bitcoin itself; proof is operator-independent |

**Pros:** the only option that actually delivers operator-independent, intermediary-free verification; infrastructure already paid for by ADR-0001; cost negligible via aggregation; durable beyond any vendor.
**Cons:** confirmation latency (immaterial here); weaker *formal* legal recognition than a qualified TSA in some jurisdictions; soft liveness dependency on calendar servers (removable — final proof is calendar-independent, and we can submit via our own node).

### Option C — Both: chain primary + optional TSA (**chosen overall**)

Chain anchor as the trust-minimization mechanism for every tenant; an additional TSA token where legal recognition is specifically wanted.

| Dimension | Assessment |
|-----------|------------|
| Trust model | Trust-minimized by default; TSA adds *recognition*, not *trust reduction* |
| Verification | Self-contained chain proof always present; TSA token additionally checkable via PKI |
| Cost | Chain ~free; TSA a small per-stamp fee only for tenants who opt in |
| Complexity | Low — `AnchorLog` already holds multiple receipts; both `AnchorMethod` variants already exist |

**Pros:** keeps the trust-minimization guarantee universal while satisfying a regulator/auditor who wants a qualified timestamp; the two layers are independent and either can be verified alone.
**Cons:** tenants electing both pay a small TSA fee and we operate two submission paths.

## Trade-off Analysis

The decisive axis is the **trust model**, because it is the entire reason anchoring exists. RFC 3161's genuine advantages — speed and statutory recognition — do not bear on the property we need: speed is irrelevant for off-critical-path anchoring, and recognition is about *legal admissibility*, not *tamper-evidence*. Choosing a TSA as the primary anchor would mean the system's tamper-evidence rests on trusting an authority we picked — re-creating in the anchor the very operator-trust the anchor is meant to eliminate. Bitcoin anchoring is the only option whose verification is self-contained and operator-independent, and ADR-0001 has already bought the infrastructure that makes it cheap and the verification path first-party.

That said, legal admissibility is a real, separable need some auditors/jurisdictions will assert, and a qualified TSA answers it directly. The honest synthesis is not to pick one religion but to recognize the two targets answer *different questions* — "can anyone prove this wasn't tampered with, without trusting you?" (chain) versus "is this timestamp recognized under our legal framework?" (TSA) — and to make the trust-minimizing answer the universal default with the legal-recognition answer available on demand.

## Consequences

**Easier:**
- The B7 / §3.3 guarantee becomes real and demonstrable: an auditor verifies tamper-evidence against the public chain with no dependence on us — the strongest possible form of "fetch, recompute, compare."
- Marginal infrastructure cost: the Bitcoin node ADR-0001 already requires is the submission *and* verification engine; Merkle aggregation makes the per-close on-chain cost negligible.
- No new privacy surface: only hashes are published; treasury posture never appears on-chain.

**Harder:**
- We operate an anchoring pipeline: aggregate heads, submit a transaction (or drive a calendar), and **upgrade** receipts to calendar-independent proofs after confirmation — plus monitor that confirmation actually happened (an anchor that never confirms is a silent gap; the coverage-monotonic `AnchorLog` helps, but liveness monitoring is new work).
- Tenants wanting eIDAS-grade recognition require the additional TSA path and its small fee and PKI handling.
- Confirmation latency means a freshly sealed head is "anchoring-pending" for a window; the disclosure/evidence UX must represent that state honestly rather than implying instant immutability.

**To revisit:**
- Whether to run our **own** OTS-style calendar server (removing even the liveness dependency on public calendars) once anchoring volume justifies it.
- Multi-chain anchoring (a second public chain) if a tenant wants redundancy beyond Bitcoin; cheap to add since only a root is published.
- The qualified-TSA vendor selection (a small follow-on decision) if/when the first tenant asserts an eIDAS requirement.

## Action Items

_Status as of v0.20.0 (main): `[x]` shipped · `[ ] *(domain done — awaiting live infra)*` pure-domain half built and tested, live integration pending · `[ ] *(needs ADR before coding)*` blocked on a decision · `[ ]` not started._

1. [ ] *(domain done — awaiting live infra)* Implement the anchoring pipeline as the I/O layer driving `treasury-anchor`: Merkle-aggregate sealed tree heads, submit one Bitcoin transaction per aggregation period, and **upgrade receipts to calendar-independent proofs** post-confirmation. *(`AnchorPipeline` does the aggregation, the submission state machine, and the post-confirmation proof upgrade — v0.16.0; the live Bitcoin wallet behind the `ChainAnchorSubmitter` seam is the remaining I/O.)*
2. [ ] *(domain done — awaiting live infra)* Add **confirmation-liveness monitoring**: an alert when a submitted anchor has not confirmed within N blocks, so a never-confirmed anchor cannot become a silent coverage gap. *(`AnchorPipeline::is_overdue` computes the overdue condition; the alerting loop that polls it is live ops.)*
3. [ ] *(domain done — awaiting live infra)* Submit and verify via the self-hosted Bitcoin node (ADR-0001); treat public OTS calendars as optional redundancy only. *(the `ChainAnchorSubmitter` seam + its `treasury-conformance` contract define what the node integration must satisfy; the node itself is pending.)*
4. [ ] *(domain done — awaiting live infra)* Represent the "anchoring-pending" window honestly in the disclosure/evidence-reproduction UX. *(the window is modeled by `PipelineState::Submitted` + `is_overdue` and a pack carries an optional anchor receipt; the auditor-facing presentation is Phase 1.)*
5. [ ] *(needs ADR before coding)* Specify the optional RFC 3161 path: when a tenant elects it, obtain a TSA token over the same tree head and record a second receipt; document the per-tenant election (mirrors the ADR-0001 optional-tertiary pattern). *(the receipt type exists — `AnchorMethod::Rfc3161Tsa` — but the qualified-TSA vendor and per-tenant election are an open decision.)*
6. [ ] Capture the confirmation-depth used for "anchored" as part of, or alongside, the per-chain finality policy (G-5, §3.5) so the anchoring threshold is itself a documented artifact. *(not started — depth is still a bare `AnchorPipeline::finalize` argument, not yet folded into the content-addressed `FinalityPolicy`; pure-Rust, ready to build.)*

-----

## Sources consulted

- RFC 3161 vs OpenTimestamps trust models, legal standing, trade-offs — [Trusted timestamping (Wikipedia)](https://en.wikipedia.org/wiki/Trusted_timestamping), [Metaspike: RFC 3161 in digital forensics](https://www.metaspike.com/trusted-timestamping-rfc-3161-digital-forensics/), [ProofStamper: blockchain vs RFC 3161 (2026)](https://proofstamper.com/en/compare/file-timestamp-tools-2026/)
- OpenTimestamps design (calendar servers, Merkle aggregation, calendar-independent proofs, confirmation latency, intermediary-free verification) — [Peter Todd: OpenTimestamps announcement](https://petertodd.org/2016/opentimestamps-announcement), [opentimestamps.org](https://opentimestamps.org/), [OpenTimestamps (Wikipedia)](https://en.wikipedia.org/wiki/OpenTimestamps)
- TSAs as single points of trust without tamper-evident ordering; standard-compliant blockchain anchoring of timestamp tokens — [Applied Sciences 2025, 15(23):12722](https://doi.org/10.3390/app152312722)
