//! The anchoring pipeline (spec v2 §3.3, ADR-0002 action items 1, 2, 6).
//!
//! Drives one aggregation from "root computed" to "anchored," modeling
//! the Bitcoin submission lifecycle as an evidence-driven state machine —
//! the same discipline as the GL posting protocol: no transition encodes
//! a guess, and an anchor that never confirms cannot become a silent
//! coverage gap (liveness is queryable).
//!
//! The actual chain wallet (broadcast a transaction, read confirmations)
//! is a thin I/O adapter that *reports outcomes into* this machine; the
//! machine itself is the tested core. On success it produces the
//! `AnchorReceipt`s — one per aggregated target, sharing the transaction
//! reference — ready to append to an [`crate::AnchorLog`].

use crate::aggregation::{aggregate, Aggregation};
use crate::policy::AnchorPolicy;
use crate::receipt::{AnchorMethod, AnchorReceipt};
use treasury_core::{ContentHash, TimestampNs};

/// One thing being anchored: an evidence-store tree head and the entry
/// count it covers (carried straight into the resulting receipt).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnchorTarget {
    /// The evidence-store tree head.
    pub tree_head: ContentHash,
    /// Entries the head covers.
    pub entry_count: u64,
}

/// Where an aggregation is in its anchoring lifecycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineState {
    /// Root computed; awaiting broadcast.
    Pending,
    /// Broadcast at chain height `submitted_height`; awaiting confirmation.
    Submitted {
        /// Chain transaction reference.
        tx_ref: String,
        /// Chain height at submission (for the liveness window).
        submitted_height: u64,
    },
    /// Included at `block_height`; current depth recorded.
    Confirmed {
        /// Chain transaction reference.
        tx_ref: String,
        /// Block height of inclusion.
        block_height: u64,
        /// Confirmation depth observed.
        depth: u64,
    },
    /// Depth met the required threshold and the proof was upgraded to a
    /// calendar-independent form. Terminal success.
    Anchored {
        /// Chain transaction reference.
        tx_ref: String,
        /// Evidence-store hash of the calendar-independent proof
        /// (block-header path) — the durable, intermediary-free artifact.
        proof_evidence: ContentHash,
    },
}

/// The anchoring pipeline for one aggregation of targets.
#[derive(Debug)]
pub struct AnchorPipeline {
    targets: Vec<AnchorTarget>,
    aggregation: Aggregation,
    state: PipelineState,
}

impl AnchorPipeline {
    /// Start a pipeline over a non-empty set of targets, computing the
    /// aggregation root and per-target inclusion proofs.
    ///
    /// # Errors
    /// [`PipelineError::NothingToAnchor`] when `targets` is empty.
    pub fn start(targets: Vec<AnchorTarget>) -> Result<Self, PipelineError> {
        let heads: Vec<ContentHash> = targets.iter().map(|t| t.tree_head).collect();
        let aggregation = aggregate(&heads).ok_or(PipelineError::NothingToAnchor)?;
        Ok(Self {
            targets,
            aggregation,
            state: PipelineState::Pending,
        })
    }

    /// The aggregation root — the value the chain adapter commits.
    #[must_use]
    pub fn root(&self) -> ContentHash {
        self.aggregation.root
    }

    /// Current state.
    #[must_use]
    pub fn state(&self) -> &PipelineState {
        &self.state
    }

    /// The chain adapter reports the commitment was broadcast.
    ///
    /// # Errors
    /// [`PipelineError::InvalidTransition`] unless `Pending`.
    pub fn submitted(
        &mut self,
        tx_ref: String,
        submitted_height: u64,
    ) -> Result<(), PipelineError> {
        if self.state != PipelineState::Pending {
            return Err(PipelineError::InvalidTransition);
        }
        self.state = PipelineState::Submitted {
            tx_ref,
            submitted_height,
        };
        Ok(())
    }

    /// The chain adapter reports inclusion at `block_height`, with the
    /// chain currently at `current_height` (depth = current − block + 1).
    ///
    /// # Errors
    /// [`PipelineError::InvalidTransition`] unless `Submitted` or already
    /// `Confirmed` (re-observation deepens the count);
    /// [`PipelineError::HeightUnderflow`] when `current_height <
    /// block_height`.
    pub fn confirmed(
        &mut self,
        block_height: u64,
        current_height: u64,
    ) -> Result<(), PipelineError> {
        let tx_ref = match &self.state {
            PipelineState::Submitted { tx_ref, .. } | PipelineState::Confirmed { tx_ref, .. } => {
                tx_ref.clone()
            }
            _ => return Err(PipelineError::InvalidTransition),
        };
        let span = current_height
            .checked_sub(block_height)
            .ok_or(PipelineError::HeightUnderflow)?;
        let depth = span.saturating_add(1);
        self.state = PipelineState::Confirmed {
            tx_ref,
            block_height,
            depth,
        };
        Ok(())
    }

    /// Finalize: require `depth >= policy.required_depth` and record the
    /// calendar-independent proof. Produces one receipt per target, sharing
    /// the transaction reference, each committing to `policy`'s hash so the
    /// confirmation threshold is in the audit trail. Ready for the log.
    ///
    /// # Errors
    /// [`PipelineError::InvalidTransition`] unless `Confirmed`;
    /// [`PipelineError::InsufficientDepth`] when the threshold is unmet;
    /// [`PipelineError::Receipt`] if the policy or a receipt fails to hash.
    pub fn finalize(
        &mut self,
        policy: &AnchorPolicy,
        proof_evidence: ContentHash,
        anchored_at: TimestampNs,
    ) -> Result<Vec<AnchorReceipt>, PipelineError> {
        let (tx_ref, depth) = match &self.state {
            PipelineState::Confirmed { tx_ref, depth, .. } => (tx_ref.clone(), *depth),
            _ => return Err(PipelineError::InvalidTransition),
        };
        if depth < policy.required_depth {
            return Err(PipelineError::InsufficientDepth {
                depth,
                required: policy.required_depth,
            });
        }
        let confirmation_policy = policy.policy_hash().map_err(PipelineError::receipt)?;
        let mut receipts = Vec::with_capacity(self.targets.len());
        for target in &self.targets {
            let receipt = AnchorReceipt {
                tree_head: target.tree_head,
                entry_count: target.entry_count,
                method: AnchorMethod::PublicChain {
                    chain: "bitcoin".to_owned(),
                    tx_ref: tx_ref.clone(),
                },
                anchored_at,
                confirmation_policy,
            };
            // Surface a hashing failure rather than emitting a bad receipt.
            receipt.receipt_hash().map_err(PipelineError::receipt)?;
            receipts.push(receipt);
        }
        self.state = PipelineState::Anchored {
            tx_ref,
            proof_evidence,
        };
        Ok(receipts)
    }

    /// Liveness: a submitted commitment is overdue if it has not
    /// confirmed within `max_blocks` of its submission height. An overdue
    /// anchor must alert — it cannot silently never confirm.
    #[must_use]
    pub fn is_overdue(&self, current_height: u64, max_blocks: u64) -> bool {
        match &self.state {
            PipelineState::Submitted {
                submitted_height, ..
            } => current_height.saturating_sub(*submitted_height) > max_blocks,
            _ => false,
        }
    }
}

/// Errors from the anchoring pipeline.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PipelineError {
    /// No targets to anchor.
    #[error("nothing to anchor")]
    NothingToAnchor,
    /// The requested transition is not legal from the current state.
    #[error("invalid anchoring transition")]
    InvalidTransition,
    /// `current_height` was below `block_height`.
    #[error("current height below block height")]
    HeightUnderflow,
    /// Confirmation depth has not reached the required threshold.
    #[error("insufficient depth: {depth} < required {required}")]
    InsufficientDepth {
        /// Observed depth.
        depth: u64,
        /// Required threshold.
        required: u64,
    },
    /// A receipt failed to hash.
    #[error("receipt hashing failed: {0}")]
    Receipt(String),
}

impl PipelineError {
    // By-value is load-bearing: used as a `map_err(PipelineError::receipt)`
    // callback, which is `FnOnce(CanonError) -> Self`; the variant holds a
    // `String`, so the error is converted via `to_string`, not moved.
    #[allow(clippy::needless_pass_by_value)]
    fn receipt(e: treasury_evidence::CanonError) -> Self {
        Self::Receipt(e.to_string())
    }
}
