# presidio-hardened-treasury

Audit-grade treasury close for crypto-first organizations. Flagship of the
[presidio-hardened](https://github.com/presidio-v) family.

The deliverable is not a dashboard: it is a **defensible quarterly close an
external auditor will sign**. See [`docs/treasury-suite-spec-v2.md`](docs/treasury-suite-spec-v2.md)
(active spec; v1, the two adversarial reviews, and the remediation analysis
that produced v2 are in [`docs/`](docs/)).

## Workspace

| Crate | Purpose |
|-------|---------|
| `treasury-core` | Domain primitives: identifier newtypes, integer money (no floats anywhere in the accounting path), bitemporal timestamps, SHA-256 content hashes |
| `treasury-evidence` | Content-addressed evidence store: float-rejecting canonical JSON, SHA-256 addressing, RFC 6962 Merkle tree heads for external anchoring |
| `treasury-ledger` | The core: claim-layered (observations → derived facts → judgments → policy outputs), bitemporal, append-only, hash-chained event ledger |

## Structural guarantees (Phase 0)

Enforced at append time, not by review:

- Append-only with per-tenant hash chaining; `verify_chain` detects any
  post-hoc mutation, insertion, or deletion.
- Bitemporal: "what did the books say as of the 10-Q filing" is a query
  (`as_of`), not an archaeology project. Corrections supersede; they never
  mutate, and supersession cannot race, cross tenants, or cross claim layers.
- Layer-specific mandatory provenance: a judgment without an approver and a
  content-addressed policy hash cannot enter the ledger.
- Floats reject at the canonicalization boundary: a value that cannot hash
  identically on every toolchain is not evidence.
- Event identity hashes are cross-verified against an independent Python
  implementation (golden vectors in the test suite).

## Development

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D clippy::all
cargo test --workspace
```

Toolchain is pinned in `rust-toolchain.toml`. All first-party crates are
`#![forbid(unsafe_code)]`; `cargo-deny` gates licenses, advisories, and
registry sources in CI.

## License

MIT — see [LICENSE](LICENSE).
