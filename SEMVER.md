# Stability & semver guarantees — presidio-hardened-treasury

For downstream integrators depending on this project.

## What is the public API

The workspace ships as one versioned unit: every crate carries the single
`[workspace.package].version` from the root `Cargo.toml`. The public API is:

- every `pub` item reachable from the crate root of the library crates
  `treasury-core`, `treasury-evidence`, `treasury-ledger`, `treasury-policy`,
  `treasury-close`, `treasury-anchor`, `treasury-ingest`, `treasury-reconcile`,
  `treasury-scope`, `treasury-lots`, `treasury-fairvalue`, `treasury-gaap`,
  `treasury-posting`, `treasury-disclosure`, `treasury-gl`,
  `treasury-chainsource` and `treasury-conformance`, with the signatures and
  error variants shown in their `rustdoc`;
- the seam traits `ChainSource`, `GlAdapter` and `ChainAnchorSubmitter`, and
  the `verify_*_contract` suites in `treasury-conformance` that a shim must
  pass;
- the byte-level formats listed under [Schema/wire stability](#schemawire-stability).

`treasury-e2e` is a test crate and is not part of the API. The in-memory
fixtures (for example `treasury_gl::fixture`) exist to drive tests and the
conformance suites; their behaviour may change in any release.

## Versioning rules (semver, pre-1.0 profile)

- **Patch (0.x.Y):** bug fixes, security fixes, dependency floor bumps. No API
  change, no behaviour change except the fixed defect. Safe to auto-upgrade; this
  is the channel security releases ship on.
- **Minor (0.X.0):** while the project is pre-1.0, a minor version may carry
  breaking changes to the API and to the ledger and evidence formats as they
  stabilise through Phase 0–1 (see [README](README.md#versioning)). Each one is
  called out in `CHANGELOG.md`.
- **Major (1.0.0+):** from 1.0 on, breaking changes require a major version.

**Pin guidance for integrators:** depend on the workspace by git tag (for
example `rev`/`tag` in a Cargo git dependency), pin to an exact release, and run
the verification step below in your CI on every upgrade.

## Behavioural guarantees (stronger than API stability)

These are security invariants, not just interfaces; weakening any of them is
treated as a breaking change regardless of which version component moves.

- **Floats never become evidence.** Canonicalization rejects every non-integer
  JSON number and any value nested deeper than `MAX_DEPTH` (128).
- **Integer money only.** Amounts are integer base units with checked
  arithmetic; overflow is a typed error, never a wrap or a panic.
- **Append-only, hash-chained ledger.** Events supersede and never mutate;
  supersession cannot cross tenants or claim layers; `verify_chain` detects any
  mutation, insertion or deletion.
- **Durable logs verify on open.** A tampered or truncated ledger or evidence
  file refuses to load; a torn tail recovers only through an explicit call.
- **Dual control is structural.** An L3 judgment without a distinct approver
  and a policy hash cannot be constructed.
- **Fail closed.** Unassessed assets, missing prices, missing materiality
  thresholds, empty scope reports, unclassified legs and two-source divergence
  all block rather than default.
- **Deny by default on egress.** The allowlist admits only exact
  endpoint/method matches.
- **Read-back is mandatory.** A GL adapter cannot be written without a
  read-back method; a lost acknowledgement resolves only through read-back.
- **Coverage-monotonic anchoring.** A receipt covering fewer entries than its
  predecessor is rejected.
- **Whole-close determinism.** The same inputs and policy produce the same
  disclosure-pack hash.

## Verifying an installation

From a checkout of the release tag:

```bash
git -c gpg.ssh.allowedSignersFile=allowed_signers verify-tag vX.Y.Z
cargo test --workspace --all-features
```

A passing result is a good signature and exit code 0. The test suite includes
the golden vectors for every hash envelope and `treasury-e2e`, which runs the
full close twice and asserts an identical disclosure-pack hash. A live shim is
verified by running the matching `treasury-conformance` suite against it.

## Schema/wire stability

Event-identity hashing, the canonical-JSON rules and every content-addressed
envelope (events, evidence blobs, policy artifacts, receipts, posting batches,
disclosure packs) are a compatibility surface. Any change to the bytes they
produce for existing input is breaking, ships in a new minor (pre-1.0) or major
(1.0+) version, and comes with new golden vectors recomputed by an independent
implementation. Merkle tree heads follow RFC 6962. The on-disk formats of the
durable ledger and evidence backends are not yet stable across minor versions
before 1.0; the release notes state when a migration is required.

## Security response

See [SECURITY.md](SECURITY.md). Security fixes ship as patch releases on the
latest minor; any minimum-safe dependency floors are bumped in the same release.
