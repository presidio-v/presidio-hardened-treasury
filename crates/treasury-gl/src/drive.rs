//! Lifecycle orchestration: drive the posting protocol against any
//! [`GlAdapter`], including honest Unknown/read-back recovery.

use crate::adapter::{GlAdapter, GlError, SubmitOutcome};
use treasury_core::{ActorId, ContentHash};
use treasury_evidence::sha256;
use treasury_posting::{PostingBatch, PostingError, PostingProtocol, PostingState};

/// The terminal outcome of driving one batch to completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriveOutcome {
    /// Posted and read-back-verified: the GL shows exactly our entries.
    Verified {
        /// GL-side reference.
        gl_ref: String,
    },
    /// Posted but read-back disagreed; escalate. Names the differences.
    VerificationFailed {
        /// Batch entries the GL does not show.
        missing: Vec<ContentHash>,
        /// GL entries the batch does not contain.
        unexpected: Vec<ContentHash>,
    },
    /// The acknowledgment was lost and a read-back proved the batch is
    /// absent: it is safe to retry under the same idempotency key. The
    /// caller decides when to retry (this returns control, not a guess).
    RetryableNotPosted,
}

/// Drive `batch` through the full posting-protocol lifecycle against
/// `adapter`, using `preparer`/`approver` for the dual-control release.
/// Evidence (GL response, read-back payload) is hashed into content
/// addresses; the orchestration never guesses a lost ack.
///
/// # Errors
/// [`DriveError::Protocol`] for an illegal protocol transition;
/// [`DriveError::Gl`] for an adapter transport/auth/rejection failure;
/// [`DriveError::Unexpected`] if the protocol lands in a state the drive
/// did not direct (defensive; should not occur).
pub fn post_batch(
    protocol: &mut PostingProtocol,
    adapter: &mut impl GlAdapter,
    batch: PostingBatch,
    preparer: ActorId,
    approver: ActorId,
) -> Result<DriveOutcome, DriveError> {
    // Register + dual-control release.
    let id = protocol.register(batch.clone(), preparer)?;
    protocol.release(&id, approver)?;
    protocol.begin_submit(&id)?;

    // Submit through the adapter under the idempotency key (= batch id).
    match adapter.submit(&batch, id)? {
        SubmitOutcome::Acknowledged { gl_ref, raw_response } => {
            let response_evidence = sha256(&raw_response);
            protocol.confirm_posted(&id, gl_ref, response_evidence)?;
        }
        SubmitOutcome::AckLost => {
            protocol.report_unknown(&id)?;
            // Resolve Unknown by evidence only: query the GL.
            let readback = adapter.read_back(id)?;
            let response_evidence = sha256(&readback.raw_payload);
            let found = readback
                .gl_ref
                .clone()
                .map(|gl_ref| (gl_ref, response_evidence));
            let absent = found.is_none();
            protocol.resolve_unknown(&id, found)?;
            if absent {
                // Proven not posted: safe-to-retry, same key. Caller decides.
                return Ok(DriveOutcome::RetryableNotPosted);
            }
        }
    }

    // Read-back verification (two-way) for the posted batch.
    let readback = adapter.read_back(id)?;
    let readback_evidence = sha256(&readback.raw_payload);
    protocol.verify(&id, &readback.entry_hashes, readback_evidence)?;

    match protocol.state(&id) {
        Some(PostingState::Verified { gl_ref, .. }) => Ok(DriveOutcome::Verified {
            gl_ref: gl_ref.clone(),
        }),
        Some(PostingState::VerificationFailed {
            missing,
            unexpected,
            ..
        }) => Ok(DriveOutcome::VerificationFailed {
            missing: missing.clone(),
            unexpected: unexpected.clone(),
        }),
        _ => Err(DriveError::Unexpected),
    }
}

/// Errors from the orchestration.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DriveError {
    /// A posting-protocol transition failed.
    #[error(transparent)]
    Protocol(#[from] PostingError),
    /// The GL adapter failed.
    #[error(transparent)]
    Gl(#[from] GlError),
    /// The protocol ended in a state the drive did not direct.
    #[error("unexpected protocol state after drive")]
    Unexpected,
}
