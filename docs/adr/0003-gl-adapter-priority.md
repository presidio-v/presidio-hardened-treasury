# ADR-0003: GL adapter priority order — NetSuite vs QuickBooks vs SAP

**Status:** Accepted
**Date:** 2026-06-07
**Deciders:** CTO, design-partner success lead, first design-partner's controller (consulted)
**Resolves:** spec v2 §9 open decision ("GL adapter priority order (NetSuite vs QuickBooks vs SAP) — driven by design-partner stack") · sequences the REQ-25 vendor I/O shims
**Related:** [ADR-0001](0001-chain-indexing-build-vs-buy.md) · `treasury-posting` (REQ-25 / R9, the vendor-agnostic posting protocol)

-----

## Context

`treasury-posting` (v0.12.0, REQ-25) implements the GL **posting protocol** as a vendor-agnostic state machine: content-addressed batches as idempotency keys, dual-control release, the `Unknown`/read-back retry discipline, and two-way verification. Per spec §3.7 it is deliberately "not an adapter — a protocol." What remains is to build the per-vendor **I/O shims** that map a `JournalEntry` to a GL's API and read posted entries back. This ADR decides the order in which those shims are built.

Two facts frame the decision:

1. **Where the segment's GLs actually are.** A March 2026 CFO-technology survey of 1,364 finance leaders found NetSuite is the **#1 finance/ERP system for mid-market firms, holding the top spot from ~$25M through at least $500M in revenue** ([ERP Peers](https://erppeers.com/netsuite-statistics/)); over 41,000 companies run it, "primarily mid-market businesses with $10M–$500M revenue that have outgrown QuickBooks." QuickBooks is #1 **below** ~$25M; SAP S/4HANA is an **above-$500M–$1B+** large-enterprise system ([QuickBooks market share](https://www.acecloudhosting.com/blog/quickbooks-market-share/)). DATs / public crypto holders — small public companies, high ACV, the §1 wedge — sit squarely in the NetSuite band, with a long micro-cap tail on QuickBooks and only the largest holders on SAP.

2. **The pattern is established and it is the one we already built.** Every crypto subledger in this space (Bitwave, Cryptio, Cryptoworth) occupies the exact slot we do — wallet-level detail pushing structured journal entries into the GL — and they all integrate **NetSuite first, then QuickBooks**; the documented control is "month-end reconciliation between the subledger and the GL" ([Cryptio NetSuite](https://cryptio.co/solutions/netsuite), [breezing: subledger tools 2026](https://breezing.io/blog/best-crypto-accounting-subledger-tools/)). That is precisely our feed-GL posting protocol with its read-back verification. NetSuite-first is the table-stakes expectation of the segment, not a differentiator.

A third, decisive constraint is the spec's own framing: priority is **"driven by design-partner stack."** A static ranking that ignores which GL the first one or two design partners actually run would be malpractice — Phase 1 has 1–2 partners and the whole point is to sign *their* auditor, on *their* GL.

## Decision

**Adopt a decision rule, not a fixed list:** the GL the committed Phase-1 design partner(s) run is built **first**, full stop. Absent or until that signal, the **default priority is NetSuite → QuickBooks Online → SAP S/4HANA**, and we build *few, deep, verification-complete* adapters rather than racing for breadth.

- **NetSuite — default first.** It is the GL of the target segment; building it first maximizes the probability that a design partner's stack is already covered, and it is the table-stakes integration the segment expects. The shim must fully satisfy the posting protocol: a stable idempotency key (external id), and a query path to **read posted journal entries back** for two-way verification — without read-back, the protocol's core guarantee is inert.
- **QuickBooks Online — second.** Covers the micro-cap / smaller-filer tail and cheaper design partners; lower deal value but broad. Built second unless a design partner forces it first.
- **SAP S/4HANA — deferred.** Large-enterprise, heaviest integration, fewest Phase-1 partners; high cost for low Phase-1 coverage. Built only when a specific (likely larger) partner requires it.

**Override:** if design partner #1 runs QuickBooks (or SAP), that adapter is built first regardless of the default — the default orders our *speculative* effort; a real partner's GL is not speculative.

The non-obvious discipline here is **restraint**: the competitors compete on connector *breadth* (1,000+ sources). Our differentiator is audit-grade reproducibility and the read-back-verified posting protocol, not the length of the integration list. Each adapter we ship must be *verification-complete* — idempotent submit plus genuine read-back — because a half-built adapter that posts but cannot read back silently defeats the one property that makes feed-GL trustworthy. Three deep adapters beat ten shallow ones for this product.

## Options Considered

### Option A — NetSuite first (default ranking) (**chosen as the default**)

| Dimension | Assessment |
|-----------|------------|
| Segment coverage | **Highest** — #1 ERP for the $25M–$500M band where DATs sit |
| Deal value | High (mid-market ACVs) |
| Protocol fit | Good — external id for idempotency; REST/SuiteTalk query for read-back (to be verified, action item) |
| Integration cost | Moderate |
| Segment expectation | Table stakes — every peer subledger does NetSuite first |

**Pros:** maximizes the chance a design partner is already covered; meets segment expectation; high-value deals.
**Cons:** moderate integration effort; not differentiating on its own.

### Option B — QuickBooks Online first

| Dimension | Assessment |
|-----------|------------|
| Segment coverage | Tail — micro-cap filers and sub-$25M; fewer true DATs |
| Deal value | Lower |
| Protocol fit | Query API exists; idempotency story weaker (needs care to satisfy read-back/dedup) |
| Integration cost | Lower |

**Pros:** cheapest/fastest adapter; broad SMB reach.
**Cons:** misses the core wedge segment; lower ACV; weaker native idempotency means more work to make the posting protocol's guarantees hold.

### Option C — SAP S/4HANA first

| Dimension | Assessment |
|-----------|------------|
| Segment coverage | Only the largest holders; rare among Phase-1 DATs |
| Deal value | Highest per deal, lowest Phase-1 count |
| Protocol fit | Capable but heaviest integration surface |
| Integration cost | **Highest** |

**Pros:** unlocks the largest enterprises; high per-deal value.
**Cons:** highest cost for the least Phase-1 coverage; premature before a partner demands it.

### Option D — Design-partner-stack-driven (**chosen as the governing rule**)

Build whatever the committed Phase-1 partner(s) actually run, first; fall back to the A-default otherwise.

**Pros:** directly serves the partner whose auditor sign-off is the Phase-1 exit; matches the spec's own framing; zero wasted speculative effort.
**Cons:** the order is not knowable until partners commit — so the default (A) governs interim planning.

## Trade-off Analysis

The static-ranking options (A/B/C) optimize for a *population*; the spec optimizes for a *specific auditor sign-off on a specific partner's books*. Those can diverge: NetSuite is the right population bet, but if the first design partner runs QuickBooks, a NetSuite-first plan delays the only close that matters in Phase 1. Hence the governing rule is D (partner-driven), with A as the default that orders speculative work when no partner signal exists yet — and A is the right default precisely because it is the modal GL of the segment, so it is also the most likely partner GL.

The restraint point is the real differentiator decision. It would be easy to chase the competitors' breadth and ship many thin adapters; for an audit-grade product that is actively harmful, because a thin adapter (post without read-back) removes the verification that distinguishes our feed-GL from a fire-and-forget export. Depth-first, verification-complete adapters are the on-brand choice and they bound the work to what a Phase-1 close actually needs.

## Consequences

**Easier:**
- A design partner on NetSuite (the likely case) is covered by the first adapter; the posting protocol's read-back verification becomes demonstrable on the GL the segment actually uses.
- Effort is bounded to verification-complete adapters for GLs that a real close needs, not a breadth race.

**Harder:**
- Each adapter must implement genuine read-back, which is more work than post-only — but it is non-negotiable for the protocol's guarantee, so the cost is deliberate.
- A partner on SAP in Phase 1 would force the heaviest integration early; acceptable as an override but expensive.

**To revisit:**
- The default order if early design-partner signal contradicts the population data (e.g. the first two partners both run QuickBooks).
- Per-vendor idempotency/read-back mechanics — a verification spike per GL (external id semantics, query-back fidelity) before committing each adapter.
- Sage Intacct as a possible fourth: it ranks well on pure financial-management depth in the mid-market and may appear in design-partner stacks ([NetSuite vs alternatives](https://www.brokenrubik.com/blog/netsuite-alternatives-top-erp-options)).

## Action Items

1. [ ] **Confirm the committed Phase-1 design partners' GLs** — this input governs the actual build order and overrides the default (§9).
2. [ ] Per-vendor **protocol-fit spike** before building each adapter: verify the idempotency mechanism (stable external id / dedup) and the **read-back query** fidelity needed for two-way verification — NetSuite first.
3. [ ] Build the NetSuite adapter as the default first shim: `JournalEntry` → SuiteTalk/REST with external id; read posted entries back into the protocol's `verify` step.
4. [ ] Build QuickBooks Online second (or first, if a partner forces it), with explicit attention to making idempotency hold given its weaker native support.
5. [ ] Hold SAP until a specific partner requires it; scope it then.
6. [ ] Encode the "verification-complete or not shipped" rule as an adapter acceptance gate: no adapter ships that can post but cannot read back.

-----

## Sources consulted

- ERP market share by company-size band (NetSuite #1 mid-market $25M–$500M; QuickBooks #1 sub-$25M; SAP large-enterprise) — [ERP Peers: NetSuite statistics 2026](https://erppeers.com/netsuite-statistics/), [QuickBooks market share 2026](https://www.acecloudhosting.com/blog/quickbooks-market-share/), [NetSuite company-size bands](https://www.houseblend.io/articles/netsuite-company-size-mid-market-revenue)
- Crypto subledger → GL integration pattern (JE push + month-end reconciliation; NetSuite-first across peers) — [Cryptio: NetSuite](https://cryptio.co/solutions/netsuite), [Best crypto subledger tools 2026](https://breezing.io/blog/best-crypto-accounting-subledger-tools/), [Cryptoworth: NetSuite](https://www.cryptoworth.com/oracle-netsuite)
- Mid-market alternatives (Sage Intacct financial-depth note) — [NetSuite alternatives](https://www.brokenrubik.com/blog/netsuite-alternatives-top-erp-options)
