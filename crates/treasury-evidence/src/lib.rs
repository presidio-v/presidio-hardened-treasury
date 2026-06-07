//! Content-addressed evidence store (spec v2 §3.3).
//!
//! Three guarantees, in order of construction:
//! 1. **Canonical bytes** — [`canon`] produces a single deterministic byte
//!    encoding for any accepted JSON value, and *rejects floats outright*:
//!    a value that cannot hash identically everywhere is not evidence.
//! 2. **Content addressing** — [`store`] keys every blob by its SHA-256.
//! 3. **External anchoring** — [`merkle`] computes RFC 6962 tree heads over
//!    the store so the root can be committed to a public chain or an
//!    RFC 3161 TSA; tamper-evidence does not depend on trusting us.

#![forbid(unsafe_code)]

pub mod canon;
pub mod file;
pub mod merkle;
pub mod store;

pub use canon::{canonical_bytes, CanonError};
pub use file::{FileEvidenceStore, FileStoreError};
pub use merkle::merkle_root;
pub use store::{EvidenceStore, InMemoryEvidenceStore, StoreError};

use sha2::{Digest, Sha256};
use treasury_core::ContentHash;

/// SHA-256 of arbitrary bytes as a [`ContentHash`].
#[must_use]
pub fn sha256(bytes: &[u8]) -> ContentHash {
    let digest = Sha256::digest(bytes);
    ContentHash(digest.into())
}
