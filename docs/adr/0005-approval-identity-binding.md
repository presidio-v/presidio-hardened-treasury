# ADR-0005: Authentication and approval identity binding

**Status:** Proposed
**Date:** 2026-06-10
**Deciders:** CTO, security lead (proposers) — design-partner's audit firm to be consulted before ratification
**Resolves:** the strength of the identity binding behind the §5 dual-control guarantee / §3.1 L3 approver identity — currently undefined
**Related:** [ADR-0001](0001-chain-indexing-build-vs-buy.md) (privacy thesis) · threat model B5 (insider) · `treasury-core::dual_control` · `treasury-policy::artifact` · spec §5 / §3.1
**Supersedes:** none

-----

## Context

The dual-control guarantee is structural: `treasury-core::dual_control::DualControlQueue::confirm` refuses to confirm an item when `approver == preparer`, and `treasury-policy::artifact` requires a non-empty approver set. That is the *liability* mechanism the spec leans on — "classification judgments remain management's … §5's dual-control design makes this structurally true" (§10) — and it is what lets us tell an auditor that a second, independent officer confirmed every L3 judgment.

But the guarantee is only as strong as the binding between an `ActorId` and a real, independently-authenticated human. Today that binding is **undefined**. `ActorId` is a `string_id!` newtype over a `String` (`treasury-core/src/ids.rs`): the dual-control check is a *string-inequality* test (`preparer == approver`), and nothing in the codebase establishes that `ActorId("controller")` was actually authenticated as a distinct person, or that the two officers are not the same human holding two opaque handles. The threat model puts the operator inside the adversary set (B5 insider); against that adversary, an unauthenticated string pair is not dual control — it is two strings.

Three forces specific to this system shape the answer:

1. **The approver identity is an audit artifact, not a session detail.** Every L3 judgment records its approvers and is anchored (B7 / ADR-0002). For the anchored record to *mean* "two distinct officers confirmed this," the identity binding must be verifiable *after the fact* by an auditor — not merely asserted at login time and discarded. This pushes toward a binding that leaves a cryptographic trace in the event itself, not just an IdP session.

2. **The adversary includes the operator.** A binding that the operator can forge (a software key the operator's infrastructure holds, an OTP the operator can intercept) does not raise the bar against B5. The stronger tiers move the signing material to a boundary the operator cannot reach.

3. **The officers are the *tenant's* management, not our staff.** Onboarding friction is borne by a small number of client-side officers (preparer, approver — typically a controller and a CFO). UX friction is real but bounded; the population is tiny and the act is infrequent (per close), which widens the room for a stronger mechanism than a high-volume consumer flow could tolerate.

This ADR documents the decision space; it does not yet ratify a tier, because the binding strength L3 *requires* is itself an open question (below).

## Decision (proposed)

**Bind each approval to a per-officer signing key whose private half never touches operator-controlled infrastructure, and record a detached signature over the judgment's content hash inside the L3 event** — so the anchored record carries its own, independently-verifiable proof that a distinct, authenticated officer confirmed it.

The proposed default tier is **(b) WebAuthn/passkey** as the baseline for every officer, with **(c) PIV/FIDO2 hardware token** or **(d) HSM-backed approval signing** mandated for officers whose confirmations gate filing-relevant closes — pending the §"Open Questions" resolution on whether the constitution requires hardware binding for L3. `ActorId` stays the human-readable handle, but it gains a bound public-key credential: confirmation requires a fresh assertion/signature over the dual-control item's `ContentHash`, and the credential's public key (and attestation) is recorded so the binding is replayable.

Tier (a) — software key in a secrets manager — is explicitly **rejected as an L3 binding** because the operator (B5) can read it; it survives only as a break-glass or non-L3 service-actor mechanism, clearly labeled as not satisfying dual control.

## Options Considered

### Option A — Software key in a secrets manager

A per-officer signing key stored in the operator's secrets manager (Vault, AWS Secrets Manager); the service signs on the officer's behalf after an IdP login.

| Dimension | Assessment |
|-----------|------------|
| Guarantee strength vs B5 | **Weak** — the operator holds the key; an insider can mint a confirmation |
| Auditor verifiability | Signature exists but proves operator access, not officer presence |
| UX friction | Lowest — invisible to the officer |
| Rotation / continuity | Easy — server-side rotation |
| Verdict | **Rejected for L3**; acceptable only for non-L3 service actors / break-glass |

### Option B — WebAuthn / passkey (proposed baseline)

Each officer registers a WebAuthn credential (platform or roaming authenticator); confirmation requires a user-verified assertion over a challenge derived from the item's content hash.

| Dimension | Assessment |
|-----------|------------|
| Guarantee strength vs B5 | **Strong** — private key is device/TPM-bound, non-extractable; operator cannot sign |
| Auditor verifiability | Good — assertion + attestation recorded; replayable against the registered public key |
| UX friction | Low — familiar biometric/PIN tap |
| Rotation / continuity | Moderate — credential re-registration; needs a recovery path that is itself dual-controlled |
| Verdict | **Proposed baseline for all officers** |

### Option C — PIV / FIDO2 hardware token

A physical security key (YubiKey-class) or PIV smartcard holds the private key; the same WebAuthn/PIV assertion flow, but the authenticator is a separate physical object an officer must possess.

| Dimension | Assessment |
|-----------|------------|
| Guarantee strength vs B5 | **Strong+** — possession factor is a distinct physical object, harder to silently clone than a platform credential |
| Auditor verifiability | Strong — hardware attestation identifies the authenticator model/batch |
| UX friction | Moderate — officer must carry and present the token |
| Rotation / continuity | Token loss needs a pre-provisioned spare + dual-controlled re-enrolment |
| Verdict | **Proposed for filing-relevant L3 confirmations** |

### Option D — HSM-backed approval signing

Approval keys live in an HSM (the same boundary considered for xpub derivation in [ADR-0006](0006-enclave-hsm-xpub-derivation.md)); the officer authenticates to the HSM, which signs the judgment hash.

| Dimension | Assessment |
|-----------|------------|
| Guarantee strength vs B5 | **Strongest** — centralized non-extractable keys, hardware audit log, formal key ceremony |
| Auditor verifiability | Strongest — HSM audit log + attested signing |
| UX friction | Highest — HSM access path, ceremony overhead |
| Rotation / continuity | Heaviest — key ceremony to rotate; availability of the HSM gates closes |
| Verdict | **Optional highest tier**; reuses ADR-0006 infrastructure if that lands |

## Trade-off Analysis

The decisive axis is **whether the operator can forge a confirmation**, because the entire reason dual control exists here is to make classification liability provably the client's. Option A fails that test by construction — it is the convenient default and it quietly hands the insider the ability to manufacture both halves of "dual" control. Options B–D all move the signing material outside the operator's reach; they differ in *how much* possession assurance and audit trail they buy versus the friction they impose.

UX friction is the usual counter-pull, but it is unusually cheap to spend here: the signer population is a handful of named officers and the act is per-close, not per-request — so the friction of a hardware tap (C) or even an HSM ceremony (D) is amortized across a high-value, low-frequency action. The real continuity cost is **recovery**: any credential that the operator cannot forge is also a credential the operator cannot restore, so every tier B–D needs a recovery/rotation path that is *itself* dual-controlled and audited, or it reintroduces the B5 hole through the back door.

The honest synthesis is a tiered baseline (B everywhere, C/D for filing-relevant closes) rather than one universal tier — provided the constitution does not mandate hardware for all L3 (open question 1).

## Consequences

**Easier:**
- The anchored L3 record becomes self-proving: an auditor verifies the approver signature against the registered public key with no trust in us — the same "fetch, recompute, compare" posture the anchor already gives tamper-evidence.
- The dual-control guarantee gains teeth against the B5 insider: "two distinct officers" stops being two strings.

**Harder:**
- `ActorId` gains a bound credential and a registration lifecycle; `confirm`/`propose` must take (and the event must persist) a signature over the content hash, not just an `ActorId`.
- A dual-controlled credential recovery/rotation path becomes mandatory infrastructure — losing a token cannot brick a close, and recovery cannot be an operator-only act.
- Onboarding gains an enrolment ceremony for each officer.

**To revisit:**
- Whether platform passkeys (synced across a vendor cloud) dilute the possession assurance enough to disqualify them for filing-relevant L3, leaving only roaming authenticators / hardware tokens.
- Convergence with ADR-0006: if an HSM lands for xpub derivation, whether approval signing should share that boundary (D) by default.

## Open Questions

1. **Does the constitution require hardware binding for L3, or is WebAuthn sufficient?** This is the gating question — it decides whether C/D are mandatory for filing-relevant closes or merely available. Needs the design-partner audit firm's view on what "independently authenticated" must mean for management's classification liability to hold.
2. Is the approver signature required to be over the *judgment* content hash specifically, or over a per-session challenge that is then bound to the judgment? (Affects replay semantics and offline verification.)
3. Who owns officer enrolment and de-provisioning — the tenant, or us under a dual-controlled process — and how is an officer's departure reflected without breaking the immutability of past anchored confirmations?

## Action Items

_Proposed; nothing built. `[ ]` not started · `[ ] *(needs ADR ratification)*` blocked on this ADR's open questions._

1. [ ] *(needs ADR ratification)* Confirm with the design-partner audit firm whether L3 requires hardware binding (open question 1).
2. [ ] Extend the L3 event and `dual_control` API to carry a per-officer signature over the item `ContentHash`, with the credential public key recorded for replay.
3. [ ] Specify the WebAuthn baseline enrolment + assertion flow and the dual-controlled recovery/rotation path.
4. [ ] Define the filing-relevant-close tier gate (C/D) once open question 1 is resolved.
5. [ ] Document the binding as an auditor-facing artifact in the evidence-reproduction UX (how an auditor verifies an approval signature independently).

-----

## References

- WebAuthn / FIDO2 credential model, attestation, user verification — [W3C Web Authentication Level 2](https://www.w3.org/TR/webauthn-2/), [FIDO Alliance specifications](https://fidoalliance.org/specifications/)
- PIV hardware-token identity binding — [NIST SP 800-157 (Derived PIV Credentials)](https://csrc.nist.gov/pubs/sp/800/157/final)
- Non-extractable keys / hardware audit log model — see [ADR-0006](0006-enclave-hsm-xpub-derivation.md) and its HSM references
- Dual-control / segregation-of-duties as a liability control — spec v2 §5, §10
