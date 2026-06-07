//! Checkpoint lineage (spec v2 §3.6, remediation R6).
//!
//! Closed periods do **not** "lock" — they become immutable nodes in a
//! checkpoint DAG. Late data, reorgs, or provider restatements create a
//! successor checkpoint with a recorded supersession edge, a reason code,
//! and a materiality-assessment evidence reference (SAB 99). Reopening is
//! an append, same trust model as everything else.
//!
//! Structural guarantees:
//! - "As filed" is a permanent pointer to a period's first checkpoint;
//!   "as corrected" is the head. Neither requires archaeology.
//! - A superseding checkpoint **must** carry a reason code and a
//!   materiality memo evidence hash; an initial checkpoint must carry
//!   neither. The workflow (detection → materiality memo → recast) is
//!   enforced by construction, not by review.
//! - Supersession cannot race, cross tenants, or cross periods.
//! - The folded state root is reproducible: it commits to the exact set
//!   of ledger events active at the checkpoint's knowledge time.

#![forbid(unsafe_code)]

pub mod checkpoint;
pub mod dag;
pub mod fold;

pub use checkpoint::{
    CheckpointDraft, CheckpointId, PeriodId, SealedCheckpoint, SupersessionReason,
};
pub use dag::{CheckpointDag, CloseError};
pub use fold::state_root;
