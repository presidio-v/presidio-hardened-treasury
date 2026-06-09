//! The contract a GL adapter shim must satisfy (ADR-0003).
//!
//! `treasury-gl` already makes read-back mandatory *by type*. What a
//! concrete NetSuite/QuickBooks/SAP adapter can still get wrong at runtime,
//! and what the posting protocol trusts it not to, is:
//!
//! 1. **Drive verifies** — a clean post, driven through the protocol,
//!    reaches `Verified` with the GL showing exactly our entries (the
//!    two-way read-back ties out).
//! 2. **Read-back fidelity** — the entry hashes the GL returns for a batch
//!    round-trip the batch fingerprint exactly: nothing missing, nothing
//!    extra.
//! 3. **Idempotency** — re-submitting the same idempotency key (the batch
//!    content hash) never creates a second posting; read-back still shows
//!    one record under that key.
//!
//! Residual only the live job covers: the lost-acknowledgment / Unknown
//! recovery paths and the dropped/extra-entry detections. Those require
//! injecting a fault that a real GL will not produce on demand, so they
//! are exercised against `FixtureGl` with [`treasury_gl::FixtureFault`] in
//! `treasury-gl`'s own tests, not here.

use crate::ContractViolation;
use treasury_core::{ActorId, ContentHash};
use treasury_gl::{post_batch, DriveOutcome, GlAdapter, SubmitOutcome};
use treasury_posting::{PostingBatch, PostingProtocol};

fn underlying(e: impl ToString) -> ContractViolation {
    ContractViolation::Underlying(e.to_string())
}

/// Entries in `left` that are not in `right` (set difference; batches are
/// small, so a linear scan is exact and cheap).
fn missing_from(left: &[ContentHash], right: &[ContentHash]) -> Vec<ContentHash> {
    left.iter()
        .filter(|hash| !right.contains(hash))
        .copied()
        .collect()
}

/// Invariant 1 — driving a clean batch through the protocol verifies.
///
/// # Errors
/// [`ContractViolation::ReadbackMismatch`] if the GL shows different
/// entries; [`ContractViolation::NotPosted`] if a clean submit reports
/// not-posted; [`ContractViolation::Underlying`] for a drive failure.
pub fn verify_drive_verifies<A: GlAdapter>(
    adapter: &mut A,
    batch: &PostingBatch,
    preparer: ActorId,
    approver: ActorId,
) -> Result<(), ContractViolation> {
    let mut protocol = PostingProtocol::new();
    match post_batch(&mut protocol, adapter, batch.clone(), preparer, approver) {
        Ok(DriveOutcome::Verified { .. }) => Ok(()),
        Ok(DriveOutcome::VerificationFailed {
            missing,
            unexpected,
        }) => Err(ContractViolation::ReadbackMismatch {
            missing,
            unexpected,
        }),
        Ok(DriveOutcome::RetryableNotPosted) => Err(ContractViolation::NotPosted),
        Err(e) => Err(underlying(e)),
    }
}

/// Invariant 2 — read-back round-trips the batch fingerprint exactly.
///
/// # Errors
/// [`ContractViolation::NotPosted`] if the GL shows no reference for the
/// key; [`ContractViolation::ReadbackMismatch`] on any difference;
/// [`ContractViolation::Underlying`] for a transport/hashing failure.
pub fn verify_readback_fidelity<A: GlAdapter>(
    adapter: &mut A,
    batch: &PostingBatch,
) -> Result<(), ContractViolation> {
    let key = batch.batch_id().map_err(underlying)?;
    let fingerprint = batch.entry_fingerprint().map_err(underlying)?;
    adapter.submit(batch, key).map_err(underlying)?;
    let readback = adapter.read_back(key).map_err(underlying)?;
    if readback.gl_ref.is_none() {
        return Err(ContractViolation::NotPosted);
    }
    let missing = missing_from(&fingerprint, &readback.entry_hashes);
    let unexpected = missing_from(&readback.entry_hashes, &fingerprint);
    if !missing.is_empty() || !unexpected.is_empty() {
        return Err(ContractViolation::ReadbackMismatch {
            missing,
            unexpected,
        });
    }
    Ok(())
}

/// Invariant 3 — re-submitting the same key does not create a second
/// posting, and read-back still shows a single record under the key.
///
/// # Errors
/// [`ContractViolation::NotIdempotent`] if two submits acknowledge with
/// different references; [`ContractViolation::NotPosted`] if the key shows
/// no record after submitting; [`ContractViolation::Underlying`] otherwise.
pub fn verify_idempotent_submit<A: GlAdapter>(
    adapter: &mut A,
    batch: &PostingBatch,
) -> Result<(), ContractViolation> {
    let key = batch.batch_id().map_err(underlying)?;
    let first = adapter.submit(batch, key).map_err(underlying)?;
    let second = adapter.submit(batch, key).map_err(underlying)?;
    if let (
        SubmitOutcome::Acknowledged { gl_ref: a, .. },
        SubmitOutcome::Acknowledged { gl_ref: b, .. },
    ) = (&first, &second)
    {
        if a != b {
            return Err(ContractViolation::NotIdempotent {
                first: a.clone(),
                second: b.clone(),
            });
        }
    }
    if adapter.read_back(key).map_err(underlying)?.gl_ref.is_none() {
        return Err(ContractViolation::NotPosted);
    }
    Ok(())
}

/// The full GL adapter contract (invariants 1–3) in one call. Uses a
/// freshly constructed adapter assumption: invariant 1 drives one batch,
/// then 2 and 3 re-exercise the same key (idempotency makes this safe).
///
/// # Errors
/// The first violated invariant.
pub fn verify_gl_adapter_contract<A: GlAdapter>(
    adapter: &mut A,
    batch: &PostingBatch,
    preparer: ActorId,
    approver: ActorId,
) -> Result<(), ContractViolation> {
    verify_drive_verifies(&mut *adapter, batch, preparer, approver)?;
    verify_readback_fidelity(&mut *adapter, batch)?;
    verify_idempotent_submit(&mut *adapter, batch)?;
    Ok(())
}
