//! Per-tenant policy activation timelines (spec v2 §3.5).
//!
//! The registry answers the bitemporal question "which policy governed at
//! knowledge time T" — required to replay any valuation byte-for-byte under
//! the policy that was actually in force when it was performed.

use crate::artifact::{PolicyArtifact, PolicyError, PolicyKind};
use std::collections::HashMap;
use treasury_core::{ContentHash, TenantId, TimestampNs};

/// One activation: at `activated_at` (knowledge time), `policy_hash`
/// became the governing policy of `kind` for `tenant`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationRecord {
    /// Tenant the activation applies to.
    pub tenant: TenantId,
    /// Policy kind being (re)activated.
    pub kind: PolicyKind,
    /// Content hash of the registered artifact taking effect.
    pub policy_hash: ContentHash,
    /// Knowledge time of activation; strictly monotonic per (tenant, kind).
    pub activated_at: TimestampNs,
}

/// Content-addressed artifact storage plus append-only activation
/// timelines per `(tenant, kind)`.
#[derive(Debug, Default)]
pub struct PolicyRegistry {
    artifacts: HashMap<ContentHash, PolicyArtifact>,
    timelines: HashMap<(TenantId, PolicyKind), Vec<ActivationRecord>>,
}

impl PolicyRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an artifact; returns its content hash. Idempotent for
    /// identical content (content addressing makes re-registration a no-op).
    ///
    /// # Errors
    /// [`RegistryError::Policy`] when the artifact cannot hash (no
    /// approvers, floats in body).
    pub fn register(&mut self, artifact: PolicyArtifact) -> Result<ContentHash, RegistryError> {
        let hash = artifact.policy_hash()?;
        self.artifacts.entry(hash).or_insert(artifact);
        Ok(hash)
    }

    /// Activate a registered artifact for `(tenant, kind)` at a knowledge
    /// time strictly after the previous activation.
    ///
    /// # Errors
    /// [`RegistryError::UnknownPolicy`] when the hash was never registered;
    /// [`RegistryError::KindMismatch`] when the artifact's kind differs from
    /// the activation kind;
    /// [`RegistryError::NonMonotonicActivation`] when the activation time
    /// does not advance the timeline.
    pub fn activate(
        &mut self,
        tenant: TenantId,
        kind: PolicyKind,
        policy_hash: ContentHash,
        activated_at: TimestampNs,
    ) -> Result<(), RegistryError> {
        let artifact = self
            .artifacts
            .get(&policy_hash)
            .ok_or(RegistryError::UnknownPolicy(policy_hash))?;
        if artifact.kind != kind {
            return Err(RegistryError::KindMismatch {
                artifact_kind: artifact.kind.clone(),
                requested: kind,
            });
        }
        let key = (tenant.clone(), kind.clone());
        let timeline = self.timelines.entry(key).or_default();
        if let Some(last) = timeline.last() {
            if activated_at <= last.activated_at {
                return Err(RegistryError::NonMonotonicActivation {
                    last: last.activated_at,
                    proposed: activated_at,
                });
            }
        }
        timeline.push(ActivationRecord {
            tenant,
            kind,
            policy_hash,
            activated_at,
        });
        Ok(())
    }

    /// The policy hash governing `(tenant, kind)` at knowledge time `at`,
    /// or `None` when no activation had yet occurred.
    #[must_use]
    pub fn active_at(
        &self,
        tenant: &TenantId,
        kind: &PolicyKind,
        at: TimestampNs,
    ) -> Option<ContentHash> {
        let timeline = self.timelines.get(&(tenant.clone(), kind.clone()))?;
        let record = timeline.iter().rev().find(|r| r.activated_at <= at);
        record.map(|r| r.policy_hash)
    }

    /// Retrieve a registered artifact by content hash.
    #[must_use]
    pub fn artifact(&self, hash: &ContentHash) -> Option<&PolicyArtifact> {
        self.artifacts.get(hash)
    }

    /// Full activation timeline for `(tenant, kind)` in activation order.
    #[must_use]
    pub fn timeline(&self, tenant: &TenantId, kind: &PolicyKind) -> &[ActivationRecord] {
        let timeline = self.timelines.get(&(tenant.clone(), kind.clone()));
        timeline.map_or(&[], Vec::as_slice)
    }
}

/// Errors from registry operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RegistryError {
    /// Activation referenced a hash that was never registered.
    #[error("unknown policy: {0}")]
    UnknownPolicy(ContentHash),
    /// Artifact kind does not match the activation kind.
    #[error("kind mismatch: artifact is {artifact_kind}, requested {requested}")]
    KindMismatch {
        /// Kind recorded in the artifact.
        artifact_kind: PolicyKind,
        /// Kind requested at activation.
        requested: PolicyKind,
    },
    /// Activation timelines are append-only with strictly increasing time.
    #[error("non-monotonic activation: last {last:?}, proposed {proposed:?}")]
    NonMonotonicActivation {
        /// Time of the timeline's last activation.
        last: TimestampNs,
        /// Rejected activation time.
        proposed: TimestampNs,
    },
    /// Artifact-level failure.
    #[error(transparent)]
    Policy(#[from] PolicyError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use treasury_core::ActorId;

    fn artifact(label: &str) -> PolicyArtifact {
        PolicyArtifact {
            kind: PolicyKind::new("principal-market/v1"),
            body: json!({ "label": label }),
            approvers: vec![ActorId::new("cfo"), ActorId::new("controller")],
            effective_from: TimestampNs::from_nanos(0),
        }
    }

    fn ts(n: i64) -> TimestampNs {
        TimestampNs::from_nanos(n)
    }

    fn tenant() -> TenantId {
        TenantId::new("acme")
    }

    fn kind() -> PolicyKind {
        PolicyKind::new("principal-market/v1")
    }

    #[test]
    fn activation_timeline_answers_bitemporal_query() {
        let mut reg = PolicyRegistry::new();
        let h1 = reg.register(artifact("v1")).unwrap_or(ContentHash([0; 32]));
        let h2 = reg.register(artifact("v2")).unwrap_or(ContentHash([0; 32]));
        let _ = reg.activate(tenant(), kind(), h1, ts(10));
        let _ = reg.activate(tenant(), kind(), h2, ts(20));

        assert_eq!(reg.active_at(&tenant(), &kind(), ts(5)), None);
        assert_eq!(reg.active_at(&tenant(), &kind(), ts(15)), Some(h1));
        assert_eq!(reg.active_at(&tenant(), &kind(), ts(25)), Some(h2));
    }

    #[test]
    fn activation_requires_registration() {
        let mut reg = PolicyRegistry::new();
        let missing = ContentHash([9; 32]);
        assert_eq!(
            reg.activate(tenant(), kind(), missing, ts(10)),
            Err(RegistryError::UnknownPolicy(missing))
        );
    }

    #[test]
    fn kind_mismatch_rejected() {
        let mut reg = PolicyRegistry::new();
        let h = reg.register(artifact("v1")).unwrap_or(ContentHash([0; 32]));
        let result = reg.activate(tenant(), PolicyKind::new("finality/v1"), h, ts(10));
        assert!(matches!(result, Err(RegistryError::KindMismatch { .. })));
    }

    #[test]
    fn activation_is_monotonic_per_tenant_and_kind() {
        let mut reg = PolicyRegistry::new();
        let h = reg.register(artifact("v1")).unwrap_or(ContentHash([0; 32]));
        let _ = reg.activate(tenant(), kind(), h, ts(10));
        assert!(matches!(
            reg.activate(tenant(), kind(), h, ts(10)),
            Err(RegistryError::NonMonotonicActivation { .. })
        ));
        // A different tenant's timeline is independent.
        assert_eq!(
            reg.activate(TenantId::new("other"), kind(), h, ts(10)),
            Ok(())
        );
    }

    #[test]
    fn register_is_idempotent() {
        let mut reg = PolicyRegistry::new();
        let h1 = reg.register(artifact("v1")).unwrap_or(ContentHash([0; 32]));
        let h2 = reg.register(artifact("v1")).unwrap_or(ContentHash([1; 32]));
        assert_eq!(h1, h2);
    }
}
