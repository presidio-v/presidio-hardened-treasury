# ADR-0008: IFRS measurement model — IAS 38 cost vs revaluation

**Status:** Proposed
**Date:** 2026-06-10
**Deciders:** CTO, design-partner's controller + audit firm (to be consulted before ratification)
**Resolves:** the single IFRS measurement model the prospective `treasury-ifrs` policy module must implement — shapes lot relief, unrealised-P&L computation, and the policy-module contract for IFRS
**Related:** [ADR-0003](0003-gl-adapter-priority.md) (posting protocol) · `treasury-gaap` · `treasury-fairvalue` · `treasury-lots` · spec §3.1 (policy-module contract ▸R-11), §8 Phase 3 (IFRS delivery)
**Supersedes:** none

-----

## Context

The policy-module contract is already shaped for two standards: modules emit *(measurement basis, statement-line target, disclosure set)* (§3.1 ▸R-11), and the spec records the GAAP/IFRS split — "GAAP and IFRS put the **same gain in a different financial statement**." For GAAP this is settled: **FASB ASU 2023-08** mandates fair-value measurement of in-scope crypto each reporting period with changes to **net income**, effective for fiscal years beginning after 15 Dec 2024 (i.e. 2025) — this is the "2025 fair-value" change, and `treasury-gaap` + `treasury-fairvalue` already implement it.

For IFRS the picture is different and a model must be *chosen*. **There is no IFRS crypto-specific standard.** The 2019 IFRS Interpretations Committee agenda decision holds that holdings of cryptocurrency are, in most cases, **intangible assets under IAS 38** (or inventory under **IAS 2** for broker-traders / held-for-sale). IAS 38 then offers two mutually exclusive measurement models, and the codebase must pick exactly one because the downstream logic differs materially:

- **Cost model** — carry at cost less accumulated impairment; **no upward revaluation**; impairment losses to P&L, limited reversals.
- **Revaluation model** — carry at fair value (only permitted when an active market exists, which crypto with a principal market satisfies); increases to **OCI / revaluation surplus (equity)**, decreases first against that asset's surplus then to P&L.

The spec's current *lean* is the revaluation model — "IAS 38 intangible, revaluation model (or IAS 2 if held for sale) … fair-value change routes to OCI / revaluation surplus (equity), losses below cost to P&L" (§3.1 table). This ADR exists to ratify that lean (or overturn it) deliberately, because the choice is **not** cosmetic: it changes the lot data model, the unrealised-P&L computation, and the `treasury-ifrs` crate's shape.

> **Faithfulness note on "the IASB amendment effective 2025-01-01."** The 2025 fair-value mandate is **US GAAP (FASB ASU 2023-08)**, *not* an IFRS/IASB amendment — the IASB has issued no crypto-specific standard. If the request's "IASB amendment effective 2025-01-01" refers to something a design partner has cited, that needs confirming (open question 1); this ADR assumes the standing IFRS position (IAS 38 / IAS 2) unless that confirmation produces a genuine new standard.

## Decision (proposed)

**Implement the IAS 38 *revaluation* model as the IFRS default in `treasury-ifrs`, with the IAS 2 broker-trader path as a separate election, and lock the measurement model per entity (per tenant), not per transaction.** This ratifies the spec's existing lean and keeps the IFRS module a pure function over L1–L3 with a fair-value snapshot, mirroring the GAAP module.

- **Revaluation model (default):** at each reporting period, mark each in-scope lot to fair value from the principal-market price source (the same `treasury-fairvalue` snapshot GAAP uses). Increases above cost → **OCI / revaluation surplus**; decreases → against that asset's surplus first, then **P&L**. This requires a **periodic mark-to-market snapshot per lot per reporting period**, retained as an L-layer input so the entry replays.
- **Cost model (alternative, must remain implementable):** carry at cost less impairment; no upward marks; impairment to P&L. Cheaper on data (no periodic mark needed for measurement, only an impairment trigger test) but produces a balance sheet that ignores gains — rarely what a crypto-holding filer wants, hence not the default.
- **Per-entity lock:** the model is an `accounting_standard`-adjacent per-tenant election (§6), rejected-at-onboarding if inconsistent, never per-transaction — consistent with "Standard is per-tenant, not per-transaction" (§4 de-risker).

The non-obvious core: the GAAP (NI) and IFRS-revaluation (OCI) modules share **the same fair-value snapshot and the same lot engine** and differ only in *statement-line routing* — which is exactly what the §3.1 policy-module contract was designed to absorb. Choosing the **cost** model instead would break that symmetry (no periodic mark, an impairment-test branch, different lot carrying values) and is the more invasive design, not the simpler one.

## Options Considered

### Option A — IAS 38 revaluation model (**proposed default**)

| Dimension | Assessment |
|-----------|------------|
| Balance-sheet faithfulness | **High** — carries holdings at fair value |
| Symmetry with GAAP module | **High** — same fair-value snapshot + lot engine; differs only in routing (OCI vs NI) |
| Lot data model | Needs **periodic mark-to-market snapshot per lot per period** (already produced for GAAP) |
| Journal pattern | Dr asset / Cr revaluation surplus (OCI) on increase; reverse then P&L on decrease |
| Auditor familiarity | Established for assets with an active market |

**Pros:** matches the spec lean; reuses GAAP's fair-value machinery; gives filers a fair-value balance sheet. **Cons:** OCI/revaluation-surplus tracking per asset (surplus balance must be carried so a later decrease hits OCI before P&L) — real state to model in `treasury-ifrs`.

### Option B — IAS 38 cost model

| Dimension | Assessment |
|-----------|------------|
| Balance-sheet faithfulness | **Low** — no upside; gains invisible until disposal |
| Symmetry with GAAP module | **Low** — no periodic measurement mark; impairment-test branch instead |
| Lot data model | No periodic mark for measurement; needs an impairment-trigger/indicator test |
| Journal pattern | Impairment loss to P&L; limited reversal |
| Auditor familiarity | Simple, conservative |

**Pros:** least data (no periodic mark); conservative. **Cons:** diverges structurally from the GAAP module (breaks the shared-snapshot symmetry); produces a balance sheet most crypto-holding filers and their investors find unhelpful; rarely the elected model in practice.

### Option C — IAS 2 (inventory, broker-trader) as the IFRS path

For entities that are commodity broker-traders / hold for sale: fair value **less costs to sell**, changes through **P&L** (§3.1 already notes this).

| Dimension | Assessment |
|-----------|------------|
| Applicability | Narrow — only broker-traders / held-for-sale, not a typical treasury holder |
| Routing | P&L (like GAAP), not OCI |
| Lot data model | Fair-value snapshot + costs-to-sell |

**Pros:** correct for the broker-trader fact pattern; routes like GAAP. **Cons:** wrong for the typical DAT/treasury holder (who is not a broker-trader); kept as a **separate election**, not the IFRS default.

## Trade-off Analysis

The decisive axis is **whether the IFRS module can share the GAAP module's measurement machinery**, because the architecture's whole IFRS bet (§3.1 ▸R-11, §8) is "one ledger + two policy modules differing only in measurement basis / statement-line / disclosures." The revaluation model (A) honours that bet: same `treasury-fairvalue` snapshot, same `treasury-lots` engine, the only difference is routing the period's fair-value delta to OCI/revaluation surplus instead of NI — plus carrying a per-asset surplus balance so a subsequent decline unwinds OCI before touching P&L. The cost model (B) is the *more* invasive choice despite sounding simpler: it removes periodic measurement, adds an impairment-indicator branch, and gives lots a different carrying basis than the GAAP path — forking the lot model the architecture deliberately kept single.

So the honest framing inverts the naive "cost is simpler": for *this* codebase, revaluation is the lower-divergence design and the one the spec already anticipated, while cost is the special case that would cost more to bolt on. IAS 2 (C) is orthogonal — a narrow broker-trader election, not a competitor to A for the typical holder. The remaining real risk is that the *auditor or domicile* may mandate cost (some jurisdictions/entities restrict revaluation), which is why this is Proposed and gated on open questions 2–3 rather than ratified outright.

## Consequences

**Easier:**
- The IFRS module (Phase 3) becomes a thin routing variant of the proven GAAP module rather than a parallel measurement engine — exactly the §3.1 contract's promise.
- Unrealised P&L computation reuses the GAAP fair-value snapshot; only the destination (OCI vs NI) and the surplus-tracking differ.

**Harder:**
- `treasury-ifrs` must carry a **per-asset revaluation-surplus balance** so a decrease correctly hits OCI down to zero surplus, then P&L — new state with its own replay/golden-vector requirements.
- A periodic **mark-to-market snapshot per lot per reporting period** must be a retained L-layer input (it already exists for GAAP, but IFRS makes its retention contractually required for replay).
- The cost model must remain *implementable* (some entity may be forced to it) without being the default — a second code path that cannot be dropped entirely.

**To revisit:**
- If a design partner's auditor mandates the cost model, whether to flip the default or support both per-entity from the start.
- Disclosure set divergence (IAS 38 revaluation disclosures vs ASU 2023-08 disclosures) — a follow-on for the disclosure pack.

## Open Questions

1. **What does "the IASB amendment effective 2025-01-01" refer to?** The 2025 fair-value mandate is US GAAP (ASU 2023-08); the IASB has issued no crypto standard. If a design partner has cited a specific IFRS development, it must be confirmed before this ADR is ratified — otherwise the standing IAS 38 / IAS 2 position governs.
2. **Has the auditor confirmed which model applies** (revaluation vs cost) for the first IFRS filer?
3. **Does the fund's domicile mandate one model?** Some jurisdictions constrain use of the IAS 38 revaluation model; the per-entity lock must respect that.
4. Is the per-asset revaluation surplus tracked per *lot* or per *asset* (aggregated across lots)? IAS 38 revaluation is asset-class level; reconciling that with a lot engine needs a defined rule.

## Action Items

_Proposed; nothing built (`treasury-ifrs` does not exist — Phase 3). `[ ]` not started · `[ ] *(needs ADR ratification)*` blocked on this ADR's open questions._

1. [ ] *(needs ADR ratification)* Resolve open question 1 (what standard/amendment the partner means) and 2 (auditor's confirmed model) — these gate the default.
2. [ ] Specify the IFRS-revaluation routing in the §3.1 policy-module contract terms: measurement basis = fair value, statement-line = OCI/revaluation surplus, disclosure set = IAS 38 revaluation.
3. [ ] Define the per-asset revaluation-surplus state and the decrease-unwinds-OCI-then-P&L rule (open question 4: lot vs asset granularity).
4. [ ] Confirm the periodic mark-to-market snapshot is a retained, replayable L-layer input for IFRS (reusing the GAAP `treasury-fairvalue` snapshot).
5. [ ] Keep the cost model implementable behind the per-entity election, even though revaluation is the default.

-----

## References

- IFRS IC agenda decision: holdings of cryptocurrencies are IAS 38 intangibles (or IAS 2 inventory for broker-traders) — [IFRS: Holdings of Cryptocurrencies (June 2019)](https://www.ifrs.org/content/dam/ifrs/supporting-implementation/agenda-decisions/2019/holdings-of-cryptocurrencies-june-2019.pdf)
- IAS 38 cost vs revaluation models — [IAS 38 Intangible Assets (IFRS)](https://www.ifrs.org/issued-standards/list-of-standards/ias-38-intangible-assets/)
- IAS 2 inventories (broker-trader fair-value-less-costs-to-sell) — [IAS 2 Inventories (IFRS)](https://www.ifrs.org/issued-standards/list-of-standards/ias-2-inventories/)
- US GAAP fair-value mandate (contrast; the "2025" change) — [FASB ASU 2023-08, Crypto Assets (Subtopic 350-60)](https://fasb.org/page/document?pdf=ASU+2023-08.pdf)
- Policy-module contract / GAAP-IFRS split — spec v2 §3.1 (▸R-11), §4, §8
