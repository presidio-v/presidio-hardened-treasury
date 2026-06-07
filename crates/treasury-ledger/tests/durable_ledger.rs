//! Durable backend invariants: reopen reproduces state byte-for-byte,
//! tampering refuses to load, torn tails recover only explicitly.

use serde_json::json;
use std::fs;
use std::path::PathBuf;
use treasury_core::{ContentHash, SourceId, TenantId, TimestampNs};
use treasury_ledger::{ClaimLayer, EventDraft, FileLedger, FileLedgerError, Ledger, Provenance};

fn tenant() -> TenantId {
    TenantId::new("acme")
}

fn ts(n: i64) -> TimestampNs {
    TimestampNs::from_nanos(n)
}

fn observation(n: u64) -> EventDraft {
    EventDraft {
        tenant: tenant(),
        layer: ClaimLayer::Observation,
        event_time: ts(1),
        supersedes: None,
        provenance: Provenance::Observation {
            source: SourceId::new("venue"),
            evidence: ContentHash([2; 32]),
        },
        payload: json!({ "n": n.to_string() }),
    }
}

/// Unique temp path per test; removed at the start in case of leftovers.
fn temp_log(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("treasury-test-{}-{}.log", name, std::process::id()));
    let _ = fs::remove_file(&path);
    path
}

#[test]
fn reopen_reproduces_state_and_chain() {
    let path = temp_log("reopen");
    let (first_id, head_count) = {
        let Ok(mut ledger) = FileLedger::open(&path) else {
            unreachable!("open must succeed");
        };
        let Ok(first) = ledger.append(observation(1), ts(10)) else {
            unreachable!("append must succeed");
        };
        let Ok(_) = ledger.append(observation(2), ts(20)) else {
            unreachable!("append must succeed");
        };
        (first, ledger.stream(&tenant()).len())
    };

    let Ok(reopened) = FileLedger::open(&path) else {
        unreachable!("reopen must succeed");
    };
    assert_eq!(reopened.stream(&tenant()).len(), head_count);
    assert_eq!(
        reopened.stream(&tenant()).first().map(|e| e.event_id),
        Some(first_id)
    );
    assert_eq!(reopened.verify_chain(&tenant()), Ok(()));
    let _ = fs::remove_file(&path);
}

#[test]
fn tampered_log_refuses_to_load() {
    let path = temp_log("tamper");
    {
        let Ok(mut ledger) = FileLedger::open(&path) else {
            unreachable!("open must succeed");
        };
        let Ok(_) = ledger.append(observation(1), ts(10)) else {
            unreachable!("append must succeed");
        };
    }
    // Flip the payload inside the recorded line: replay must detect it.
    let Ok(contents) = fs::read_to_string(&path) else {
        unreachable!("file exists");
    };
    let tampered = contents.replace("\"n\":\"1\"", "\"n\":\"9\"");
    assert_ne!(contents, tampered, "tamper target must exist");
    let Ok(()) = fs::write(&path, tampered) else {
        unreachable!("write must succeed");
    };

    assert!(matches!(
        FileLedger::open(&path),
        Err(FileLedgerError::CorruptRecord { .. })
    ));
    let _ = fs::remove_file(&path);
}

#[test]
fn torn_tail_is_typed_and_recovery_is_explicit() {
    let path = temp_log("torn");
    {
        let Ok(mut ledger) = FileLedger::open(&path) else {
            unreachable!("open must succeed");
        };
        let Ok(_) = ledger.append(observation(1), ts(10)) else {
            unreachable!("append must succeed");
        };
    }
    // Simulate a crash mid-write: append half a record, no newline.
    let Ok(contents) = fs::read_to_string(&path) else {
        unreachable!("file exists");
    };
    let Ok(()) = fs::write(&path, format!("{contents}{{\"event_id\":\"dead")) else {
        unreachable!("write must succeed");
    };

    // Plain open fails closed.
    assert!(matches!(
        FileLedger::open(&path),
        Err(FileLedgerError::TornTail)
    ));

    // Explicit recovery truncates the tail; the committed event survives
    // and the log is appendable again.
    let Ok(mut recovered) = FileLedger::open_with_recovery(&path) else {
        unreachable!("recovery must succeed");
    };
    assert_eq!(recovered.stream(&tenant()).len(), 1);
    let Ok(_) = recovered.append(observation(2), ts(20)) else {
        unreachable!("append after recovery must succeed");
    };
    assert_eq!(recovered.verify_chain(&tenant()), Ok(()));
    let _ = fs::remove_file(&path);
}
