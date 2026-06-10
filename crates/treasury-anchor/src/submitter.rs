//! The chain-wallet I/O seam (ADR-0002): the boundary the live Bitcoin
//! wallet implements and the [`crate::AnchorPipeline`] is driven from.
//!
//! ADR-0002 keeps the submission integration deliberately outside the
//! tested core: the pipeline is an evidence-driven state machine, and the
//! wallet only *reports outcomes into* it (broadcast a commitment, observe
//! confirmations, fetch the calendar-independent proof). This trait names
//! that seam so a concrete wallet (Bitcoin Core `sendrawtransaction` +
//! `getrawtransaction`/`gettxout`) and the in-memory fixture are the same
//! shape — and so the conformance suite can drive either one identically.

use std::cell::Cell;
use treasury_core::ContentHash;
use treasury_evidence::sha256;

/// The result of broadcasting a commitment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Broadcast {
    /// The chain transaction reference.
    pub tx_ref: String,
    /// The chain height at submission (the start of the liveness window).
    pub submitted_height: u64,
}

/// A point-in-time confirmation observation for a broadcast transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Confirmation {
    /// The chain tip at observation.
    pub current_height: u64,
    /// The block height the transaction was included at, once mined
    /// (`None` while still in the mempool).
    pub included_at: Option<u64>,
}

/// The chain wallet that commits an aggregation root and reports its
/// confirmation status back into the [`crate::AnchorPipeline`].
///
/// A real wallet is this same trait over RPC; the pipeline never trusts
/// the wallet for anything but outcome reports, and finalization still
/// requires the confirmation depth and the calendar-independent proof.
pub trait ChainAnchorSubmitter {
    /// Broadcast a commitment to `root`, returning the transaction
    /// reference and the submission height.
    ///
    /// # Errors
    /// [`SubmitterError`] on a transport/wallet failure.
    fn broadcast(&mut self, root: ContentHash) -> Result<Broadcast, SubmitterError>;

    /// Observe the current confirmation status of `tx_ref`.
    ///
    /// # Errors
    /// [`SubmitterError::NotBroadcast`] for an unknown reference;
    /// [`SubmitterError::Transport`] on a wallet failure.
    fn poll(&self, tx_ref: &str) -> Result<Confirmation, SubmitterError>;

    /// The calendar-independent (block-header-path) proof for a confirmed
    /// transaction — the durable artifact that survives the submitting
    /// service disappearing. Hashed into the evidence store; its content
    /// hash is what finalization records.
    ///
    /// # Errors
    /// [`SubmitterError::NotConfirmed`] when the transaction is not yet
    /// mined; [`SubmitterError::Transport`] on a wallet failure.
    fn calendar_proof(&self, tx_ref: &str) -> Result<ContentHash, SubmitterError>;
}

/// Errors a chain submitter may raise.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SubmitterError {
    /// The wallet could not be reached.
    #[error("submitter transport error: {0}")]
    Transport(String),
    /// The transaction reference was never broadcast by this submitter.
    #[error("transaction not broadcast")]
    NotBroadcast,
    /// A proof was requested before the transaction confirmed.
    #[error("transaction not yet confirmed")]
    NotConfirmed,
}

/// An in-memory submitter for tests: deterministic, so the pipeline can be
/// driven to "anchored" without a live chain. The chain tip advances by
/// one on every [`ChainAnchorSubmitter::poll`]; the transaction is
/// included `blocks_to_inclusion` blocks after broadcast (or never, when
/// `never_confirms` is set, to exercise the overdue/liveness path).
#[derive(Debug)]
pub struct FixtureChainSubmitter {
    tip: Cell<u64>,
    blocks_to_inclusion: u64,
    never_confirms: bool,
    broadcast_height: Cell<Option<u64>>,
    included_height: Cell<Option<u64>>,
    seq: Cell<u64>,
}

impl FixtureChainSubmitter {
    /// A submitter starting at chain height `start_tip` that confirms a
    /// broadcast `blocks_to_inclusion` blocks later.
    #[must_use]
    pub fn new(start_tip: u64, blocks_to_inclusion: u64) -> Self {
        Self {
            tip: Cell::new(start_tip),
            blocks_to_inclusion,
            never_confirms: false,
            broadcast_height: Cell::new(None),
            included_height: Cell::new(None),
            seq: Cell::new(0),
        }
    }

    /// A submitter whose broadcast never confirms — for the overdue path.
    #[must_use]
    pub fn never_confirming(start_tip: u64) -> Self {
        Self {
            never_confirms: true,
            ..Self::new(start_tip, 0)
        }
    }
}

impl ChainAnchorSubmitter for FixtureChainSubmitter {
    fn broadcast(&mut self, _root: ContentHash) -> Result<Broadcast, SubmitterError> {
        let submitted_height = self.tip.get();
        let n = self.seq.get().saturating_add(1);
        self.seq.set(n);
        self.broadcast_height.set(Some(submitted_height));
        if self.never_confirms {
            self.included_height.set(None);
        } else {
            self.included_height.set(Some(
                submitted_height.saturating_add(self.blocks_to_inclusion),
            ));
        }
        Ok(Broadcast {
            tx_ref: format!("fixture-tx-{n}"),
            submitted_height,
        })
    }

    fn poll(&self, _tx_ref: &str) -> Result<Confirmation, SubmitterError> {
        if self.broadcast_height.get().is_none() {
            return Err(SubmitterError::NotBroadcast);
        }
        let current_height = self.tip.get().saturating_add(1);
        self.tip.set(current_height);
        let included_at = self
            .included_height
            .get()
            .filter(|included| current_height >= *included);
        Ok(Confirmation {
            current_height,
            included_at,
        })
    }

    fn calendar_proof(&self, tx_ref: &str) -> Result<ContentHash, SubmitterError> {
        let included = self
            .included_height
            .get()
            .ok_or(SubmitterError::NotConfirmed)?;
        if self.tip.get() < included {
            return Err(SubmitterError::NotConfirmed);
        }
        Ok(sha256(format!("calendar-proof:{tx_ref}").as_bytes()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{AnchorPipeline, AnchorTarget, PipelineState};
    use crate::policy::AnchorPolicy;
    use treasury_core::TimestampNs;

    fn target(byte: u8, entry_count: u64) -> AnchorTarget {
        AnchorTarget {
            tree_head: ContentHash([byte; 32]),
            entry_count,
        }
    }

    #[test]
    fn pipeline_drives_to_anchored_from_submitter_outcomes() {
        let mut submitter = FixtureChainSubmitter::new(800_000, 1);
        let Ok(mut pipeline) = AnchorPipeline::start(vec![target(1, 10), target(2, 5)]) else {
            unreachable!("non-empty targets start");
        };

        let Ok(broadcast) = submitter.broadcast(pipeline.root()) else {
            unreachable!("broadcast succeeds");
        };
        assert!(!broadcast.tx_ref.is_empty());
        let Ok(()) = pipeline.submitted(broadcast.tx_ref.clone(), broadcast.submitted_height)
        else {
            unreachable!("Pending → Submitted is legal");
        };

        let mut last_height = broadcast.submitted_height;
        let required_depth = 3;
        for _ in 0..16 {
            let Ok(confirmation) = submitter.poll(&broadcast.tx_ref) else {
                unreachable!("poll succeeds after broadcast");
            };
            assert!(confirmation.current_height >= last_height);
            last_height = confirmation.current_height;
            let Some(included) = confirmation.included_at else {
                continue;
            };
            let Ok(()) = pipeline.confirmed(included, confirmation.current_height) else {
                unreachable!("Submitted/Confirmed → Confirmed is legal");
            };
            if let PipelineState::Confirmed { depth, .. } = pipeline.state() {
                if *depth >= required_depth {
                    break;
                }
            }
        }

        let Ok(proof) = submitter.calendar_proof(&broadcast.tx_ref) else {
            unreachable!("proof available after confirmation");
        };
        let Ok(receipts) = pipeline.finalize(
            &AnchorPolicy::new(required_depth),
            proof,
            TimestampNs::from_nanos(1),
        ) else {
            unreachable!("depth threshold met");
        };
        assert_eq!(receipts.len(), 2);
        assert!(matches!(pipeline.state(), PipelineState::Anchored { .. }));
    }

    #[test]
    fn never_confirming_broadcast_is_flagged_overdue() {
        let mut submitter = FixtureChainSubmitter::never_confirming(800_000);
        let Ok(mut pipeline) = AnchorPipeline::start(vec![target(1, 1)]) else {
            unreachable!("non-empty targets start");
        };
        let Ok(broadcast) = submitter.broadcast(pipeline.root()) else {
            unreachable!("broadcast succeeds");
        };
        let Ok(()) = pipeline.submitted(broadcast.tx_ref.clone(), broadcast.submitted_height)
        else {
            unreachable!("Pending → Submitted is legal");
        };

        let mut current_height = broadcast.submitted_height;
        for _ in 0..10 {
            let Ok(confirmation) = submitter.poll(&broadcast.tx_ref) else {
                unreachable!("poll succeeds after broadcast");
            };
            assert!(confirmation.included_at.is_none());
            current_height = confirmation.current_height;
        }
        assert!(pipeline.is_overdue(current_height, 5));
    }
}
