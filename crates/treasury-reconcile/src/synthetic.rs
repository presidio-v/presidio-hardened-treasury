//! Synthetic adversarial baseline corpus (spec v2 §5: "synthetic
//! adversarial cases: batched withdrawals, fee-on-transfer tokens,
//! exchange-internal moves that never hit chain").
//!
//! Deliberately includes a case the tier-0/1/2 matcher **cannot** place
//! (a batched 1-out→2-in withdrawal): the baseline report shows it as
//! `missed`, which is the honest number — a labeled limitation that
//! drives future N:M matching work, visible in every SLO run instead of
//! hidden in a code comment.

use crate::corpus::{Corpus, ExpectedNet, LabeledCase};
use crate::leg::{Direction, TransferLeg};
use treasury_core::{AssetAmount, AssetId, ContentHash, TenantId, TimestampNs, VenueId};

fn btc(atoms: i128) -> AssetAmount {
    AssetAmount::new(AssetId::new("BTC"), atoms)
}

fn leg(id: u8, dir: Direction, atoms: i128, time: i64) -> TransferLeg {
    TransferLeg {
        leg_id: ContentHash([id; 32]),
        tenant: TenantId::new("corpus-tenant"),
        venue: VenueId::new("venue"),
        direction: dir,
        amount: btc(atoms),
        fee: None,
        tx_hash: None,
        address: None,
        event_time: TimestampNs::from_nanos(time),
    }
}

fn pair(out: u8, inn: u8) -> ExpectedNet {
    ExpectedNet {
        out_leg: ContentHash([out; 32]),
        in_leg: ContentHash([inn; 32]),
    }
}

/// The baseline corpus. Pure function: identical on every call, so its
/// content hash is stable across runs and machines.
#[must_use]
pub fn synthetic_baseline() -> Corpus {
    let mut cases = Vec::new();

    // 1. Clean on-chain move: same tx hash both sides → tier 0 auto-net.
    let mut out = leg(1, Direction::Outflow, 50_000, 10);
    let mut inn = leg(2, Direction::Inflow, 49_900, 30);
    out.tx_hash = Some("tx-clean".to_owned());
    inn.tx_hash = Some("tx-clean".to_owned());
    cases.push(LabeledCase {
        name: "clean_tier0".to_owned(),
        legs: vec![out, inn],
        expected_nets: vec![pair(1, 2)],
    });

    // 2. Fee taken mid-flight, small amount, address corroborated →
    //    tier 1 auto-net below materiality.
    let mut out = leg(3, Direction::Outflow, 50_000, 10);
    out.fee = Some(btc(500));
    out.address = Some("bc1q-dest".to_owned());
    let mut inn = leg(4, Direction::Inflow, 49_500, 400);
    inn.address = Some("bc1q-dest".to_owned());
    cases.push(LabeledCase {
        name: "fee_on_transfer".to_owned(),
        legs: vec![out, inn],
        expected_nets: vec![pair(3, 4)],
    });

    // 3. Same shape but material → correctly queued, not auto-netted.
    let mut out = leg(5, Direction::Outflow, 5_000_000, 10);
    out.address = Some("bc1q-big".to_owned());
    let mut inn = leg(6, Direction::Inflow, 5_000_000, 400);
    inn.address = Some("bc1q-big".to_owned());
    cases.push(LabeledCase {
        name: "material_transfer_queues".to_owned(),
        legs: vec![out, inn],
        expected_nets: vec![pair(5, 6)],
    });

    // 4. Batched withdrawal: one outflow arrives as two inflows. The
    //    pairwise matcher cannot place it — shows as `missed` (known
    //    limitation, kept visible).
    let out = leg(7, Direction::Outflow, 1_000_000, 10);
    let in_a = leg(8, Direction::Inflow, 600_000, 200);
    let in_b = leg(9, Direction::Inflow, 400_000, 220);
    cases.push(LabeledCase {
        name: "batched_withdrawal_known_miss".to_owned(),
        legs: vec![out, in_a, in_b],
        expected_nets: vec![pair(7, 8), pair(7, 9)],
    });

    // 5. Exchange-internal move that never hits chain: no tx hash, no
    //    addresses → tier 2, correctly surfaced to a human.
    let out = leg(10, Direction::Outflow, 75_000, 10);
    let inn = leg(11, Direction::Inflow, 75_000, 300);
    cases.push(LabeledCase {
        name: "exchange_internal_queues".to_owned(),
        legs: vec![out, inn],
        expected_nets: vec![pair(10, 11)],
    });

    // 6. Decoy: an unrelated inflow with a coincidentally equal amount
    //    inside the window. NOT a transfer — correct behavior is to
    //    queue it (review workload), never to auto-net it.
    let out = leg(12, Direction::Outflow, 33_000, 10);
    let decoy = leg(13, Direction::Inflow, 33_000, 200);
    cases.push(LabeledCase {
        name: "decoy_same_amount".to_owned(),
        legs: vec![out, decoy],
        expected_nets: vec![],
    });

    // 7. Ambiguity: two identical corroborated candidates → both surface
    //    to the queue; a human picks. The true pair counts as queued,
    //    the other as workload.
    let mut out = leg(14, Direction::Outflow, 20_000, 10);
    out.address = Some("bc1q-amb".to_owned());
    let mut in_true = leg(15, Direction::Inflow, 20_000, 100);
    in_true.address = Some("bc1q-amb".to_owned());
    let mut in_twin = leg(16, Direction::Inflow, 20_000, 150);
    in_twin.address = Some("bc1q-amb".to_owned());
    cases.push(LabeledCase {
        name: "ambiguous_twins_queue".to_owned(),
        legs: vec![out, in_true, in_twin],
        expected_nets: vec![pair(14, 15)],
    });

    Corpus { cases }
}
