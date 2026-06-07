//! SLO harness over the synthetic adversarial baseline (REQ-21).

use std::collections::BTreeMap;
use treasury_core::{AssetAmount, AssetId, ContentHash, TenantId, TimestampNs, VenueId};
use treasury_reconcile::{
    evaluate, slo_check, synthetic_baseline, Corpus, Direction, ExpectedNet, LabeledCase,
    MatcherConfig, Ratio, SloTargets, SloViolation, TransferLeg,
};

fn config() -> MatcherConfig {
    let mut materiality = BTreeMap::new();
    materiality.insert(AssetId::new("BTC"), 1_000_000_i128);
    MatcherConfig {
        time_window_ns: 1_000,
        materiality_atoms: materiality,
    }
}

#[test]
fn baseline_counts_are_exact() {
    let corpus = synthetic_baseline();
    let Ok(report) = evaluate(&corpus, &config()) else {
        unreachable!("evaluation must succeed");
    };
    // The number that must be zero: phantom auto-nets.
    assert_eq!(report.auto_fp, 0);
    assert_eq!(report.auto_tp, 2);
    assert_eq!(report.queued_expected, 3);
    assert_eq!(report.queued_unexpected, 2);
    // The batched withdrawal is a labeled, visible limitation.
    assert_eq!(report.missed, 2);
    assert_eq!(report.per_case.len(), 7);
}

#[test]
fn evaluation_is_deterministic_and_content_addressed() {
    let corpus = synthetic_baseline();
    let a = evaluate(&corpus, &config());
    let b = evaluate(&corpus, &config());
    assert_eq!(a, b);
    let hash_a = a.and_then(|r| r.report_hash());
    let hash_b = b.and_then(|r| r.report_hash());
    assert_eq!(hash_a, hash_b);
    assert!(hash_a.is_ok());
}

#[test]
fn baseline_meets_zero_phantom_slo() {
    let corpus = synthetic_baseline();
    let Ok(report) = evaluate(&corpus, &config()) else {
        unreachable!("evaluation must succeed");
    };
    let targets = SloTargets {
        min_auto_precision: Ratio {
            numerator: 1,
            denominator: 1,
        },
        min_found_recall: Ratio {
            numerator: 2,
            denominator: 3,
        },
        max_missed: 2,
    };
    assert_eq!(slo_check(&report, &targets), Vec::new());
}

#[test]
fn tightened_targets_report_violations() {
    let corpus = synthetic_baseline();
    let Ok(report) = evaluate(&corpus, &config()) else {
        unreachable!("evaluation must succeed");
    };
    let targets = SloTargets {
        min_auto_precision: Ratio {
            numerator: 1,
            denominator: 1,
        },
        min_found_recall: Ratio {
            numerator: 9,
            denominator: 10,
        },
        max_missed: 0,
    };
    let violations = slo_check(&report, &targets);
    assert_eq!(violations.len(), 2);
    assert!(matches!(
        violations.first(),
        Some(SloViolation::FoundRecall { .. })
    ));
    assert!(matches!(
        violations.get(1),
        Some(SloViolation::Missed { .. })
    ));
}

#[test]
fn ratio_comparison_is_exact() {
    let five_sevenths = Ratio {
        numerator: 5,
        denominator: 7,
    };
    let two_thirds = Ratio {
        numerator: 2,
        denominator: 3,
    };
    let nine_tenths = Ratio {
        numerator: 9,
        denominator: 10,
    };
    assert!(five_sevenths.at_least(&two_thirds));
    assert!(!five_sevenths.at_least(&nine_tenths));
    assert!(five_sevenths.at_least(&five_sevenths));
}

/// Golden vector — independently recomputed in Python over the full
/// serde shape of a minimal corpus.
#[test]
fn golden_corpus_hash_matches_independent_implementation() {
    let corpus = Corpus {
        cases: vec![LabeledCase {
            name: "g".to_owned(),
            legs: vec![TransferLeg {
                leg_id: ContentHash([1; 32]),
                tenant: TenantId::new("t"),
                venue: VenueId::new("v"),
                direction: Direction::Outflow,
                amount: AssetAmount::new(AssetId::new("BTC"), 5),
                fee: None,
                tx_hash: None,
                address: None,
                event_time: TimestampNs::from_nanos(10),
            }],
            expected_nets: vec![ExpectedNet {
                out_leg: ContentHash([1; 32]),
                in_leg: ContentHash([2; 32]),
            }],
        }],
    };
    let hash = corpus.corpus_hash().map(|h| h.to_hex());
    assert_eq!(
        hash.as_deref(),
        Ok("161f2138b9df2f856a9d55baf0a4f7d1ced58c8b5be39cf1cdffaf7515858212")
    );
}
