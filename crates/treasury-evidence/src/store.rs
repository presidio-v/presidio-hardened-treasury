//! Content-addressed evidence store (spec v2 §3.3).
//!
//! Append-only by construction: there is no delete and no overwrite — a
//! `put` of bytes that already exist is a no-op returning the same hash
//! (content addressing makes idempotency free). Insertion order is
//! retained because the RFC 6962 tree head commits to it.

use crate::{merkle::merkle_root, sha256};
use std::collections::HashMap;
use treasury_core::ContentHash;

/// Behavior every evidence store backend must provide.
pub trait EvidenceStore {
    /// Store raw evidence bytes; returns their content hash.
    ///
    /// Idempotent: storing identical bytes twice returns the same hash and
    /// records one entry.
    ///
    /// # Errors
    /// Backend-specific failures (I/O, capacity).
    fn put(&mut self, bytes: &[u8]) -> Result<ContentHash, StoreError>;

    /// Retrieve evidence bytes by content hash.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when the hash is unknown.
    fn get(&self, hash: &ContentHash) -> Result<&[u8], StoreError>;

    /// Verify that stored bytes still hash to their key.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] for unknown hashes;
    /// [`StoreError::IntegrityViolation`] when stored bytes no longer match.
    fn verify(&self, hash: &ContentHash) -> Result<(), StoreError>;

    /// Current RFC 6962 tree head over all entries in insertion order —
    /// the value to anchor externally.
    fn tree_head(&self) -> ContentHash;

    /// RFC 6962 tree head over the first `entry_count` entries in
    /// insertion order, or `None` when `entry_count` exceeds the store.
    /// Anchor verification recomputes anchored prefixes through this.
    fn tree_head_at(&self, entry_count: usize) -> Option<ContentHash>;

    /// Number of entries.
    fn len(&self) -> usize;

    /// Whether the store is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// In-memory reference implementation (Phase 0; durable backends implement
/// the same trait).
#[derive(Debug, Default)]
pub struct InMemoryEvidenceStore {
    blobs: HashMap<ContentHash, Vec<u8>>,
    order: Vec<ContentHash>,
}

impl InMemoryEvidenceStore {
    /// Create an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl EvidenceStore for InMemoryEvidenceStore {
    fn put(&mut self, bytes: &[u8]) -> Result<ContentHash, StoreError> {
        let hash = sha256(bytes);
        if let std::collections::hash_map::Entry::Vacant(e) = self.blobs.entry(hash) {
            e.insert(bytes.to_vec());
            self.order.push(hash);
        }
        Ok(hash)
    }

    fn get(&self, hash: &ContentHash) -> Result<&[u8], StoreError> {
        self.blobs
            .get(hash)
            .map(Vec::as_slice)
            .ok_or(StoreError::NotFound(*hash))
    }

    fn verify(&self, hash: &ContentHash) -> Result<(), StoreError> {
        let bytes = self.get(hash)?;
        if &sha256(bytes) == hash {
            Ok(())
        } else {
            Err(StoreError::IntegrityViolation(*hash))
        }
    }

    fn tree_head(&self) -> ContentHash {
        merkle_root(&self.order)
    }

    fn tree_head_at(&self, entry_count: usize) -> Option<ContentHash> {
        let prefix = self.order.get(..entry_count)?;
        Some(merkle_root(prefix))
    }

    fn len(&self) -> usize {
        self.order.len()
    }
}

/// Errors from evidence store operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum StoreError {
    /// No entry under this content hash.
    #[error("evidence not found: {0}")]
    NotFound(ContentHash),
    /// Stored bytes no longer match their content hash.
    #[error("integrity violation for {0}")]
    IntegrityViolation(ContentHash),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_get_round_trip() {
        let mut store = InMemoryEvidenceStore::new();
        let h = store.put(b"payload").unwrap_or(ContentHash([0; 32]));
        assert_eq!(store.get(&h), Ok(b"payload".as_slice()));
        assert_eq!(store.verify(&h), Ok(()));
    }

    #[test]
    fn put_is_idempotent() {
        let mut store = InMemoryEvidenceStore::new();
        let h1 = store.put(b"x").unwrap_or(ContentHash([0; 32]));
        let head = store.tree_head();
        let h2 = store.put(b"x").unwrap_or(ContentHash([1; 32]));
        assert_eq!(h1, h2);
        assert_eq!(store.len(), 1);
        assert_eq!(
            store.tree_head(),
            head,
            "idempotent put must not move the tree head"
        );
    }

    #[test]
    fn tree_head_changes_with_new_evidence() {
        let mut store = InMemoryEvidenceStore::new();
        let empty_head = store.tree_head();
        let _ = store.put(b"a");
        let one = store.tree_head();
        let _ = store.put(b"b");
        let two = store.tree_head();
        assert_ne!(empty_head, one);
        assert_ne!(one, two);
    }

    #[test]
    fn unknown_hash_not_found() {
        let store = InMemoryEvidenceStore::new();
        let missing = ContentHash([7; 32]);
        assert_eq!(store.get(&missing), Err(StoreError::NotFound(missing)));
    }
}
