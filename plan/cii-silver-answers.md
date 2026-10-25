---
status: working sheet
owner: vstantch
target: OpenSSF Best Practices Badge — silver level
project_url: https://github.com/presidio-v/presidio-hardened-treasury
---

# CII Best Practices — SILVER answer sheet

Answers for the **silver** tab at bestpractices.dev, filled from the evidence in
this repository. It covers only the criteria silver *adds* on top of passing;
passing answers carry over unchanged (see `cii-passing-answers.md`).

Preconditions before submitting silver (each is a separate human gate):

- passing badge earned;
- `ceoofcyber` added as a write collaborator and code owner, and required
  code-owner review enabled on `main` (two-person gate);
- a first signed release tag (`v0.22.0`) pushed, so `signed_releases` and
  `version_tags_signed` can move from Unmet to Met.

`REPO` = `https://github.com/presidio-v/presidio-hardened-treasury`.

## Backing docs

- `GOVERNANCE.md` — governance model, roles, continuity
- `ARCHITECTURE.md` — components, core flow, trust boundaries
- `ASSURANCE.md` — consolidated security assurance case (`assurance_case`)
- `allowed_signers` — public release signing key for local tag verification
- `CONTRIBUTING.md` — DCO sign-off requirement
- `SECURITY.md` — reporter credit, how to obtain signing keys, assurance-case link
- `README.md` — 12-month roadmap + links to GOVERNANCE/ARCHITECTURE/ASSURANCE

---

## Governance & continuity

| Criterion | Status | Justification to paste |
|---|---|---|
| `dco` | **Met** | Every commit must carry a DCO `Signed-off-by` line (`git commit -s`); enforced in review. Documented at `REPO/blob/main/CONTRIBUTING.md#licensing-and-developer-certificate-of-origin-dco`. Inbound = outbound `MIT`. |
| `code_of_conduct` | **Met** | Contributor Covenant at `REPO/blob/main/CODE_OF_CONDUCT.md` (standard location). |
| `governance` | **Met** | Maintainer-led, single-steward model documented at `REPO/blob/main/GOVERNANCE.md`: ordinary changes decided on the PR, security-relevant changes need a written rationale and may not weaken a default, API changes follow `SEMVER.md`, disagreements escalate to PRESIDIO Group. |
| `roles_responsibilities` | **Met** | Key roles (steward org, maintainer, security contact, release manager, contributor) documented at `REPO/blob/main/GOVERNANCE.md#roles-and-responsibilities`. |
| `access_continuity` | **Met** | Continuity rests on the steward organisation: the repository is owned by the `presidio-v` GitHub org, not a personal account; the project publishes no registry package, so no personal publishing token exists; the release signing key is held in the organisation password manager and is recoverable, with its public half in `allowed_signers`; the release procedure is written down (`GOVERNANCE.md#project-continuity`, `SECURITY.md`); and a second person (`ceoofcyber`) holds write access and code-owner review rights. URL: `REPO/blob/main/GOVERNANCE.md#project-continuity`. |
| `bus_factor` (SHOULD) | **Met** | Two people can review and merge (`vstantch`, `ceoofcyber`, both code owners); repository, signing key and release flow are organisation-held and documented. See `REPO/blob/main/GOVERNANCE.md#project-continuity`. |

## Documentation

| Criterion | Status | Justification to paste |
|---|---|---|
| `documentation_roadmap` | **Met** | `REPO#roadmap-next-12-months` — now (design-partner selection, deciding Proposed ADRs 0005–0008), next (Phase 1 live I/O shims against a partner stack, first live close), later (enclave xpub derivation, auditor UX, Phase 2 provenance and SOC 2). |
| `documentation_architecture` | **Met** | `REPO/blob/main/ARCHITECTURE.md` — claim-layer model, crate layering, close pipeline, I/O seams, trust boundaries; linked from the README. |
| `documentation_security` | **Met** | `REPO/blob/main/SECURITY.md` (controls, reporting), `REPO/blob/main/docs/threat-model.md` (STRIDE per boundary, operator in scope), `REPO/blob/main/ASSURANCE.md` (assurance case). |
| `documentation_quick_start` | **Met** | README "Development" section: clone, then `cargo fmt --all --check`, `cargo clippy …`, `cargo test --workspace` with the pinned toolchain; `docs/auditor-evidence-guide.md` is the quick start for auditors reproducing a close. |
| `documentation_current` | **Met** | Docs are updated in the same PR as the code they describe (`PRESIDIO-REQ.md` delivery status, `CHANGELOG.md` per version, ADRs). |
| `documentation_achievements` | **Met** | The OpenSSF Best Practices badge is displayed and hyperlinked on the README front page (added with the badge id after registration). |

## Change control & reporting

| Criterion | Status | Justification to paste |
|---|---|---|
| `contribution_requirements` | **Met** | `REPO/blob/main/CONTRIBUTING.md#requirements-for-acceptable-contributions` — style, tests, security-change rules, dependency bar. |
| `report_tracker` | **Met** | GitHub Issues: `REPO/issues`. |
| `maintenance_or_update` | **Met** | `REPO/blob/main/SECURITY.md#supported-versions` states which line is supported; `REPO/blob/main/SEMVER.md` documents the upgrade path and what counts as a breaking change. |
| `vulnerability_report_credit` | **Met** | `REPO/blob/main/SECURITY.md#reporting-a-vulnerability` — reporters are credited by name in the published advisory and the CHANGELOG entry unless they request anonymity. |
| `vulnerability_response_process` | **Met** | `REPO/blob/main/SECURITY.md#reporting-a-vulnerability` — private GitHub Security Advisory intake, acknowledgement within 72 hours, fix target 30 days. |

## Quality & testing

| Criterion | Status | Justification to paste |
|---|---|---|
| `tests_documented_added` | **Met** | `REPO/blob/main/CONTRIBUTING.md#tests` states the policy that changes adding/modifying functionality ship with tests in the same PR. |
| `test_policy_mandated` | **Met** | Formal written policy at `REPO/blob/main/CONTRIBUTING.md#tests`: functionality changes ship with tests; bug fixes include a regression test. Enforced in review and by the coverage gate. |
| `automated_integration_testing` | **Met** | `REPO/blob/main/.github/workflows/ci.yml` runs the full suite on every push and PR with the pinned toolchain, including the `treasury-conformance` contract suites and the `treasury-e2e` whole-close determinism test. |
| `regression_tests_added50` | **Met** | Policy requires a regression test with every bug fix. In the last 6 months no product-code defect fix has been merged (the `fix` commits concern CI configuration); development has been test-first, e.g. PR #34 added adversarial fixtures proving each conformance suite rejects a violating implementation. |
| `test_statement_coverage80` | **Met** | CI gate `cargo llvm-cov --workspace --all-features --fail-under-lines 90` (`coverage` job, `.github/workflows/ci.yml`); measured 93.8% lines on 2026-10-25. |
| `warnings_strict` | **Met** | clippy `-D clippy::all` plus workspace denies on `unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `arithmetic_side_effects`, `float_arithmetic`; `unsafe_code` forbidden; `clippy::pedantic` as warnings. CI fails on any denied finding. |
| `coding_standards` | **Met** | `REPO/blob/main/CONTRIBUTING.md#style` — rustfmt and clippy, configured in `Cargo.toml` `[workspace.lints]` and `rust-toolchain.toml`. |
| `coding_standards_enforced` | **Met** | `cargo fmt --all --check` and clippy run in CI on every PR and are required checks. |
| `installation_common` | **Met** | Consumed as a Cargo git dependency, e.g. `treasury-core = { git = "https://github.com/presidio-v/presidio-hardened-treasury", tag = "v0.22.0" }`; built with `cargo build`. |
| `installation_development_quick` | **Met** | `REPO/blob/main/CONTRIBUTING.md#local-verification` — clone and run the four cargo commands; `rustup` installs the pinned toolchain automatically. |
| `build_repeatable` (SHOULD) | **Met** | `Cargo.lock` is committed, the toolchain is pinned (`rust-toolchain.toml`, 1.88.0), crates come only from crates.io (`cargo-deny` sources check), Actions are SHA-pinned and the Docker base image is digest-pinned, so a rebuild resolves the same sources. Bit-for-bit reproducible builds are not claimed (REQ-33, Phase 2). |
| `build_standard_variables` | **Met** | Built with plain `cargo`; there are no `build.rs` scripts and no hard-coded compiler or linker flags, so `RUSTFLAGS`, `CC` and `CARGO_*` variables pass through unchanged. |
| `build_preserve_debug` | **Met** | Debug information is controlled by standard Cargo profiles; nothing strips it (no `strip` setting; `[profile.release]` only enables `overflow-checks`). |
| `build_non_recursive` | **Met** | A single Cargo workspace build; no recursive make or subdirectory builds. |
| `installation_standard_variables` | **N/A** | The workspace is a set of libraries consumed through Cargo; there is no install step to which `DESTDIR`-style conventions apply. |

## Dependencies & components

| Criterion | Status | Justification to paste |
|---|---|---|
| `external_dependencies` | **Met** | Listed machine-readably in `Cargo.toml` (`[workspace.dependencies]` and each crate's manifest) and pinned in `Cargo.lock`. |
| `updateable_reused_components` | **Met** | All reused components are crates.io packages managed by Cargo (no vendored copies); Dependabot tracks Cargo, Actions and Docker updates. |
| `interfaces_current` | **Met** | Dependencies are kept current by Dependabot (e.g. `thiserror` 2.0.21, `sha2` 0.11); `cargo-deny` rejects yanked crates; deprecation warnings fail clippy. |

## Security

| Criterion | Status | Justification to paste |
|---|---|---|
| `assurance_case` | **Met** (URL required) | URL: `REPO/blob/main/ASSURANCE.md`. Threat model, trust boundaries (B1–B7), secure-design argument and common-weakness argument, built on `docs/threat-model.md`. |
| `implement_secure_design` | **Met** | Fail-safe: every gate fails closed (unassessed assets, missing prices, divergence all block). Complete mediation: canonicalization and append-time provenance checks are the only path into the ledger; logs replay-verify on open. Least privilege: no keys or network in the core. Defence in depth: hash chain + external anchoring + dual control. Economy of mechanism: one encoding, one hash (SHA-256 via `sha2`), unconstructible invalid states. See `REPO/blob/main/ASSURANCE.md#3-secure-design-principles-applied`. |
| `input_validation` | **Met** | Data crossing the untrusted-input boundary is validated before use: `treasury_evidence::canonical_bytes` rejects any non-integer number and nesting beyond 128 levels before anything is hashed or booked; the egress allowlist admits only exact endpoint/method matches; chain history is admitted only when two independent sources agree. See `ARCHITECTURE.md#trust-boundaries`. |
| `hardening` | **Met** | `#![forbid(unsafe_code)]` workspace-wide; strict clippy deny-list; `overflow-checks = true` in release; SHA-pinned GitHub Actions with least-privilege `permissions:`; digest-pinned Docker base image; `cargo-deny` registry pinning and license allowlist; secret scanning with push protection. |
| `crypto_weaknesses` | **Met** | The only cryptographic function is SHA-256 (plus RFC 6962 Merkle hashing over it); no MD5, SHA-1 or DES. |
| `crypto_algorithm_agility` (SHOULD) | **N/A** | The project negotiates no algorithms. SHA-256 is fixed by the content-addressing format; migrating it would be a versioned, breaking format change with new golden vectors (`SEMVER.md#schemawire-stability`), not a runtime switch. |
| `crypto_credential_agility` | **N/A** | The project stores and uses no keys or credentials. |
| `crypto_used_network` | **N/A** | The project makes no network calls; live I/O shims are out of tree (Phase 1). |
| `crypto_tls12` | **N/A** | No network communication. |
| `crypto_certificate_verification` | **N/A** | No TLS client. |
| `crypto_verification_private` | **N/A** | No network communication. |
| `signed_releases` | **Unmet** | No release has been cut yet. Becomes Met with the first SSH-signed tag (`v0.22.0`), verifiable with `allowed_signers` as documented at `REPO/blob/main/SECURITY.md#verifying-releases-and-obtaining-public-signing-keys`. |
| `version_tags_signed` | **Unmet** | As above: Met once release tags exist; each will be SSH-signed with the org key and shown as Verified on GitHub. |
| `sites_password_security` | **N/A** | The project runs no site that stores passwords. |

## Analysis & monitoring

| Criterion | Status | Justification to paste |
|---|---|---|
| `static_analysis_common_vulnerabilities` | **Met** | CodeQL for Rust and Actions (`REPO/blob/main/.github/workflows/codeql.yml`) on every push/PR and weekly; clippy security-relevant denies (`arithmetic_side_effects`, `indexing_slicing`, `unwrap_used`); OpenSSF Scorecard weekly. |
| `dynamic_analysis_unsafe` | **N/A** | Safe Rust only (`unsafe_code = "forbid"`); the cargo-fuzz target runs on every PR regardless. |
| `dependency_monitoring` | **Met** | Dependabot (Cargo, Actions, Docker) and Dependabot security updates; `cargo-deny check all` against the RustSec advisory database on every PR; OpenSSF Scorecard weekly. |

## Accessibility & internationalization

| Criterion | Status | Justification to paste |
|---|---|---|
| `accessibility_best_practices` | **N/A** | A library workspace with no graphical or end-user UI (the auditor-facing UX is Phase 1 and not in this repository). |
| `internationalization` | **N/A** | No user-facing localizable strings. |
