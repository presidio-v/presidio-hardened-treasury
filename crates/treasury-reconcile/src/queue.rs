//! Dual-control confirmation queue (spec v2 §5).
//!
//! The vendor never classifies: a queued proposal needs a **preparer
//! assertion** and a **confirmation by a different approver**, both
//! client actors. Terminal states are immutable — reversing a confirmed
//! decision is a new superseding L3 judgment in the ledger, not an edit
//! here.

use crate::decision::MatchProposal;
use std::collections::HashMap;
use treasury_core::{ActorId, ContentHash};
use treasury_evidence::CanonError;

/// Lifecycle of a queued proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueState {
    /// Surfaced by the matcher; awaiting the preparer.
    Pending,
    /// Preparer asserted it is an internal transfer; awaiting a second,
    /// different approver.
    Asserted {
        /// The asserting preparer.
        preparer: ActorId,
    },
    /// Confirmed under dual control. Terminal.
    Confirmed {
        /// The asserting preparer.
        preparer: ActorId,
        /// The confirming approver (≠ preparer).
        approver: ActorId,
    },
    /// Rejected: not an internal transfer. Terminal.
    Rejected {
        /// The rejecting actor.
        actor: ActorId,
        /// Why (free text, recorded in the L3 judgment).
        reason: String,
    },
}

impl QueueState {
    /// Whether the state is terminal.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Confirmed { .. } | Self::Rejected { .. })
    }
}

/// The confirmation queue. Items are keyed by decision hash.
#[derive(Debug, Default)]
pub struct ConfirmationQueue {
    items: HashMap<ContentHash, (MatchProposal, QueueState)>,
}

impl ConfirmationQueue {
    /// Create an empty queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueue a proposal; returns its decision hash. Idempotent for an
    /// identical proposal; re-enqueueing never resets state.
    ///
    /// # Errors
    /// [`QueueError::Canon`] on envelope failure (structurally
    /// unreachable).
    pub fn enqueue(&mut self, proposal: MatchProposal) -> Result<ContentHash, QueueError> {
        let id = proposal.decision_hash()?;
        self.items
            .entry(id)
            .or_insert((proposal, QueueState::Pending));
        Ok(id)
    }

    /// Preparer asserts the proposal is an internal transfer.
    ///
    /// # Errors
    /// [`QueueError::UnknownItem`] for unknown ids;
    /// [`QueueError::InvalidTransition`] unless the item is `Pending`.
    pub fn assert_match(&mut self, id: &ContentHash, preparer: ActorId) -> Result<(), QueueError> {
        let (_, state) = self.items.get_mut(id).ok_or(QueueError::UnknownItem(*id))?;
        if *state != QueueState::Pending {
            return Err(QueueError::InvalidTransition);
        }
        *state = QueueState::Asserted { preparer };
        Ok(())
    }

    /// A second approver confirms. Dual control: the approver must differ
    /// from the preparer.
    ///
    /// # Errors
    /// [`QueueError::UnknownItem`] for unknown ids;
    /// [`QueueError::InvalidTransition`] unless the item is `Asserted`;
    /// [`QueueError::DualControlViolation`] when approver == preparer.
    pub fn confirm(&mut self, id: &ContentHash, approver: ActorId) -> Result<(), QueueError> {
        let (_, state) = self.items.get_mut(id).ok_or(QueueError::UnknownItem(*id))?;
        let QueueState::Asserted { preparer } = state.clone() else {
            return Err(QueueError::InvalidTransition);
        };
        if preparer == approver {
            return Err(QueueError::DualControlViolation);
        }
        *state = QueueState::Confirmed { preparer, approver };
        Ok(())
    }

    /// Any actor rejects the proposal (it is not an internal transfer).
    /// Legal from `Pending` or `Asserted`.
    ///
    /// # Errors
    /// [`QueueError::UnknownItem`] for unknown ids;
    /// [`QueueError::InvalidTransition`] when already terminal.
    pub fn reject(
        &mut self,
        id: &ContentHash,
        actor: ActorId,
        reason: String,
    ) -> Result<(), QueueError> {
        let (_, state) = self.items.get_mut(id).ok_or(QueueError::UnknownItem(*id))?;
        if state.is_terminal() {
            return Err(QueueError::InvalidTransition);
        }
        *state = QueueState::Rejected { actor, reason };
        Ok(())
    }

    /// State of an item.
    #[must_use]
    pub fn state(&self, id: &ContentHash) -> Option<&QueueState> {
        self.items.get(id).map(|(_, s)| s)
    }

    /// Decision hashes of items not yet terminal — close blockers, in
    /// deterministic (sorted) order.
    #[must_use]
    pub fn open_items(&self) -> Vec<ContentHash> {
        let mut open: Vec<ContentHash> = self
            .items
            .iter()
            .filter(|(_, (_, s))| !s.is_terminal())
            .map(|(id, _)| *id)
            .collect();
        open.sort_unstable();
        open
    }
}

/// Errors from queue operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum QueueError {
    /// No item under this decision hash.
    #[error("unknown queue item: {0}")]
    UnknownItem(ContentHash),
    /// The requested transition is not legal from the current state.
    #[error("invalid state transition")]
    InvalidTransition,
    /// Approver and preparer must be different actors.
    #[error("dual-control violation: approver must differ from preparer")]
    DualControlViolation,
    /// Envelope canonicalization failure.
    #[error(transparent)]
    Canon(#[from] CanonError),
}
