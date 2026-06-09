# Auditor evidence and reproduction guide

**Audience:** the external auditor of a tenant whose quarterly crypto close is prepared with this system.
**Status:** reference · tracks the implemented system (workspace v0.18.0) and the active spec [`treasury-suite-spec-v2.md`](treasury-suite-spec-v2.md).
**Companion documents:** [`SECURITY.md`](../SECURITY.md) (hardening baseline), [`threat-model.md`](threat-model.md) (STRIDE per trust boundary), [`adr/`](adr/) (decision records).

-----

## 1. What this system is, and what it is not

This system is **management's evidence-preparation tool**. The client's controller owns and re-performs the close; the system's job is to make every figure in the close **independently re-performable from hashed inputs**. In audit-evidence terms (PCAOB AS 1105), the question it is built to answer is *"is this evidence sufficient and reliable?"* — not *"can we rely on this vendor's controls?"*. In Phase 1 it is **not** positioned as a relied-upon service organisation; that posture (AS 2601, gated on SOC 2 Type I) is a later phase. Classification judgments — whether a movement is an internal transfer, whether an asset is in ASU 2023-08 scope, which lots a disposal relieves — remain **management's**, recorded with the responsible approvers' identities.

The practical consequence for the audit: you are not asked to trust the operator (the vendor) or the client's word for any number. You are given a procedure to reconstruct each number from raw, hashed inputs and check it yourself.

## 2. The trust model you are relying on

The operator is treated as **inside** the threat model, not outside it (see [`threat-model.md`](threat-model.md), boundary B5). The system is therefore designed so that its integrity does not depend on trusting the operator:

- **Content addressing.** Every raw input (a venue/chain API payload, a price snapshot, a GL response) is stored under the SHA-256 of its canonical bytes. A figure references the hash of the inputs it was derived from. You can re-hash the bytes and confirm the reference.
- **Append-only, hash-chained ledger.** The book of record is an append-only event log, chained per tenant; recomputing the chain (`verify_chain`) detects any post-hoc mutation, insertion, or deletion. Corrections **supersede**, never overwrite — so "what the books said as of the 10-Q filing" remains answerable alongside "what we now know."
- **External anchoring.** The evidence store's RFC 6962 tree head is periodically committed to **Bitcoin** (ADR-0002). Once confirmed, that commitment is verifiable by anyone with a Bitcoin node — including you, against your own node — with **no dependence on the operator**. This is the strongest form of "the operator could not have rewritten history after the fact."
- **No floats anywhere in the accounting path.** All money is integer base units with checked arithmetic; values that cannot hash identically on every toolchain (i.e. floats) are rejected at the canonicalisation boundary. Two independent implementations therefore produce byte-identical hashes — which is what makes cross-verification meaningful.

## 3. The claim-layer model: how a figure traces to raw bytes

The ledger records four kinds of claim, each with mandatory provenance, so every number's lineage is a walk down the layers:

1. **L1 — Observations.** Raw, normalised facts from a venue or chain, each referencing the evidence-store hash of the raw payload it came from.
2. **L2 — Derived facts.** Deterministic computations over observations (e.g. an internal-transfer auto-net), carrying the hash of the code version that produced them.
3. **L3 — Judgments.** Human decisions that are not facts — internal-transfer confirmation, ASU-scope designation, principal-market election, lot relief method. Each carries the governing policy hash **and the approver identities** (dual control: a preparer and a distinct approver).
4. **L4 — Policy outputs.** GAAP/IFRS journal entries and disclosures — pure functions over L1–L3.

A judgment without an approver and a content-addressed policy hash **cannot enter the ledger**; this is enforced at append time, not by review. So when you ask "who decided this, and under what policy?", the answer is structural, not anecdotal.

## 4. How to verify any figure in the close

The disclosure pack **is** the product (spec §2.7). It is content-addressed and carries an **evidence-reproduction manifest**: the sorted, deduplicated closure of every artifact hash the pack depends on — the as-filed checkpoint, the valuation report, the policies in force, the anchor receipt, and the evidence behind each roll-forward row. Citing one pack hash cites the entire close.

The verification procedure is **fetch → recompute → compare**, with no trusted intermediary at any step:

1. **Start at the pack hash.** Recompute it from the pack's canonical bytes; confirm it matches what management cited.
2. **Walk the manifest.** For each referenced hash, fetch the artifact from the evidence store and re-hash it; confirm the hash matches. A mismatch is a tamper signal, not a discrepancy to reconcile.
3. **Re-perform the arithmetic.** Roll-forward rows are structurally guaranteed to satisfy *opening + additions − disposals + remeasurement = closing* (a row that does not roll cannot be constructed), and the pack ties each row's closing balance to the valuation report **both directions** — a missing row, an extra row, or a closing mismatch is named explicitly. Valuation is a pure function of `(lots, price-snapshot, policy)`; re-running it under the same three hashed inputs reproduces the figure byte-for-byte.
4. **Trace a number to raw bytes.** Pick any figure, follow its L4 entry → the L1–L3 events it was computed from → the evidence-store hashes those reference → the raw venue/chain/GL payloads. Every hop is a hash you can independently check.
5. **Confirm the period was anchored.** The anchor receipt names the Bitcoin transaction (or RFC 3161 token) committing the evidence tree head; verify that commitment against the public chain yourself.

If every hash matches and the recomputations agree, you have reconstructed the close from first principles without trusting us.

## 5. Controls, by pipeline stage

Each stage carries a control that is **structural** (enforced by construction) rather than procedural:

- **Ingestion** is read-only *by construction*: venue traffic is confined to an egress allowlist of read-only endpoint+method pairs, so a mis-scoped API key cannot place a trade even if the venue reports the scope wrongly. On-chain history is taken from **two independent in-house sources per chain** (ADR-0004) — for Bitcoin, two indexers (electrs and Fulcrum) over Bitcoin Core; for Ethereum, two execution clients (reth and Erigon). Their settled histories must hash-match or the address **blocks close**; a divergence is surfaced for human resolution, never auto-reconciled. A finality policy (confirmation depth for Bitcoin, consensus-finalised height for Ethereum) excludes reorg churn from the comparison by a documented rule.
- **Reconciliation** of internal transfers uses a tiered matcher with **no numeric confidence score** (discrete corroboration classes), a per-asset materiality threshold below which a strong match auto-nets, and a structural **false-negative bias**: a leg the matcher cannot place lands as a close blocker and *cannot* silently book as a disposal + acquisition (the error that manufactures phantom P&L). Ambiguous matches go to a dual-control queue; the vendor never classifies.
- **Asset scope** is a gate, not an engine: an asset proceeds to valuation only through a dual-control-confirmed assessment finding all six ASU 2023-08 criteria met. Unassessed assets, undetermined criteria, and out-of-scope assets all **hard-block** — honest rejection rather than silent mis-valuation.
- **Lots / cost basis** conserve basis exactly under partial relief (floor division with the remainder retained), keep fees decomposed from basis (so capitalise-vs-expense is a recorded election), and preserve the original acquisition date across internal transfers.
- **GL posting** is a protocol, not a fire-and-forget export: a batch's content hash is its idempotency key; a lost acknowledgment resolves *only* through read-back evidence (never a guess); and posting is verified by reading the entries back and comparing **both directions**, with the GL reconciliation itself recorded as evidence.
- **Period close** is a checkpoint DAG: a correction is a new node with a reason code and a SAB 99 materiality memo, "as filed" and "as corrected" are permanent pointers, and the folded state root reproduces byte-for-byte from the ledger.

## 6. Reproducibility guarantees you can rely on

- **Cross-implementation hashing.** Every hash definition in the system (event identity, canonical JSON, RFC 6962 tree heads and inclusion proofs, every policy/report/entry envelope) is cross-verified against an independent implementation; the test suite carries the golden vectors.
- **Whole-close determinism.** The complete pipeline — ingestion through disclosure pack — runs twice on the same inputs and produces the **identical** pack hash. This is the Phase 0 exit criterion promoted to the whole system.
- **Source reproducibility.** Each ingestion source must pass a reproducibility gate (re-querying the same range reproduces the history hash) before it is trusted as a system of record.

## 7. What is management's responsibility, not the system's

- **All classification judgments** (internal transfer vs disposal, ASU scope, lot election, fee election, principal-market election) are management's, made under dual control and recorded with approver identities and policy hashes. The system presents evidence and enforces that *a* human decided; it does not decide.
- **The principal-market price-source policy body** is co-designed with you (the auditor) as a Phase 0 exit deliverable; the system content-addresses and version-controls whatever policy is agreed, but does not author it.
- **The completeness of the address set** (which xpubs / addresses constitute the tenant's holdings) is management's representation; the system reconciles histories for the addresses it is given.

## 8. Residual risks, stated plainly

- **Operator + mapping common factor.** The two independent ingestion sources per chain still flow through the same operator and the same (minimal, golden-vectored) indexer-output → observation mapping code. Independence ends at the node+indexer boundary. A tenant who wants operator-independent corroboration can opt into an address-scoped third-party tertiary check (ADR-0001), at a disclosed privacy cost.
- **Finality windows.** Movements above the settled height are excluded from the two-source comparison by policy; a reorg deeper than the configured confirmation depth would require a restatement, handled through the checkpoint-supersession workflow.
- **Anchoring latency.** A freshly sealed evidence head is "anchoring-pending" until its Bitcoin transaction confirms; during that window the external-anchor guarantee is not yet in force, and the evidence/UX represents that state honestly rather than implying instant immutability.
- **Phase scope.** Several components named here as "implemented" are the *domain cores*; the live I/O shims that connect them to real venues, nodes, GLs, and the Bitcoin network are the integration layer, sequenced with the design-partner engagement. This guide describes the guarantees the system is built to provide once those shims are in place; an engagement-specific letter will state which are live for that close.

-----

*This guide is a map of the guarantees, not a substitute for the spec. Where this guide and [`treasury-suite-spec-v2.md`](treasury-suite-spec-v2.md) differ, the spec governs.*
