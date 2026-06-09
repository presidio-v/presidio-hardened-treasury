# Architecture Decision Records

Significant, hard-to-reverse decisions for presidio-hardened-treasury, in
the [standard ADR form](https://github.com/joelparkerhenderson/architecture-decision-record):
context → decision → options → trade-offs → consequences → action items.
The active spec is [`../treasury-suite-spec-v2.md`](../treasury-suite-spec-v2.md);
ADRs resolve its §9 open decisions and record why a path was chosen.

| ADR | Title | Status | Resolves |
|-----|-------|--------|----------|
| [0001](0001-chain-indexing-build-vs-buy.md) | Chain indexing — build vs buy | Proposed | §9 raw-chain-indexing build-vs-buy; REQ-20 |
| [0002](0002-anchor-submission-target.md) | Anchor submission target — public chain vs RFC 3161 TSA | Proposed | REQ-8 / §3.3 anchoring target |
| [0003](0003-gl-adapter-priority.md) | GL adapter priority order — NetSuite vs QuickBooks vs SAP | Proposed | §9 GL priority; sequences REQ-25 shims |
| [0004](0004-node-indexer-selection.md) | Per-chain node and indexer selection (BTC, ETH) | Proposed | follow-on to 0001; per-chain §3.3 two-source |
