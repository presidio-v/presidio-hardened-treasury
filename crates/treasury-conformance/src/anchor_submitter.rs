//! The contract a chain-wallet anchor submitter must satisfy (ADR-0002).
//!
//! The [`AnchorPipeline`] is the tested state machine; the wallet only
//! reports outcomes into it. The invariants a real wallet can silently
//! break — and that the pipeline trusts it not to — are:
//!
//! 1. **Broadcast yields a usable reference** at a concrete submission
//!    height (the start of the liveness window).
//! 2. **Heights are monotonic** — the reported chain tip never goes
//!    backwards across polls (a backwards tip corrupts every depth
//!    computation downstream).
//! 3. **Liveness** — a healthy broadcast reaches the required confirmation
//!    depth within a bounded number of polls; one that does not is
//!    surfaced, never silently pending forever.
//! 4. **Finalization is faithful** — driving the reported outcomes through
//!    the pipeline produces exactly one receipt per target, all sharing
//!    the transaction reference.
//!
//! Residual only the live job covers: a wallet that reports a confirmation
//! it cannot later prove (the `calendar_proof` must correspond to the same
//! transaction), and real mempool eviction / RBF. The fixture confirms
//! deterministically, so this suite proves the drive wiring; the live
//! regtest job proves the wallet.

use crate::ContractViolation;
use treasury_anchor::{
    AnchorPipeline, AnchorPolicy, AnchorReceipt, AnchorTarget, ChainAnchorSubmitter, PipelineState,
};
use treasury_core::TimestampNs;

fn underlying(e: impl ToString) -> ContractViolation {
    ContractViolation::Underlying(e.to_string())
}

/// Drive `submitter` through one full anchoring lifecycle — broadcast,
/// poll to the required depth, finalize — asserting invariants 1–4. On
/// success returns the receipts ready for the anchor log.
///
/// # Errors
/// The first violated invariant; underlying pipeline/wallet failures are
/// wrapped in [`ContractViolation::Underlying`].
pub fn verify_anchor_submitter_contract<S: ChainAnchorSubmitter>(
    submitter: &mut S,
    targets: Vec<AnchorTarget>,
    required_depth: u64,
    poll_budget: u32,
    anchored_at: TimestampNs,
) -> Result<Vec<AnchorReceipt>, ContractViolation> {
    let target_count = targets.len();
    let mut pipeline = AnchorPipeline::start(targets).map_err(underlying)?;

    // Invariant 1: broadcast yields a usable reference.
    let broadcast = submitter.broadcast(pipeline.root()).map_err(underlying)?;
    if broadcast.tx_ref.is_empty() {
        return Err(ContractViolation::Underlying(
            "broadcast returned an empty tx_ref".to_owned(),
        ));
    }
    let tx_ref = broadcast.tx_ref.clone();
    pipeline
        .submitted(broadcast.tx_ref, broadcast.submitted_height)
        .map_err(underlying)?;

    // Invariants 2 + 3: monotonic heights, confirmation within budget.
    let mut last_height = broadcast.submitted_height;
    let mut reached_depth = false;
    for _ in 0..poll_budget {
        let confirmation = submitter.poll(&tx_ref).map_err(underlying)?;
        if confirmation.current_height < last_height {
            return Err(ContractViolation::HeightWentBackwards {
                previous: last_height,
                observed: confirmation.current_height,
            });
        }
        last_height = confirmation.current_height;
        let Some(included) = confirmation.included_at else {
            continue;
        };
        pipeline
            .confirmed(included, confirmation.current_height)
            .map_err(underlying)?;
        if let PipelineState::Confirmed { depth, .. } = pipeline.state() {
            if *depth >= required_depth {
                reached_depth = true;
                break;
            }
        }
    }
    if !reached_depth {
        return Err(ContractViolation::AnchorNeverConfirmed {
            required: required_depth,
            polls: poll_budget,
        });
    }

    // Invariant 4: finalization is faithful.
    let proof = submitter.calendar_proof(&tx_ref).map_err(underlying)?;
    let receipts = pipeline
        .finalize(&AnchorPolicy::new(required_depth), proof, anchored_at)
        .map_err(underlying)?;
    verify_receipt_coverage(target_count, &receipts)?;
    Ok(receipts)
}

/// The finalization invariant: exactly one receipt per anchored target.
///
/// The pipeline upholds this by construction (it emits one receipt per
/// target), so a healthy run can never violate it — the guard is exposed
/// as its own function so the invariant is directly testable and a faulty
/// finalizer that dropped or duplicated a receipt would be caught.
///
/// # Errors
/// [`ContractViolation::ReceiptCountMismatch`] when the counts differ.
pub fn verify_receipt_coverage(
    expected: usize,
    receipts: &[AnchorReceipt],
) -> Result<(), ContractViolation> {
    if receipts.len() != expected {
        return Err(ContractViolation::ReceiptCountMismatch {
            expected,
            found: receipts.len(),
        });
    }
    Ok(())
}
