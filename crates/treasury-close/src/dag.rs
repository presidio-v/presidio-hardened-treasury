//! The checkpoint DAG (spec v2 §3.6): append-only, reason-coded,
//! materiality-backed supersession lineage per (tenant, period).

use crate::checkpoint::{CheckpointDraft, CheckpointId, PeriodId, SealedCheckpoint};
use std::collections::{HashMap, HashSet};
use treasury_core::{TenantId, TimestampNs};
use treasury_evidence::CanonError;

/// Append-only checkpoint lineages per (tenant, period).
#[derive(Debug, Default)]
pub struct CheckpointDag {
    lineages: HashMap<(TenantId, PeriodId), Vec<SealedCheckpoint>>,
    by_id: HashMap<CheckpointId, (TenantId, PeriodId, usize)>,
    superseded: HashSet<CheckpointId>,
}

impl CheckpointDag {
    /// Create an empty DAG.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seal a checkpoint into its (tenant, period) lineage.
    ///
    /// Validations (all structural, none waivable):
    /// - sealing time strictly greater than the lineage's last;
    /// - an initial checkpoint carries neither reason nor materiality memo;
    /// - a superseding checkpoint carries **both** a reason code and a
    ///   materiality memo evidence hash;
    /// - the superseded target exists, belongs to the same (tenant,
    ///   period), and is not already superseded;
    /// - a lineage's second and later checkpoints must supersede (no
    ///   parallel "first" checkpoints for a period).
    ///
    /// # Errors
    /// See [`CloseError`].
    pub fn seal(
        &mut self,
        draft: CheckpointDraft,
        sealed_at: TimestampNs,
    ) -> Result<CheckpointId, CloseError> {
        let lineage_key = (draft.tenant.clone(), draft.period.clone());

        if let Some(last) = self.lineages.get(&lineage_key).and_then(|l| l.last()) {
            if sealed_at <= last.sealed_at {
                return Err(CloseError::NonMonotonicSealTime {
                    last: last.sealed_at,
                    proposed: sealed_at,
                });
            }
        }

        match draft.supersedes {
            None => {
                if draft.reason.is_some() || draft.materiality_memo.is_some() {
                    return Err(CloseError::InitialWithCorrectionFields);
                }
                let existing = self.lineages.get(&lineage_key);
                if existing.is_some_and(|l| !l.is_empty()) {
                    return Err(CloseError::ParallelInitialCheckpoint);
                }
            }
            Some(target_id) => {
                if draft.reason.is_none() || draft.materiality_memo.is_none() {
                    return Err(CloseError::SupersessionWithoutJustification);
                }
                let (t_tenant, t_period, _) = self
                    .by_id
                    .get(&target_id)
                    .ok_or(CloseError::SupersedeTargetMissing(target_id))?;
                if *t_tenant != draft.tenant || *t_period != draft.period {
                    return Err(CloseError::SupersedeAcrossLineage(target_id));
                }
                if self.superseded.contains(&target_id) {
                    return Err(CloseError::AlreadySuperseded(target_id));
                }
            }
        }

        let checkpoint_id = SealedCheckpoint::compute_id(sealed_at, &draft)?;
        let supersedes = draft.supersedes;
        let sealed = SealedCheckpoint {
            checkpoint_id,
            sealed_at,
            draft,
        };

        let lineage = self.lineages.entry(lineage_key.clone()).or_default();
        let idx = lineage.len();
        lineage.push(sealed);
        let position = (lineage_key.0, lineage_key.1, idx);
        self.by_id.insert(checkpoint_id, position);
        if let Some(s) = supersedes {
            self.superseded.insert(s);
        }
        Ok(checkpoint_id)
    }

    /// The permanent "as filed" pointer: the period's first checkpoint.
    #[must_use]
    pub fn as_filed(&self, tenant: &TenantId, period: &PeriodId) -> Option<&SealedCheckpoint> {
        let lineage = self.lineages.get(&(tenant.clone(), period.clone()))?;
        lineage.first()
    }

    /// The "as corrected" head: the period's latest non-superseded
    /// checkpoint.
    #[must_use]
    pub fn head(&self, tenant: &TenantId, period: &PeriodId) -> Option<&SealedCheckpoint> {
        self.lineages
            .get(&(tenant.clone(), period.clone()))?
            .iter()
            .rev()
            .find(|c| !self.superseded.contains(&c.checkpoint_id))
    }

    /// Full lineage for a period in sealing order.
    #[must_use]
    pub fn lineage(&self, tenant: &TenantId, period: &PeriodId) -> &[SealedCheckpoint] {
        let lineage = self.lineages.get(&(tenant.clone(), period.clone()));
        lineage.map_or(&[], Vec::as_slice)
    }
}

/// Errors from checkpoint sealing.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CloseError {
    /// Sealing time must strictly increase within a lineage.
    #[error("non-monotonic seal time: last {last:?}, proposed {proposed:?}")]
    NonMonotonicSealTime {
        /// Sealing time of the lineage's last checkpoint.
        last: TimestampNs,
        /// Rejected sealing time.
        proposed: TimestampNs,
    },
    /// An initial checkpoint must not carry correction fields.
    #[error("initial checkpoint must not carry reason or materiality memo")]
    InitialWithCorrectionFields,
    /// A period has exactly one initial checkpoint; corrections supersede.
    #[error("period already has an initial checkpoint; corrections must supersede")]
    ParallelInitialCheckpoint,
    /// Supersession requires both a reason code and a materiality memo.
    #[error("supersession requires a reason code and a materiality memo evidence hash")]
    SupersessionWithoutJustification,
    /// Supersession target does not exist.
    #[error("supersede target missing: {0}")]
    SupersedeTargetMissing(CheckpointId),
    /// Supersession cannot cross tenants or periods.
    #[error("supersede target belongs to another lineage: {0}")]
    SupersedeAcrossLineage(CheckpointId),
    /// The target has already been superseded; corrections never race.
    #[error("target already superseded: {0}")]
    AlreadySuperseded(CheckpointId),
    /// Envelope canonicalization failure.
    #[error(transparent)]
    Canon(#[from] CanonError),
}
