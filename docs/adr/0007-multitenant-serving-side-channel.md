# ADR-0007: Multi-tenant serving layer and side-channel posture

**Status:** Proposed
**Date:** 2026-06-10
**Deciders:** CTO, security lead, infra/SRE lead (proposers)
**Resolves:** the threat model B6 residual ("side channels — timing, shared caches — assessed when the serving layer exists, Phase 1") · the serving-layer shape implied by §3.1 tenant isolation
**Related:** [ADR-0001](0001-chain-indexing-build-vs-buy.md) · [ADR-0006](0006-enclave-hsm-xpub-derivation.md) · threat model B6 (tenant ↔ tenant) · `treasury-gl::GlAdapter` · spec §3.1 (per-tenant streams/keys)
**Supersedes:** none

-----

## Context

Tenant isolation today is a **data-layer** property and a strong one: "per-tenant encryption keys and per-tenant event-stream partitions; no cross-tenant query path exists in the data layer" (§3.1), and the threat model confirms B6 is covered *at rest* — "streams, checkpoints, policy timelines, and anchor logs are all keyed by tenant; supersession is structurally tenant-bound." But B6's residual is explicit and deferred: "**Side channels (timing, shared caches) assessed when the serving layer exists (Phase 1).**" There is no serving layer yet, so the residual has never been designed.

When a serving layer lands — the process that answers a tenant's queries and the auditor-facing evidence-reproduction UX — the isolation guarantee has to extend from *data at rest* to *execution*. Two tenants whose data is cryptographically partitioned can still leak across each other if they share a process whose timing, cache state, memory pressure, or error behavior is observable: one fund inferring another fund's holdings, cadence, or close activity from response-time deltas or resource contention is exactly the A1 "treasury posture" leak the whole architecture exists to prevent — arriving through the serving layer instead of through ingestion.

Three forces specific to this system frame the choice:

1. **A1 makes timing a confidentiality channel, not just a performance concern.** Most products treat query timing as a latency-SLA matter; here a timing oracle that reveals "tenant B has many lots / just closed / holds asset X" is a *security* failure against the named primary asset. This raises the bar above the usual multi-tenant SaaS posture.

2. **The adversary may be a tenant, and tenants are few and high-value.** B6's adversary is another tenant (or someone who has compromised one). At Phase-1 scale (1–2 design partners), the cost of strong per-tenant isolation is bounded and the value of each tenant's secrecy is existential (§10 concentration risk) — which, like ADR-0001, inverts the usual "shared is cheaper, ship shared" default toward stronger isolation.

3. **The `GlAdapter` and derivation seams are already per-tenant in spirit.** `treasury-gl::GlAdapter` is `&mut self` and carries per-tenant credentials/connection; ADR-0006's derivation boundary is per-tenant by key. So the serving layer's isolation model has a direct consequence: does each tenant get its own adapter/boundary instance (clean, isolated) or a shared pool keyed by tenant (cheaper, leakier)?

## Decision (proposed)

**Default to process-per-tenant isolation for the serving layer in Phase 1, with per-tenant `GlAdapter`/derivation instances bound to that process** — accept the higher ops cost while the tenant count is tiny and each tenant's secrecy is existential. Layer **timing normalisation, per-tenant resource quotas, and partitioned audit logs** on top regardless of the isolation model, so that even within the strong default there is no resource- or timing-inference path. Re-evaluate a shared-process model (b) only when tenant count grows enough that process-per-tenant ops cost dominates — and only if timing normalisation has been proven sufficient.

- **Request authentication boundary:** every served request authenticates to a single tenant *before* any data-layer access; the tenant identity is established at the edge and is immutable for the request's lifetime, so a request can never widen its tenant scope mid-flight. (Aligns with ADR-0005: officer identity is bound *within* a tenant.)
- **`GlAdapter` instancing:** **one adapter instance per tenant**, owned by that tenant's serving process — never a shared adapter keyed by tenant argument. The per-tenant credential and connection state stay inside the tenant's isolation unit; this is the serving-layer analogue of "no cross-tenant query path in the data layer."
- **Audit-log partitioning:** the support-access transparency log (REQ-33, Phase 2) and operational logs are tenant-partitioned so that observing one tenant's audit/ops stream reveals nothing about another's.

## Options Considered

### Option A — Process-per-tenant (**proposed default**)

Each tenant served by its own process (or stronger: its own container/VM), with its own adapter and derivation handles.

| Dimension | Assessment |
|-----------|------------|
| Isolation strength | **Strongest** — OS-level memory/cache/fault isolation; no shared-process timing oracle |
| Side-channel surface | Smallest — cross-tenant timing/cache inference largely removed by construction |
| Ops cost | **High** — N processes, per-tenant deploy/scale/monitor |
| Fit to Phase-1 scale | Good — N is 1–2; cost bounded |
| `GlAdapter` model | Clean — one instance per process |

**Pros:** matches the existential value of tenant secrecy; the strongest answer to B6's residual; clean adapter/derivation ownership. **Cons:** ops cost grows linearly with tenants — needs the re-evaluation trigger before it becomes the bottleneck.

### Option B — Shared process + DB row-level isolation + query timing normalisation

One serving process for many tenants; isolation enforced by per-tenant keys/row scoping plus deliberate timing normalisation (constant-time-ish responses, padded latencies) and cache partitioning.

| Dimension | Assessment |
|-----------|------------|
| Isolation strength | Moderate — depends on the *correctness* of every normalisation/scoping check |
| Side-channel surface | **Largest** — shared caches, allocator, timing; mitigations are easy to get subtly wrong |
| Ops cost | **Lowest** — one fleet |
| Fit to Phase-1 scale | Overkill-cheap, but premature given A1 |
| `GlAdapter` model | Shared pool keyed by tenant — the leakier option |

**Pros:** cheapest to operate; scales to many tenants. **Cons:** every isolation guarantee becomes a *runtime check* that can regress silently; timing normalisation against a determined tenant-adversary is hard to prove correct — the wrong place to start when A1 is the asset.

### Option C — Separate DB schemas per tenant (shared process)

A middle option: shared serving process, but each tenant's data in its own schema/database rather than row-scoped shared tables.

| Dimension | Assessment |
|-----------|------------|
| Isolation strength | Better than B at the data layer; **same** shared-process timing/cache surface as B |
| Side-channel surface | Process-level still shared — does not address the B6 residual |
| Ops cost | Moderate — schema-per-tenant management |

**Pros:** stronger data-layer separation than row scoping; familiar pattern. **Cons:** leaves the *execution* side channel (the actual B6 residual) unaddressed; solves a problem §3.1 has largely already solved while ignoring the new one.

## Trade-off Analysis

The decisive axis is **execution isolation**, because §3.1 already solved data-at-rest isolation — the open part of B6 is purely the side channel that appears when code *runs* on behalf of tenants. Option C is therefore a near-miss: it strengthens the layer that is already strong and leaves the new gap open. The real choice is A vs B: pay ops cost for OS-level isolation that removes most of the side-channel surface by construction (A), or pay engineering cost to *prove* timing/cache normalisation correct in a shared process against a tenant-adversary (B).

At Phase-1 scale A is cheap enough and B's correctness burden is high enough that A is the right default — the same logic as ADR-0001 (own the boundary while the wedge is small and the asset is existential). The honest caveat is that A does not scale ops-linearly forever; the decision therefore carries an explicit re-evaluation trigger rather than pretending process-per-tenant is the end state. Timing normalisation and per-tenant quotas are *not* an either/or with the isolation model — they are layered on regardless, because even process-per-tenant shares a host and a network whose contention can leak if quotas are absent.

## Consequences

**Easier:**
- B6's residual gets a concrete, defensible answer before the serving layer ships, instead of being discovered in production: cross-tenant timing/cache inference is removed by construction under the default.
- `GlAdapter` and derivation ownership become unambiguous — one instance per tenant, inside the tenant's isolation unit — which also simplifies credential blast-radius reasoning.

**Harder:**
- Process-per-tenant is real ops: per-tenant deploy, scale, monitor, and cost; an explicit trigger and a migration story to a shared model must exist so the default does not silently become the scaling ceiling.
- Timing normalisation and per-tenant resource quotas are new cross-cutting work even under A, with their own correctness tests.
- The request-authentication boundary must be proven to fix tenant scope at the edge for the request's whole lifetime.

**To revisit:**
- The shared-process model (B) when tenant count makes A's ops cost dominate — gated on timing normalisation being demonstrably sufficient.
- Whether the derivation boundary (ADR-0006) and serving process should be co-located per tenant or separated for blast-radius reasons.

## Open Questions

1. **Is tenant isolation a regulatory requirement (DORA? MiCA?) or purely operational?** If a regulation applicable to a design partner mandates a specific isolation/segregation posture, that *raises the floor* and may remove the option to ever move to B — needs a compliance read against each design partner's domicile and activity.
2. What is the concrete side-channel threat model we commit to defend against — a co-tenant measuring response timing over the network, or a stronger co-resident/host-level adversary? (Determines whether host-sharing under A is acceptable or VMs/dedicated hosts are required.)
3. What is the re-evaluation trigger for moving off process-per-tenant — a tenant count, a cost threshold, or a proven-normalisation milestone?

## Action Items

_Proposed; nothing built (no serving layer yet). `[ ]` not started · `[ ] *(needs ADR ratification)*` blocked on this ADR's open questions._

1. [ ] *(needs ADR ratification)* Compliance read: does DORA/MiCA (or another applicable regime) mandate a tenant-isolation posture for any design partner (open question 1)?
2. [ ] Specify the request authentication boundary: tenant identity established at the edge, immutable for the request lifetime, no mid-flight scope widening.
3. [ ] Design per-tenant resource quotas + query timing normalisation as layered controls independent of the isolation model.
4. [ ] Define `GlAdapter`/derivation instancing as one-per-tenant inside the tenant isolation unit; document the anti-pattern (shared adapter keyed by tenant arg).
5. [ ] Partition the support-access transparency log (REQ-33) and ops logs by tenant.
6. [ ] Record the re-evaluation trigger for A→B (open question 3).

-----

## References

- Multi-tenant isolation models (process/pool/silo) and their trade-offs — [AWS SaaS tenant isolation strategies](https://docs.aws.amazon.com/whitepapers/latest/saas-tenant-isolation-strategies/saas-tenant-isolation-strategies.html)
- Timing / micro-architectural side channels as a confidentiality threat — [OWASP: Timing attack](https://owasp.org/www-community/attacks/Timing_attack)
- DORA (operational resilience) and MiCA (crypto-asset markets) applicability — [EU DORA Regulation (EU) 2022/2554](https://eur-lex.europa.eu/eli/reg/2022/2554/oj), [EU MiCA Regulation (EU) 2023/1114](https://eur-lex.europa.eu/eli/reg/2023/1114/oj)
- Existing data-layer isolation — spec v2 §3.1; threat model B6
