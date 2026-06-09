//! The vendor-agnostic GL adapter contract.

use treasury_core::ContentHash;
use treasury_posting::PostingBatch;

/// What a GL reported when we submitted a batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmitOutcome {
    /// The GL acknowledged the post: its reference, and the raw response
    /// bytes (which the orchestration hashes into the evidence store).
    Acknowledged {
        /// GL-side reference (journal id / document number).
        gl_ref: String,
        /// Raw GL response payload.
        raw_response: Vec<u8>,
    },
    /// The acknowledgment was lost (timeout, dropped connection). The
    /// post may or may not have landed — only a read-back can tell.
    AckLost,
}

/// What a GL returned when we read a previously-submitted batch back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlReadback {
    /// The GL-side reference, if the batch is present (absent → not
    /// posted, a safe-to-retry signal).
    pub gl_ref: Option<String>,
    /// Our entry hashes the GL shows for this batch, mapped back from
    /// the external id we stamped at post time. Compared both ways
    /// against the batch fingerprint by the protocol.
    pub entry_hashes: Vec<ContentHash>,
    /// Raw read-back payload (hashed into the evidence store).
    pub raw_payload: Vec<u8>,
}

/// The contract every concrete GL implements.
///
/// **Read-back is mandatory by type** (ADR-0003): an adapter that can
/// post but cannot read back cannot implement this trait, so it cannot
/// be driven by [`crate::post_batch`]. Idempotency is the adapter's
/// responsibility *and* the protocol's: `submit` is called with the
/// batch's content hash as the idempotency key, and a compliant adapter
/// dedupes on it so a retry of the same key never double-posts.
pub trait GlAdapter {
    /// Submit a batch under an idempotency key (the batch's content
    /// hash). Re-submitting the same key must not create a duplicate.
    ///
    /// # Errors
    /// [`GlError`] for transport/permission failures distinct from a
    /// lost ack (which is a normal [`SubmitOutcome::AckLost`]).
    fn submit(
        &mut self,
        batch: &PostingBatch,
        idempotency_key: ContentHash,
    ) -> Result<SubmitOutcome, GlError>;

    /// Read a previously-submitted batch back from the GL by its
    /// idempotency key. The presence/absence and the returned entry
    /// hashes drive the protocol's verification and Unknown-recovery.
    ///
    /// # Errors
    /// [`GlError`] for transport/permission failures.
    fn read_back(&self, idempotency_key: ContentHash) -> Result<GlReadback, GlError>;
}

/// Errors a GL adapter may raise (distinct from a lost ack, which is a
/// normal outcome).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GlError {
    /// Transport failure (network, 5xx).
    #[error("gl transport error: {0}")]
    Transport(String),
    /// Authentication/authorization failure.
    #[error("gl auth error: {0}")]
    Auth(String),
    /// The GL rejected the batch (mapping, locked period, validation).
    #[error("gl rejected the batch: {0}")]
    Rejected(String),
}
