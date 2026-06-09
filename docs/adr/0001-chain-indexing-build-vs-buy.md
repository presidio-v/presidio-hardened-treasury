# ADR-0001: Chain indexing — build vs buy

**Status:** Proposed
**Date:** 2026-06-07
**Deciders:** CTO, security lead, first design-partner's audit firm (consulted)
**Resolves:** spec v2 §9 open decision ("Build-vs-buy on raw chain indexing (ingestion) vs third-party data providers") · informs REQ-20 (read-only ingestion)
**Supersedes:** none

-----

## Context

Stage 1 of the close pipeline (REQ-20) ingests **full transaction history** — not just balances — for every address a tenant holds, across the chains its assets live on (Phase 1: BTC and ETH against the design partners' actual holdings; §9 venue set still open). Everything downstream — reconciliation, lots, valuation, the disclosure pack — is a pure function of those ingested observations, so the ingestion source is the **root of the entire evidence chain**. Three forces from the existing architecture constrain the choice, and they do not point the same way as a generic application's would:

1. **Privacy is a load-bearing security property, not a feature (threat model A1, A2).** The threat model names *treasury posture* (wallet clusters, custody relationships, cadence, liquidity — A1) and *the xpub/address tree* (A2) as the primary assets an attacker wants; the system holds no keys that move funds, so information leakage **is** the catastrophic outcome. §3.4 enforces read-only ingestion at the network layer and derives addresses in an enclave **specifically so the address set never leaks**. A third-party indexer query is, by construction, an egress of exactly that address set to an outside party — the precise event the egress allowlist and the xpub enclave exist to prevent. This inverts the usual "buy the data, build the product" default: here, *the data sourcing is the trust boundary*.

2. **Reproducibility is the product (REQ-7, §3.3).** The Phase 0 exit criterion is "any historical figure reproduces byte-for-byte from hashed inputs," and the moat is auditor acceptance of that reproduction. A figure can only reproduce if its raw input is *derivable from first principles* — i.e. recomputable from the canonical chain. A third-party indexer's output is opaque: we cannot reproduce it, its indexing logic versions without notice, and it can silently revise served history (the provider-revision scenario already modeled in §3.6 and review finding F7). Hashing an opaque payload proves we received it, not that it is correct or complete.

3. **Completeness already mandates ≥2 independent sources (§3.3 completeness controls).** The architecture does not permit a single source regardless of build/buy: cross-validation against ≥2 chain nodes or an independent indexer is a stated control. So the real question is not "one source, build or buy" but **which source is the authoritative, reproducible system-of-record, and what provides genuinely independent corroboration.**

Additional constraints: a per-chain **finality/confirmation-depth policy** is a required content-addressed audit artifact (gap G-5, §3.5) — reorg handling must be ours to document and reproduce, not a vendor's undisclosed default. Phase 1 scale is small (1–2 design partners, a handful of chains), so operational burden is bounded for the wedge even if it grows later.

## Decision

Adopt a **hybrid with an in-house authoritative source**:

- **Primary (system of record): self-hosted full node + deterministic open-source indexer**, per chain — the byte-for-byte reproducible source whose output feeds the evidence store. BTC: Bitcoin Core + an Electrum-protocol indexer (`electrs`/Fulcrum) and/or Core RPC directly; ETH: a full/archive execution client (Erigon or reth) + a deterministic address indexer (Blockbook-class or a thin first-party indexer). This source derives every observation from the canonical chain, so it reproduces and it never sends the address set off-box.
- **Independent corroboration (the §3.3 second source): a second in-house implementation**, deliberately *different code over a different node instance* — e.g. Core RPC cross-checking `electrs`, or reth cross-checking Erigon — so a silent bug or revision in one implementation is caught without any external data egress. This is the default completeness control.
- **Third-party data API: optional, tenant-consented, address-scoped tertiary check only — never required, never the system of record.** Permitted (if a tenant opts in) as an extra corroboration signal for a *specific* asset/balance, routed through the egress allowlist and rate-limited, with the privacy cost disclosed in the design-partner contract. It is demoted from "the obvious buy" to a removable accessory.

The non-obvious core of this decision: for most products the right answer is *buy the data, build the product*; here the privacy and reproducibility requirements make the **data sourcing itself the differentiator**, so we build the primary, build the second source too, and treat purchased data as an optional garnish rather than the foundation.

## Options Considered

### Option A — Buy: managed multi-chain data API as primary

Use a managed provider ([Bitquery](https://bitquery.io/), [Covalent/GoldRush](https://docs.base.org/get-started/data-indexers), CoinStats, etc.) as the authoritative ingestion source; thin client maps their payloads into observations.

| Dimension | Assessment |
|-----------|------------|
| Complexity | **Low** — no node ops; one HTTP client |
| Time to first close | **Fast** — days |
| Privacy (A1/A2) | **Fails by construction** — the tenant's full address set is sent to a third party; defeats the §3.4 enclave + egress-allowlist design |
| Reproducibility (REQ-7) | **Weak** — opaque, unversioned, silently revisable; cannot recompute from first principles |
| Finality control (G-5) | **None** — vendor's undisclosed reorg/confirmation handling |
| Cost at Phase-1 scale | Low subscription; rises with addresses/queries |
| Independence | A second vendor adds operator-independence but doubles the privacy leak |

**Pros:** fastest to a demo; zero infra; broad chain coverage immediately.
**Cons:** structurally violates the product's central security guarantee; the auditor cannot be shown a reproducible derivation; we inherit the vendor's revisions as our restatements.

### Option B — Build: self-hosted nodes + deterministic indexers, in-house second source

Run full nodes and open-source indexers; a second independent in-house implementation provides corroboration. No external data dependency.

| Dimension | Assessment |
|-----------|------------|
| Complexity | **High** — node sync, disk, uptime, upgrades per chain |
| Time to first close | **Slower** — node sync + indexer hardening (weeks) |
| Privacy (A1/A2) | **Strong** — address set never leaves our infrastructure |
| Reproducibility (REQ-7) | **Strong** — every observation derives from the canonical chain; replayable |
| Finality control (G-5) | **Full** — confirmation depth is our documented, hashed policy |
| Cost at Phase-1 scale | Bounded infra (BTC full ≈ 745 GB as of Jun 2026 and rising; ETH archive ≈ 1.8–2.2 TB on Erigon, ≈ 4 TB on reth) + engineering time — the real cost is ops, not money |
| Independence | Two in-house implementations over distinct nodes give code-independence without leak; they share *our operator* as a common factor |

**Pros:** satisfies privacy, reproducibility, and finality control simultaneously; no vendor revision risk; matches the presidio-hardened thesis (own the trust boundary).
**Cons:** real operational burden — "the hard part is everything after the node runs: keeping it synced and upgraded" ([Chainstack](https://chainstack.com/self-hosted-blockchain-node-diy-vs-managed/)); slower start; in-house-only corroboration shares the operator as a residual common-failure factor (mitigated by Option C's optional tertiary).

### Option C — Hybrid: build primary + in-house second source, third-party as optional tertiary (**chosen**)

Option B as the foundation, plus a *removable* third-party corroboration path a tenant may opt into for specific assets, address-scoped and egress-allowlisted.

| Dimension | Assessment |
|-----------|------------|
| Complexity | **High** (= B) + a small optional adapter |
| Privacy (A1/A2) | **Strong by default**; the only leak path is opt-in, scoped, and disclosed |
| Reproducibility (REQ-7) | **Strong** — the system of record is always the self-hosted derivation |
| Finality control (G-5) | **Full** |
| Independence | Best available: code-independent in-house second source by default; operator-independent vendor check when a tenant accepts the privacy cost |

**Pros:** keeps every guarantee of B while letting a willing tenant buy *additional* operator-independent assurance; the third-party path can be removed without touching the system of record.
**Cons:** carries B's ops burden; the optional path needs careful scoping so "optional" cannot quietly become "load-bearing."

## Trade-off Analysis

The decisive axis is not cost or speed — it is **whether the ingestion source can be both private and reproducible**, because those two are the product's reason to exist. Option A is fastest and cheapest and fails both; no amount of operational convenience offsets sending the tenant's entire treasury posture to a third party in a product whose pitch is "we are the auditor's reproducible source of truth." Option B satisfies the guarantees but leaves a residual independence gap (one operator) and a slower start. Option C closes the independence gap *optionally* without compromising the default posture, at the cost of a little extra surface that must be disciplined.

Speed-to-demo (A's main pull) is real but addressable without conceding the architecture: a **disposable demo profile** may use a public RPC/API against *throwaway* addresses for a sales demo, explicitly fenced off from any tenant data and never a path to production ingestion. That preserves the fast-demo benefit without letting it set the production default.

The cost argument cuts toward B/C at the wedge: "at scale, self-hosting is significantly more cost-effective than managed nodes" ([Chainstack](https://chainstack.com/cloud-vs-self-hosted-blockchain-nodes/)), and Phase-1 scale (a few chains, 1–2 partners) keeps the ops burden bounded while the team learns the failure modes before they matter at scale.

## Consequences

**Easier:**
- The §3.4 privacy design becomes coherent end to end — there is no longer a giant address-egress hole at ingestion that the enclave/allowlist were silently failing to cover.
- Reproducibility (REQ-7) extends to the *raw* layer: an auditor can be walked from a disclosure figure all the way to "this is the canonical-chain derivation, here is the node, here is the deterministic indexer version."
- The finality policy (G-5) and the §3.3 completeness control get concrete, controllable implementations instead of depending on vendor behavior.

**Harder:**
- We own blockchain node operations: sync time, disk growth (BTC full ≈ 745 GB and climbing; ETH archive ≈ 2 TB on Erigon, more on other clients), client upgrades, reorg handling, monitoring, and uptime — a standing SRE cost that begins now, not at scale. ([sizes — Bitcoin](https://www.blockchainsize.org/), [Ethereum archive clients](https://www.7blocklabs.com/blog/ethereum-archive-node-disk-size-2026-vs-erigon-archive-node-disk-size-2026-vs-geth-full-node-disk-size-2026))
- First production ingestion is weeks out, not days; the demo profile must be firewalled so it never leaks into that timeline.
- Adding a chain means standing up and hardening its node + indexer, not adding a vendor endpoint — chain coverage grows linearly with effort.

**To revisit:**
- The per-chain client/indexer selection (Core RPC vs `electrs` vs Fulcrum for BTC; Erigon vs reth + indexer for ETH) is a follow-on technical ADR once the Phase-1 chain set is fixed by design partners.
- The independence model: if two in-house implementations prove insufficiently independent in practice (shared library, shared node bug), revisit whether the optional third-party tertiary should become a default for specific high-value assets.
- Managed *node* hosting (e.g. Chainstack-style "your infra, their lifecycle tooling") as a middle path that keeps data in our trust boundary while outsourcing ops — worth re-evaluating if SRE burden outpaces the team.

## Action Items

1. [ ] Confirm the Phase-1 chain set with design partners (unblocks client selection) — §9.
2. [ ] Follow-on ADR-0002: per-chain node client + deterministic indexer selection, with the reproducibility test (re-index a fixed block range twice → identical observation hashes) as the acceptance gate.
3. [ ] Specify the in-house second-source corroboration as the concrete §3.3 completeness control (which two implementations, what diff cadence, how a divergence surfaces — ties into the existing re-fetch + diff workflow).
4. [ ] Write the per-chain finality/confirmation-depth policy as a content-addressed artifact (G-5, §3.5).
5. [ ] Define the optional third-party tertiary's contract: address scoping, egress-allowlist entries, rate limits, tenant consent language, and the disclosure of its privacy cost.
6. [ ] Stand up the firewalled demo profile (public RPC, throwaway addresses) with a written guarantee it cannot reach tenant data or production ingestion.

-----

## Sources consulted

- Self-hosted vs managed nodes, ops burden, cost at scale — [Chainstack: Cloud vs self-hosted nodes](https://chainstack.com/cloud-vs-self-hosted-blockchain-nodes/), [Chainstack: Self-hosted node DIY vs managed](https://chainstack.com/self-hosted-blockchain-node-diy-vs-managed/)
- Self-hosted Bitcoin indexing (privacy-preserving, runs on user infra) — [electrs / Start9](https://start9labs.medium.com/new-service-electrs-b70122464765), [Blockbook (Trezor)](https://getblock.io/marketplace/projects/blockbook/)
- Managed multi-chain data providers and coverage — [Bitquery](https://bitquery.io/), [Base data indexers (Covalent/GoldRush)](https://docs.base.org/get-started/data-indexers)
- xpub privacy leakage (an xpub exposes full history + balance; querying a public explorer links identity) — [Trezor: What is an xPub](https://trezor.io/learn/advanced/what-is-a-public-key-xpub)
- Crypto audit data completeness / reconciling on-chain vs off-chain, second-source reconciliation — [Cryptio: internal controls for completeness](https://blog.cryptio.co/internal-controls-for-data-completeness-and-accuracy), [TRES: reconciliation processes for crypto audits](https://tres.finance/crypto-audit/reconciliation-processes-for-crypto-audits/)
