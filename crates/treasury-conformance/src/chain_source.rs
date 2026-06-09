//! The contract a chain node+indexer shim must satisfy (ADR-0004).
//!
//! Both Bitcoin sources (Core+electrs, Core+Fulcrum) and both Ethereum
//! sources (reth, Erigon) implement the one [`ChainSource`] trait, so they
//! share one contract. The invariants the pure reconciliation core
//! assumes — and that a real indexer can silently break — are:
//!
//! 1. **Identity** — it reports the chain it serves and a stable non-empty
//!    id (recorded in provenance and divergence reports).
//! 2. **Reproducibility** — the same `(address, height)` query reproduces
//!    its settled-history hash byte-for-byte (the ADR-0001/0004 acceptance
//!    test; a non-deterministic source cannot be a system of record).
//! 3. **Settled stability** — advancing the observed tip may reveal new
//!    movements but must never rewrite history already settled under the
//!    finality policy (the reorg-safety the §3.3 comparison depends on).
//! 4. **Cross-source agreement** — two independent sources over the same
//!    chain state agree (hash-identical settled history); divergence is
//!    the alarm, not the norm.
//!
//! Residual only the live job covers: an indexer that is *actually*
//! non-deterministic across process restarts or chain reorgs. The fixture
//! is deterministic by construction, so (2) and (3) here prove the harness
//! wiring; the live regtest job proves the real client.

use crate::ContractViolation;
use treasury_chainsource::{
    reconcile, reproducibility_gate, Chain, ChainSource, FinalityPolicy, Reconciliation, ReproError,
};
use treasury_core::ContentHash;

fn underlying(e: impl ToString) -> ContractViolation {
    ContractViolation::Underlying(e.to_string())
}

/// Invariant 1 — the source reports `expected` and a non-empty id.
///
/// # Errors
/// [`ContractViolation::WrongChain`] or [`ContractViolation::EmptySourceId`].
pub fn verify_identity<S: ChainSource>(
    source: &S,
    expected: Chain,
) -> Result<(), ContractViolation> {
    if source.chain() != expected {
        return Err(ContractViolation::WrongChain {
            expected,
            found: source.chain(),
        });
    }
    if source.source_id().is_empty() {
        return Err(ContractViolation::EmptySourceId);
    }
    Ok(())
}

/// Invariant 2 — the source reproduces its settled-history hash for a
/// repeated query. Returns the reproduced hash.
///
/// # Errors
/// [`ContractViolation::NotReproducible`] when the two queries disagree;
/// [`ContractViolation::Underlying`] for a source/hashing failure.
pub fn verify_reproducible<S: ChainSource>(
    source: &S,
    policy: &FinalityPolicy,
    address: &str,
    observed_height: u64,
) -> Result<ContentHash, ContractViolation> {
    match reproducibility_gate(source, policy, address, observed_height) {
        Ok(hash) => Ok(hash),
        Err(ReproError::NotReproducible { first, second, .. }) => {
            Err(ContractViolation::NotReproducible { first, second })
        }
        Err(other) => Err(underlying(other)),
    }
}

/// Invariant 3 — history settled at the lower tip is unchanged when the
/// source is queried at a higher tip. `low_tip <= high_tip` is the caller's
/// responsibility; the settled height is derived through `policy`.
///
/// # Errors
/// [`ContractViolation::SettledHistoryRewritten`] when the settled prefix
/// changes; [`ContractViolation::Underlying`] for a source/hashing failure.
pub fn verify_settled_history_stable<S: ChainSource>(
    source: &S,
    policy: &FinalityPolicy,
    address: &str,
    low_tip: u64,
    high_tip: u64,
) -> Result<(), ContractViolation> {
    let settled_height = policy.settled_height(low_tip);
    let mut at_low = source
        .address_history(address, low_tip)
        .map_err(underlying)?;
    let mut at_high = source
        .address_history(address, high_tip)
        .map_err(underlying)?;
    // Clamp both observations to the prefix settled at the lower tip and
    // compare by hash: a correct source can only *extend* settled history.
    at_low.settled_to_height = settled_height;
    at_high.settled_to_height = settled_height;
    let low = at_low.history_hash().map_err(underlying)?;
    let high = at_high.history_hash().map_err(underlying)?;
    if low != high {
        return Err(ContractViolation::SettledHistoryRewritten {
            settled_height,
            low,
            high,
        });
    }
    Ok(())
}

/// Invariant 4 — two independent sources over the same chain state agree.
/// Returns the agreed settled-history hash.
///
/// # Errors
/// [`ContractViolation::SourcesDisagree`] when they diverge;
/// [`ContractViolation::Underlying`] for a reconcile failure.
pub fn verify_sources_agree<A: ChainSource, B: ChainSource>(
    source_a: &A,
    source_b: &B,
    policy: &FinalityPolicy,
    address: &str,
    observed_height: u64,
) -> Result<ContentHash, ContractViolation> {
    match reconcile(source_a, source_b, policy, address, observed_height) {
        Ok(Reconciliation::Agreed { history_hash, .. }) => Ok(history_hash),
        Ok(Reconciliation::Diverged { .. }) => Err(ContractViolation::SourcesDisagree),
        Err(e) => Err(underlying(e)),
    }
}

/// The full single-source contract (invariants 1–3) in one call, for a
/// shim's smoke test. Cross-source agreement (invariant 4) needs two
/// sources and is [`verify_sources_agree`].
///
/// # Errors
/// The first violated invariant.
pub fn verify_chain_source_contract<S: ChainSource>(
    source: &S,
    expected: Chain,
    policy: &FinalityPolicy,
    address: &str,
    low_tip: u64,
    high_tip: u64,
) -> Result<(), ContractViolation> {
    verify_identity(source, expected)?;
    verify_reproducible(source, policy, address, high_tip)?;
    verify_settled_history_stable(source, policy, address, low_tip, high_tip)?;
    Ok(())
}
