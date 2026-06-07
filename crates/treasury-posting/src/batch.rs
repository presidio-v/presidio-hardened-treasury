//! Posting batches: content-addressed, and the address is the
//! idempotency key.

use serde_json::json;
use treasury_core::{ContentHash, TenantId};
use treasury_evidence::{canonical_bytes, sha256, CanonError};
use treasury_gaap::JournalEntry;

/// Schema tag committed into every batch hash; bump on change.
pub const BATCH_SCHEMA: &str = "treasury-posting/batch/v1";

/// One release of journal entries toward a target GL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostingBatch {
    /// Tenant whose GL this targets.
    pub tenant: TenantId,
    /// Target GL identifier (e.g. `"netsuite:prod"`). Part of the
    /// idempotency key: the same entries posted to two GLs are two
    /// batches.
    pub target_gl: String,
    /// Period tag (e.g. `"2026Q2"`), part of the key.
    pub period: String,
    /// The entries, in posting order.
    pub entries: Vec<JournalEntry>,
}

impl PostingBatch {
    /// The batch's content hash — **the idempotency key**. A retry of
    /// the same batch carries the same key by construction.
    ///
    /// # Errors
    /// Propagates entry hashing / canonicalization failures.
    pub fn batch_id(&self) -> Result<ContentHash, CanonError> {
        let mut entry_hashes: Vec<String> = Vec::new();
        for entry in &self.entries {
            let hash = match entry.entry_hash() {
                Ok(h) => h,
                Err(treasury_gaap::EntryError::Canon(e)) => return Err(e),
                // Entry invariants were enforced at construction; any
                // other error here is unreachable, but never swallowed.
                Err(_) => return Err(CanonError::Encode("entry hash failed".to_owned())),
            };
            entry_hashes.push(hash.to_hex());
        }
        let envelope = json!({
            "schema": BATCH_SCHEMA,
            "tenant": self.tenant.clone(),
            "target_gl": self.target_gl.clone(),
            "period": self.period.clone(),
            "entries": entry_hashes,
        });
        let bytes = canonical_bytes(&envelope)?;
        Ok(sha256(&bytes))
    }

    /// Sorted entry hashes — the verification fingerprint.
    ///
    /// # Errors
    /// Same as [`PostingBatch::batch_id`].
    pub fn entry_fingerprint(&self) -> Result<Vec<ContentHash>, CanonError> {
        let mut hashes: Vec<ContentHash> = Vec::new();
        for entry in &self.entries {
            let hash = match entry.entry_hash() {
                Ok(h) => h,
                Err(treasury_gaap::EntryError::Canon(e)) => return Err(e),
                Err(_) => return Err(CanonError::Encode("entry hash failed".to_owned())),
            };
            hashes.push(hash);
        }
        hashes.sort_unstable();
        hashes.dedup();
        Ok(hashes)
    }
}
