//! Posting protocol (REQ-25 / R9): dual-control release, retry safety
//! through the Unknown state, read-back verification both directions.

use treasury_core::{ActorId, AssetAmount, AssetId, ContentHash, DualControlError, TenantId};
use treasury_gaap::{JournalEntry, JournalLine, Side, StatementLine};
use treasury_posting::{PostingBatch, PostingError, PostingProtocol, PostingState};

fn usd(minor: i128) -> AssetAmount {
    AssetAmount::new(AssetId::new("USD"), minor)
}

fn entry(minor: i128) -> JournalEntry {
    let Ok(entry) = JournalEntry::new(
        "asu2023_08_remeasurement",
        vec![
            JournalLine {
                side: Side::Debit,
                line: StatementLine::CryptoAssets,
                amount: usd(minor),
            },
            JournalLine {
                side: Side::Credit,
                line: StatementLine::UnrealizedCryptoGainLoss,
                amount: usd(minor),
            },
        ],
        ContentHash([9; 32]),
    ) else {
        unreachable!("entry is balanced");
    };
    entry
}

fn batch() -> PostingBatch {
    PostingBatch {
        tenant: TenantId::new("acme"),
        target_gl: "netsuite:prod".to_owned(),
        period: "2026Q2".to_owned(),
        entries: vec![entry(30), entry(70)],
    }
}

fn released(protocol: &mut PostingProtocol) -> ContentHash {
    let Ok(id) = protocol.register(batch(), ActorId::new("alice")) else {
        unreachable!("register must succeed");
    };
    let Ok(()) = protocol.release(&id, ActorId::new("bob")) else {
        unreachable!("release must succeed");
    };
    id
}

#[test]
fn happy_path_posts_and_verifies() {
    let mut protocol = PostingProtocol::new();
    let id = released(&mut protocol);
    assert_eq!(protocol.begin_submit(&id), Ok(()));
    assert_eq!(
        protocol.confirm_posted(&id, "JE-1001".to_owned(), ContentHash([5; 32])),
        Ok(())
    );

    let Ok(fingerprint) = batch().entry_fingerprint() else {
        unreachable!("fingerprint must compute");
    };
    assert_eq!(
        protocol.verify(&id, &fingerprint, ContentHash([6; 32])),
        Ok(())
    );
    assert!(matches!(
        protocol.state(&id),
        Some(PostingState::Verified { .. })
    ));
}

#[test]
fn release_requires_dual_control() {
    let mut protocol = PostingProtocol::new();
    let Ok(id) = protocol.register(batch(), ActorId::new("alice")) else {
        unreachable!("register must succeed");
    };
    assert_eq!(
        protocol.release(&id, ActorId::new("alice")),
        Err(PostingError::DualControl(
            DualControlError::DualControlViolation
        ))
    );
    // Unreleased batches cannot submit.
    assert_eq!(
        protocol.begin_submit(&id),
        Err(PostingError::InvalidTransition)
    );
}

#[test]
fn batch_id_is_a_stable_idempotency_key() {
    let Ok(a) = batch().batch_id() else {
        unreachable!("hash must compute");
    };
    let Ok(b) = batch().batch_id() else {
        unreachable!("hash must compute");
    };
    assert_eq!(a, b);

    // A different target GL is a different batch.
    let mut other = batch();
    other.target_gl = "quickbooks:prod".to_owned();
    let Ok(c) = other.batch_id() else {
        unreachable!("hash must compute");
    };
    assert_ne!(a, c);
}

#[test]
fn lost_ack_resolves_only_through_readback() {
    let mut protocol = PostingProtocol::new();
    let id = released(&mut protocol);
    let _ = protocol.begin_submit(&id);
    assert_eq!(protocol.report_unknown(&id), Ok(()));

    // Guessing is not a transition: posting/verifying from Unknown fails.
    assert_eq!(
        protocol.confirm_posted(&id, "JE-X".to_owned(), ContentHash([5; 32])),
        Err(PostingError::InvalidTransition)
    );

    // Read-back proves absence → safe retry under the SAME key.
    assert_eq!(protocol.resolve_unknown(&id, None), Ok(()));
    assert_eq!(protocol.state(&id), Some(&PostingState::ReadyToSubmit));

    // Second attempt: ack lost again, but this time read-back finds it.
    let _ = protocol.begin_submit(&id);
    let _ = protocol.report_unknown(&id);
    let found = Some(("JE-1002".to_owned(), ContentHash([7; 32])));
    assert_eq!(protocol.resolve_unknown(&id, found), Ok(()));
    assert!(matches!(
        protocol.state(&id),
        Some(PostingState::Posted { .. })
    ));
}

#[test]
fn verification_names_missing_and_unexpected() {
    let mut protocol = PostingProtocol::new();
    let id = released(&mut protocol);
    let _ = protocol.begin_submit(&id);
    let _ = protocol.confirm_posted(&id, "JE-1003".to_owned(), ContentHash([5; 32]));

    // The GL shows one of our entries plus a stranger.
    let Ok(fingerprint) = batch().entry_fingerprint() else {
        unreachable!("fingerprint must compute");
    };
    let Some(first) = fingerprint.first() else {
        unreachable!("two entries");
    };
    let stranger = ContentHash([0xAA; 32]);
    let readback = vec![*first, stranger];
    assert_eq!(
        protocol.verify(&id, &readback, ContentHash([6; 32])),
        Ok(())
    );

    let Some(PostingState::VerificationFailed {
        missing,
        unexpected,
        ..
    }) = protocol.state(&id)
    else {
        unreachable!("mismatch must fail verification");
    };
    assert_eq!(missing.len(), 1);
    assert_eq!(unexpected, &vec![stranger]);
}

#[test]
fn reregistration_never_resets_state() {
    let mut protocol = PostingProtocol::new();
    let id = released(&mut protocol);
    let Ok(id2) = protocol.register(batch(), ActorId::new("carol")) else {
        unreachable!("register must succeed");
    };
    assert_eq!(id, id2);
    assert_eq!(protocol.state(&id), Some(&PostingState::ReadyToSubmit));
}

/// Golden vector — independently recomputed in Python (batch envelope
/// over entry hashes).
#[test]
fn golden_batch_id_matches_independent_implementation() {
    let Ok(id) = batch().batch_id() else {
        unreachable!("hash must compute");
    };
    assert_eq!(
        id.to_hex(),
        "f5dccc8bbc075c4bb5dd53f625c57978fa1bb95999407b2878473f9232ae805c".to_owned()
    );
}
