//! Venue API key scope validation — fail closed (spec v2 §3.4).
//!
//! This is the onboarding hygiene check that *precedes* the egress
//! allowlist: a key reporting any trade/withdraw/transfer capability is
//! rejected outright, and a scope we cannot positively identify as a read
//! rejects too. The allowlist remains the enforcement layer either way —
//! this gate exists so a dangerous key never even gets stored.

use serde::{Deserialize, Serialize};
use treasury_core::VenueId;

/// A capability claimed by a venue API key, as reported by the venue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "name", rename_all = "snake_case")]
pub enum ScopeClaim {
    /// Positively identified read capability (view balances, history).
    Read,
    /// Capability to place or manage orders.
    Trade,
    /// Capability to withdraw assets.
    Withdraw,
    /// Capability to move assets internally (sub-accounts, allocations).
    Transfer,
    /// A scope string we cannot positively classify. Fails closed.
    Unknown(String),
}

/// One rejected capability and which key position reported it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeViolation {
    /// Venue that reported the scope.
    pub venue: VenueId,
    /// The offending claim.
    pub claim: ScopeClaim,
}

/// Outcome of scope validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeDecision {
    /// Every reported scope is a positively identified read.
    Accept,
    /// At least one scope is dangerous or unidentifiable; the key must
    /// not be stored. All violations are reported, not just the first.
    Reject(Vec<ScopeViolation>),
}

/// Validate a venue key's reported scopes. Fail closed: only positively
/// identified reads pass; everything else — including scopes we merely
/// don't recognize, and an **empty scope report** (broken or absent venue
/// scope API gives no positive identification at all) — rejects.
#[must_use]
pub fn validate_scopes(venue: &VenueId, claims: &[ScopeClaim]) -> ScopeDecision {
    if claims.is_empty() {
        let violation = ScopeViolation {
            venue: venue.clone(),
            claim: ScopeClaim::Unknown("empty scope report".to_owned()),
        };
        return ScopeDecision::Reject(vec![violation]);
    }
    let mut violations = Vec::new();
    for claim in claims {
        if *claim != ScopeClaim::Read {
            violations.push(ScopeViolation {
                venue: venue.clone(),
                claim: claim.clone(),
            });
        }
    }
    if violations.is_empty() {
        ScopeDecision::Accept
    } else {
        ScopeDecision::Reject(violations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn venue() -> VenueId {
        VenueId::new("kraken")
    }

    #[test]
    fn read_only_key_accepted() {
        let decision = validate_scopes(&venue(), &[ScopeClaim::Read, ScopeClaim::Read]);
        assert_eq!(decision, ScopeDecision::Accept);
    }

    #[test]
    fn trade_scope_rejected() {
        let decision = validate_scopes(&venue(), &[ScopeClaim::Read, ScopeClaim::Trade]);
        let ScopeDecision::Reject(violations) = decision else {
            unreachable!("trade scope must reject");
        };
        assert_eq!(violations.len(), 1);
        assert_eq!(
            violations.first().map(|v| &v.claim),
            Some(&ScopeClaim::Trade)
        );
    }

    #[test]
    fn unknown_scope_fails_closed() {
        let claims = [ScopeClaim::Unknown("margin_funding".to_owned())];
        let decision = validate_scopes(&venue(), &claims);
        assert!(matches!(decision, ScopeDecision::Reject(_)));
    }

    #[test]
    fn all_violations_reported() {
        let claims = [
            ScopeClaim::Trade,
            ScopeClaim::Withdraw,
            ScopeClaim::Transfer,
        ];
        let decision = validate_scopes(&venue(), &claims);
        let ScopeDecision::Reject(violations) = decision else {
            unreachable!("must reject");
        };
        assert_eq!(violations.len(), 3);
    }

    #[test]
    fn empty_scope_report_fails_closed() {
        // No positive identification of read capability — reject.
        let decision = validate_scopes(&venue(), &[]);
        assert!(matches!(decision, ScopeDecision::Reject(_)));
    }
}
