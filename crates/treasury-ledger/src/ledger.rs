//! Append-only ledger with per-tenant hash-chained streams and bitemporal
//! queries (spec v2 §3.1–§3.2).

use crate::event::{genesis_hash, ClaimLayer, EventDraft, EventId, SealedEvent};
use std::collections::{HashMap, HashSet};
use treasury_core::{TenantId, TimestampNs};
use treasury_evidence::CanonError;

/// Behavior every ledger backend must provide.
pub trait Ledger {
    /// Append a draft event at the given knowledge time.
    ///
    /// Validations (all structural, none waivable):
    /// - knowledge time strictly greater than the stream's last;
    /// - `draft.layer` matches `draft.provenance.layer()`;
    /// - `supersedes`, when present, references an existing event in the
    ///   same tenant stream and the same layer, not already superseded;
    /// - payload canonicalizes (floats reject).
    ///
    /// # Errors
    /// See [`LedgerError`].
    fn append(
        &mut self,
        draft: EventDraft,
        knowledge_time: TimestampNs,
    ) -> Result<EventId, LedgerError>;

    /// Events of a tenant that were *active* at `knowledge_time`: booked at
    /// or before it, and not superseded by an event booked at or before it.
    /// This is the "what did the books say as of the filing" query.
    fn as_of(&self, tenant: &TenantId, knowledge_time: TimestampNs) -> Vec<&SealedEvent>;

    /// Recompute every hash in a tenant's stream and verify chain linkage.
    ///
    /// # Errors
    /// [`LedgerError::ChainViolation`] at the first event whose recomputed
    /// hash or predecessor linkage does not match.
    fn verify_chain(&self, tenant: &TenantId) -> Result<(), LedgerError>;

    /// Full stream of a tenant in append order.
    fn stream(&self, tenant: &TenantId) -> &[SealedEvent];
}

/// In-memory reference implementation (Phase 0; durable backends implement
/// the same trait and the same validation set).
#[derive(Debug, Default)]
pub struct InMemoryLedger {
    streams: HashMap<TenantId, Vec<SealedEvent>>,
    by_id: HashMap<EventId, (TenantId, usize)>,
    superseded: HashSet<EventId>,
}

impl InMemoryLedger {
    /// Create an empty ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn find(&self, id: &EventId) -> Option<&SealedEvent> {
        let (tenant, idx) = self.by_id.get(id)?;
        self.streams.get(tenant)?.get(*idx)
    }
}

impl Ledger for InMemoryLedger {
    fn append(
        &mut self,
        draft: EventDraft,
        knowledge_time: TimestampNs,
    ) -> Result<EventId, LedgerError> {
        // Layer/provenance pairing is structural.
        if draft.layer != draft.provenance.layer() {
            return Err(LedgerError::LayerProvenanceMismatch {
                layer: draft.layer,
                provenance_layer: draft.provenance.layer(),
            });
        }

        // Knowledge time is strictly monotonic per tenant stream.
        // (No side effects before all validations pass: a rejected append
        // must leave the ledger byte-identical.)
        if let Some(last) = self.streams.get(&draft.tenant).and_then(|s| s.last()) {
            if knowledge_time <= last.knowledge_time {
                return Err(LedgerError::NonMonotonicKnowledgeTime {
                    last: last.knowledge_time,
                    proposed: knowledge_time,
                });
            }
        }

        // Supersession discipline: same tenant, same layer, exists, and the
        // target is not already superseded (no correction races).
        if let Some(target_id) = draft.supersedes {
            let target = self
                .by_id
                .get(&target_id)
                .ok_or(LedgerError::SupersedeTargetMissing(target_id))?;
            if target.0 != draft.tenant {
                return Err(LedgerError::SupersedeCrossTenant(target_id));
            }
            let target_event = self
                .streams
                .get(&target.0)
                .and_then(|s| s.get(target.1))
                .ok_or(LedgerError::SupersedeTargetMissing(target_id))?;
            if target_event.draft.layer != draft.layer {
                return Err(LedgerError::SupersedeCrossLayer {
                    target: target_event.draft.layer,
                    proposed: draft.layer,
                });
            }
            if self.superseded.contains(&target_id) {
                return Err(LedgerError::AlreadySuperseded(target_id));
            }
        }

        // Chain position and identity.
        let prev = match self.streams.get(&draft.tenant).and_then(|s| s.last()) {
            Some(last) => last.event_id,
            None => genesis_hash(&draft.tenant),
        };
        let event_id = SealedEvent::compute_id(&prev, knowledge_time, &draft)?;

        let supersedes = draft.supersedes;
        let tenant = draft.tenant.clone();
        let sealed = SealedEvent { event_id, prev, knowledge_time, draft };

        let stream = self.streams.entry(tenant.clone()).or_default();
        let idx = stream.len();
        stream.push(sealed);
        self.by_id.insert(event_id, (tenant, idx));
        if let Some(s) = supersedes {
            self.superseded.insert(s);
        }
        Ok(event_id)
    }

    fn as_of(&self, tenant: &TenantId, knowledge_time: TimestampNs) -> Vec<&SealedEvent> {
        let Some(stream) = self.streams.get(tenant) else {
            return Vec::new();
        };
        // An event is active at time T when booked ≤ T and no superseding
        // event was booked ≤ T.
        let superseded_by_t: HashSet<EventId> = stream
            .iter()
            .filter(|e| e.knowledge_time <= knowledge_time)
            .filter_map(|e| e.draft.supersedes)
            .collect();
        stream
            .iter()
            .filter(|e| e.knowledge_time <= knowledge_time)
            .filter(|e| !superseded_by_t.contains(&e.event_id))
            .collect()
    }

    fn verify_chain(&self, tenant: &TenantId) -> Result<(), LedgerError> {
        let Some(stream) = self.streams.get(tenant) else {
            return Ok(());
        };
        let mut expected_prev = genesis_hash(tenant);
        for event in stream {
            if event.prev != expected_prev {
                return Err(LedgerError::ChainViolation(event.event_id));
            }
            let recomputed =
                SealedEvent::compute_id(&event.prev, event.knowledge_time, &event.draft)?;
            if recomputed != event.event_id {
                return Err(LedgerError::ChainViolation(event.event_id));
            }
            expected_prev = event.event_id;
        }
        Ok(())
    }

    fn stream(&self, tenant: &TenantId) -> &[SealedEvent] {
        self.streams.get(tenant).map_or(&[], Vec::as_slice)
    }
}

/// Errors from ledger operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LedgerError {
    /// Knowledge time must strictly increase within a tenant stream.
    #[error("non-monotonic knowledge time: last {last:?}, proposed {proposed:?}")]
    NonMonotonicKnowledgeTime {
        /// Knowledge time of the stream's last event.
        last: TimestampNs,
        /// Rejected knowledge time.
        proposed: TimestampNs,
    },
    /// Claim layer and provenance variant must pair.
    #[error("layer {layer:?} does not match provenance layer {provenance_layer:?}")]
    LayerProvenanceMismatch {
        /// Declared layer.
        layer: ClaimLayer,
        /// Layer implied by provenance.
        provenance_layer: ClaimLayer,
    },
    /// Supersession target does not exist.
    #[error("supersede target missing: {0}")]
    SupersedeTargetMissing(EventId),
    /// Supersession across tenants is forbidden.
    #[error("supersede target belongs to another tenant: {0}")]
    SupersedeCrossTenant(EventId),
    /// Supersession across claim layers is forbidden.
    #[error("supersede across layers: target {target:?}, proposed {proposed:?}")]
    SupersedeCrossLayer {
        /// Layer of the target event.
        target: ClaimLayer,
        /// Layer of the proposed superseding event.
        proposed: ClaimLayer,
    },
    /// The target has already been superseded; corrections never race.
    #[error("target already superseded: {0}")]
    AlreadySuperseded(EventId),
    /// Recomputed hash or predecessor linkage mismatch — tampering signal.
    #[error("hash chain violation at {0}")]
    ChainViolation(EventId),
    /// Payload failed canonicalization (floats, depth).
    #[error(transparent)]
    Canon(#[from] CanonError),
}
