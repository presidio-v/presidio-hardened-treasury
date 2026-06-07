//! Append-only, coverage-monotonic anchor log (spec v2 §3.3).

use crate::receipt::AnchorReceipt;
use treasury_core::{ContentHash, TimestampNs};
use treasury_evidence::EvidenceStore;

/// Append-only log of anchor receipts for one evidence store.
#[derive(Debug, Default)]
pub struct AnchorLog {
    receipts: Vec<AnchorReceipt>,
}

impl AnchorLog {
    /// Create an empty log.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a receipt.
    ///
    /// Validations (structural):
    /// - anchor time strictly greater than the last receipt's;
    /// - entry count not less than the last receipt's (evidence stores are
    ///   append-only; coverage can only grow).
    ///
    /// # Errors
    /// See [`AnchorError`].
    pub fn append(&mut self, receipt: AnchorReceipt) -> Result<(), AnchorError> {
        if let Some(last) = self.receipts.last() {
            if receipt.anchored_at <= last.anchored_at {
                return Err(AnchorError::NonMonotonicAnchorTime {
                    last: last.anchored_at,
                    proposed: receipt.anchored_at,
                });
            }
            if receipt.entry_count < last.entry_count {
                return Err(AnchorError::CoverageRegression {
                    last: last.entry_count,
                    proposed: receipt.entry_count,
                });
            }
        }
        self.receipts.push(receipt);
        Ok(())
    }

    /// The most recent receipt, if any.
    #[must_use]
    pub fn latest(&self) -> Option<&AnchorReceipt> {
        self.receipts.last()
    }

    /// All receipts in anchor order.
    #[must_use]
    pub fn receipts(&self) -> &[AnchorReceipt] {
        &self.receipts
    }

    /// Verify every receipt against the live store: the store's head over
    /// the receipt's first `entry_count` entries must equal the anchored
    /// head. Detects post-anchor tampering of any anchored prefix.
    ///
    /// # Errors
    /// [`AnchorError::EntryCountExceedsStore`] when a receipt claims more
    /// entries than the store holds;
    /// [`AnchorError::HeadMismatch`] when a recomputed prefix head differs
    /// from the anchored head — the tampering signal.
    pub fn verify_against<S: EvidenceStore>(&self, store: &S) -> Result<(), AnchorError> {
        for receipt in &self.receipts {
            let count = usize::try_from(receipt.entry_count).ok();
            let head = count.and_then(|c| store.tree_head_at(c));
            let Some(recomputed) = head else {
                return Err(AnchorError::EntryCountExceedsStore {
                    claimed: receipt.entry_count,
                    store_len: store.len(),
                });
            };
            if recomputed != receipt.tree_head {
                return Err(AnchorError::HeadMismatch {
                    anchored: receipt.tree_head,
                    recomputed,
                });
            }
        }
        Ok(())
    }
}

/// Errors from anchor log operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AnchorError {
    /// Anchor times must strictly increase.
    #[error("non-monotonic anchor time: last {last:?}, proposed {proposed:?}")]
    NonMonotonicAnchorTime {
        /// Time of the last receipt.
        last: TimestampNs,
        /// Rejected anchor time.
        proposed: TimestampNs,
    },
    /// Coverage can only grow on an append-only store.
    #[error("coverage regression: last covered {last}, proposed {proposed}")]
    CoverageRegression {
        /// Entries covered by the last receipt.
        last: u64,
        /// Rejected (smaller) coverage.
        proposed: u64,
    },
    /// A receipt claims more entries than the store holds.
    #[error("receipt claims {claimed} entries, store holds {store_len}")]
    EntryCountExceedsStore {
        /// Entry count claimed by the receipt.
        claimed: u64,
        /// Entries actually in the store.
        store_len: usize,
    },
    /// Recomputed prefix head differs from the anchored head — tampering.
    #[error("anchored head {anchored} != recomputed {recomputed}")]
    HeadMismatch {
        /// Head recorded in the receipt.
        anchored: ContentHash,
        /// Head recomputed from the live store.
        recomputed: ContentHash,
    },
}
