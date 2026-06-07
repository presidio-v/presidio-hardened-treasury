# Spec v1 — Review Assessment & Remediations

**Inputs:** `treasury-suite-spec-v1.md` · GPT-5.5 adversarial review (14 findings) · Grok adversarial review (7 sections)
**Date:** 2026-06-07
**Verdict format:** Accept / Partial / Reject, with the spec delta each remediation requires.

-----

## 1. Review quality assessment

The two reviews converge on the same five fault lines, which raises confidence they are real: audit-reliance sequencing, internal-transfer matching rigor, ASC 820 policy defensibility, the "economic facts only" abstraction, and security depth. The GPT-5.5 review is the stronger artifact — line-anchored, separable findings, correct citations (AS 1105 / AS 2601 / ASU 2023-08 scope criteria / IFRS agenda decision verified). Grok adds three things GPT-5.5 lacks: skepticism that venue "read-only" scopes are trustworthy as reported, the confirmation-bias problem (who confirms ambiguous matches, under what incentives), and the liability/insurance gap. Grok's remainder duplicates GPT-5.5 at lower resolution.

Both reviews share one systematic bias: they push nearly everything into Phase 0/1. Adopting that wholesale recreates the "chase both standards through the first audit" failure mode the spec itself warns about (§8). The correct triage: **architecturally load-bearing items move early; governance items move to where the auditor first touches them; market-validation items are not spec changes.** Section 4 applies that triage.

## 2. Consolidated findings (deduped)

| # | Finding | Source | Verdict | Severity |
|---|---------|--------|---------|----------|
| F1 | Phase 1 demands auditor sign-off before controls/SoD/SOC 2 exist (Phase 2) — sequencing inverted vs AS 1105/AS 2601 | GPT-1, Grok-3 | **Accept** | Fatal |
| F2 | Internal-transfer matching under-specified: no confidence model, FP/FN tolerances, approval rules, auditor-test path | GPT-2, Grok-1 | **Accept** | Fatal |
| F3 | Who confirms ambiguous matches — client treasury has earnings incentives; vendor confirmation imports liability | Grok-1 | **Accept** | Fatal (sub-item of F2) |
| F4 | Principal-market policy is existential, listed as "open decision" | GPT-3, Grok-2 | **Accept** | High |
| F5 | ASU 2023-08 scope is per-asset, not "crypto" — no classification stage exists | GPT-5 | **Accept** | High |
| F6 | "Economic facts only" conflates observations with judgments | GPT-6, Grok-5 | **Accept** | High |
| F7 | Closed-period locks conflict with late data / reorgs / provider restatements | GPT-7, Grok-5 | **Accept** | High |
| F8 | Evidence store proves reproducibility, not completeness/correctness/independence; "moat" claim overstated | GPT-8, Grok-7 | **Accept** (moat reframe: Partial — the moat argument vs custody incumbents stands, but the moat is auditor acceptance, not hashing) | High |
| F9 | Security thin for a presidio-hardened flagship: tenant isolation, insider, audit-log tamper resistance, enclave attestation, supply chain | GPT-9, Grok-4 | **Accept** | High |
| F10 | Venue "read-only" scope reporting cannot be trusted; server-side scope validation is insufficient enforcement | Grok-4 | **Accept** | High |
| F11 | IFRS is not just statement-line routing — IAS 2 path changes measurement basis | GPT-4 | **Partial** — spec already names IAS 2; delta is to widen the policy-module contract beyond routing | Medium |
| F12 | Feed-GL adapter under-modeled; product is no longer read-only once it writes to client GL | GPT-10 | **Accept** | Medium-high |
| F13 | Multi-entity exclusion may break the DAT wedge | GPT-11, Grok-6 | **Partial** — keep exclusion, add design-partner validation gate | Medium |
| F14 | Disclosure pack "is the product" but ships Phase 2 | GPT-12 | **Accept** (folds into F1) | Medium |
| F15 | TAM/retention unsourced; liability caps/insurance/indemnification absent | GPT-13, Grok-6 | **Accept** (commercial section, not architecture) | Medium |
| F16 | Tenant flags break for dual reporters / FPIs / transition periods | GPT-14 | **Accept** — resolve by explicit rejection criteria, not support | Low |

**Rejected claims:** none outright. Closest is Grok's "research-grade problem" framing of F2 — overstated; deterministic tiers cover the large majority of real transfer volume (see R2), the probabilistic tail is what needs the controls.

## 3. Remediations

### R1 — Invert the audit posture (resolves F1, F14)

Do not enter the first audit as a *relied-upon service organization* (AS 2601 — triggers SOC-report demands you can't meet in Phase 1). Enter as **management's evidence-preparation tool**: the client's controller re-performs and owns the close; the product's job is to make every figure independently re-performable from hashed inputs (AS 1105 sufficiency). This makes the auditor's question "is this evidence reliable?" — which the evidence store answers — instead of "can we rely on this vendor's controls?" — which nothing in Phase 1 answers.

Spec deltas:
- §7 Phase 1: rename exit to *"one quarterly close where the auditor accepted our evidence pack under substantive testing, with the client as preparer."* Reliance-as-service-org becomes the Phase 2 exit, gated on SOC 2 Type I.
- §7 Phase 1: move the minimal disclosure pack and the auditor-facing evidence-reproduction UX **into Phase 1** (they are the product; cf. F14). Phase 2 keeps hardening, SoD, approvals, SOC 2.
- §7 Phase 0: add deliverable *"control matrix draft + auditor co-design"* — recruit the design partner's audit firm during Phase 0, not at close time. The first auditor is a design partner too.

### R2 — Specify the reconciliation subsystem as tiers + controls (resolves F2, F3)

Replace "auto-detect" with a tiered matcher where each tier's authority is explicit:

- **Tier 0 (deterministic):** same tx hash observed at both venues' API records and on-chain — auto-net, no human.
- **Tier 1 (strong corroboration):** amount−fee match within venue-specific fee model, time window, and address linkage from the derived xpub tree — auto-net **only below a per-tenant materiality threshold**; above it, queue for confirmation.
- **Tier 2 (probabilistic):** everything else — never auto-net, always queued.

Controls around the queue (this answers Grok's incentive problem):
- Confirmation is **dual-control**: client preparer asserts, second client approver (not the preparer) confirms; the vendor never classifies — it only presents evidence. This keeps classification liability with management, consistent with R1.
- Every match decision (auto or human) is itself a **ledger event** carrying: matcher version hash, tier, confidence, evidence hashes, approver identities. Auditor tests the algorithm by replaying the versioned matcher against the period's inputs — byte-for-byte, same as any other figure.
- Publish **precision/recall SLOs measured on a labeled corpus** (design-partner history + synthetic adversarial cases: batched withdrawals, fee-on-transfer tokens, exchange internal ledger moves that never hit chain). The corpus and metrics are audit artifacts.
- Default bias is **false-negative**: an unmatched internal transfer surfaces as two flagged legs pending classification — it must be impossible for it to silently book as disposal + acquisition. Unclassified legs block period close.

Spec delta: rewrite §5 with the above; add matcher-version to the §3.3 evidence chain.

### R3 — Make policy a content-addressed artifact; close the principal-market decision in Phase 0 (resolves F4)

The spec's purity claim is incomplete: valuation must be a pure function of **(lots, price-snapshot-hash, policy-hash)** — the policy version is missing from the memoization key. Fix that, then:

- Price-source policy is **policy-as-code**: versioned, content-addressed, approval-signed. Every valuation references the policy hash it ran under. An auditor dispute then has an exact, dated object to dispute — and a restatement under a corrected policy is a clean re-run, not archaeology.
- ASC 820's principal market is **entity-specific**: the market with the greatest volume and level of activity *that the entity can access*, with a presumption toward where it normally transacts. The policy must therefore take the tenant's actual execution venues as input, not a global venue ranking. Per-asset, per-tenant determination, documented.
- Defensive layers in the policy: fallback hierarchy for stale/halted/inactive markets; cross-venue deviation monitor that flags candidate manipulation or non-orderly prints *before* the snapshot is taken (detecting the dispute pre-filing instead of receiving it post-filing).
- Move "principal-market policy" from §9 open decisions to a **Phase 0 exit criterion**, co-designed with the design partner's auditor (R1 makes that auditor available).

### R4 — Layer the ledger into claims with provenance (resolves F6, and the substrate for F2/F5)

Replace "economic facts only" with four append-only claim layers, all bitemporal:

1. **Observations** — raw venue/chain payloads (already the evidence store).
2. **Derived facts** — deterministic computations over observations (address derivation, fee decomposition), carrying the code version hash.
3. **Judgments** — classifications that are decisions, not facts: internal-transfer confirmation, ASU scope designation, principal-market election, restriction status. Each carries policy hash + approver identity. Judgments are events; revising one is a new event superseding the old.
4. **Policy outputs** — GAAP/IFRS entries, pure functions over layers 1–3.

This preserves the standard-agnostic core (judgments are standard-independent; only layer 4 branches) while making every number's provenance a walk down the layers — which is exactly the auditor walkthrough. Spec delta: rewrite §3.1's ledger description; the "economic facts" phrase goes.

### R5 — Add an asset-scoping stage before valuation (resolves F5)

New pipeline stage between reconciliation and lot engine: **per-asset accounting designation** against the six ASU 2023-08 criteria (intangible, no enforceable claim on other assets, DLT, cryptographically secured, fungible, not self-issued). Designation is a layer-3 judgment (R4) with evidence.

v1 ships with a **scope gate, not a scope engine**: in-scope assets (BTC, ETH, and whatever the design partners hold that cleanly qualifies) proceed; everything else (stablecoins, wrapped, NFTs, staking claims, issuer tokens) **hard-blocks with an explicit "out of v1 scope" designation** rather than mis-valuing. Honest rejection is audit-defensible; silent misclassification is not. Spec delta: §2 pipeline becomes seven stages; §2 out-of-scope list gains "out-of-ASU-scope asset accounting."

### R6 — Checkpoint lineage instead of locked periods (resolves F7)

Closed periods don't "lock" — they become **immutable nodes in a checkpoint DAG**. Late data, reorgs, or provider restatements create a successor checkpoint with a recorded supersession edge, materiality assessment (SAB 99 reference), and reason code. "As filed" is a permanent pointer to the original node; "as corrected" is the head. Reopening is therefore not an exceptional unlock — it's an append, same trust model as everything else. Add the explicit workflow: detection → materiality memo → recast → disclosure-impact diff. Spec delta: §3.5.

### R7 — Independence and completeness for the evidence store (resolves F8)

Hashing proves integrity, not authority. Add three properties:

- **External anchoring:** periodically commit the evidence-store Merkle root to a public chain and/or RFC 3161 TSA. Tamper-evidence then doesn't depend on trusting us — fitting, since the insider threat (F9) includes us. Crypto-native clients will understand this immediately.
- **Completeness controls:** independent dual-source checks per venue class (balance assertion vs. transaction-history fold; cross-validation against ≥2 chain nodes / independent indexer); scheduled re-fetch + diff to detect silent provider revisions, with revisions landing as new observations (R4 layer 1), surfacing F7's workflow.
- **Canonicalization spec** per provider (JSON normalization, pagination reassembly) so hashes are stable across re-fetches.

Reframe §3.3's moat sentence: the moat is *auditor acceptance of the evidence model*, the hashing is table stakes. Spec delta: §3.3.

### R8 — Egress-proxy enforcement of read-only; security work program (resolves F9, F10)

F10's point is sharp: venue scope flags are venue-implemented and venue-buggy. Don't trust them as the enforcement layer. **Route all venue traffic through an internal egress proxy that allowlists endpoint+method pairs known to be read-only.** A trading call can then not leave the network even with a mis-scoped key. Scope validation at onboarding remains as a hygiene check; the proxy is the control. The proxy's allowlist is versioned config — itself an audit artifact.

Remaining security deltas (new §3.6, threat model as Phase 0 deliverable):
- **Tenant isolation:** per-tenant encryption keys and per-tenant event-stream partitions; no cross-tenant query path in the data layer at all (isolation by construction, matching the spec's own design language).
- **Insider/supply chain:** signed reproducible builds (the Rust core helps), SLSA-style provenance, dual-control for any production data access, support-access transparency log visible to the tenant.
- **Audit-log tamper evidence:** the ledger itself is the audit log; R7's external anchoring covers it.
- **Enclave:** require remote attestation verified on every derivation session; document rotation and availability model when the technology choice (§9) is made — the *requirements* are not open even though the vendor is.

### R9 — Treat feed-GL as a posting protocol, not an adapter (resolves F12)

Idempotency keys per journal batch; read-back verification loop (post, re-fetch, compare — the GL reconciliation is itself evidence); CoA/dimension mapping as versioned, tenant-approved config; client-side approval step before any posting; period-lock and failure semantics defined per target GL. Architecturally: the GL writer is a **separate egress service with its own credentials** so the ingestion plane remains provably read-only — the write capability never coexists with ingestion credentials. Spec delta: §3.1 export adapters, §7 Phase 1.

### R10 — Explicit rejection criteria + commercial risk section (resolves F13, F15, F16)

- §6 gains a **supported-configuration contract**: single legal entity per tenant, single standard, no dual reporters/FPIs/transition-period parallel books in v1 — checked at onboarding, rejected loudly. Phase 0 adds a gate: *validate design partners' actual entity structure and asset list against v1 constraints before commitment* (F13's real fix).
- New §10 (commercial risk): liability caps and indemnification posture in design-partner contracts, E&O/tech-liability insurance before first filing-relevant close, and the explicit position that classification judgments remain management's (R1/R2 make this structurally true, which is what makes the contractual position credible).
- §1: mark the ~85–175 TAM figure as unsourced; validate during design-partner recruitment. Not an architecture item.

### R11 — Widen the policy-module contract (resolves F11)

The module contract emits *(measurement basis, statement-line target, disclosure set)* — not routing alone. IAS 2 vs IAS 38 is a measurement-basis difference (and IAS 2 broker-trader is FV-through-P&L, ironically GAAP-like). The de-risk stays the same (IFRS delivery Phase 3), but the contract shape is day-one. Spec delta: §4 consequence paragraph.

## 4. What both reviews missed

1. **Rewards/airdrops/forks hit ingestion in Phase 1 regardless of scope.** "Yield management" is excluded, but a design partner's wallet *will* receive staking rewards or an airdrop. Without a designation path these become unclassifiable observations that block close (per R2's bias). Needs at minimum a layer-3 judgment type "non-purchase acquisition" with income-recognition treatment deferred to the policy modules. The reviews critiqued scope but missed that the data arrives whether or not it's in scope.
2. **The memoization key omission** — valuation defined as pure over `(lots, price-snapshot-hash)` without the policy hash (fixed in R3). Both reviews quoted the line; neither caught it.
3. **Transaction-cost treatment.** ASU 2023-08 is deliberately *silent* on acquisition costs (the Board considered mandating expensing and declined); in the absence of guidance, practice diverges between expensing and capitalizing per ASC 350-30-30-1. That makes fee treatment a documented per-tenant policy election — a layer-4 policy decision with an R3-style hash, not a lot-engine constant. The lot engine must store fees decomposed from basis so either election replays cleanly. Affects R4 layering.
4. **FX/reporting currency.** Price sources are USD-dominated; an IFRS filer (Phase 3) reports in EUR/other. Translation policy (rate source, timestamp alignment with the price snapshot) is a second policy artifact with the same R3 treatment. Cheap to reserve now, painful to retrofit.
5. **Finality policy as audit artifact.** Reorg handling was mentioned (F7), but no review asked for a documented, per-chain confirmation-depth/finality policy — which the auditor will. It is a layer-2/3 policy with a hash, same machinery as R3.

## 5. Revised phase gating (net effect)

- **Phase 0 adds:** claim-layered ledger (R4) · principal-market + finality policies as content-addressed artifacts (R3) · egress proxy + threat model (R8) · control-matrix draft + auditor co-design (R1) · design-partner constraint validation (R10).
- **Phase 1 adds:** scope gate (R5) · tiered matcher with dual-control queue + labeled corpus (R2) · minimal disclosure pack + evidence-reproduction UX (R1/F14) · GL posting protocol (R9). Exit rewritten per R1.
- **Phase 2 keeps:** SoD/approvals hardening, SOC 2 Type I, SoR mode — *plus* the posture upgrade to relied-upon service organization.
- **Phase 3 unchanged** (IFRS delivery; contract already widened per R11; FX policy lands here).
- **Phases stay four; nothing new enters v1 scope** — every remediation reshapes existing stages or adds gates, which is the answer to both reviews' Phase 0 inflation bias.
