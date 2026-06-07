# Crypto Treasury Suite — v2 Specification

**Status:** Active · single source of truth for v1-product scope and architecture
**Last updated:** 2026-06-07
**Supersedes:** `treasury-suite-spec-v1.md` (retained unmodified)
**Change provenance:** every clause changed from v1 is tagged **▸R-n** against `spec-v1-review-assessment-and-remediations.md`; **▸G-n** marks gaps both adversarial reviews missed.

-----

## 1. Thesis

Build the **audit-grade treasury close** for **crypto-first organizations**, entering through **Digital Asset Treasury companies (DATs)** — public companies holding crypto on the balance sheet.

The deliverable is not a dashboard. It is a **defensible quarterly close an external auditor will sign**. Everything in v1 serves that and nothing else.

### Why DATs first (locked)

- **Reporting pain is regulatory-forced, not optional.** ASU 2023-08 requires fair-value measurement of in-scope crypto every reporting period, with changes hitting the income statement. A CFO, an auditor, and an SEC deadline are attached. People with deadlines pay.
- **Hybrid / BYO custody is their actual posture.** DATs self-custody or use Coinbase Prime / Fireblocks / BitGo / Anchorage and want a *reporting + controls overlay*, not a custody re-platform.
- **The lane is open.** Incumbents are bundling custody + ops and aiming up-market; nobody cleanly owns "audit-grade, fair-value, filing-ready treasury reporting for a listed crypto holder."

Serviceable count is small (~85–175 public crypto holders — **unsourced estimate; validate during design-partner recruitment ▸R-10**), sales cycles are long (3–9 mo), but ACVs are high and retention is structural — once you are embedded in the audit workflow, switching cost is the audit itself.

### Locked decisions

|Decision          |Value                                                            |
|------------------|-----------------------------------------------------------------|
|Primary segment   |DATs (public crypto-holding companies)                           |
|Custody stance    |Hybrid: BYO-custody + optional managed (managed deferred past v1)|
|v1 wedge          |Reporting / accounting / audit                                   |
|Accounting surface|GAAP + IFRS (architecturally day one; IFRS *delivery* in Phase 3)|
|Close model       |System-of-record **and** feed-GL, configurable **per tenant**    |
|Audit posture     |**Phase 1: management's evidence-preparation tool (AS 1105). Phase 2+: relied-upon service organization (AS 2601), gated on SOC 2 Type I. ▸R-1**|
|Implementation    |Rust core; `#![forbid(unsafe_code)]` in all first-party crates; presidio-hardened SDLC|

-----

## 2. Scope

### In scope — the close pipeline

**Seven** ordered stages, each feeding the next: **▸R-5**

1. **Read-only ingestion** — on-chain addresses (xpubs / view keys) + exchange & custodian APIs. Full *transaction history*, not just balances. Read-only is enforced at the network layer, not by venue scope flags (§3.4).
2. **Reconciliation** — classify every transaction; net out **internal transfers** between the org's own venues via the tiered matcher (§5). Non-purchase acquisitions (staking rewards, airdrops, fork proceeds) are designated as first-class judgments, not forced into purchase/disposal shapes. **▸G-1**
3. **Asset accounting designation** — per-asset classification against the six ASU 2023-08 scope criteria (intangible · no enforceable claim on other assets · DLT · cryptographically secured · fungible · not self-issued). v1 ships a **scope gate, not a scope engine**: in-scope assets proceed; stablecoins, wrapped tokens, NFTs, staking claims, and issuer tokens **hard-block with an explicit out-of-scope designation** rather than mis-valuing. **▸R-5**
4. **Lot / cost-basis engine** — per-lot acquisition tracking. Fees are stored **decomposed from basis**; capitalize-vs-expense is a per-tenant policy election applied at the policy layer (ASU 2023-08 is deliberately silent on transaction costs), so either election replays cleanly. **▸G-3**
5. **Fair-value engine** — price each asset at the reporting instant under the tenant's content-addressed **principal-market policy** (§3.5, §9). Valuation is a pure function of `(lots, price-snapshot-hash, policy-hash)`. **▸R-3**
6. **GL output** — GAAP/IFRS journal entries exported via the posting protocol (§3.7) to NetSuite / QuickBooks / SAP.
7. **Disclosure pack + audit trail** — quarterly roll-forward, fair-value disclosures, and reproducible evidence for every number. *This is the product, and it ships in Phase 1.* **▸R-1**

### Explicitly out of scope for v1

Managed custody · payments / payroll · yield & DeFi position management · cross-chain rebalancing · multi-entity consolidation · tax-lot optimization · **accounting for out-of-ASU-scope assets (designated and blocked, never silently mis-valued) ▸R-5** · **dual reporters, foreign private issuers, and transition-period parallel books (rejected at onboarding — §6) ▸R-10**.

All of the above ride the same ledger later. None is required to win the first auditor. **Do not let them into v1.**

-----

## 3. Architecture — load-bearing decisions

### 3.1 Standard-agnostic ledger; opinion at the edges

One ledger + two policy modules + two export adapters collapses the 2×2 of (GAAP|IFRS) × (SoR|feed-GL) into a single core.

```
            ┌──────────────────────────────────────────┐
 Sources →  │ Ingestion (read-only via egress proxy)    │ ▸R-8
            └───────────────────┬──────────────────────┘
                                │ observations
            ┌───────────────────▼──────────────────────┐
            │  LEDGER — bitemporal, event-sourced,      │
            │  append-only, standard-agnostic,          │
            │  CLAIM-LAYERED:                  ▸R-4     │
            │   L1 observations  (raw, hashed)          │
            │   L2 derived facts (code-version hash)    │
            │   L3 judgments     (policy hash+approver) │
            │   L4 policy outputs (pure fn of L1–L3)    │
            └───────┬───────────────────────┬──────────┘
                    │                       │
         ┌──────────▼────────┐   ┌──────────▼─────────┐
         │ GAAP policy module │   │ IFRS policy module │  ← pure functions;
         │ (ASU 2023-08 → NI) │   │ (IAS 38/IAS 2)     │    contract: measurement
         └──────────┬────────┘   └──────────┬─────────┘    basis + statement-line
                    └───────────┬───────────┘              + disclosure set ▸R-11
                    ┌───────────▼───────────┐
                    │ Export adapters        │
                    │ • SoR (we present)     │
                    │ • Feed-GL — separate   │
                    │   egress service ▸R-9  │
                    └────────────────────────┘
```

- **Claim layers (replaces v1's "economic facts only") ▸R-4:**
  - **L1 Observations** — raw venue/chain payloads, content-addressed (§3.3).
  - **L2 Derived facts** — deterministic computations over L1 (address derivation, fee decomposition), each carrying the hash of the code version that produced it.
  - **L3 Judgments** — decisions, not facts: internal-transfer confirmation, ASU scope designation, principal-market election, restriction status, non-purchase-acquisition designation. Each carries policy hash + approver identity. Judgments are events; revising one appends a superseding event.
  - **L4 Policy outputs** — GAAP/IFRS entries and disclosures, pure functions over L1–L3.
  - The standard-agnostic claim survives because L1–L3 are standard-independent; only L4 branches. Every number's provenance is a walk down the layers — which is exactly the auditor walkthrough.
- **Policy-module contract ▸R-11:** modules emit *(measurement basis, statement-line target, disclosure set)* — not routing alone. IAS 2 vs IAS 38 is a measurement-basis difference (IAS 2 broker-trader: fair value less costs to sell through P&L).
- **SoR vs feed-GL = two export adapters.** Ledger is always authoritative internally; the tenant flag sets only *external* authority. The GL writer is a **separate egress service with its own credentials** so write capability never coexists with ingestion credentials and the ingestion plane remains provably read-only. **▸R-9**

### 3.2 Bitemporal, event-sourced ledger

Track **event time** (when it happened on-chain) *and* **knowledge time** (when we booked it). Append-only; corrections are superseding entries, never mutations. Public filers must answer "what did the books say *as of the 10-Q filing*" vs "what we now know" without destroying history. Accounting is already event-sourced — match the grain.

### 3.3 Content-addressed evidence store — integrity *and* independence ▸R-7

Hash every raw input (API payload, price snapshot). Report figures reference the hash. Three properties beyond hashing:

- **External anchoring:** the evidence-store Merkle root is periodically committed to a public chain and/or an RFC 3161 timestamp authority. Tamper-evidence does not depend on trusting us — the insider threat model includes us.
- **Completeness controls:** per venue class, independent dual-source checks (balance assertion vs. transaction-history fold; cross-validation against ≥2 chain nodes or an independent indexer). Scheduled re-fetch + diff detects silent provider revisions; revisions land as new L1 observations and trigger the §3.6 supersession workflow.
- **Canonicalization spec** per provider (JSON normalization, pagination reassembly) so hashes are stable across re-fetches.

The moat is **auditor acceptance of the evidence model and the workflow around it**; hashing is table stakes. **▸R-7 (v1's "moat" claim corrected)**

### 3.4 Read-only by construction at the network layer ▸R-8, ▸R-10

- Venue scope flags are venue-implemented and venue-buggy; they are a hygiene check, not the control. **All venue traffic routes through an internal egress proxy that allowlists endpoint+method pairs known to be read-only.** A trading call cannot leave the network even with a mis-scoped key. The allowlist is versioned config — itself an audit artifact.
- Server-side scope validation at onboarding still rejects any key carrying trade/withdraw permission.
- **xpubs are secrets** — they leak the entire address tree. Derivation happens in an enclave with **remote attestation verified on every derivation session**; rotation and availability model documented when the vendor is chosen (§9). Persist only derived addresses, never the master key.

### 3.5 Policy-as-code, content-addressed ▸R-3

Every accounting-relevant policy is a **versioned, content-addressed, approval-signed artifact**: principal-market price-source policy, fee-treatment election **▸G-3**, per-chain finality/confirmation-depth policy **▸G-5**, FX/reporting-currency translation policy (reserved now, delivered Phase 3) **▸G-4**.

- Valuation memoization key is `(lots, price-snapshot-hash, policy-hash)` — the policy version is part of the function input. **▸R-3 (v1 omitted policy-hash)**
- The principal-market policy is **entity-specific** per ASC 820: the market with the greatest volume and level of activity *that the tenant can access*, presumption toward where it normally transacts. It takes the tenant's actual execution venues as input — never a global venue ranking.
- Defensive layers: fallback hierarchy for stale/halted/inactive markets; cross-venue deviation monitor flagging candidate manipulation or non-orderly prints *before* the snapshot is taken.
- An auditor dispute therefore has an exact, dated object to dispute; a restatement under a corrected policy is a clean re-run, not archaeology.

### 3.6 Incremental close with checkpoint lineage ▸R-6

- Fold the event log into running state; snapshot at period boundaries; replay only deltas. Closes drop from minutes to seconds.
- Closed periods do **not** "lock" — they become **immutable nodes in a checkpoint DAG**. Late data, reorgs, or provider restatements create a successor checkpoint with a recorded supersession edge, a materiality assessment (SAB 99), and a reason code. "As filed" is a permanent pointer to the original node; "as corrected" is the head.
- Reopening is an append, same trust model as everything else. Workflow: detection → materiality memo → recast → disclosure-impact diff.

### 3.7 Feed-GL posting protocol ▸R-9

Not an adapter — a protocol: idempotency keys per journal batch · read-back verification loop (post, re-fetch, compare; the GL reconciliation is itself L1 evidence) · CoA/dimension mapping as versioned, tenant-approved config · client-side approval before any posting · period-lock and failure semantics defined per target GL.

### 3.8 Security architecture (presidio-hardened baseline) ▸R-8

Threat model is a **Phase 0 deliverable**. Floor:

- **Tenant isolation by construction:** per-tenant encryption keys and per-tenant event-stream partitions; no cross-tenant query path exists in the data layer.
- **Insider / supply chain:** signed reproducible builds (Rust core), SLSA-style provenance, dual-control for production data access, support-access transparency log visible to the tenant.
- **Audit-log tamper evidence:** the ledger is the audit log; §3.3 external anchoring covers it.
- **Memory safety:** first-party crates `#![forbid(unsafe_code)]`; money is integer base-units (no floats anywhere in the accounting path).

-----

## 4. Accounting routing (the design driver people miss)

GAAP and IFRS put the **same gain in a different financial statement**. The policy layer controls **measurement basis, statement-line routing, and disclosure set** — not labels. **▸R-11**

|                           |GAAP (ASU 2023-08)      |IFRS (no crypto std)                                             |
|---------------------------|------------------------|-----------------------------------------------------------------|
|Default treatment          |Fair value              |IAS 38 intangible, revaluation model (or IAS 2 if held for sale) |
|Fair-value change routes to|**Net income**          |**OCI / revaluation surplus (equity)** — losses below cost to P&L|
|Implication                |Volatility hits earnings|Volatility largely bypasses earnings                             |

**De-risker:** a public company files under one standard. Standard is per-tenant, not per-transaction. Configurations outside the supported contract (§6) are rejected at onboarding, not approximated. **▸R-10**

-----

## 5. Reconciliation subsystem (the landmine) ▸R-2

**Internal transfers are the hardest correctness problem in v1.** A wallet→exchange move is not a disposal; mis-booking it manufactures phantom P&L in a public filer's income statement.

### Tiered matcher — authority explicit per tier

|Tier|Signal|Authority|
|----|------|---------|
|**0 — deterministic**|Same tx hash observed in both venues' records *and* on-chain|Auto-net, no human|
|**1 — strong corroboration**|Amount−fee match within venue fee model + time window + address linkage from derived xpub tree|Auto-net **below per-tenant materiality threshold**; queue above it|
|**2 — probabilistic**|Everything else|**Never auto-net**; always queued|

### Controls

- **Dual-control confirmation:** client preparer asserts, a second client approver (not the preparer) confirms. **The vendor never classifies — it only presents evidence.** Classification liability stays with management (consistent with §7 Phase 1 posture and §10). ▸R-2/F-3
- **Every match decision is a ledger event** (L3) carrying matcher version hash, tier, confidence, evidence hashes, approver identities. Auditors test the algorithm by replaying the versioned matcher against the period's inputs.
- **Precision/recall SLOs on a labeled corpus** (design-partner history + synthetic adversarial cases: batched withdrawals, fee-on-transfer tokens, exchange-internal moves that never hit chain). Corpus and metrics are audit artifacts.
- **False-negative bias, structurally:** an unmatched internal transfer surfaces as two flagged legs pending classification; it is *impossible* for it to silently book as disposal + acquisition. **Unclassified legs block period close.**
- Non-purchase acquisitions (rewards, airdrops, forks) get their own L3 designation type; income-recognition treatment is a policy-module concern. They arrive in Phase 1 whether or not "yield" is in scope. **▸G-1**

Critical path for Phase 1.

-----

## 6. Tenant configuration model

Two flags, set once per tenant, over shared infrastructure:

|Flag                 |Values                        |Drives                                          |
|---------------------|------------------------------|------------------------------------------------|
|`accounting_standard`|`gaap` \| `ifrs`              |Which policy module is active                   |
|`close_mode`         |`system_of_record` \| `feed_gl`|Which export adapter is authoritative externally|

**Supported-configuration contract ▸R-10:** single legal entity per tenant · single standard · no dual reporters, FPIs, or transition-period parallel books. Checked at onboarding; violations rejected loudly. No per-transaction branching.

-----

## 7. Roadmap

Phases gated by **audit reality**, not feature count. Four phases; nothing new enters v1 scope — remediations reshape stages and add gates. **▸R-1**

### Phase 0 — Foundations (no UI)

Claim-layered bitemporal event-sourced ledger (§3.1) · content-addressed evidence store with anchoring + canonicalization (§3.3) · egress-proxy read-only ingestion (§3.4) · reconciliation scaffolding · **threat model (§3.8)** · **principal-market + finality policies as content-addressed artifacts (§3.5)** · **control-matrix draft + auditor co-design — recruit the design partner's audit firm now, not at close time** · **design-partner constraint validation against §6 before commitment**.
**Exit:** any historical figure reproduces byte-for-byte from hashed inputs **and** the principal-market policy is closed (§9 → resolved).

### Phase 1 — GAAP close, feed-GL mode, 1–2 design-partner DATs

Ingestion → tiered reconciliation with dual-control queue + labeled corpus (§5) → **scope gate (§2.3)** → lot engine → GAAP fair value → posting protocol (§3.7) → **minimal disclosure pack + auditor-facing evidence-reproduction UX** (they are the product).
**Posture:** management's evidence-preparation tool; the client's controller owns and re-performs the close (AS 1105 — the auditor's question is "is this evidence reliable?", which the evidence store answers, not "can we rely on this vendor's controls?", which nothing in Phase 1 answers). **▸R-1**
**Exit:** one quarterly close where the external auditor **accepted our evidence pack under substantive testing, with the client as preparer**. *(Whole-company milestone.)*

### Phase 2 — Audit-grade hardening + system-of-record mode

Full disclosure pack & roll-forward · approvals / segregation-of-duties · SOC 2 Type I · **posture upgrade to relied-upon service organization (AS 2601)** · then SoR close mode.
**Exit:** pass a client's real audit under reliance + land SOC 2.

### Phase 3 — IFRS module + second standard

Activate IFRS policy module (IAS 38 revaluation / OCI routing, IAS 2 path — contract already shaped per §3.1) · **FX/reporting-currency policy delivery ▸G-4** · land first IFRS filer · multi-entity consolidation begins.
**Exit:** an IFRS-filer close accepted.

### Phase 4 — Expand the surface

Deferred items (optional managed custody, payments, yield-position reporting) — each a module on the same ledger, sold into an installed base that already trusts your numbers.

-----

## 8. Critical path & risks

|Risk|Severity|Mitigation|
|----|--------|----------|
|Internal-transfer misclassification leaks phantom P&L into a filer's income statement|**Fatal**|Tiered matcher; never-silent-net by construction; unclassified legs block close; dual-control client confirmation; replayable versioned matcher (§5)|
|Auditor refuses reliance on an untested vendor at first close|**Fatal**|Posture inversion: Phase 1 enters as evidence-preparation tool, not service org; auditor co-design from Phase 0 (§7) ▸R-1|
|Principal-market policy disputed by auditor/SEC|High|Entity-specific policy-as-code, content-addressed, closed in Phase 0; deviation monitors pre-filing (§3.5)|
|IFRS measurement-basis retrofit → rewrite|High|Policy-module contract emits measurement basis + statement-line + disclosures day one (§3.1) ▸R-11|
|Chasing both standards through the first audit|High|IFRS delivery deferred to Phase 3; architecture present day one|
|Mis-scoped venue key enables trading|High|Egress-proxy endpoint allowlist — enforcement independent of venue scope semantics (§3.4) ▸R-8|
|Out-of-ASU-scope asset silently mis-valued|High|Scope gate hard-blocks with explicit designation (§2.3) ▸R-5|
|Silent provider history revision splits the record|Medium-high|Re-fetch + diff; revisions as new L1 events; checkpoint supersession workflow (§3.3, §3.6) ▸R-7|
|GL adapter posts badly on retry|Medium-high|Posting protocol: idempotency + read-back verification (§3.7) ▸R-9|
|xpub leakage (privacy)|Medium|Enclave derivation with per-session attestation; xpubs never persisted (§3.4)|

-----

## 9. Open decisions

- ~~Principal-market price-source policy~~ → **promoted to Phase 0 exit criterion (§3.5, §7)** ▸R-3
- Supported chains & venues for Phase 1 (minimum set to cover design partners).
- GL adapter priority order (NetSuite vs QuickBooks vs SAP) — driven by design-partner stack.
- Enclave technology vendor (requirements fixed in §3.4: per-session remote attestation, documented rotation/availability) ▸R-8.
- Build-vs-buy on raw chain indexing — note third-party indexers become part of the §3.3 completeness/independence surface either way ▸R-7.

-----

## 10. Commercial risk ▸R-10

- **Liability posture:** classification judgments remain management's — §5's dual-control design makes this structurally true, which is what makes the contractual position credible. Liability caps and indemnification defined in design-partner contracts.
- **Insurance:** E&O / tech-liability coverage in place before the first filing-relevant close.
- **Concentration:** the first 1–2 design partners are existential; one restatement attributable to the product likely ends the segment. This is priced into §5's structural false-negative bias — the system prefers asking to guessing.
