//! Checkpoint node model (spec v2 §3.6).

use serde::{Deserialize, Serialize};
use serde_json::json;
use treasury_core::{ContentHash, TenantId, TimestampNs};
use treasury_evidence::{canonical_bytes, sha256, CanonError};

/// Schema tag committed into every checkpoint hash; bump on change.
pub const CHECKPOINT_SCHEMA: &str = "treasury-close/checkpoint/v1";

/// Identity of a sealed checkpoint: SHA-256 of its canonical envelope.
pub type CheckpointId = ContentHash;

/// A reporting period, e.g. `"2026Q2"`. Free-form at this layer; the
/// close pipeline validates period grammar per tenant calendar.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PeriodId(String);

impl PeriodId {
    /// Construct from any string-like value.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The period as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for PeriodId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Why a checkpoint superseded its predecessor (spec v2 §3.6 reason codes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupersessionReason {
    /// Late-arriving venue or chain data.
    LateData,
    /// A data provider silently revised previously served history.
    ProviderRevision,
    /// Chain reorganization deeper than the finality policy assumed.
    ChainReorg,
    /// A policy artifact was corrected and the period re-run under it.
    PolicyCorrection,
    /// Classification judgment revised (e.g. transfer reclassified).
    JudgmentRevision,
}

/// A checkpoint proposed for sealing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointDraft {
    /// Tenant whose period this checkpoint closes.
    pub tenant: TenantId,
    /// Reporting period.
    pub period: PeriodId,
    /// Knowledge time the period state was folded at (`as_of` input).
    pub as_of: TimestampNs,
    /// Reproducible commitment to the folded period state (see `fold`).
    pub state_root: ContentHash,
    /// Predecessor checkpoint being superseded, if this is a correction.
    pub supersedes: Option<CheckpointId>,
    /// Mandatory with `supersedes`: why the predecessor was wrong.
    pub reason: Option<SupersessionReason>,
    /// Mandatory with `supersedes`: evidence-store hash of the
    /// materiality assessment (SAB 99 memo).
    pub materiality_memo: Option<ContentHash>,
}

/// A checkpoint sealed into the DAG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedCheckpoint {
    /// Content-derived identity.
    pub checkpoint_id: CheckpointId,
    /// Knowledge time the checkpoint was sealed; strictly monotonic per
    /// (tenant, period) lineage.
    pub sealed_at: TimestampNs,
    /// The proposed content.
    pub draft: CheckpointDraft,
}

impl SealedCheckpoint {
    /// Compute a draft's identity at a given sealing time.
    ///
    /// # Errors
    /// [`CanonError`] is structurally unreachable for this envelope but
    /// propagated rather than swallowed.
    pub fn compute_id(
        sealed_at: TimestampNs,
        draft: &CheckpointDraft,
    ) -> Result<CheckpointId, CanonError> {
        let envelope = json!({
            "schema": CHECKPOINT_SCHEMA,
            "tenant": draft.tenant.clone(),
            "period": draft.period.clone(),
            "as_of": draft.as_of,
            "sealed_at": sealed_at,
            "state_root": draft.state_root.to_hex(),
            "supersedes": draft.supersedes.map(|s| s.to_hex()),
            "reason": draft.reason,
            "materiality_memo": draft.materiality_memo.map(|m| m.to_hex()),
        });
        let bytes = canonical_bytes(&envelope)?;
        Ok(sha256(&bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> CheckpointDraft {
        CheckpointDraft {
            tenant: TenantId::new("acme"),
            period: PeriodId::new("2026Q2"),
            as_of: TimestampNs::from_nanos(100),
            state_root: ContentHash([5; 32]),
            supersedes: None,
            reason: None,
            materiality_memo: None,
        }
    }

    #[test]
    fn id_commits_to_state_root() {
        let a = draft();
        let mut b = draft();
        b.state_root = ContentHash([6; 32]);
        let t = TimestampNs::from_nanos(200);
        assert_ne!(
            SealedCheckpoint::compute_id(t, &a),
            SealedCheckpoint::compute_id(t, &b)
        );
    }

    #[test]
    fn id_commits_to_supersession_edge() {
        let a = draft();
        let mut b = draft();
        b.supersedes = Some(ContentHash([7; 32]));
        b.reason = Some(SupersessionReason::LateData);
        b.materiality_memo = Some(ContentHash([8; 32]));
        let t = TimestampNs::from_nanos(200);
        assert_ne!(
            SealedCheckpoint::compute_id(t, &a),
            SealedCheckpoint::compute_id(t, &b)
        );
    }
}
