# Crypto Treasury Suite — v1 Specification

**Status:** Ideation · single source of truth for v1 scope and architecture
**Last updated:** 2026-06-07
**Owner:** *(you)*

-----

## 1. Thesis

Build the **audit-grade treasury close** for **crypto-first organizations**, entering through **Digital Asset Treasury companies (DATs)** — public companies holding crypto on the balance sheet.

The deliverable is not a dashboard. It is a **defensible quarterly close an external auditor will sign**. Everything in v1 serves that and nothing else.

### Why DATs first (locked)

- **Reporting pain is regulatory-forced, not optional.** ASU 2023-08 requires fair-value measurement of crypto every reporting period, with changes hitting the income statement. A CFO, an auditor, and an SEC deadline are attached. People with deadlines pay.
- **Hybrid / BYO custody is their actual posture.** DATs self-custody or use Coinbase Prime / Fireblocks / BitGo / Anchorage and want a *reporting + controls overlay*, not a custody re-platform.
- **The lane is open.** Incumbents are bundling custody + ops and aiming up-market; nobody cleanly owns “audit-grade, fair-value, filing-ready treasury reporting for a listed crypto holder.”

**Serviceable count is small (~85–175 public crypto holders), sales cycles are long (3–9 mo), but ACVs are high and retention is structural** — once you are the auditor’s source of truth, switching cost is the audit itself.

### Locked decisions

|Decision          |Value                                                            |
|------------------|-----------------------------------------------------------------|
|Primary segment   |DATs (public crypto-holding companies)                           |
|Custody stance    |Hybrid: BYO-custody + optional managed (managed deferred past v1)|
|v1 wedge          |Reporting / accounting / audit                                   |
|Accounting surface|GAAP + IFRS (architecturally day one; IFRS *delivery* in Phase 3)|
|Close model       |System-of-record **and** feed-GL, configurable **per tenant**    |

-----

## 2. Scope

### In scope — the close pipeline

Six ordered stages, each feeding the next:

1. **Read-only ingestion** — on-chain addresses (xpubs / view keys) + exchange & custodian APIs. Full *transaction history*, not just balances.
1. **Reconciliation** — classify every transaction; net out **internal transfers** between the org’s own venues. (First-class subsystem — see §5.)
1. **Lot / cost-basis engine** — per-lot acquisition tracking (needed for tax, comparatives, roll-forward even under fair value).
1. **Fair-value engine** — price each asset at the reporting instant from a single documented **principal-market source policy** (ASC 820). The price-source policy is an audit artifact.
1. **GL output** — GAAP/IFRS journal entries (incl. periodic mark-to-market) exported to NetSuite / QuickBooks / SAP.
1. **Disclosure pack + audit trail** — quarterly roll-forward, fair-value disclosures, and reproducible evidence for every number. *This is the product.*

### Explicitly out of scope for v1

Managed custody · payments / payroll · yield & DeFi position management · cross-chain rebalancing · multi-entity consolidation · tax-lot optimization.

All of the above ride the same ledger later. None is required to win the first auditor. **Do not let them into v1.**

-----

## 3. Architecture — load-bearing decisions

### 3.1 Standard-agnostic ledger; opinion at the edges

The pattern that makes “GAAP + IFRS” and “SoR + feed-GL” survivable instead of fatal. It collapses a 2×2 of four products into **one ledger + two policy modules + two export adapters.**

```
            ┌─────────────────────────────────────────┐
 Sources →  │  Ingestion (read-only, privilege-reject) │
            └───────────────────┬─────────────────────┘
                                │ economic facts only
            ┌───────────────────▼─────────────────────┐
            │   LEDGER  (bitemporal, event-sourced,     │  ← always the
            │   append-only, standard-agnostic)         │     internal SoR
            └───────┬───────────────────────┬──────────┘
                    │                       │
         ┌──────────▼────────┐   ┌──────────▼─────────┐
         │ GAAP policy module │   │ IFRS policy module │   ← pure functions
         │ (ASU 2023-08 → NI) │   │ (IAS 38 → OCI…)    │     over the ledger
         └──────────┬────────┘   └──────────┬─────────┘
                    └───────────┬───────────┘
                    ┌───────────▼───────────┐
                    │   Export adapters       │
                    │  • SoR (we present)     │
                    │  • Feed-GL (NetSuite…)  │
                    └─────────────────────────┘
```

- **Ledger** stores only economic facts (“acquired 10 BTC at T, cost basis Y, from venue Z”). No notion of GAAP/IFRS or journal-entry shape. Immutable, bitemporal, the internal system of record **always**.
- **GAAP / IFRS = two policy modules**, pure functions over the ledger emitting entries + disclosures. Swap the lens; never branch the core.
- **SoR vs feed-GL = two export adapters**, not two systems. Ledger is always authoritative internally; the flag only sets the *external* authority.

### 3.2 Bitemporal, event-sourced ledger

Track **event time** (when it happened on-chain) *and* **knowledge time** (when we booked it). Append-only; corrections are reversing entries, never mutations.

- Public filers must restate prior periods when late data arrives. Bitemporal answers “what did the books say *as of the 10-Q filing*” vs “what we now know” without destroying history.
- Accounting is already an event-sourced domain (debits/credits are never deleted) — we match the grain rather than fight it.

### 3.3 Content-addressed evidence store

Hash every raw input (API payload, price snapshot). Report figures **reference the hash**.

- Any number an auditor challenges *years later* reproduces byte-for-byte from immutable hashed inputs.
- This is the reason a public company picks us over a spreadsheet, and a **moat** incumbents bolting reporting onto custody cannot easily copy.

### 3.4 Privilege rejection at the ingestion boundary

- Validate API-key scopes **server-side** and *reject* any key carrying trade/withdraw permission. Accept only read scopes and public addresses. The catastrophic attack surface is eliminated **by construction**, not by policy.
- Treat **xpubs as secrets** — they leak the entire address tree (privacy). Derive addresses in an enclave; persist only derived addresses, never the master key.

### 3.5 Incremental close with period checkpoints

- Don’t replay all history each quarter. Fold the event log into running state; snapshot at each **closed** period; replay only the delta.
- Closed periods lock. Closes drop from minutes to seconds.
- Valuation is a **pure function of `(lots, price-snapshot-hash)`**, so it memoizes for free and is trivially reproducible.

-----

## 4. Accounting routing (the design driver people miss)

GAAP and IFRS do not merely differ in disclosure wording — they put the **same gain in a different financial statement**. The policy layer must control **statement-line routing**, not labels.

|                           |GAAP (ASU 2023-08)      |IFRS (no crypto std)                                             |
|---------------------------|------------------------|-----------------------------------------------------------------|
|Default treatment          |Fair value              |IAS 38 intangible, revaluation model (or IAS 2 if held for sale) |
|Fair-value change routes to|**Net income**          |**OCI / revaluation surplus (equity)** — losses below cost to P&L|
|Implication                |Volatility hits earnings|Volatility largely bypasses earnings                             |

**Consequence for the build:** design the policy module to emit *statement-line targets*, not just amounts and labels, on day one — otherwise the IFRS module becomes a rewrite.

**De-risker:** a public company files under **one** standard. Standard is a **per-tenant setting, not per-transaction** — no combinatorial explosion. Close mode is likewise a tenant flag over shared infrastructure.

-----

## 5. Reconciliation subsystem (the landmine)

**Internal transfers are the hardest correctness problem in v1 and the thing that earns or loses auditor trust.**

A wallet→exchange move is **not a disposal**. Mis-booking it manufactures phantom gains/losses that flow straight to a *public company’s income statement* under fair-value rules.

Requirements:

- Auto-detect same-owner transfers across heterogeneous venues with **no shared identifier**, differing timestamps, and fees taken mid-flight.
- Treat as a **first-class subsystem**, not a heuristic buried in classification.
- Every netting decision must be backed by the §3.3 evidence store so it is independently auditable.
- Surface ambiguous matches for human confirmation with full evidence; never silently net.

This subsystem is on the **critical path** for Phase 1.

-----

## 6. Tenant configuration model

Two flags, set once per tenant, over shared infrastructure:

|Flag                 |Values                        |Drives                                          |
|---------------------|------------------------------|------------------------------------------------|
|`accounting_standard`|`gaap` | `ifrs`               |Which policy module is active                   |
|`close_mode`         |`system_of_record` | `feed_gl`|Which export adapter is authoritative externally|

No per-transaction branching. No combinatorial product matrix.

-----

## 7. Roadmap

Phases are gated by **audit reality** — “an auditor accepted a close” and “passed SOC 2” — not feature count.

### Phase 0 — Foundations (no UI)

Standard-agnostic bitemporal event-sourced ledger · content-addressed evidence store · read-only ingestion with privilege rejection · reconciliation subsystem scaffolding.
**Exit:** any historical figure reproduces byte-for-byte from hashed inputs.

### Phase 1 — GAAP close, feed-GL mode, 1–2 design-partner DATs

Ingestion → internal-transfer reconciliation → lot engine → GAAP fair value → journal-entry export to NetSuite/QuickBooks. Feed-GL first: client keeps their GL, so the trust ask is small and you get in the door.
**Exit:** one real quarterly close signed off by an external auditor. *(Whole-company milestone.)*

### Phase 2 — Audit-grade hardening + system-of-record mode

Disclosure pack & roll-forward · auditor-facing evidence-reproduction UX · approvals / segregation-of-duties · SOC 2 Type I · then SoR close mode.
**Exit:** pass a client’s real audit + land SOC 2.

### Phase 3 — IFRS module + second standard

Activate IFRS policy module (IAS 38 revaluation / OCI routing, IAS 2 path) · land first IFRS filer · multi-entity consolidation begins.
**Exit:** an IFRS-filer close accepted.

### Phase 4 — Expand the surface

Deferred items (optional managed custody, payments, yield-position reporting) — each a module on the same ledger, sold into an installed base that already trusts your numbers.

-----

## 8. Critical path & risks

|Risk                                                                                                  |Severity                     |Mitigation                                                                             |
|------------------------------------------------------------------------------------------------------|-----------------------------|---------------------------------------------------------------------------------------|
|Internal-transfer reconciliation leaks phantom P&L into a filer’s income statement                    |**Fatal** (ends relationship)|First-class subsystem from Phase 0; evidence-backed; human confirm on ambiguity        |
|IFRS statement-line routing retrofitted late → rewrite                                                |High                         |Policy module emits statement-line targets on day one                                  |
|Chasing both standards through the first audit doubles validation surface when you can least afford it|High                         |IFRS *delivery* deferred to Phase 3; architecture present day one                      |
|Price-source policy disputed by auditor                                                               |High                         |Single documented principal-market policy per ASC 820; snapshot + hash every price used|
|xpub leakage (privacy)                                                                                |Medium                       |xpubs treated as secrets; address derivation in enclave                                |

-----

## 9. Open decisions (pending)

- Principal-market price-source policy: which venues, tie-breaking, stale-price handling.
- Supported chains & venues for Phase 1 (minimum set to cover design partners).
- GL adapter priority order (NetSuite vs QuickBooks vs SAP) — driven by design-partner stack.
- Enclave technology choice for xpub handling.
- Build-vs-buy on raw chain indexing (ingestion) vs third-party data providers.
