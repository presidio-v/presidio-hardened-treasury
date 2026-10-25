# Contributing to presidio-hardened-treasury

Thanks for your interest. This project is held to a stricter bar than a typical library — the
checklist below is what a change needs to clear before it can be merged.

## Reporting a security vulnerability

**Do not open a public issue for a security vulnerability.** Use the private reporting
process in [SECURITY.md](SECURITY.md) — GitHub Security Advisories, via the repository's
"Security" tab, or contact security@presidio-group.eu. You will get an acknowledgement
within 5 business days.

## Reporting bugs and requesting features

Open a [GitHub issue](https://github.com/presidio-v/presidio-hardened-treasury/issues). Search existing issues first.
For a bug, include:

- the commit or version you are on (`[workspace.package].version` in `Cargo.toml`) and your
  `rustc --version`
- what you expected to happen, and what happened instead
- a minimal reproduction if you can produce one

Please strip any secrets, credentials, or personal data from anything you paste into a
public issue.

## New to the project?

Issues labelled [`good first issue`](https://github.com/presidio-v/presidio-hardened-treasury/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22)
are scoped to be approachable without deep knowledge of the codebase and are a good place to
start.

## How changes are made

All changes go through a pull request against `main`. Direct pushes to `main` are blocked by
branch protection, and every PR must pass the required status checks before it can merge.

1. Fork the repository and create a branch off `main`.
2. Make your change, with tests (see the test policy below).
3. Run the local verification block until it is clean.
4. Update `CHANGELOG.md` under `## [Unreleased]`.
5. Open a PR describing what changed and why.

## Code review

Every pull request — including a maintainer's own — is reviewed before it merges. This is
not advisory: `main` requires an approving review from a code owner
([CODEOWNERS](.github/CODEOWNERS)) who is **not** the author of the change, enforced by
branch protection (required review, code-owner review, stale-approval dismissal, and
last-push re-approval; admins are included).

What a reviewer confirms before approving:

- **Tests** — new or changed functionality ships with tests; bug fixes include a regression
  test; the coverage floors hold.
- **Security reasoning** — for changes to a security-sensitive area (see below), the PR
  explains the reasoning, and no existing default is weakened without an explicit rationale.
- **Compatibility** — changes to the public API surface, event/record shapes, or exception
  types follow [SEMVER.md](SEMVER.md); breaking changes are called out.
- **Style and scope** — the linter is clean, the change is focused, and `CHANGELOG.md` is
  updated.

Reviewers approve via GitHub's review flow. A change that needs rework is returned with
specific requested changes rather than merged with caveats.

## Requirements for acceptable contributions

A change is merged when it meets all of the following.

### Style

Formatting and linting are enforced by the project linter and are not a matter of taste — CI
rejects anything that does not conform.

Formatting is `rustfmt` (`cargo fmt --all --check` in CI). Linting is `clippy` with
`-D clippy::all`; the workspace lint table in `Cargo.toml` additionally denies
`unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `arithmetic_side_effects` and
`float_arithmetic`, and forbids `unsafe_code`. `clippy::pedantic` is advisory (warn). The
toolchain is pinned in `rust-toolchain.toml`; `rustdoc` must build without errors.

Each module uses a single consistent import (or include) style. Do not mix conventions for
the same dependency within one module.

### Tests

**Test policy: any change that adds or modifies functionality must ship with tests in the
same pull request.** Bug fixes must include a regression test that fails before the fix and
passes after it. This is enforced in review, and by the coverage gate.

CI enforces a workspace **line-coverage floor of 90%** (`cargo llvm-cov --workspace
--all-features --fail-under-lines 90`, the `coverage` job in `.github/workflows/ci.yml`).
Changes to hashing, canonicalization or any event envelope must also update the golden
vectors, recomputed by an independent implementation, in the same pull request.


### Security-sensitive changes

This project's security controls are the product. If your change touches any of the
security-sensitive modules, then the reviewer bar above applies in full:

- `treasury-evidence` — canonical JSON (`canon`), SHA-256 addressing, RFC 6962 Merkle
  trees, the durable store's verify-on-open
- `treasury-ledger` — hash chaining, `verify_chain`, supersession rules, replay-on-open
- `treasury-core` — integer money, content hashes, `dual_control`
- `treasury-anchor` — receipts, coverage monotonicity, the anchoring state machine
- `treasury-ingest` — egress allowlist and key-scope gate
- `treasury-chainsource` — two-source reconciliation, finality and reproducibility gates
- `treasury-posting` and `treasury-gl` — the only outbound write path and read-back
- `treasury-policy` — policy artifact hashing and approval requirements
- `treasury-conformance` — the contracts live shims must pass; weakening one weakens
  every future shim

- explain the security reasoning in the PR description, not only the mechanics
- do not weaken a default. New controls are opt-in; relaxations of existing controls need
  an explicit rationale
- never re-implement cryptographic primitives — call a vetted standard library or crypto
  dependency instead
- functions that produce a stable serialized or digest output are byte-stability contracts.
  Changing their output for existing input is a breaking change even if no signature changes

### Public API and compatibility

The public API surface and what counts as a breaking change are defined in
[SEMVER.md](SEMVER.md). Read it before changing anything exported from the public API, and
note that event/record shapes and exception types are part of the contract that downstream
consumers depend on.

### Dependencies

New runtime dependencies are a high bar for a security-focused library and need justification
in the PR. Prefer the standard library. The domain crates deliberately depend only on
`serde`, `serde_json`, `sha2`, `hex` and `thiserror`; networking and I/O libraries belong in
out-of-core shims behind the trait seams, never in the accounting core. Every new crate must
pass `cargo deny check all` (license allowlist, advisories, crates.io only).

## Local verification

Run this before opening a PR, and fix anything it reports:

```bash
cargo fmt --all --check \
  && cargo clippy --workspace --all-targets --all-features -- -D clippy::all \
  && cargo test --workspace --all-features \
  && cargo deny check all
```

`rustup` picks up the pinned toolchain from `rust-toolchain.toml` automatically. CI runs the
same steps plus the coverage gate, CodeQL and a short fuzz run; a change must pass all of
them.

## Commit messages

Write in the imperative mood ("add TTL bound", not "added" or "adds"). Explain *why* the
change is being made where that is not obvious from the diff.

## Licensing and Developer Certificate of Origin (DCO)

The project is MIT licensed, and contributions are accepted under the same
terms (inbound = outbound).

To assert that you have the right to submit your contribution, every commit must
be **signed off** under the [Developer Certificate of Origin](https://developercertificate.org/)
1.1. Signing off means adding a `Signed-off-by` line to the commit message with
your real name and email:

```
Signed-off-by: Jane Developer <jane@example.com>
```

`git commit -s` adds this line for you. By signing off you certify the DCO —
in short, that you wrote the change or otherwise have the right to submit it
under the project's MIT license. Pull requests whose commits are not signed off
will be asked to amend before merge.

## Code of conduct

Participation is governed by the [Code of Conduct](CODE_OF_CONDUCT.md).
