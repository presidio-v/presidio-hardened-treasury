# ADR-0004: Per-chain node and indexer selection (BTC, ETH)

**Status:** Proposed
**Date:** 2026-06-07
**Deciders:** CTO, security lead, infra/SRE lead
**Resolves:** the per-chain concretisation of [ADR-0001](0001-chain-indexing-build-vs-buy.md) — which node clients and indexers run as the two independent in-house sources, and how the §3.3 two-source requirement is met chain-by-chain
**Related:** ADR-0001 (self-hosted primary; in-house second source) · spec §3.3 (completeness controls) · gap G-5 / §3.5 (finality policy) · REQ-20 (ingestion)

-----

## Context

ADR-0001 committed us to self-hosting the authoritative ingestion source and to a **second in-house implementation over a distinct node** as the §3.3 completeness control — code-independent, no external data egress. It left the concrete clients open (its action item: a follow-on ADR once the Phase-1 chain set is fixed). Phase 1 targets **BTC and ETH** (the design partners' actual holdings; §9 still pins the exact venue set). This ADR picks the clients and indexers.

One realisation reframes the whole decision and makes the two chains' answers *different*: **the independence axis must sit where silent bugs realistically occur, and that location differs by chain.**

- **Bitcoin:** alternative full-node *consensus* implementations carry a parity risk. The Bitcoin community's own guidance is blunt — alternate clients "are not recommended for serious use because it is currently difficult to determine whether they implement the consensus rules with 100% accuracy, and even very slight inaccuracies could cause serious problems" ([Bitcoin Wiki: Full node](https://en.bitcoin.it/wiki/Full_node)). For an audit-grade ledger, running a second consensus implementation as the *independence axis* would manufacture false divergences (or, worse, quietly accept an invalid chain). The realistic silent-bug surface here is the **indexer** — address derivation, history reconstruction, fee attribution — not consensus. So BTC independence belongs at the **indexing layer**, with consensus pinned to the reference client.
- **Ethereum:** the opposite. Execution-**client** diversity is a celebrated, well-understood safety property: "client diversity has already proven an important defence … the 2016 Shanghai DoS tricked the dominant client (Geth) … alternative clients which did not share the vulnerability let Ethereum continue" ([ethereum.org: client diversity](https://ethereum.org/developers/docs/nodes-and-clients/client-diversity/)), and staking operators combine clients M-of-N so that "when one client diverges, the validator continues through the rest" ([Coinbase: execution client diversity](https://www.coinbase.com/developer-platform/discover/insights-analysis/execution-client-diversity)). For ETH, running **two different execution clients** that independently re-execute the chain is the strongest possible independence axis — it catches consensus-execution *and* indexing bugs at once.

## Decision

**Two independent in-house sources per chain, with the independence axis placed where bugs live.**

### Bitcoin — consensus on the reference client, independence at the indexer

- **Consensus source of record:** **Bitcoin Core** (the reference implementation; we do not substitute its consensus).
- **Source A:** Bitcoin Core + **electrs** (Rust) for address history.
- **Source B:** a *second* Bitcoin Core instance + **Fulcrum** (modern C++) for address history.
- electrs and Fulcrum are protocol-compatible but "make very different trade-offs" and are entirely separate codebases in different languages ([The Bitcoin Way: Bitcoin indexers](https://www.thebitcoinway.com/articles/bitcoin-indexers-explained-electrs-fulcrum-and-choosing-the-right-tool)) — so a silent indexing bug in one is caught by the other. Bitcoin Core's own wallet-less RPC (`txindex`, `scantxoutset`) is a third, consensus-direct cross-check on balances independent of either indexer.
- Optional deeper check, **not** an independence axis: a `btcd` (Go) instance may be run to compare consensus state; any Core-vs-`btcd` divergence is *escalated for investigation* ("which is right?"), never auto-resolved — precisely because of the parity caveat above.

### Ethereum — two execution clients, each with its own history path

- **Source A (primary):** **reth** (Rust, Paradigm) — modern, archive-capable (~4 TB archive), and its `trace_*` namespace reconstructs per-address history.
- **Source B (independent):** **Erigon** (Go) — storage-efficient archive (~1.8–2.2 TB) whose built-in `ots_`/`trace_filter` history indexing (the engine behind local **Otterscan**) gives full address history "fully private … on your local machine" ([Otterscan + Erigon](https://docs.erigon.tech/fundamentals/otterscan)), lowering the separate-indexer burden to near zero.
- reth and Erigon are genuinely independent: different teams, different languages (Rust vs Go), both **non-Geth** (so we are not running two Geth-derived clients), and each re-executes the chain *and* indexes history independently. **Nethermind** (C#, the largest non-Geth client by share) is the designated drop-in for Source B if a maximally-different stack is preferred or if reth/Erigon ever share a defect.

### How the §3.3 two-source requirement is satisfied, per chain

The completeness control compares the two sources' **observation output** (the per-address transaction histories that become L1 observations), not raw node internals. For every (tenant, address, period):

1. Both sources produce a normalised, canonicalised history; their content hashes must match. A divergence **surfaces for human resolution and blocks close** — it is never auto-reconciled (consistent with the §3.6 / re-fetch-and-diff workflow).
2. **Reproducibility gate** (ADR-0001 action item): re-indexing a fixed block range twice on a given source yields identical observation hashes; this is the per-source acceptance test before a source is trusted.
3. The per-chain **finality/confirmation-depth policy** (G-5, §3.5) governs when a history is "settled" enough to compare — BTC at a documented confirmation depth (≈6), ETH at consensus-layer finality (~2 epochs) — so reorg churn is excluded from divergence checks by policy, not by guesswork.

## Options Considered

### Bitcoin

#### B-A — Core + two independent indexers (electrs + Fulcrum) (**chosen**)
| Dimension | Assessment |
|-----------|------------|
| Independence axis | The indexer — where interpretation bugs actually live |
| Consensus correctness | Reference Core on both; no parity risk |
| False-divergence risk | Low — both trust the same canonical chain |
| Effort | Moderate — two indexers, two Core instances |

**Pros:** independence exactly where it matters; no consensus parity gamble; electrs(Rust)/Fulcrum(C++) are deeply different code.
**Cons:** both sources share Bitcoin Core consensus (mitigated by the optional `btcd` escalation check).

#### B-B — Core + btcd as the two consensus sources (rejected)
**Pros:** real implementation independence at consensus.
**Cons:** btcd's consensus parity is not guaranteed; a divergence is as likely a *false* alarm or an accepted-invalid-chain risk as a real catch — wrong place for an audit ledger's independence axis. Kept only as an optional escalation probe.

### Ethereum

#### E-A — reth + Erigon, two execution clients (**chosen**)
| Dimension | Assessment |
|-----------|------------|
| Independence axis | Two full re-executions of the chain (consensus + indexing) |
| Client diversity | Both non-Geth, different languages/teams |
| History indexing | reth `trace_*`; Erigon `ots_`/Otterscan (near-free) |
| Storage | reth archive ~4 TB; Erigon archive ~2 TB |

**Pros:** strongest possible independence; the celebrated ETH client-diversity property applied to our reconciliation; Erigon's built-in history indexing removes a moving part.
**Cons:** two archive nodes is real disk/ops cost; reth is younger than Geth/Erigon (mitigated: Nethermind as the designated substitute).

#### E-B — one client + two indexers (rejected)
**Pros:** cheaper (one execution node).
**Cons:** a consensus-execution bug in the single client is invisible to both indexers — it forfeits the ETH ecosystem's strongest independence axis to save disk. Wrong trade for an audit ledger.

## Trade-off Analysis

The unifying principle is **place independence where the failure mode is, not where it is cheapest or most symmetric.** A naïve "run two of everything identically per chain" misreads both chains: on Bitcoin it would gamble consensus parity for an independence that buys little (consensus bugs in Core are not the realistic risk; indexer bugs are), and on Ethereum it would *under*-use the one place independence is cheap and proven (client diversity). So the design is deliberately asymmetric: Bitcoin keeps one consensus truth and diversifies the indexer; Ethereum diversifies the whole client.

The residual common factor is honest and the same on both chains: both sources still flow through **our** ingestion-mapping code (indexer output → canonical L1 observation) and **our** operator. The independence ends at the node+indexer boundary. Two mitigations, both already in the architecture: keep the mapping layer minimal and golden-vectored (so its correctness is itself cross-verified, REQ-7), and retain ADR-0001's *optional, tenant-consented* third-party tertiary for the rare case a tenant wants operator-independent corroboration despite the privacy cost.

## Consequences

**Easier:**
- The §3.3 completeness control becomes concrete and testable per chain, with a reproducibility gate and a finality-gated divergence check.
- ETH inherits the ecosystem's strongest safety property (client diversity) directly into the audit reconciliation; BTC puts independence exactly on the indexer layer where silent bugs occur.
- Erigon's built-in history indexing and electrs/Fulcrum's maturity mean little custom indexing code — less first-party surface to verify.

**Harder:**
- Four node deployments to operate (2× Core for BTC, reth + Erigon for ETH) plus two BTC indexers — real disk (BTC full ≈ 745 GB ×2; ETH archive ≈ 4 TB + 2 TB) and SRE burden, beginning now.
- Two history paths per chain must be kept version-pinned and their divergence-rate monitored; a chronically diverging pair is a defect to fix, not noise to ignore.
- Adding a third chain later repeats this whole analysis for that chain's failure modes (it is not copy-paste).

**To revisit:**
- Substitute Source B (Nethermind for Erigon/reth; or a different BTC indexer) if a chosen pair proves insufficiently independent in practice (shared dependency, correlated bug).
- Whether the optional `btcd` consensus probe earns its keep, or whether two Core instances + two indexers is corroboration enough.
- L2/sidechain ingestion if a design partner holds assets there — a separate failure-mode analysis.

## Action Items

1. [ ] Stand up BTC Source A (Core + electrs) and Source B (second Core + Fulcrum); wire both into the §3.3 history-divergence check.
2. [ ] Stand up ETH Source A (reth) and Source B (Erigon + Otterscan/`ots_`); wire both history paths into the same check.
3. [ ] Implement the **reproducibility gate** as each source's acceptance test: re-index a fixed block range twice → identical observation hashes (ADR-0001 action item 2).
4. [ ] Author the per-chain **finality/confirmation-depth policy** as a content-addressed artifact (G-5, §3.5): BTC confirmation depth; ETH consensus-layer finality.
5. [ ] Keep the indexer-output → L1-observation mapping minimal and golden-vectored (REQ-7), since it is the residual single point across both sources.
6. [ ] Define divergence-rate monitoring + alerting per chain so a chronically diverging source pair is surfaced as a defect.
7. [ ] Version-pin all four clients and both indexers; record the pins as part of the source's provenance.

-----

## Sources consulted

- BTC node implementations and the consensus-parity caveat; electrs vs Fulcrum trade-offs — [Bitcoin Wiki: Full node](https://en.bitcoin.it/wiki/Full_node), [The Bitcoin Way: Bitcoin indexers explained](https://www.thebitcoinway.com/articles/bitcoin-indexers-explained-electrs-fulcrum-and-choosing-the-right-tool), [btcd (GitHub)](https://github.com/btcsuite/btcd), [Fulcrum (GitHub)](https://github.com/cculianu/Fulcrum)
- ETH execution-client diversity, the 33%/66% thresholds, Shanghai-DoS precedent, M-of-N operator practice — [ethereum.org: client diversity](https://ethereum.org/developers/docs/nodes-and-clients/client-diversity/), [clientdiversity.org](https://clientdiversity.org/), [Coinbase: diversifying execution clients](https://www.coinbase.com/developer-platform/discover/insights-analysis/execution-client-diversity)
- ETH self-hosted address-history indexing — [Otterscan + Erigon](https://docs.erigon.tech/fundamentals/otterscan), [Otterscan (GitHub)](https://github.com/otterscan/otterscan), [Blockscout](https://docs.blockscout.com/)
