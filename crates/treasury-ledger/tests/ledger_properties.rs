//! Property tests (proptest): the ledger's bitemporal, append-only,
//! hash-chained invariants hold for arbitrary valid append sequences —
//! the chain always verifies, `as_of` returns exactly the booked-by-T
//! prefix, and knowledge time is strictly monotonic per tenant.

use proptest::prelude::*;
use serde_json::json;
use treasury_core::{ContentHash, SourceId, TenantId, TimestampNs};
use treasury_ledger::{ClaimLayer, EventDraft, InMemoryLedger, Ledger, Provenance};

fn tenant() -> TenantId {
    TenantId::new("acme")
}

/// A well-formed L1 observation whose payload varies with `n`.
fn observation(n: i64) -> EventDraft {
    EventDraft {
        tenant: tenant(),
        layer: ClaimLayer::Observation,
        event_time: TimestampNs::from_nanos(n),
        supersedes: None,
        provenance: Provenance::Observation {
            source: SourceId::new("venue"),
            evidence: ContentHash([7; 32]),
        },
        payload: json!({ "i": n }),
    }
}

/// Knowledge time for the `i`-th appended event: 10, 20, 30, … (strictly
/// increasing, so every append is legal).
fn kt_for(i: usize) -> i64 {
    let idx = i64::try_from(i).unwrap_or(i64::MAX);
    idx.saturating_add(1).saturating_mul(10)
}

proptest! {
    /// Any sequence of valid appends leaves the per-tenant hash chain
    /// fully verifiable, with one sealed event per append.
    #[test]
    fn appending_observations_keeps_the_chain_verifiable(n in 0_usize..50) {
        let mut ledger = InMemoryLedger::new();
        for i in 0..n {
            let kt = kt_for(i);
            let appended = ledger.append(observation(kt), TimestampNs::from_nanos(kt));
            prop_assert!(appended.is_ok());
        }
        prop_assert_eq!(ledger.stream(&tenant()).len(), n);
        prop_assert_eq!(ledger.verify_chain(&tenant()), Ok(()));
    }

    /// `as_of(T)` returns exactly the events booked at knowledge time ≤ T —
    /// the prefix active as of T, never more, never fewer.
    #[test]
    fn as_of_returns_the_booked_by_t_prefix(n in 0_usize..40, cut in 0_usize..40) {
        let mut ledger = InMemoryLedger::new();
        for i in 0..n {
            let kt = kt_for(i);
            let appended = ledger.append(observation(kt), TimestampNs::from_nanos(kt));
            prop_assert!(appended.is_ok());
        }
        let cutoff = cut.min(n);
        // Event i is booked at (i+1)*10, so cutoff*10 admits exactly the
        // first `cutoff` events.
        let t_cut = i64::try_from(cutoff).unwrap_or(i64::MAX).saturating_mul(10);
        let active = ledger.as_of(&tenant(), TimestampNs::from_nanos(t_cut));
        prop_assert_eq!(active.len(), cutoff);
    }

    /// Knowledge time is strictly monotonic per tenant: a second event
    /// booked at ≤ the first's knowledge time is rejected.
    #[test]
    fn non_monotonic_knowledge_time_is_rejected(
        first_kt in 1_i64..1_000_000,
        delta in 0_i64..1_000_000,
    ) {
        let mut ledger = InMemoryLedger::new();
        let first = ledger.append(observation(first_kt), TimestampNs::from_nanos(first_kt));
        prop_assert!(first.is_ok());
        let second_kt = first_kt.saturating_sub(delta);
        let second = ledger.append(observation(second_kt), TimestampNs::from_nanos(second_kt));
        prop_assert!(second.is_err());
    }
}
