---
status: working sheet
owner: vstantch
target: OpenSSF Best Practices Badge — passing level
project_url: https://github.com/presidio-v/presidio-hardened-treasury
---

# CII Best Practices — passing-level answer sheet

Answers for <https://www.bestpractices.dev> (passing level), filled from the
evidence in this repository. Every cited URL must resolve on `main` before the
answers are submitted.

## Before you start

1. **Register the URL as exactly** `https://github.com/presidio-v/presidio-hardened-treasury`.
   Scorecard does a literal DB string match. A trailing slash or `www.` returns
   `NotFound` → score 0 despite a real badge.
2. **Log in with GitHub but decline the org grant.** BadgeApp requests `read:org`
   and no code path consumes it.
3. **Merge the hardening PR first** — `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`,
   `SECURITY.md`, `CHANGELOG.md`, `SEMVER.md`, `ASSURANCE.md` must be on `main`.
4. Record the badge id in `hardening.toml` (`[badge] bestpractices_id`).

Shorthand below: `REPO` = `https://github.com/presidio-v/presidio-hardened-treasury`.

---

## Basics — project website content

| Criterion | Status | Justification / URL |
|---|---|---|
| `description_good` | **Met** | `REPO#readme` — the README opens with what the project is (audit-grade treasury close for crypto-first organizations) and the problem it solves (a defensible quarterly close an external auditor will sign, reproducible from hashed inputs). |
| `interact` | **Met** | `REPO#readme` — README covers obtaining the source (this repository; build with `cargo`), contact (office@), security reports (`SECURITY.md`), and contributing (`CONTRIBUTING.md`); issues at `REPO/issues`. |
| `contribution` | **Met** | URL: `REPO/blob/main/CONTRIBUTING.md` — documents the fork → branch → PR flow against `main`. |
| `contribution_requirements` | **Met** | URL: `REPO/blob/main/CONTRIBUTING.md#requirements-for-acceptable-contributions` — rustfmt/clippy rules, test and coverage policy, security-change rules, dependency bar. |

## Basics — FLOSS license

| Criterion | Status | Justification / URL |
|---|---|---|
| `floss_license` | **Met** | `MIT`. |
| `floss_license_osi` | **Met** | `MIT` is OSI-approved. |
| `license_location` | **Met** | URL: `REPO/blob/main/LICENSE` |

## Basics — documentation

| Criterion | Status | Justification / URL |
|---|---|---|
| `documentation_basics` | **Met** | README (workspace map, structural guarantees, Development section with the build/test commands), `ARCHITECTURE.md`, the active spec `docs/treasury-suite-spec-v2.md`, the ADRs in `docs/adr/`, and `docs/auditor-evidence-guide.md`. |
| `documentation_interface` | **Met** | The external interface is the public Rust API, documented with `rustdoc` on every public item (`missing_docs` is a workspace lint; CI runs `cargo doc --workspace --no-deps`) and enumerated in `REPO/blob/main/SEMVER.md#what-is-the-public-api`. |

## Basics — other

| Criterion | Status | Justification / URL |
|---|---|---|
| `sites_https` | **Met** | The project site is the GitHub repository (HTTPS). The project runs no other hosted site or service. |
| `discussion` | **Met** | GitHub Issues: `REPO/issues` — searchable, URL-addressable, open, no proprietary client. |
| `english` | **Met** | All docs and issue handling in English. |
| `maintained` | **Met** | Workspace version 0.21.0; commits on `main` through October 2026 (README and research-frame updates, Dependabot merges), CI and Dependabot active. |

## Change control — repository

| Criterion | Status | Justification / URL |
|---|---|---|
| `repo_public` | **Met** | `REPO` |
| `repo_track` | **Met** | git. |
| `repo_interim` | **Met** | Every change lands through a pull request from a pushed branch; interim work is visible between versions. |
| `repo_distributed` | **Met** | git. |

## Change control — versioning

| Criterion | Status | Justification / URL |
|---|---|---|
| `version_unique` | **Met** | A single workspace version (`[workspace.package].version` in `Cargo.toml`) identifies each version; `CHANGELOG.md` records one entry per version. |
| `version_semver` | **Met** | URL: `REPO/blob/main/SEMVER.md` — Semantic Versioning with the documented pre-1.0 profile. |
| `version_tags` | **Unmet** | Versions are not yet git-tagged (stated in `CHANGELOG.md`). Becomes Met with the first signed release tag (`v0.21.0`). SUGGESTED, not a blocker. |

## Change control — release notes

| Criterion | Status | Justification / URL |
|---|---|---|
| `release_notes` | **Met** | URL: `REPO/blob/main/CHANGELOG.md` — Keep a Changelog format, hand-written per version with a "why it matters" line, not VCS log output. |
| `release_notes_vulns` | **N/A** | No publicly known vulnerability has been fixed in any version: no security advisories have been published for this repository. |

## Reporting — bug reports

| Criterion | Status | Justification / URL |
|---|---|---|
| `report_process` | **Met** | URL: `REPO/blob/main/CONTRIBUTING.md#reporting-bugs-and-requesting-features` |
| `report_tracker` | **Met** | GitHub Issues. |
| `report_responses` | **Met** | Met by the documented process. No bug reports from outside the project have been received yet; the issues on file (#49, #62) were opened by the maintainer as tracked work. |
| `enhancement_responses` | **Met** | Met by the documented process. No enhancement requests from outside the project have been received yet; #49 (x402 decision-ref evidence ingestion) is a maintainer-filed enhancement tracked in the open. |
| `report_archive` | **Met** | URL: `REPO/issues?q=is%3Aissue` — public and searchable. |

## Reporting — vulnerability reports

| Criterion | Status | Justification / URL |
|---|---|---|
| `vulnerability_report_process` | **Met** | URL: `REPO/blob/main/SECURITY.md#reporting-a-vulnerability` |
| `vulnerability_report_private` | **Met** | URL: `REPO/blob/main/SECURITY.md#reporting-a-vulnerability` — private GitHub Security Advisory via the Security tab, or security@presidio-group.eu; acknowledgement within 72 hours. |
| `vulnerability_report_response` | **N/A** | No vulnerability reports have been received in the last 6 months (no advisories, draft or published, exist for this repository). |

## Quality — build system

| Criterion | Status | Justification / URL |
|---|---|---|
| `build` | **Met** | `cargo build --workspace` rebuilds everything from source; the toolchain is pinned in `rust-toolchain.toml`. |
| `build_common_tools` | **Met** | `rustup`, `cargo` and `rustc`, the standard Rust toolchain. |
| `build_floss_tools` | **Met** | The entire toolchain is FLOSS. |

## Quality — automated test suite

| Criterion | Status | Justification / URL |
|---|---|---|
| `test` | **Met** | Unit tests in every crate plus integration tests under `crates/*/tests/`: property tests (`proptest`), golden vectors cross-checked against an independent Python implementation, the `treasury-conformance` contract suites, and `treasury-e2e`, which runs the whole close twice. How to run: `CONTRIBUTING.md#local-verification`. |
| `test_invocation` | **Met** | `cargo test --workspace --all-features`. |
| `test_most` | **Met** | CI enforces a 90% workspace line-coverage floor (`cargo llvm-cov --fail-under-lines 90`, `coverage` job in `.github/workflows/ci.yml`); measured 93.8% lines / 92.3% regions on 2026-10-25. |
| `test_continuous_integration` | **Met** | GitHub Actions on every push to `main` and every PR (`.github/workflows/ci.yml`): rustfmt, clippy, tests, docs, coverage, fuzz smoke, cargo-deny on Ubuntu with the pinned toolchain (1.88.0). |

## Quality — new functionality testing

| Criterion | Status | Justification / URL |
|---|---|---|
| `test_policy` | **Met** | URL: `REPO/blob/main/CONTRIBUTING.md#tests` — functional changes ship with tests; fixes ship with regression tests. |
| `tests_are_added` | **Met** | Example: PR #37 (confirmation depth as a content-addressed `AnchorPolicy`, v0.21.0) shipped with `crates/treasury-anchor/tests/anchor_lifecycle.rs`, `anchoring_pipeline.rs`, the anchor-submitter conformance contract and an updated `treasury-e2e` golden close in the same change. PR #29 (agreed-reconciliation → L1 mapping) added `crates/treasury-chainsource/tests/observation_mapping.rs`. |
| `tests_documented_added` | **Met** | The policy is stated in the contribution instructions themselves (`CONTRIBUTING.md#tests`). |

## Quality — warning flags

| Criterion | Status | Justification / URL |
|---|---|---|
| `warnings` | **Met** | `cargo clippy --workspace --all-targets --all-features -- -D clippy::all` and `cargo fmt --check` in CI; rustc warnings plus `missing_docs`. |
| `warnings_fixed` | **Met** | CI fails on any clippy finding; `main` is clean. |
| `warnings_strict` | **Met** | Beyond defaults, the workspace lint table denies `unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `arithmetic_side_effects`, `float_arithmetic`, forbids `unsafe_code`, and enables `clippy::pedantic` as warnings (`Cargo.toml` `[workspace.lints]`). |

## Security — secure development knowledge

| Criterion | Status | Justification / URL |
|---|---|---|
| `know_secure_design` | **Met** | The design applies fail-closed gates throughout, least privilege (no keys, no network in the core), defence in depth (hash chain + external anchoring + dual control + replay-on-open), and makes dangerous states unconstructible. Argued in `REPO/blob/main/ASSURANCE.md#3-secure-design-principles-applied`; operator-in-scope threat model in `docs/threat-model.md`. |
| `know_common_errors` | **Met** | `docs/threat-model.md` (STRIDE per trust boundary) and `ASSURANCE.md#4-common-implementation-weaknesses-countered` map input validation, integer overflow, resource exhaustion, memory safety, crypto misuse, secrets and vulnerable dependencies to named countermeasures. |

## Security — cryptographic practices

| Criterion | Status | Justification / URL |
|---|---|---|
| `crypto_published` | **Met** | SHA-256 (FIPS 180-4) for content addressing and hash chaining; RFC 6962 Merkle tree hashing. |
| `crypto_call` | **Met** | SHA-256 comes from the RustCrypto `sha2` crate; no primitive is re-implemented. |
| `crypto_floss` | **Met** | `sha2` is MIT/Apache-2.0. |
| `crypto_keylength` | **Met** | 256-bit digests. The project uses no keys. |
| `crypto_working` | **Met** | No MD4, MD5, single DES, RC4 or Dual_EC_DRBG anywhere in the codebase or its dependency graph. |
| `crypto_weaknesses` | **Met** | No SHA-1 and no CBC-mode use. |
| `crypto_pfs` | **N/A** | The project implements no key-agreement or network protocol. |
| `crypto_password_storage` | **N/A** | Stores no passwords. |
| `crypto_random` | **N/A** | The project generates no keys, nonces or other security-relevant random values. (`rand` appears only as a dev-dependency of `proptest`.) |

## Security — delivery

| Criterion | Status | Justification / URL |
|---|---|---|
| `delivery_mitm` | **Met** | Distributed as source from GitHub over HTTPS/SSH only; no package-registry publishing. |
| `delivery_unsigned` | **Met** | No hash is fetched over plain HTTP. |

## Security — known vulnerabilities

| Criterion | Status | Justification / URL |
|---|---|---|
| `vulnerabilities_fixed_60_days` | **Met** | No known unpatched medium+ vulnerabilities. `cargo-deny check all` (RustSec advisory database, yanked crates) is a required CI check; Dependabot security updates enabled. |
| `vulnerabilities_critical_fixed` | **Met** | No critical vulnerability has arisen in the project or its dependencies; `cargo-deny` would fail CI on one. |

## Security — other

| Criterion | Status | Justification / URL |
|---|---|---|
| `no_leaked_credentials` | **Met** | gitleaks 8.21.2 over the full history (all refs, 125 commits) on 2026-10-25: no leaks. GitHub secret scanning with push protection is enabled and has no alerts. |

## Analysis — static

| Criterion | Status | Justification / URL |
|---|---|---|
| `static_analysis` | **Met** | CodeQL for Rust and GitHub Actions (`.github/workflows/codeql.yml`, results in GitHub code scanning) and clippy with the strict deny-list on every PR. |
| `static_analysis_common_vulnerabilities` | **Met** | CodeQL `security-and-quality` queries; clippy's `arithmetic_side_effects`, `indexing_slicing`, `unwrap_used` target overflow and panic classes; `cargo-deny` covers known-vulnerable dependencies. |
| `static_analysis_fixed` | **Met** | Findings are fixed before merge; CI blocks on clippy and cargo-deny findings. |
| `static_analysis_often` | **Met** | CodeQL runs on every push and PR to `main`, plus a weekly scheduled run. |

## Analysis — dynamic

| Criterion | Status | Justification / URL |
|---|---|---|
| `dynamic_analysis` | **Met** | cargo-fuzz (libFuzzer) target `canonical_json` (`fuzz/`) feeds arbitrary bytes through JSON parsing and `treasury_evidence::canonical_bytes`, asserting no panic and that canonical output is a fixed point; runs 60 s on every PR (`fuzz` job). Property-based tests (`proptest`) cover the integer invariants. |
| `dynamic_analysis_unsafe` | **N/A** | The project is written in safe Rust (`unsafe_code = "forbid"` in every crate). |
| `dynamic_analysis_enable_assertions` | **Met** | Tests and fuzz targets are assertion-based; the fuzz build enables debug assertions, and release builds keep `overflow-checks = true`. |
| `dynamic_analysis_fixed` | **Met** | No unfixed medium+ findings. |
