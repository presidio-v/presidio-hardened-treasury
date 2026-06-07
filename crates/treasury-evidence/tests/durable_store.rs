//! Durable evidence-store invariants: reopen preserves the tree head,
//! corruption refuses to load, torn tails recover only explicitly.

use std::fs;
use std::path::PathBuf;
use treasury_evidence::{EvidenceStore, FileEvidenceStore, FileStoreError};

fn temp_log(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("treasury-evd-{}-{}.log", name, std::process::id()));
    let _ = fs::remove_file(&path);
    path
}

#[test]
fn reopen_preserves_entries_and_tree_head() {
    let path = temp_log("reopen");
    let head = {
        let Ok(mut store) = FileEvidenceStore::open(&path) else {
            unreachable!("open must succeed");
        };
        let Ok(_) = store.put(b"payload-a") else {
            unreachable!("put must succeed");
        };
        let Ok(hash_b) = store.put(b"payload-b") else {
            unreachable!("put must succeed");
        };
        // Idempotent re-put persists nothing new.
        let Ok(hash_b2) = store.put(b"payload-b") else {
            unreachable!("put must succeed");
        };
        assert_eq!(hash_b, hash_b2);
        store.tree_head()
    };

    let Ok(reopened) = FileEvidenceStore::open(&path) else {
        unreachable!("reopen must succeed");
    };
    assert_eq!(reopened.len(), 2);
    assert_eq!(
        reopened.tree_head(),
        head,
        "anchored tree heads must survive restart"
    );
    let _ = fs::remove_file(&path);
}

#[test]
fn corrupted_blob_refuses_to_load() {
    let path = temp_log("corrupt");
    {
        let Ok(mut store) = FileEvidenceStore::open(&path) else {
            unreachable!("open must succeed");
        };
        let Ok(_) = store.put(b"payload-a") else {
            unreachable!("put must succeed");
        };
    }
    let Ok(contents) = fs::read_to_string(&path) else {
        unreachable!("file exists");
    };
    // Flip one hex nibble of the blob; the recorded hash now mismatches.
    let tampered = contents.replacen("\"blob\":\"7", "\"blob\":\"8", 1);
    assert_ne!(contents, tampered, "tamper target must exist");
    let Ok(()) = fs::write(&path, tampered) else {
        unreachable!("write must succeed");
    };
    assert!(matches!(
        FileEvidenceStore::open(&path),
        Err(FileStoreError::CorruptRecord { .. })
    ));
    let _ = fs::remove_file(&path);
}

#[test]
fn torn_tail_recovery_is_explicit() {
    let path = temp_log("torn");
    {
        let Ok(mut store) = FileEvidenceStore::open(&path) else {
            unreachable!("open must succeed");
        };
        let Ok(_) = store.put(b"payload-a") else {
            unreachable!("put must succeed");
        };
    }
    let Ok(contents) = fs::read_to_string(&path) else {
        unreachable!("file exists");
    };
    let Ok(()) = fs::write(&path, format!("{contents}{{\"hash\":\"de")) else {
        unreachable!("write must succeed");
    };

    assert!(matches!(
        FileEvidenceStore::open(&path),
        Err(FileStoreError::TornTail)
    ));
    let Ok(mut recovered) = FileEvidenceStore::open_with_recovery(&path) else {
        unreachable!("recovery must succeed");
    };
    assert_eq!(recovered.len(), 1);
    let Ok(_) = recovered.put(b"payload-b") else {
        unreachable!("put after recovery must succeed");
    };
    let _ = fs::remove_file(&path);
}
