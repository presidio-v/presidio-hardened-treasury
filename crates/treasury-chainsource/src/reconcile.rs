//! The §3.3 two-source completeness control + the reproducibility gate.

use crate::finality::FinalityPolicy;
use crate::history::{AddressHistory, HistoryError};
use crate::source::{ChainSource, SourceError};
use treasury_core::ContentHash;

/// The outcome of reconciling two independent sources for one address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reconciliation {
    /// Both sources agree on the settled history (hash-identical). This
    /// history is what proceeds to L1 observations.
    Agreed {
        /// The agreed settled history.
        history: AddressHistory,
        /// Its content hash.
        history_hash: ContentHash,
    },
    /// The sources disagree. The address **blocks close** until a human
    /// resolves it; never auto-reconciled.
    Diverged {
        /// Source A's id and history hash.
        a: (String, ContentHash),
        /// Source B's id and history hash.
        b: (String, ContentHash),
        /// The settled height both were compared at.
        settled_to_height: u64,
    },
}

impl Reconciliation {
    /// Whether reconciliation permits the address to proceed to close.
    #[must_use]
    pub fn agreed(&self) -> bool {
        matches!(self, Self::Agreed { .. })
    }
}

/// Reconcile two independent sources for `address` at `observed_height`,
/// comparing only history settled under `policy` (gap G-5). Both sources
/// must serve the same chain as the policy.
///
/// # Errors
/// [`ReconcileError::ChainMismatch`] when the sources or policy disagree
/// on chain; [`ReconcileError::Source`] for a source failure;
/// [`ReconcileError::History`] when a history cannot hash.
pub fn reconcile<A: ChainSource, B: ChainSource>(
    source_a: &A,
    source_b: &B,
    policy: &FinalityPolicy,
    address: &str,
    observed_height: u64,
) -> Result<Reconciliation, ReconcileError> {
    if source_a.chain() != policy.chain
        || source_b.chain() != policy.chain
        || source_a.chain() != source_b.chain()
    {
        return Err(ReconcileError::ChainMismatch);
    }
    let settled_to_height = policy.settled_height(observed_height);

    let history_a = settled_view(source_a, address, observed_height, settled_to_height)?;
    let history_b = settled_view(source_b, address, observed_height, settled_to_height)?;
    let hash_a = history_a.history_hash()?;
    let hash_b = history_b.history_hash()?;

    if hash_a == hash_b {
        Ok(Reconciliation::Agreed {
            history: history_a,
            history_hash: hash_a,
        })
    } else {
        Ok(Reconciliation::Diverged {
            a: (source_a.source_id().to_owned(), hash_a),
            b: (source_b.source_id().to_owned(), hash_b),
            settled_to_height,
        })
    }
}

/// The reproducibility gate (ADR-0001/0004 acceptance test): a source
/// queried twice for the same address and height must reproduce its
/// settled-history hash byte-for-byte. Returns the reproduced hash on
/// success.
///
/// # Errors
/// [`ReproError::NotReproducible`] when the two queries disagree;
/// [`ReproError::Source`] / [`ReproError::History`] on underlying failure.
pub fn reproducibility_gate<S: ChainSource>(
    source: &S,
    policy: &FinalityPolicy,
    address: &str,
    observed_height: u64,
) -> Result<ContentHash, ReproError> {
    if source.chain() != policy.chain {
        return Err(ReproError::Source(SourceError::ChainMismatch));
    }
    let settled_to_height = policy.settled_height(observed_height);
    let first = settled_view(source, address, observed_height, settled_to_height)?.history_hash()?;
    let second =
        settled_view(source, address, observed_height, settled_to_height)?.history_hash()?;
    if first == second {
        Ok(first)
    } else {
        Err(ReproError::NotReproducible {
            source_id: source.source_id().to_owned(),
            first,
            second,
        })
    }
}

/// Fetch a source's history and clamp its settled height to the policy's,
/// so both sides of a comparison are settled to exactly the same height.
fn settled_view<S: ChainSource>(
    source: &S,
    address: &str,
    observed_height: u64,
    settled_to_height: u64,
) -> Result<AddressHistory, SourceError> {
    let mut history = source.address_history(address, observed_height)?;
    history.settled_to_height = settled_to_height;
    Ok(history)
}

/// Errors from reconciliation.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReconcileError {
    /// Sources and policy must all serve the same chain.
    #[error("chain mismatch between sources and policy")]
    ChainMismatch,
    /// A source failed.
    #[error(transparent)]
    Source(#[from] SourceError),
    /// A history could not hash.
    #[error(transparent)]
    History(#[from] HistoryError),
}

/// Errors from the reproducibility gate.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReproError {
    /// The same query produced two different hashes — the source is not
    /// deterministic and cannot be trusted as a system of record.
    #[error("source {source_id} not reproducible: {first} != {second}")]
    NotReproducible {
        /// The non-deterministic source's id.
        source_id: String,
        /// First query's hash.
        first: ContentHash,
        /// Second query's hash.
        second: ContentHash,
    },
    /// A source failed.
    #[error(transparent)]
    Source(#[from] SourceError),
    /// A history could not hash.
    #[error(transparent)]
    History(#[from] HistoryError),
}
