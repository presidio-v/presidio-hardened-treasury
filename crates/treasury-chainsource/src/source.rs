//! The node+indexer source trait and a fixture implementation.

use crate::history::{AddressHistory, Chain};
use std::collections::HashMap;

/// One node+indexer source (Bitcoin Core + electrs, reth, Erigon, …).
/// Concrete sources are I/O shims; the trait is the seam the pure-domain
/// reconciliation and reproducibility logic operate over.
pub trait ChainSource {
    /// The chain this source serves.
    fn chain(&self) -> Chain;

    /// A stable identifier for the source (e.g. `"core+electrs"`,
    /// `"erigon"`), recorded in provenance and divergence reports.
    fn source_id(&self) -> &str;

    /// The normalized history of `address` up to `observed_height` (a
    /// chain tip or a consensus-finalized height; the caller's
    /// [`crate::FinalityPolicy`] decides what is settled).
    ///
    /// # Errors
    /// [`SourceError`] for transport / not-found / chain-mismatch.
    fn address_history(
        &self,
        address: &str,
        observed_height: u64,
    ) -> Result<AddressHistory, SourceError>;
}

/// Errors a source may raise.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SourceError {
    /// The source could not be reached.
    #[error("source transport error: {0}")]
    Transport(String),
    /// The address was queried against the wrong chain's source.
    #[error("chain mismatch")]
    ChainMismatch,
}

/// An in-memory source for tests: deterministic, so the reproducibility
/// gate is meaningful and reconciliation can be exercised without live
/// nodes. A real source is the same trait over a node+indexer RPC.
#[derive(Debug, Clone)]
pub struct FixtureSource {
    chain: Chain,
    source_id: String,
    histories: HashMap<String, AddressHistory>,
}

impl FixtureSource {
    /// Create a fixture source for a chain with a stable id.
    #[must_use]
    pub fn new(chain: Chain, source_id: impl Into<String>) -> Self {
        Self {
            chain,
            source_id: source_id.into(),
            histories: HashMap::new(),
        }
    }

    /// Seed the history this source will return for an address.
    #[must_use]
    pub fn with_history(mut self, history: AddressHistory) -> Self {
        self.histories.insert(history.address.clone(), history);
        self
    }
}

impl ChainSource for FixtureSource {
    fn chain(&self) -> Chain {
        self.chain
    }

    fn source_id(&self) -> &str {
        &self.source_id
    }

    fn address_history(
        &self,
        address: &str,
        observed_height: u64,
    ) -> Result<AddressHistory, SourceError> {
        match self.histories.get(address) {
            None => Ok(AddressHistory {
                chain: self.chain,
                address: address.to_owned(),
                settled_to_height: observed_height,
                movements: Vec::new(),
            }),
            Some(history) => Ok(history.clone()),
        }
    }
}
