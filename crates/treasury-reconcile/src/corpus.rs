//! Labeled corpus + SLO harness (spec v2 §5, REQ-21).
//!
//! "Precision/recall SLOs on a labeled corpus. The corpus and metrics
//! are audit artifacts." Concretely:
//! - A [`Corpus`] of labeled cases is content-addressed; the
//!   [`CorpusReport`] commits to both the corpus hash and the matcher
//!   config hash, so a reported number is replayable byte-for-byte.
//! - **No floats.** Rates are exact rationals ([`Ratio`]); SLO checks
//!   compare by cross-multiplication in 128-bit integers.
//! - The taxonomy separates *errors* from *abstentions*: an auto-netted
//!   pair that is wrong (`auto_fp`) manufactures phantom P&L and is the
//!   number that must be zero; a queued pair is the system correctly
//!   asking a human, and is a workload metric, not an error.

use crate::config::MatcherConfig;
use crate::decision::Disposition;
use crate::leg::{LegId, TransferLeg};
use crate::matcher::{match_legs, MatchError};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeSet;
use treasury_core::ContentHash;
use treasury_evidence::{canonical_bytes, sha256};

/// Schema tag committed into every corpus hash; bump on change.
pub const CORPUS_SCHEMA: &str = "treasury-reconcile/corpus/v1";
/// Schema tag committed into every report hash; bump on change.
pub const REPORT_SCHEMA: &str = "treasury-reconcile/slo-report/v1";

/// Ground truth: this outflow and this inflow are one internal transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ExpectedNet {
    /// The outflow leg.
    pub out_leg: LegId,
    /// The inflow leg.
    pub in_leg: LegId,
}

/// One labeled scenario.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabeledCase {
    /// Stable case name (shows up in per-case results).
    pub name: String,
    /// The legs as ingested.
    pub legs: Vec<TransferLeg>,
    /// Every true internal-transfer pair in `legs`.
    pub expected_nets: Vec<ExpectedNet>,
}

/// A content-addressed labeled corpus.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Corpus {
    /// Labeled cases, evaluated in order.
    pub cases: Vec<LabeledCase>,
}

impl Corpus {
    /// The corpus hash — committed into every report.
    ///
    /// # Errors
    /// [`MatchError::Canon`] on envelope failure (structurally
    /// unreachable: legs serialize float-free by construction).
    pub fn corpus_hash(&self) -> Result<ContentHash, MatchError> {
        let envelope = json!({
            "schema": CORPUS_SCHEMA,
            "cases": self.cases.clone(),
        });
        let bytes = canonical_bytes(&envelope)?;
        Ok(sha256(&bytes))
    }
}

/// An exact rational. Never a float.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ratio {
    /// Numerator.
    pub numerator: u64,
    /// Denominator (zero denominators never occur; see callers).
    pub denominator: u64,
}

impl Ratio {
    /// Whether `self >= other`, by cross-multiplication in 128 bits.
    #[must_use]
    pub fn at_least(&self, other: &Ratio) -> bool {
        let left = u128::from(self.numerator).saturating_mul(u128::from(other.denominator));
        let right = u128::from(other.numerator).saturating_mul(u128::from(self.denominator));
        left >= right
    }
}

/// Counts for one case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseResult {
    /// Case name.
    pub name: String,
    /// Auto-netted pairs that are true internal transfers.
    pub auto_tp: u64,
    /// Auto-netted pairs that are NOT true internal transfers — phantom
    /// P&L. The SLO number that must be zero.
    pub auto_fp: u64,
    /// True pairs surfaced to the queue (correct abstention).
    pub queued_expected: u64,
    /// Non-pairs surfaced to the queue (review workload).
    pub queued_unexpected: u64,
    /// True pairs in no proposal at all (missed entirely).
    pub missed: u64,
}

/// The corpus-level report — a content-addressed audit artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusReport {
    /// Hash of the corpus evaluated.
    pub corpus_hash: ContentHash,
    /// Hash of the matcher config evaluated under.
    pub config_hash: ContentHash,
    /// Per-case counts, in corpus order.
    pub per_case: Vec<CaseResult>,
    /// Sum of `auto_tp`.
    pub auto_tp: u64,
    /// Sum of `auto_fp`.
    pub auto_fp: u64,
    /// Sum of `queued_expected`.
    pub queued_expected: u64,
    /// Sum of `queued_unexpected`.
    pub queued_unexpected: u64,
    /// Sum of `missed`.
    pub missed: u64,
}

impl CorpusReport {
    /// Auto-net precision: `auto_tp / (auto_tp + auto_fp)`. When nothing
    /// was auto-netted there are no wrong auto-nets — vacuously `1/1`.
    #[must_use]
    pub fn auto_precision(&self) -> Ratio {
        let denominator = self.auto_tp.saturating_add(self.auto_fp);
        if denominator == 0 {
            return Ratio {
                numerator: 1,
                denominator: 1,
            };
        }
        Ratio {
            numerator: self.auto_tp,
            denominator,
        }
    }

    /// Recall over true pairs, counting a queued true pair as found:
    /// `(auto_tp + queued_expected) / total_expected`. Vacuously `1/1`
    /// for a corpus with no expected pairs.
    #[must_use]
    pub fn found_recall(&self) -> Ratio {
        let found = self.auto_tp.saturating_add(self.queued_expected);
        let total = found.saturating_add(self.missed);
        if total == 0 {
            return Ratio {
                numerator: 1,
                denominator: 1,
            };
        }
        Ratio {
            numerator: found,
            denominator: total,
        }
    }

    /// The report's content hash — what a control narrative cites.
    ///
    /// # Errors
    /// [`MatchError::Canon`] on envelope failure (structurally
    /// unreachable).
    pub fn report_hash(&self) -> Result<ContentHash, MatchError> {
        let envelope = json!({
            "schema": REPORT_SCHEMA,
            "report": self.clone(),
        });
        let bytes = canonical_bytes(&envelope)?;
        Ok(sha256(&bytes))
    }
}

/// SLO targets. Phrase thresholds as exact rationals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SloTargets {
    /// Minimum auto-net precision (e.g. `1/1`: zero phantom nets).
    pub min_auto_precision: Ratio,
    /// Minimum found-recall (e.g. `99/100`).
    pub min_found_recall: Ratio,
    /// Maximum count of entirely missed true pairs.
    pub max_missed: u64,
}

/// A violated target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SloViolation {
    /// Auto-net precision below target — phantom P&L risk.
    AutoPrecision {
        /// Measured value.
        measured: Ratio,
    },
    /// Found-recall below target.
    FoundRecall {
        /// Measured value.
        measured: Ratio,
    },
    /// More missed pairs than allowed.
    Missed {
        /// Measured count.
        measured: u64,
    },
}

/// Check a report against targets. Empty result means all SLOs hold.
#[must_use]
pub fn slo_check(report: &CorpusReport, targets: &SloTargets) -> Vec<SloViolation> {
    let mut violations = Vec::new();
    let precision = report.auto_precision();
    if !precision.at_least(&targets.min_auto_precision) {
        violations.push(SloViolation::AutoPrecision {
            measured: precision,
        });
    }
    let recall = report.found_recall();
    if !recall.at_least(&targets.min_found_recall) {
        violations.push(SloViolation::FoundRecall { measured: recall });
    }
    if report.missed > targets.max_missed {
        violations.push(SloViolation::Missed {
            measured: report.missed,
        });
    }
    violations
}

/// Evaluate a corpus under a config. Deterministic: same corpus, same
/// config, same report (and same report hash).
///
/// # Errors
/// Propagates [`MatchError`] (mixed tenants in a case, canon failure).
pub fn evaluate(corpus: &Corpus, config: &MatcherConfig) -> Result<CorpusReport, MatchError> {
    let corpus_hash = corpus.corpus_hash()?;
    let config_hash = config.config_hash()?;

    let mut per_case: Vec<CaseResult> = Vec::new();
    let mut totals = CaseResult {
        name: String::new(),
        auto_tp: 0,
        auto_fp: 0,
        queued_expected: 0,
        queued_unexpected: 0,
        missed: 0,
    };

    for case in &corpus.cases {
        let outcome = match_legs(&case.legs, config)?;
        let expected: BTreeSet<(LegId, LegId)> = case
            .expected_nets
            .iter()
            .map(|e| (e.out_leg, e.in_leg))
            .collect();

        let mut result = CaseResult {
            name: case.name.clone(),
            auto_tp: 0,
            auto_fp: 0,
            queued_expected: 0,
            queued_unexpected: 0,
            missed: 0,
        };
        let mut surfaced: BTreeSet<(LegId, LegId)> = BTreeSet::new();
        for proposal in &outcome.proposals {
            let pair = (proposal.out_leg, proposal.in_leg);
            surfaced.insert(pair);
            let is_expected = expected.contains(&pair);
            match (proposal.disposition, is_expected) {
                (Disposition::AutoNet, true) => result.auto_tp = result.auto_tp.saturating_add(1),
                (Disposition::AutoNet, false) => result.auto_fp = result.auto_fp.saturating_add(1),
                (Disposition::Queue, true) => {
                    result.queued_expected = result.queued_expected.saturating_add(1);
                }
                (Disposition::Queue, false) => {
                    result.queued_unexpected = result.queued_unexpected.saturating_add(1);
                }
            }
        }
        for pair in &expected {
            if !surfaced.contains(pair) {
                result.missed = result.missed.saturating_add(1);
            }
        }

        totals.auto_tp = totals.auto_tp.saturating_add(result.auto_tp);
        totals.auto_fp = totals.auto_fp.saturating_add(result.auto_fp);
        totals.queued_expected = totals
            .queued_expected
            .saturating_add(result.queued_expected);
        totals.queued_unexpected = totals
            .queued_unexpected
            .saturating_add(result.queued_unexpected);
        totals.missed = totals.missed.saturating_add(result.missed);
        per_case.push(result);
    }

    Ok(CorpusReport {
        corpus_hash,
        config_hash,
        per_case,
        auto_tp: totals.auto_tp,
        auto_fp: totals.auto_fp,
        queued_expected: totals.queued_expected,
        queued_unexpected: totals.queued_unexpected,
        missed: totals.missed,
    })
}
