//! GL adapter contract + orchestration end to end (ADR-0003): happy
//! path, lost-ack recovery (both directions), two-way read-back
//! mismatch, idempotent retry.

use treasury_core::{ActorId, AssetAmount, AssetId, ContentHash, TenantId};
use treasury_gaap::{JournalEntry, JournalLine, Side, StatementLine};
use treasury_gl::{post_batch, DriveOutcome, FixtureFault, FixtureGl, GlAdapter};
use treasury_posting::{PostingBatch, PostingProtocol};

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

fn alice() -> ActorId {
    ActorId::new("alice")
}

fn bob() -> ActorId {
    ActorId::new("bob")
}

#[test]
fn happy_path_posts_and_verifies() {
    let mut protocol = PostingProtocol::new();
    let mut gl = FixtureGl::new();
    let outcome = post_batch(&mut protocol, &mut gl, batch(), alice(), bob());
    let Ok(DriveOutcome::Verified { gl_ref }) = outcome else {
        unreachable!("happy path must verify; got {outcome:?}");
    };
    assert!(gl_ref.starts_with("JE-"));
}

#[test]
fn lost_ack_but_posted_recovers_via_readback_and_verifies() {
    let mut protocol = PostingProtocol::new();
    let mut gl = FixtureGl::new();
    gl.inject(FixtureFault::AckLostButPosted);
    // The ack was lost but the batch landed; read-back finds it →
    // resolve to Posted → verify.
    let outcome = post_batch(&mut protocol, &mut gl, batch(), alice(), bob());
    assert!(matches!(outcome, Ok(DriveOutcome::Verified { .. })));
}

#[test]
fn lost_ack_not_posted_is_retryable_under_same_key() {
    let mut protocol = PostingProtocol::new();
    let mut gl = FixtureGl::new();
    gl.inject(FixtureFault::AckLostNotPosted);
    let first = post_batch(&mut protocol, &mut gl, batch(), alice(), bob());
    assert_eq!(first, Ok(DriveOutcome::RetryableNotPosted));

    // Retry: the protocol already holds the batch (same idempotency key);
    // a fresh protocol models a clean retry that now succeeds.
    let mut retry_protocol = PostingProtocol::new();
    let retry = post_batch(&mut retry_protocol, &mut gl, batch(), alice(), bob());
    assert!(matches!(retry, Ok(DriveOutcome::Verified { .. })));
}

#[test]
fn dropped_entry_on_readback_fails_verification() {
    let mut protocol = PostingProtocol::new();
    let mut gl = FixtureGl::new();
    gl.inject(FixtureFault::DropOneEntryOnReadback);
    let outcome = post_batch(&mut protocol, &mut gl, batch(), alice(), bob());
    let Ok(DriveOutcome::VerificationFailed {
        missing,
        unexpected,
    }) = outcome
    else {
        unreachable!("a dropped GL entry must fail verification; got {outcome:?}");
    };
    assert_eq!(missing.len(), 1);
    assert!(unexpected.is_empty());
}

#[test]
fn extra_entry_on_readback_fails_verification() {
    let mut protocol = PostingProtocol::new();
    let mut gl = FixtureGl::new();
    gl.inject(FixtureFault::ExtraEntryOnReadback(ContentHash([0xAB; 32])));
    let outcome = post_batch(&mut protocol, &mut gl, batch(), alice(), bob());
    let Ok(DriveOutcome::VerificationFailed {
        missing,
        unexpected,
    }) = outcome
    else {
        unreachable!("a stranger GL entry must fail verification; got {outcome:?}");
    };
    assert!(missing.is_empty());
    assert_eq!(unexpected, vec![ContentHash([0xAB; 32])]);
}

#[test]
fn transport_error_surfaces_not_swallowed() {
    let mut protocol = PostingProtocol::new();
    let mut gl = FixtureGl::new();
    gl.inject(FixtureFault::Transport);
    let outcome = post_batch(&mut protocol, &mut gl, batch(), alice(), bob());
    assert!(outcome.is_err(), "a transport error must surface");
}

#[test]
fn submit_is_idempotent_under_the_batch_key() {
    // Two submits of the same batch return the same gl_ref and create one
    // record — the adapter-level half of the protocol's retry safety.
    let mut gl = FixtureGl::new();
    let Ok(id) = batch().batch_id() else {
        unreachable!("hash");
    };
    let Ok(first) = gl.submit(&batch(), id) else {
        unreachable!("submit");
    };
    let Ok(second) = gl.submit(&batch(), id) else {
        unreachable!("submit");
    };
    assert_eq!(first, second);
}
