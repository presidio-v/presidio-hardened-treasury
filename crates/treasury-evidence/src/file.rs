//! Durable evidence store: an append-only JSON-lines log behind the
//! in-memory implementation.
//!
//! Each line records `{"hash": <hex>, "blob": <hex>}`. On open, every
//! line is re-verified — the blob must hash to its recorded key — and a
//! store that fails verification **refuses to load** (fail closed). A
//! torn trailing line (crash mid-write) is a typed error; recovery is an
//! explicit, separate call that truncates the tail — never a silent
//! default.

use crate::sha256;
use crate::store::{EvidenceStore, InMemoryEvidenceStore, StoreError};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;
use treasury_core::ContentHash;

/// A file-backed evidence store. The file is the durability layer; the
/// in-memory store remains the serving layer.
#[derive(Debug)]
pub struct FileEvidenceStore {
    inner: InMemoryEvidenceStore,
    file: File,
}

impl FileEvidenceStore {
    /// Open (or create) a store at `path`, re-verifying every record.
    ///
    /// # Errors
    /// [`FileStoreError::Io`] on filesystem failures;
    /// [`FileStoreError::CorruptRecord`] when a line is malformed or its
    /// blob does not hash to its recorded key;
    /// [`FileStoreError::TornTail`] when the final line is incomplete —
    /// see [`FileEvidenceStore::open_with_recovery`].
    pub fn open(path: &Path) -> Result<Self, FileStoreError> {
        Self::open_internal(path, false)
    }

    /// Open, truncating a torn trailing line (crash mid-write). An
    /// explicit choice: the torn append never committed.
    ///
    /// # Errors
    /// As [`FileEvidenceStore::open`], minus the torn-tail case.
    pub fn open_with_recovery(path: &Path) -> Result<Self, FileStoreError> {
        Self::open_internal(path, true)
    }

    fn open_internal(path: &Path, recover: bool) -> Result<Self, FileStoreError> {
        let mut file = OpenOptions::new()
            .read(true)
            .create(true)
            .append(true)
            .open(path)
            .map_err(FileStoreError::io)?;

        let mut raw = String::new();
        let mut reader = BufReader::new(&file);
        reader
            .read_to_string(&mut raw)
            .map_err(FileStoreError::io)?;

        let complete_len = match raw.rfind('\n') {
            Some(last_newline) => last_newline.saturating_add(1),
            None => 0,
        };
        let torn = raw.len() > complete_len;
        if torn && !recover {
            return Err(FileStoreError::TornTail);
        }

        let mut inner = InMemoryEvidenceStore::new();
        let Some(complete) = raw.get(..complete_len) else {
            return Err(FileStoreError::corrupt(0, "tail split"));
        };
        for (index, line) in complete.lines().enumerate() {
            let record: serde_json::Value = serde_json::from_str(line)
                .map_err(|_| FileStoreError::corrupt(index, "not json"))?;
            let Some(hash_hex) = record.get("hash").and_then(|v| v.as_str()) else {
                return Err(FileStoreError::corrupt(index, "missing hash"));
            };
            let Some(blob_hex) = record.get("blob").and_then(|v| v.as_str()) else {
                return Err(FileStoreError::corrupt(index, "missing blob"));
            };
            let recorded = ContentHash::from_hex(hash_hex)
                .map_err(|_| FileStoreError::corrupt(index, "bad hash hex"))?;
            let blob = hex::decode(blob_hex)
                .map_err(|_| FileStoreError::corrupt(index, "bad blob hex"))?;
            if sha256(&blob) != recorded {
                return Err(FileStoreError::corrupt(index, "blob does not match hash"));
            }
            let stored = inner.put(&blob).map_err(FileStoreError::Store)?;
            if stored != recorded {
                return Err(FileStoreError::corrupt(index, "hash mismatch on replay"));
            }
        }

        if torn {
            let truncate_to = u64::try_from(complete_len)
                .map_err(|_| FileStoreError::corrupt(0, "length overflow"))?;
            file.set_len(truncate_to).map_err(FileStoreError::io)?;
            file.seek(SeekFrom::End(0)).map_err(FileStoreError::io)?;
        }
        Ok(Self { inner, file })
    }

    fn persist(&mut self, hash: ContentHash, bytes: &[u8]) -> Result<(), FileStoreError> {
        let line = format!(
            "{{\"hash\":\"{}\",\"blob\":\"{}\"}}\n",
            hash.to_hex(),
            hex::encode(bytes)
        );
        self.file
            .write_all(line.as_bytes())
            .map_err(FileStoreError::io)?;
        self.file.sync_all().map_err(FileStoreError::io)?;
        Ok(())
    }
}

impl EvidenceStore for FileEvidenceStore {
    fn put(&mut self, bytes: &[u8]) -> Result<ContentHash, StoreError> {
        let before = self.inner.len();
        let hash = self.inner.put(bytes)?;
        if self.inner.len() > before {
            // New entry: persist before reporting success.
            self.persist(hash, bytes)
                .map_err(|e| StoreError::Backend(e.to_string()))?;
        }
        Ok(hash)
    }

    fn get(&self, hash: &ContentHash) -> Result<&[u8], StoreError> {
        self.inner.get(hash)
    }

    fn verify(&self, hash: &ContentHash) -> Result<(), StoreError> {
        self.inner.verify(hash)
    }

    fn tree_head(&self) -> ContentHash {
        self.inner.tree_head()
    }

    fn tree_head_at(&self, entry_count: usize) -> Option<ContentHash> {
        self.inner.tree_head_at(entry_count)
    }

    fn len(&self) -> usize {
        self.inner.len()
    }
}

/// Errors from the file backend.
#[derive(Debug, thiserror::Error)]
pub enum FileStoreError {
    /// Filesystem failure (message preserved; `io::Error` is not `Eq`).
    #[error("io failure: {0}")]
    Io(String),
    /// A record is malformed or fails hash verification — fail closed.
    #[error("corrupt record at line {line}: {detail}")]
    CorruptRecord {
        /// Zero-based line index.
        line: usize,
        /// What failed.
        detail: String,
    },
    /// The final line is incomplete (crash mid-write). Use
    /// [`FileEvidenceStore::open_with_recovery`] to truncate it.
    #[error("torn trailing record; open_with_recovery truncates it")]
    TornTail,
    /// Inner store failure.
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl FileStoreError {
    // By-value is load-bearing: used as a `map_err(FileStoreError::io)`
    // callback, which is `FnOnce(io::Error) -> Self`; the variant holds a
    // `String`, so the error is converted via `to_string`, not moved.
    #[allow(clippy::needless_pass_by_value)]
    fn io(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }

    fn corrupt(line: usize, detail: &str) -> Self {
        Self::CorruptRecord {
            line,
            detail: detail.to_owned(),
        }
    }
}
