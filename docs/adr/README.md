# Architecture Decision Records

Significant, hard-to-reverse decisions for presidio-hardened-treasury, in
the [standard ADR form](https://github.com/joelparkerhenderson/architecture-decision-record):
context → decision → options → trade-offs → consequences → action items.
The active spec is [`../treasury-suite-spec-v2.md`](../treasury-suite-spec-v2.md);
ADRs resolve its §9 open decisions and record why a path was chosen.

| ADR | Title | Status | Resolves |
|-----|-------|--------|----------|
| [0001](0001-chain-indexing-build-vs-buy.md) | Chain indexing — build vs buy | Proposed | §9 raw-chain-indexing build-vs-buy; REQ-20 |

Pending (sequenced next):
- **ADR-0002** — anchor submission target: public chain vs RFC 3161 TSA (REQ-8 / §3.3).
- **ADR-0003** — GL adapter priority order: NetSuite vs QuickBooks vs SAP (§2.6 / §9), driven by design-partner stack.
- **ADR-0004** — per-chain node client + deterministic indexer selection (follow-on to 0001).
