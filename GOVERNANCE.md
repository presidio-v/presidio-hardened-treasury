# Project Governance

`presidio-hardened-treasury` is developed and maintained by **PRESIDIO Group**
and published under the `presidio-v` GitHub organisation. This document
describes how the project is governed, who holds which responsibilities, and how
the project continues to function if any single individual becomes unavailable.

## Governance model

The project follows a **maintainer-led, single-steward** model.
PRESIDIO Group is the steward organisation: it owns the repository,
the release infrastructure, and the published artefacts, and it sets the security
bar the project is held to.

Day-to-day technical decisions are made by the maintainers by rough consensus:

- **Ordinary changes** (bug fixes, tests, documentation, dependency floors) are
  decided by the reviewing maintainer on the pull request.
- **Security-relevant changes** (anything touching the project's
  security-sensitive modules, listed below) require an explicit security
  rationale in the pull request and must not weaken an existing default. See
  [CONTRIBUTING.md](CONTRIBUTING.md#security-sensitive-changes).
  For this project they are canonicalization and content addressing
  (`treasury-evidence`), the hash-chained ledger and its replay-on-open
  (`treasury-ledger`), dual control and integer money (`treasury-core`),
  external anchoring (`treasury-anchor`), the egress allowlist and scope gate
  (`treasury-ingest`), two-source chain reconciliation (`treasury-chainsource`),
  the GL posting path (`treasury-posting`, `treasury-gl`), policy artifacts
  (`treasury-policy`), and the seam contracts (`treasury-conformance`).
- **Public-API and compatibility changes** are governed by [SEMVER.md](SEMVER.md);
  a breaking change requires a major/minor bump per the documented policy.
- **Disagreements** that cannot be resolved on the pull request are escalated to
  the steward organisation (PRESIDIO Group), whose decision is final.

There is no pay-to-play influence: contribution weight is earned through review
history and sustained, high-quality contribution, not funding.

## Roles and responsibilities

| Role | Held by | Responsibilities |
|---|---|---|
| **Steward organisation** | PRESIDIO Group (`presidio-v` GitHub org) | Owns the repository and release infrastructure; holds and can recover all release credentials; final escalation authority; ensures continuity (see below). |
| **Maintainer** | PRESIDIO Group engineering | Reviews and merges pull requests; enforces the test, style, and security bars; cuts releases; triages issues; is the point of accountability for the codebase. |
| **Security contact** | PRESIDIO Group security | Receives and triages private vulnerability reports (see [SECURITY.md](SECURITY.md)); coordinates fixes, advisories, and reporter credit. Reachable at `security@presidio-group.eu` and via GitHub Security Advisories. |
| **Release manager** | PRESIDIO Group engineering | Owns the signed-tag release flow and verifies each release tag shows as Verified. Build provenance and SBOMs are not produced yet (REQ-33, Phase 2). |
| **Contributor** | Anyone | Opens issues and pull requests under [CONTRIBUTING.md](CONTRIBUTING.md) and the [Code of Conduct](CODE_OF_CONDUCT.md). |

Roles are held by function within PRESIDIO Group rather than being
tied to one named individual, so responsibilities transfer without a change to
this document.

## Becoming a maintainer

A contributor with a sustained track record of merged, high-quality changes —
particularly to the security-sensitive modules — may be invited by the steward
organisation to become a maintainer. Maintainership is granted by
PRESIDIO Group and recorded by adding the person to the
`presidio-v` organisation with the appropriate repository role.

## Project continuity

The project is structured so that it continues to function — issues can be
triaged and closed, contributions can be reviewed and accepted, and new versions
can be released — within one week even if any single individual becomes
unavailable. This is a property of the **organisation**, not of any one person:

- **Repository ownership** is held by the `presidio-v` GitHub organisation,
  not by a personal account. Organisation owners can grant repository and release
  access to another member at any time.
- **Release credentials are not personal.** The workspace is distributed as
  source from this repository; it is not published to a package registry, so
  there is no registry API token at all. Releases are SSH-signed git tags. The
  release signing key is held in the organisation's password manager (custody
  ultimately with PRESIDIO Group's leadership), not solely on any one
  contributor's machine, so it is recoverable; its public half is in
  [`allowed_signers`](allowed_signers).
- **The release process is documented** in
  [SECURITY.md](SECURITY.md#verifying-releases-and-obtaining-public-signing-keys):
  bump `[workspace.package].version`, move the `CHANGELOG.md` entries out of
  `[Unreleased]`, merge through the two-person gate, then create and push a
  signed tag (`git tag -s vX.Y.Z`). Any authorised PRESIDIO Group engineer can
  cut a release by following it.
- **PRESIDIO Group is a staffed organisation** with more than one
  person able to assume the maintainer, security-contact, and release-manager
  roles above.

The project's bus factor is therefore backed by the steward organisation rather
than a lone maintainer.
