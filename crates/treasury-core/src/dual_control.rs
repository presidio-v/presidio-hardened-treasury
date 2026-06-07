//! Generic dual-control state machine (spec v2 §5 discipline, reusable).
//!
//! Items are keyed by a caller-computed content hash, carry an arbitrary
//! payload, and move Asserted → Confirmed | Rejected. The proposer is the
//! preparer; confirmation must come from a *different* approver; terminal
//! states are immutable; re-proposing identical content is idempotent and
//! never resets state.

use crate::hash::ContentHash;
use crate::ids::ActorId;
use std::collections::HashMap;

/// Lifecycle of a dual-controlled item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DualControlState {
    /// Proposed by a preparer; awaiting a different approver.
    Asserted {
        /// The proposing preparer.
        preparer: ActorId,
    },
    /// Confirmed under dual control. Terminal.
    Confirmed {
        /// The proposing preparer.
        preparer: ActorId,
        /// The confirming approver (≠ preparer).
        approver: ActorId,
    },
    /// Rejected. Terminal.
    Rejected {
        /// The rejecting actor.
        actor: ActorId,
        /// Why.
        reason: String,
    },
}

impl DualControlState {
    /// Whether the state is terminal.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Confirmed { .. } | Self::Rejected { .. })
    }
}

/// A dual-control queue over payloads of type `P`, keyed by content hash.
#[derive(Debug)]
pub struct DualControlQueue<P> {
    items: HashMap<ContentHash, (P, DualControlState)>,
}

impl<P> Default for DualControlQueue<P> {
    fn default() -> Self {
        Self {
            items: HashMap::new(),
        }
    }
}

impl<P> DualControlQueue<P> {
    /// Create an empty queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Propose an item under its content hash. Idempotent for an existing
    /// id: the stored payload and state are never reset.
    pub fn propose(&mut self, id: ContentHash, payload: P, preparer: ActorId) {
        self.items
            .entry(id)
            .or_insert((payload, DualControlState::Asserted { preparer }));
    }

    /// A second approver confirms. Dual control enforced.
    ///
    /// # Errors
    /// [`DualControlError::UnknownItem`] for unknown ids;
    /// [`DualControlError::InvalidTransition`] unless `Asserted`;
    /// [`DualControlError::DualControlViolation`] when approver == preparer.
    pub fn confirm(
        &mut self,
        id: &ContentHash,
        approver: ActorId,
    ) -> Result<(), DualControlError> {
        let (_, state) = self
            .items
            .get_mut(id)
            .ok_or(DualControlError::UnknownItem(*id))?;
        let DualControlState::Asserted { preparer } = state.clone() else {
            return Err(DualControlError::InvalidTransition);
        };
        if preparer == approver {
            return Err(DualControlError::DualControlViolation);
        }
        *state = DualControlState::Confirmed { preparer, approver };
        Ok(())
    }

    /// Reject an item. Terminal.
    ///
    /// # Errors
    /// [`DualControlError::UnknownItem`] for unknown ids;
    /// [`DualControlError::InvalidTransition`] when already terminal.
    pub fn reject(
        &mut self,
        id: &ContentHash,
        actor: ActorId,
        reason: String,
    ) -> Result<(), DualControlError> {
        let (_, state) = self
            .items
            .get_mut(id)
            .ok_or(DualControlError::UnknownItem(*id))?;
        if state.is_terminal() {
            return Err(DualControlError::InvalidTransition);
        }
        *state = DualControlState::Rejected { actor, reason };
        Ok(())
    }

    /// State of an item.
    #[must_use]
    pub fn state(&self, id: &ContentHash) -> Option<&DualControlState> {
        self.items.get(id).map(|(_, s)| s)
    }

    /// Payload of an item.
    #[must_use]
    pub fn payload(&self, id: &ContentHash) -> Option<&P> {
        self.items.get(id).map(|(p, _)| p)
    }

    /// Ids of items not yet terminal, in deterministic (sorted) order.
    #[must_use]
    pub fn open_items(&self) -> Vec<ContentHash> {
        let mut open: Vec<ContentHash> = Vec::new();
        for (id, (_, state)) in &self.items {
            if !state.is_terminal() {
                open.push(*id);
            }
        }
        open.sort_unstable();
        open
    }
}

/// Errors from dual-control operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DualControlError {
    /// No item under this id.
    #[error("unknown dual-control item: {0}")]
    UnknownItem(ContentHash),
    /// The requested transition is not legal from the current state.
    #[error("invalid state transition")]
    InvalidTransition,
    /// Approver and preparer must be different actors.
    #[error("dual-control violation: approver must differ from preparer")]
    DualControlViolation,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u8) -> ContentHash {
        ContentHash([n; 32])
    }

    #[test]
    fn lifecycle_and_dual_control() {
        let mut q: DualControlQueue<&str> = DualControlQueue::new();
        q.propose(id(1), "payload", ActorId::new("alice"));
        assert_eq!(
            q.confirm(&id(1), ActorId::new("alice")),
            Err(DualControlError::DualControlViolation)
        );
        assert_eq!(q.confirm(&id(1), ActorId::new("bob")), Ok(()));
        assert!(q.state(&id(1)).is_some_and(DualControlState::is_terminal));
        assert_eq!(
            q.reject(&id(1), ActorId::new("carol"), "late".to_owned()),
            Err(DualControlError::InvalidTransition)
        );
        assert!(q.open_items().is_empty());
    }

    #[test]
    fn idempotent_propose_never_resets() {
        let mut q: DualControlQueue<u8> = DualControlQueue::new();
        q.propose(id(1), 1, ActorId::new("alice"));
        let _ = q.reject(&id(1), ActorId::new("bob"), "no".to_owned());
        q.propose(id(1), 2, ActorId::new("carol"));
        assert!(q.state(&id(1)).is_some_and(DualControlState::is_terminal));
        assert_eq!(q.payload(&id(1)), Some(&1));
    }

    #[test]
    fn unknown_item_errors() {
        let mut q: DualControlQueue<u8> = DualControlQueue::new();
        assert_eq!(
            q.confirm(&id(9), ActorId::new("bob")),
            Err(DualControlError::UnknownItem(id(9)))
        );
    }
}
