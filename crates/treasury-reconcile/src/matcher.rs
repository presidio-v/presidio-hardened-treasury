//! The deterministic tiered matcher (spec v2 §5).
//!
//! Pure function of `(legs, config)`. Legs are processed in
//! (event-time, leg-id) order so a replay under the same config hash
//! reproduces the same proposals byte-for-byte.

use crate::config::MatcherConfig;
use crate::decision::{Disposition, MatchProposal, Tier};
use crate::leg::{Direction, LegId, TransferLeg};
use std::collections::HashSet;
use treasury_core::{AssetAmount, ContentHash};
use treasury_evidence::CanonError;

/// Everything that must be resolved before the period can close:
/// the false-negative bias made queryable.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CloseBlockers {
    /// Legs the matcher could not place at all. Each must be classified
    /// by a human (internal transfer, disposal, acquisition, or
    /// non-purchase acquisition) before close.
    pub unmatched_legs: Vec<LegId>,
    /// Decision hashes of queued proposals awaiting dual-control
    /// confirmation.
    pub queued_decisions: Vec<ContentHash>,
}

impl CloseBlockers {
    /// Whether the period may close from reconciliation's perspective.
    #[must_use]
    pub fn close_permitted(&self) -> bool {
        self.unmatched_legs.is_empty() && self.queued_decisions.is_empty()
    }
}

/// Result of one matcher run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchOutcome {
    /// All proposals, auto-net and queued, in deterministic order.
    pub proposals: Vec<MatchProposal>,
    /// What still blocks the close.
    pub blockers: CloseBlockers,
}

/// Errors from a matcher run.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MatchError {
    /// All legs in one run must belong to one tenant; the matcher must
    /// never see (let alone net) movements across tenant boundaries.
    #[error("legs from more than one tenant in a single run")]
    MixedTenants,
    /// Envelope canonicalization failure.
    #[error(transparent)]
    Canon(#[from] CanonError),
}

/// Run the tiered matcher over a tenant's legs.
///
/// Tier 0: identical on-chain tx hash on an outflow and an inflow —
/// auto-net. Tier 1: inflow amount equals outflow amount minus fee,
/// within the config time window after the outflow, with both addresses
/// present and equal — auto-net below the asset's materiality threshold,
/// queue at or above it; **ambiguity (multiple corroborated candidates)
/// demotes to tier 2**. Tier 2: amount+window match without address
/// corroboration, or ambiguous tier 1 — always queued, one proposal per
/// candidate pair, legs not consumed; a human resolves. Anything else:
/// unmatched, blocks close.
///
/// # Errors
/// [`MatchError::MixedTenants`] when legs span tenants;
/// [`MatchError::Canon`] on envelope failure (structurally unreachable).
// Deliberately one function: it reads top-to-bottom as the spec §5 tier
// sequence, which is the property an auditor walkthrough needs most.
#[allow(clippy::too_many_lines)]
pub fn match_legs(
    legs: &[TransferLeg],
    config: &MatcherConfig,
) -> Result<MatchOutcome, MatchError> {
    if let Some(first) = legs.first() {
        if legs.iter().any(|l| l.tenant != first.tenant) {
            return Err(MatchError::MixedTenants);
        }
    }
    let config_hash = config.config_hash()?;

    let mut outflows: Vec<&TransferLeg> = Vec::new();
    let mut inflows: Vec<&TransferLeg> = Vec::new();
    for leg in legs {
        match leg.direction {
            Direction::Outflow => outflows.push(leg),
            Direction::Inflow => inflows.push(leg),
        }
    }
    outflows.sort_by_key(|l| (l.event_time, l.leg_id));
    inflows.sort_by_key(|l| (l.event_time, l.leg_id));

    let mut proposals: Vec<MatchProposal> = Vec::new();
    let mut used: HashSet<LegId> = HashSet::new();

    // Tier 0 — deterministic tx-hash identity.
    for out in &outflows {
        let Some(out_tx) = out.tx_hash.as_deref() else {
            continue;
        };
        let mut matched: Option<&TransferLeg> = None;
        for inflow in &inflows {
            let same_tx = inflow.tx_hash.as_deref() == Some(out_tx);
            let free = !used.contains(&inflow.leg_id);
            if same_tx && free && inflow.amount.asset() == out.amount.asset() {
                matched = Some(*inflow);
                break;
            }
        }
        if let Some(inflow) = matched {
            used.insert(out.leg_id);
            used.insert(inflow.leg_id);
            proposals.push(MatchProposal {
                out_leg: out.leg_id,
                in_leg: inflow.leg_id,
                tier: Tier::Deterministic,
                disposition: Disposition::AutoNet,
                config_hash,
            });
        }
    }

    // Tier 1 / Tier 2 — amount−fee within window, address corroboration.
    for out in &outflows {
        if used.contains(&out.leg_id) {
            continue;
        }
        let expected_in = expected_inflow(out);

        let mut candidates: Vec<&TransferLeg> = Vec::new();
        for inflow in &inflows {
            if used.contains(&inflow.leg_id) {
                continue;
            }
            if amount_and_window_match(out, inflow, expected_in.as_ref(), config) {
                candidates.push(*inflow);
            }
        }

        let mut corroborated: Vec<&TransferLeg> = Vec::new();
        for inflow in &candidates {
            if addresses_corroborate(out, inflow) {
                corroborated.push(*inflow);
            }
        }

        if let [single] = corroborated.as_slice() {
            // Exactly one address-corroborated candidate: tier 1.
            let inflow: &TransferLeg = single;
            let below = out.amount.atoms() < config.threshold_for(out.amount.asset());
            let disposition = if below {
                Disposition::AutoNet
            } else {
                Disposition::Queue
            };
            used.insert(out.leg_id);
            used.insert(inflow.leg_id);
            proposals.push(MatchProposal {
                out_leg: out.leg_id,
                in_leg: inflow.leg_id,
                tier: Tier::StrongCorroboration,
                disposition,
                config_hash,
            });
            continue;
        }

        // Ambiguous or under-corroborated: tier 2, one queued proposal
        // per candidate, legs not consumed — a human resolves.
        for inflow in &candidates {
            proposals.push(MatchProposal {
                out_leg: out.leg_id,
                in_leg: inflow.leg_id,
                tier: Tier::Probabilistic,
                disposition: Disposition::Queue,
                config_hash,
            });
        }
    }

    // Blockers: queued decisions + legs in no proposal at all.
    let mut queued_decisions: Vec<ContentHash> = Vec::new();
    let mut proposed_legs: HashSet<LegId> = HashSet::new();
    for proposal in &proposals {
        proposed_legs.insert(proposal.out_leg);
        proposed_legs.insert(proposal.in_leg);
        if proposal.disposition == Disposition::Queue {
            queued_decisions.push(proposal.decision_hash()?);
        }
    }
    let mut unmatched_legs: Vec<LegId> = Vec::new();
    for leg in legs {
        if !proposed_legs.contains(&leg.leg_id) {
            unmatched_legs.push(leg.leg_id);
        }
    }
    unmatched_legs.sort_unstable();

    Ok(MatchOutcome {
        proposals,
        blockers: CloseBlockers {
            unmatched_legs,
            queued_decisions,
        },
    })
}

/// Outflow amount minus its fee (the amount expected to arrive). `None`
/// when the fee is in a different asset — such legs cannot be tier 1.
fn expected_inflow(out: &TransferLeg) -> Option<AssetAmount> {
    match &out.fee {
        None => Some(out.amount.clone()),
        Some(fee) => out.amount.checked_sub(fee).ok(),
    }
}

fn amount_and_window_match(
    out: &TransferLeg,
    inflow: &TransferLeg,
    expected_in: Option<&AssetAmount>,
    config: &MatcherConfig,
) -> bool {
    let Some(expected) = expected_in else {
        return false;
    };
    if inflow.amount != *expected {
        return false;
    }
    let out_ns = out.event_time.as_nanos();
    let Some(delta) = inflow.event_time.as_nanos().checked_sub(out_ns) else {
        return false;
    };
    (0..=config.time_window_ns).contains(&delta)
}

fn addresses_corroborate(out: &TransferLeg, inflow: &TransferLeg) -> bool {
    match (out.address.as_deref(), inflow.address.as_deref()) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}
