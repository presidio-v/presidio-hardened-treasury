//! Durable ledger: an append-only JSON-lines event log behind the
//! in-memory implementation, **replay-verified on open**.
//!
//! Loading does not trust the file. Every recorded event is re-appended
//! through the full validation path (layer/provenance pairing,
//! monotonicity, supersession discipline, hash chaining), and the
//! resulting event id must equal the recorded one. A log that fails any
//! of this — tampering, truncation in the middle, or hash-definition
//! drift — **refuses to load**. A torn trailing line is a typed error
//! with explicit recovery, never a silent truncation.

use crate::event::{EventDraft, EventId, SealedEvent};
use crate::ledger::{InMemoryLedger, Ledger, LedgerError};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;
use treasury_core::{TenantId, TimestampNs};

/// A file-backed ledger. The file is the durability layer; the
/// in-memory ledger remains the serving and validation layer.
#[derive(Debug)]
pub struct FileLedger {
    inner: InMemoryLedger,
    file: File,
}

impl FileLedger {
    /// Open (or create) a ledger at `path`, replaying every event
    /// through full validation.
    ///
    /// # Errors
    /// [`FileLedgerError::Io`] on filesystem failures;
    /// [`FileLedgerError::CorruptRecord`] when a line is malformed,
    /// fails validation on replay, or replays to a different event id;
    /// [`FileLedgerError::TornTail`] for an incomplete final line — see
    /// [`FileLedger::open_with_recovery`].
    pub fn open(path: &Path) -> Result<Self, FileLedgerError> {
        Self::open_internal(path, false)
    }

    /// Open, truncating a torn trailing line (crash mid-write). An
    /// explicit choice: the torn append never committed.
    ///
    /// # Errors
    /// As [`FileLedger::open`], minus the torn-tail case.
    pub fn open_with_recovery(path: &Path) -> Result<Self, FileLedgerError> {
        Self::open_internal(path, true)
    }

    fn open_internal(path: &Path, recover: bool) -> Result<Self, FileLedgerError> {
        let mut file = OpenOptions::new()
            .read(true)
            .create(true)
            .append(true)
            .open(path)
            .map_err(FileLedgerError::io)?;

        let mut raw = String::new();
        let mut reader = BufReader::new(&file);
        reader
            .read_to_string(&mut raw)
            .map_err(FileLedgerError::io)?;

        let complete_len = match raw.rfind('\n') {
            Some(last_newline) => last_newline.saturating_add(1),
            None => 0,
        };
        let torn = raw.len() > complete_len;
        if torn && !recover {
            return Err(FileLedgerError::TornTail);
        }

        let mut inner = InMemoryLedger::new();
        let Some(complete) = raw.get(..complete_len) else {
            return Err(FileLedgerError::corrupt(0, "tail split"));
        };
        for (index, line) in complete.lines().enumerate() {
            let recorded: SealedEvent = serde_json::from_str(line)
                .map_err(|_| FileLedgerError::corrupt(index, "not a sealed event"))?;
            let replayed = inner
                .append(recorded.draft.clone(), recorded.knowledge_time)
                .map_err(|_| FileLedgerError::corrupt(index, "fails validation on replay"))?;
            if replayed != recorded.event_id {
                return Err(FileLedgerError::corrupt(index, "replayed id differs"));
            }
        }

        if torn {
            let truncate_to = u64::try_from(complete_len)
                .map_err(|_| FileLedgerError::corrupt(0, "length overflow"))?;
            file.set_len(truncate_to).map_err(FileLedgerError::io)?;
            file.seek(SeekFrom::End(0)).map_err(FileLedgerError::io)?;
        }
        Ok(Self { inner, file })
    }

    fn persist(&mut self, tenant: &TenantId) -> Result<(), FileLedgerError> {
        let Some(sealed) = self.inner.stream(tenant).last() else {
            return Err(FileLedgerError::corrupt(0, "no event after append"));
        };
        let line = serde_json::to_string(sealed)
            .map_err(|_| FileLedgerError::corrupt(0, "serialize failed"))?;
        self.file
            .write_all(line.as_bytes())
            .map_err(FileLedgerError::io)?;
        self.file.write_all(b"\n").map_err(FileLedgerError::io)?;
        self.file.sync_all().map_err(FileLedgerError::io)?;
        Ok(())
    }
}

impl Ledger for FileLedger {
    fn append(
        &mut self,
        draft: EventDraft,
        knowledge_time: TimestampNs,
    ) -> Result<EventId, LedgerError> {
        let tenant = draft.tenant.clone();
        let id = self.inner.append(draft, knowledge_time)?;
        // Persist before reporting success.
        self.persist(&tenant)
            .map_err(|e| LedgerError::Backend(e.to_string()))?;
        Ok(id)
    }

    fn as_of(&self, tenant: &TenantId, knowledge_time: TimestampNs) -> Vec<&SealedEvent> {
        self.inner.as_of(tenant, knowledge_time)
    }

    fn verify_chain(&self, tenant: &TenantId) -> Result<(), LedgerError> {
        self.inner.verify_chain(tenant)
    }

    fn stream(&self, tenant: &TenantId) -> &[SealedEvent] {
        self.inner.stream(tenant)
    }
}

/// Errors from the file backend.
#[derive(Debug, thiserror::Error)]
pub enum FileLedgerError {
    /// Filesystem failure (message preserved; `io::Error` is not `Eq`).
    #[error("io failure: {0}")]
    Io(String),
    /// A record is malformed or fails replay verification — fail closed.
    #[error("corrupt record at line {line}: {detail}")]
    CorruptRecord {
        /// Zero-based line index.
        line: usize,
        /// What failed.
        detail: String,
    },
    /// The final line is incomplete (crash mid-write). Use
    /// [`FileLedger::open_with_recovery`] to truncate it.
    #[error("torn trailing record; open_with_recovery truncates it")]
    TornTail,
}

impl FileLedgerError {
    // By-value is load-bearing: used as a `map_err(FileLedgerError::io)`
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
