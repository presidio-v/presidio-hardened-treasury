//! A fixture GL implementing [`GlAdapter`] with controllable faults, so
//! the orchestration and the posting protocol can be exercised end to
//! end without a live vendor. A real NetSuite/QuickBooks/SAP adapter is
//! the same trait over HTTP.

use crate::adapter::{GlAdapter, GlError, GlReadback, SubmitOutcome};
use std::collections::HashMap;
use treasury_core::ContentHash;
use treasury_posting::PostingBatch;

/// A fault to inject for the next `submit`, to test the recovery paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FixtureFault {
    /// The submit lands in the GL but the acknowledgment is lost.
    AckLostButPosted,
    /// The submit never reaches the GL and the ack is lost.
    AckLostNotPosted,
    /// The post lands, but the GL later shows one fewer entry than we
    /// sent (a dropped line) — read-back must catch it.
    DropOneEntryOnReadback,
    /// The post lands, but the GL also shows a stranger entry not in our
    /// batch — read-back must catch it.
    ExtraEntryOnReadback(ContentHash),
    /// The adapter raises a hard transport error.
    Transport,
}

/// Per-batch record of what the fixture GL "stored".
#[derive(Debug, Clone)]
struct Posted {
    gl_ref: String,
    entry_hashes: Vec<ContentHash>,
    readback_drop_one: bool,
    readback_extra: Option<ContentHash>,
}

/// An in-memory GL. Idempotent by construction: posting the same key
/// twice returns the same `gl_ref` and stores one record.
#[derive(Debug, Default)]
pub struct FixtureGl {
    posted: HashMap<ContentHash, Posted>,
    next_fault: Option<FixtureFault>,
    seq: u64,
}

impl FixtureGl {
    /// Create an empty fixture GL.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inject a fault for the next `submit` only.
    pub fn inject(&mut self, fault: FixtureFault) {
        self.next_fault = Some(fault);
    }

    fn fingerprint(batch: &PostingBatch) -> Result<Vec<ContentHash>, GlError> {
        batch
            .entry_fingerprint()
            .map_err(|e| GlError::Rejected(e.to_string()))
    }

    fn record(&mut self, key: ContentHash, batch: &PostingBatch) -> Result<String, GlError> {
        if let Some(existing) = self.posted.get(&key) {
            // Idempotent: same key → same reference, no duplicate.
            return Ok(existing.gl_ref.clone());
        }
        let fingerprint = Self::fingerprint(batch)?;
        self.seq = self.seq.saturating_add(1);
        let gl_ref = format!("JE-{}", self.seq);
        let (drop_one, extra) = match self.next_fault {
            Some(FixtureFault::DropOneEntryOnReadback) => (true, None),
            Some(FixtureFault::ExtraEntryOnReadback(h)) => (false, Some(h)),
            _ => (false, None),
        };
        self.posted.insert(
            key,
            Posted {
                gl_ref: gl_ref.clone(),
                entry_hashes: fingerprint,
                readback_drop_one: drop_one,
                readback_extra: extra,
            },
        );
        Ok(gl_ref)
    }
}

impl GlAdapter for FixtureGl {
    fn submit(
        &mut self,
        batch: &PostingBatch,
        idempotency_key: ContentHash,
    ) -> Result<SubmitOutcome, GlError> {
        let fault = self.next_fault.take();
        match fault {
            Some(FixtureFault::Transport) => {
                Err(GlError::Transport("injected".to_owned()))
            }
            Some(FixtureFault::AckLostNotPosted) => {
                // Nothing stored; read-back will show absent.
                Ok(SubmitOutcome::AckLost)
            }
            Some(FixtureFault::AckLostButPosted) => {
                // Store as posted, but report the ack as lost.
                self.next_fault = None;
                let _ = self.record(idempotency_key, batch)?;
                Ok(SubmitOutcome::AckLost)
            }
            other => {
                // Re-arm a read-back-shaping fault so `record` sees it.
                self.next_fault = other;
                let gl_ref = self.record(idempotency_key, batch)?;
                self.next_fault = None;
                Ok(SubmitOutcome::Acknowledged {
                    gl_ref,
                    raw_response: b"fixture-ack".to_vec(),
                })
            }
        }
    }

    fn read_back(&self, idempotency_key: ContentHash) -> Result<GlReadback, GlError> {
        match self.posted.get(&idempotency_key) {
            None => Ok(GlReadback {
                gl_ref: None,
                entry_hashes: Vec::new(),
                raw_payload: b"fixture-readback-absent".to_vec(),
            }),
            Some(posted) => {
                let mut hashes = posted.entry_hashes.clone();
                if posted.readback_drop_one && !hashes.is_empty() {
                    hashes.pop();
                }
                if let Some(extra) = posted.readback_extra {
                    hashes.push(extra);
                }
                Ok(GlReadback {
                    gl_ref: Some(posted.gl_ref.clone()),
                    entry_hashes: hashes,
                    raw_payload: b"fixture-readback".to_vec(),
                })
            }
        }
    }
}
