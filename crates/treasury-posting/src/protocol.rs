//! The posting state machine.

use crate::batch::PostingBatch;
use std::collections::{BTreeSet, HashMap};
use treasury_core::{ActorId, ContentHash, DualControlError, DualControlQueue, DualControlState};
use treasury_evidence::CanonError;

/// Where a batch is in its life. Transitions are evidence-driven; there
/// is no transition that encodes a guess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostingState {
    /// Registered; awaiting dual-control release.
    AwaitingApproval,
    /// Released for submission (or proven absent after `Unknown`).
    ReadyToSubmit,
    /// Submission in flight.
    Submitting,
    /// The GL acknowledged the batch.
    Posted {
        /// GL-side reference (journal id, document number).
        gl_ref: String,
        /// Evidence-store hash of the raw GL response.
        response_evidence: ContentHash,
    },
    /// Submission outcome unknown (ack lost). Exits only via read-back.
    Unknown,
    /// Read-back matched the batch exactly. Terminal.
    Verified {
        /// GL-side reference.
        gl_ref: String,
        /// Evidence-store hash of the raw read-back payload.
        readback_evidence: ContentHash,
    },
    /// Read-back mismatched. Terminal; named differences; escalate.
    VerificationFailed {
        /// Batch entries the GL does not show.
        missing: Vec<ContentHash>,
        /// GL entries the batch does not contain.
        unexpected: Vec<ContentHash>,
        /// Evidence-store hash of the raw read-back payload.
        readback_evidence: ContentHash,
    },
}

/// The protocol over all batches of one deployment.
#[derive(Debug, Default)]
pub struct PostingProtocol {
    approvals: DualControlQueue<PostingBatch>,
    states: HashMap<ContentHash, PostingState>,
}

impl PostingProtocol {
    /// Create an empty protocol instance.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a batch for release; the registrar is the preparer.
    /// Idempotent: re-registering an identical batch returns the same
    /// key and never resets state.
    ///
    /// # Errors
    /// [`PostingError::Canon`] on batch hashing failure.
    pub fn register(
        &mut self,
        batch: PostingBatch,
        preparer: ActorId,
    ) -> Result<ContentHash, PostingError> {
        let id = batch.batch_id()?;
        self.approvals.propose(id, batch, preparer);
        self.states
            .entry(id)
            .or_insert(PostingState::AwaitingApproval);
        Ok(id)
    }

    /// Dual-control release: a different approver confirms; the batch
    /// becomes ready to submit.
    ///
    /// # Errors
    /// Propagates [`DualControlError`]; [`PostingError::UnknownBatch`].
    pub fn release(&mut self, id: &ContentHash, approver: ActorId) -> Result<(), PostingError> {
        self.approvals.confirm(id, approver)?;
        let state = self
            .states
            .get_mut(id)
            .ok_or(PostingError::UnknownBatch(*id))?;
        if *state != PostingState::AwaitingApproval {
            return Err(PostingError::InvalidTransition);
        }
        *state = PostingState::ReadyToSubmit;
        Ok(())
    }

    /// Mark submission in flight.
    ///
    /// # Errors
    /// [`PostingError::UnknownBatch`]; [`PostingError::InvalidTransition`]
    /// unless `ReadyToSubmit`.
    pub fn begin_submit(&mut self, id: &ContentHash) -> Result<(), PostingError> {
        let state = self
            .states
            .get_mut(id)
            .ok_or(PostingError::UnknownBatch(*id))?;
        if *state != PostingState::ReadyToSubmit {
            return Err(PostingError::InvalidTransition);
        }
        *state = PostingState::Submitting;
        Ok(())
    }

    /// The GL acknowledged the submission.
    ///
    /// # Errors
    /// [`PostingError::UnknownBatch`]; [`PostingError::InvalidTransition`]
    /// unless `Submitting`.
    pub fn confirm_posted(
        &mut self,
        id: &ContentHash,
        gl_ref: String,
        response_evidence: ContentHash,
    ) -> Result<(), PostingError> {
        let state = self
            .states
            .get_mut(id)
            .ok_or(PostingError::UnknownBatch(*id))?;
        if *state != PostingState::Submitting {
            return Err(PostingError::InvalidTransition);
        }
        *state = PostingState::Posted {
            gl_ref,
            response_evidence,
        };
        Ok(())
    }

    /// The acknowledgment was lost: outcome unknown. The honest state.
    ///
    /// # Errors
    /// [`PostingError::UnknownBatch`]; [`PostingError::InvalidTransition`]
    /// unless `Submitting`.
    pub fn report_unknown(&mut self, id: &ContentHash) -> Result<(), PostingError> {
        let state = self
            .states
            .get_mut(id)
            .ok_or(PostingError::UnknownBatch(*id))?;
        if *state != PostingState::Submitting {
            return Err(PostingError::InvalidTransition);
        }
        *state = PostingState::Unknown;
        Ok(())
    }

    /// Resolve `Unknown` with read-back evidence: the batch was found
    /// (→ `Posted`) or proven absent (→ `ReadyToSubmit`, same
    /// idempotency key — the retry is safe by construction).
    ///
    /// # Errors
    /// [`PostingError::UnknownBatch`]; [`PostingError::InvalidTransition`]
    /// unless `Unknown`.
    pub fn resolve_unknown(
        &mut self,
        id: &ContentHash,
        found: Option<(String, ContentHash)>,
    ) -> Result<(), PostingError> {
        let state = self
            .states
            .get_mut(id)
            .ok_or(PostingError::UnknownBatch(*id))?;
        if *state != PostingState::Unknown {
            return Err(PostingError::InvalidTransition);
        }
        *state = match found {
            Some((gl_ref, response_evidence)) => PostingState::Posted {
                gl_ref,
                response_evidence,
            },
            None => PostingState::ReadyToSubmit,
        };
        Ok(())
    }

    /// Read-back verification: the GL's entries (as hashes, mapped back
    /// by the adapter) must equal the batch's, both directions.
    ///
    /// # Errors
    /// [`PostingError::UnknownBatch`]; [`PostingError::InvalidTransition`]
    /// unless `Posted`; [`PostingError::Canon`] on fingerprint failure.
    pub fn verify(
        &mut self,
        id: &ContentHash,
        readback_entry_hashes: &[ContentHash],
        readback_evidence: ContentHash,
    ) -> Result<(), PostingError> {
        let batch = self
            .approvals
            .payload(id)
            .ok_or(PostingError::UnknownBatch(*id))?;
        let expected = batch.entry_fingerprint()?;

        let state = self
            .states
            .get_mut(id)
            .ok_or(PostingError::UnknownBatch(*id))?;
        let PostingState::Posted { gl_ref, .. } = state.clone() else {
            return Err(PostingError::InvalidTransition);
        };

        let expected_set: BTreeSet<ContentHash> = expected.iter().copied().collect();
        let seen_set: BTreeSet<ContentHash> = readback_entry_hashes.iter().copied().collect();
        let missing: Vec<ContentHash> = expected_set.difference(&seen_set).copied().collect();
        let unexpected: Vec<ContentHash> = seen_set.difference(&expected_set).copied().collect();

        *state = if missing.is_empty() && unexpected.is_empty() {
            PostingState::Verified {
                gl_ref,
                readback_evidence,
            }
        } else {
            PostingState::VerificationFailed {
                missing,
                unexpected,
                readback_evidence,
            }
        };
        Ok(())
    }

    /// State of a batch.
    #[must_use]
    pub fn state(&self, id: &ContentHash) -> Option<&PostingState> {
        self.states.get(id)
    }

    /// Approval state of a batch (for audit surfaces).
    #[must_use]
    pub fn approval(&self, id: &ContentHash) -> Option<&DualControlState> {
        self.approvals.state(id)
    }
}

/// Errors from protocol operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PostingError {
    /// No batch under this id.
    #[error("unknown batch: {0}")]
    UnknownBatch(ContentHash),
    /// The requested transition is not legal from the current state.
    #[error("invalid posting transition")]
    InvalidTransition,
    /// Dual-control failure on release.
    #[error(transparent)]
    DualControl(#[from] DualControlError),
    /// Batch hashing failure.
    #[error(transparent)]
    Canon(#[from] CanonError),
}
