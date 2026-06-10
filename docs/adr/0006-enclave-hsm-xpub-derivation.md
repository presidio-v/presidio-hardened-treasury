# ADR-0006: Enclave/HSM vendor for xpub derivation (REQ-11)

**Status:** Proposed
**Date:** 2026-06-10
**Deciders:** CTO, security lead, infra/SRE lead (proposers) — design-partner's audit firm consulted before ratification
**Resolves:** spec v2 §9 open decision ("Enclave technology vendor — requirements fixed in §3.4: per-session remote attestation, documented rotation/availability") · concretises REQ-11
**Related:** [ADR-0001](0001-chain-indexing-build-vs-buy.md) (privacy thesis; xpub/address tree is the asset) · [ADR-0005](0005-approval-identity-binding.md) (shared HSM boundary) · threat model A2 (xpub theft), A1 (treasury posture) · spec §3.4
**Supersedes:** none

-----

## Context

REQ-11 and §3.4 are unambiguous about the *requirement*: "xpubs are secrets — they leak the entire address tree. Derivation happens in an enclave with **remote attestation verified on every derivation session**; … persist only derived addresses, never the master key." The threat model names the xpub/address tree as primary asset A2 and treasury posture as A1; ADR-0001 made data-sourcing the trust boundary precisely so the address set never leaks. The xpub is the seed of that address set — a software-resident xpub is extractable, and an extracted xpub hands the adversary A2 in full (it reconstructs every past and future address, hence the entire history and balance).

What is **not** decided is the vendor and the operational shape. No derivation code exists yet — the system today onboards design partners with **per-address watch lists, not xpubs** (threat model A2 mitigation, Phase 0), so A2 does not yet exist in the system. This ADR is what must be settled before xpub-based onboarding turns on in Phase 1.

Three system-specific forces frame the choice:

1. **The hardware boundary is the control, not a feature.** §3.4 already commits to "derivation in an enclave with per-session remote attestation." So the question is not *whether* to use a hardware boundary but *which* one, and the binding requirement is **attestation on every derivation session** — the caller must cryptographically verify it is talking to genuine, unmodified derivation code inside genuine hardware before it will release or use a result.

2. **This is derivation, not signing — the system holds no keys that move funds (ADR-0001).** A BIP-32 *public* parent key (the xpub) derives *non-hardened* child public keys without any private key. So the minimal operation is: hold the master public key (or a seed from which the xpub is derived) inside the boundary, derive child *addresses*, and emit only the derived addresses. Whether the boundary must also hold *private* keys (and therefore whether it must ever sign) is an open question (below) — but the default posture, consistent with the read-only thesis, is **watch-only: no spend keys, no signing**.

3. **Reproducibility still applies (REQ-7).** Derived addresses feed ingestion (ADR-0001/0004) and must be reproducible from first principles: given the same xpub and derivation path, the same addresses must come out, and the *fact* that they came from attested hardware must be recordable as provenance.

## Decision (proposed)

**Derive addresses inside a hardware-attested boundary that holds the xpub (and, if required, the seed) and exposes only a narrow watch-only derivation interface; emit derived addresses plus an attestation record, never the master key.** The proposed default is a **cloud HSM with BIP-32 support and remote attestation** for Phase 1, with an **on-prem HSM** path documented for tenants whose domicile requires key material to stay in a named jurisdiction.

Two viable families, decided per the open questions:

- **Confidential-computing enclave** (AWS Nitro Enclaves, or an SGX/SEV-SNP/TDX enclave) running our own BIP-32 derivation code, with the seed/xpub sealed to the enclave measurement and **per-session remote attestation** (Nitro attestation document / DCAP quote) verified by the caller before any address is accepted. This matches §3.4's "enclave" wording literally and keeps BIP-32 logic ours and golden-vectored (REQ-7).
- **Managed/standalone HSM** (AWS CloudHSM, Azure Managed HSM, Google Cloud HSM, or on-prem Thales/nCipher/Utimaco) holding key material in FIPS 140-2/3 hardware, with derivation either inside the HSM (where BIP-32 is natively supported) or in a thin attested shim that calls the HSM. Stronger formal certification and key-ceremony tooling; less flexibility for custom derivation logic.

### Minimal interface the derivation crate must expose

A new crate (working name `treasury-keys`/`treasury-derive`) wraps the boundary behind a seam mirroring the `ChainSource`/`GlAdapter` pattern:

- `derive_addresses(account_xpub_ref, path_range) -> (Vec<Address>, Attestation)` — derive non-hardened children for a range; return addresses **and** the attestation proving they came from genuine attested hardware.
- `verify_attestation(Attestation) -> Result<(), AttestationError>` — the caller-side per-session check; no derived address is trusted until this passes.
- **No** export, no signing, no seed-read path in the public interface — the absence is the control (mirrors ADR-0003's "read-back mandatory by type" discipline: extraction must be unrepresentable, not merely discouraged).

## Options Considered

### Option A — Confidential-computing enclave, our BIP-32 code (e.g. AWS Nitro Enclave)

| Dimension | Assessment |
|-----------|------------|
| §3.4 fit | **Literal** — "enclave with per-session remote attestation" |
| BIP-32 | Ours — golden-vectored, fully reproducible (REQ-7) |
| Attestation | Strong — Nitro attestation doc / DCAP quote per session |
| Key extraction | Sealed to enclave measurement; not extractable by the host operator |
| Cost / ops | Moderate — enclave build/measurement pipeline; no FIPS appliance |
| Lock-in | Lower — portable across confidential-compute substrates with effort |

**Pros:** matches the spec wording exactly; keeps derivation logic ours and reproducible; per-session attestation is native. **Cons:** we own the derivation code's correctness inside a constrained runtime; enclave tooling/measurement pipeline is real work.

### Option B — Managed cloud HSM (AWS CloudHSM / Azure Managed HSM / Google Cloud HSM)

| Dimension | Assessment |
|-----------|------------|
| §3.4 fit | Good — hardware boundary; attestation via HSM partition/cert, per-session to be designed |
| BIP-32 | Native on some HSMs; otherwise a thin attested shim derives |
| Attestation | Hardware-rooted; "every derivation session" needs explicit design |
| Key extraction | FIPS 140-2/3 non-extractable |
| Cost / ops | Higher fixed cost; managed lifecycle; established ceremony tooling |
| Lock-in | Higher per-vendor; mitigated by the crate seam |

**Pros:** strongest formal certification; mature key-ceremony and audit-log tooling; reuses for ADR-0005 (D). **Cons:** cost; per-session attestation semantics are HSM-specific and must be engineered; custom derivation flexibility limited.

### Option C — On-prem HSM (Thales / nCipher / Utimaco)

| Dimension | Assessment |
|-----------|------------|
| §3.4 fit | Good — hardware boundary in a named jurisdiction |
| Jurisdiction | **Best** — key material physically located where a tenant/domicile requires |
| Cost / ops | **Highest** — procurement, datacentre, HA, ceremony staff |
| Lock-in | Vendor + physical |

**Pros:** answers data-residency/jurisdiction mandates directly; full physical control. **Cons:** heaviest cost and ops; overkill for Phase-1 scale unless a tenant's domicile forces it.

### Option D — Open-source Nitro Enclave path (subset of A, called out)

Run BIP-32 derivation in an AWS Nitro Enclave using open-source attestation libraries, seed sealed via KMS-with-attestation. This is the concrete, lowest-lock-in instance of Option A and the proposed Phase-1 default.

## Trade-off Analysis

The decisive axis is **which boundary delivers per-session attestation while keeping BIP-32 derivation reproducible (REQ-7) and watch-only (ADR-0001)** — not raw certification level. Option B/C buy the strongest FIPS certification and the best ceremony tooling, which matters if the open questions turn out to require *signing* (then the boundary holds spend keys and certification is paramount). But for pure xpub→address derivation with no spend keys, the enclave path (A/D) matches §3.4's wording literally, keeps the derivation code ours and golden-vectored, and gives clean per-session attestation at lower lock-in — so it is the proposed Phase-1 default, with HSM (B) reserved for the signing case or a tenant that contractually demands FIPS hardware, and on-prem (C) reserved for a jurisdiction mandate.

Cost and lock-in cut toward A/D at the wedge (small partner count); the crate seam keeps the vendor swappable, so the Phase-1 choice is not a one-way door. The residual risk is honest: an enclave makes *us* responsible for the derivation code's correctness inside the boundary, where an HSM with native BIP-32 outsources that to a certified appliance — a real reason the answer could flip if the audit firm weights certification over code-ownership.

## Consequences

**Easier:**
- A2 (xpub theft) gets a concrete control instead of a Phase-0 deferral: the master key never exists outside attested hardware, and the address set is all that is ever persisted (§3.4 satisfied end to end).
- The address-derivation provenance becomes auditable: each derived address batch carries an attestation that it came from genuine, unmodified derivation hardware.

**Harder:**
- A new derivation crate + boundary become standing infrastructure with an availability model (a derivation outage blocks onboarding/address refresh) and a documented rotation/key-ceremony process — the §3.4 "rotation and availability model documented when the vendor is chosen" obligation lands here.
- Per-session remote attestation must be implemented and verified caller-side, with a defined failure mode (no address trusted on attestation failure).
- Reproducibility (REQ-7) must extend into the boundary: identical xpub + path → identical addresses, golden-vectored.

**To revisit:**
- Convergence with ADR-0005 (D): if an HSM is chosen here, whether approval signing shares the boundary.
- Multi-region availability/HA once a tenant depends on derivation for live address refresh.

## Open Questions

1. **Does the HSM/enclave need to *sign* Bitcoin transactions too, or only derive/expose the xpub?** This is the gating question. ADR-0001 says the system holds no keys that move funds, which argues for **watch-only derivation, no signing** — but if any tenant ever expects the platform to co-sign, the boundary must hold spend keys and the certification calculus shifts decisively toward Option B/C.
2. **Who approves the key ceremony?** Generation/import of the seed/xpub, and any rotation, is a dual-controlled event (cf. ADR-0005) — but the ceremony participants, witnessing, and the artifact recording it are undefined.
3. Does any design-partner domicile mandate key material remain in a specific jurisdiction (forcing Option C)?
4. Is BIP-32 derivation done *inside* the boundary, or does the boundary only hold/unwrap the xpub while a thin attested shim derives? (Affects the wrapping/unwrapping model and which vendors qualify.)

## Action Items

_Proposed; nothing built. `[ ]` not started · `[ ] *(needs ADR ratification)*` blocked on this ADR's open questions._

1. [ ] *(needs ADR ratification)* Resolve open question 1 (sign vs derive-only) — it gates the whole vendor family choice.
2. [ ] Spin a derivation spike on the proposed default (Nitro Enclave, our BIP-32, sealed seed, per-session attestation) and validate REQ-7 reproducibility (same xpub+path → same addresses, golden-vectored).
3. [ ] Define the `treasury-keys` crate seam: `derive_addresses` + `verify_attestation`, with no export/sign/seed-read path in the public interface.
4. [ ] Document the rotation + availability model for the chosen boundary (the §3.4 obligation).
5. [ ] Specify the key ceremony and its dual-controlled approval + content-addressed artifact (open question 2; ties to ADR-0005).

-----

## References

- BIP-32 hierarchical deterministic wallets; non-hardened child *public* key derivation from an xpub without the private key — [BIP-32 (bitcoin/bips)](https://github.com/bitcoin/bips/blob/master/bip-0032.mediawiki)
- xpub leakage (an xpub exposes full history + balance) — [Trezor: What is an xPub](https://trezor.io/learn/advanced/what-is-a-public-key-xpub)
- Confidential computing / enclave attestation — [AWS Nitro Enclaves](https://docs.aws.amazon.com/enclaves/latest/user/nitro-enclave.html), [Intel SGX DCAP attestation](https://www.intel.com/content/www/us/en/developer/tools/software-guard-extensions/attestation-services.html)
- Cloud and on-prem HSMs — [AWS CloudHSM](https://docs.aws.amazon.com/cloudhsm/), [Azure Managed HSM](https://learn.microsoft.com/en-us/azure/key-vault/managed-hsm/), [Google Cloud HSM](https://cloud.google.com/kms/docs/hsm), [Thales / Utimaco / nCipher general-purpose HSMs]
- FIPS 140-2/3 key non-extractability — [NIST FIPS 140-3](https://csrc.nist.gov/pubs/fips/140-3/final)
