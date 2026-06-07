//! Content-addressed egress allowlists (spec v2 §3.4).

use serde::{Deserialize, Serialize};
use serde_json::json;
use treasury_core::{ActorId, ContentHash, VenueId};
use treasury_evidence::{canonical_bytes, sha256, CanonError};

/// Schema tag committed into every allowlist hash; bump on change.
pub const ALLOWLIST_SCHEMA: &str = "treasury-ingest/allowlist/v1";

/// HTTP methods an allowlist entry may name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HttpMethod {
    /// Read.
    Get,
    /// Read (headers only).
    Head,
    /// Used by some venues for authenticated reads; requires justification.
    Post,
}

impl HttpMethod {
    /// Whether the method is read-only by HTTP semantics.
    #[must_use]
    pub fn is_read(&self) -> bool {
        matches!(self, Self::Get | Self::Head)
    }
}

/// A path pattern: exact, or a literal prefix. No regex — nothing an
/// auditor cannot evaluate by eye, nothing an attacker can backtrack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "match", content = "path", rename_all = "snake_case")]
pub enum PathPattern {
    /// The request path must equal this string exactly.
    Exact(String),
    /// The request path must start with this literal prefix.
    Prefix(String),
}

impl PathPattern {
    /// Whether a request path matches this pattern.
    #[must_use]
    pub fn matches(&self, path: &str) -> bool {
        match self {
            Self::Exact(p) => path == p,
            Self::Prefix(p) => path.starts_with(p.as_str()),
        }
    }
}

/// One allowlist entry: requests to `venue` with `method` and a path
/// matching `pattern` may leave the network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllowlistEntry {
    /// Venue the entry applies to.
    pub venue: VenueId,
    /// Permitted HTTP method.
    pub method: HttpMethod,
    /// Permitted path shape.
    pub pattern: PathPattern,
    /// Mandatory when `method` is not read-only by HTTP semantics:
    /// why this endpoint is nonetheless a read (e.g. venue serves
    /// authenticated account history over POST).
    pub justification: Option<String>,
}

/// A sealed, content-addressed egress allowlist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgressAllowlist {
    entries: Vec<AllowlistEntry>,
    approvers: Vec<ActorId>,
    allowlist_hash: ContentHash,
}

impl EgressAllowlist {
    /// Seal an allowlist from entries and approvers.
    ///
    /// # Errors
    /// [`AllowlistError::NoApprovers`] when unapproved;
    /// [`AllowlistError::UnjustifiedNonReadMethod`] when a non-read entry
    /// lacks a justification;
    /// [`AllowlistError::Canon`] on envelope canonicalization failure.
    pub fn seal(
        entries: Vec<AllowlistEntry>,
        approvers: Vec<ActorId>,
    ) -> Result<Self, AllowlistError> {
        if approvers.is_empty() {
            return Err(AllowlistError::NoApprovers);
        }
        for (index, entry) in entries.iter().enumerate() {
            let justified = entry.justification.as_ref().is_some_and(|j| !j.is_empty());
            if !entry.method.is_read() && !justified {
                return Err(AllowlistError::UnjustifiedNonReadMethod { index });
            }
        }
        let envelope = json!({
            "schema": ALLOWLIST_SCHEMA,
            "entries": entries.clone(),
            "approvers": approvers.clone(),
        });
        let bytes = canonical_bytes(&envelope)?;
        let allowlist_hash = sha256(&bytes);
        Ok(Self {
            entries,
            approvers,
            allowlist_hash,
        })
    }

    /// The artifact hash a deployment and an audit both reference.
    #[must_use]
    pub fn allowlist_hash(&self) -> ContentHash {
        self.allowlist_hash
    }

    /// Approving actors.
    #[must_use]
    pub fn approvers(&self) -> &[ActorId] {
        &self.approvers
    }

    /// Decide whether a request may leave the network. Deny by default:
    /// the absence of a matching entry is a denial, never a pass-through.
    #[must_use]
    pub fn decide(&self, venue: &VenueId, method: HttpMethod, path: &str) -> EgressDecision {
        for (entry_index, entry) in self.entries.iter().enumerate() {
            let venue_ok = entry.venue == *venue;
            let method_ok = entry.method == method;
            if venue_ok && method_ok && entry.pattern.matches(path) {
                return EgressDecision::Allow { entry_index };
            }
        }
        EgressDecision::Deny
    }
}

/// Outcome of an egress decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EgressDecision {
    /// Request matches the listed entry and may proceed.
    Allow {
        /// Index of the matching entry (for the egress audit log).
        entry_index: usize,
    },
    /// No entry matches; the request must not leave the network.
    Deny,
}

/// Errors sealing an allowlist.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AllowlistError {
    /// An unapproved allowlist has no identity and cannot deploy.
    #[error("allowlist requires at least one approver")]
    NoApprovers,
    /// Non-read methods require a written justification.
    #[error("entry {index} uses a non-read method without justification")]
    UnjustifiedNonReadMethod {
        /// Index of the offending entry.
        index: usize,
    },
    /// Envelope canonicalization failure.
    #[error(transparent)]
    Canon(#[from] CanonError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn venue() -> VenueId {
        VenueId::new("coinbase-prime")
    }

    fn get_entry(prefix: &str) -> AllowlistEntry {
        AllowlistEntry {
            venue: venue(),
            method: HttpMethod::Get,
            pattern: PathPattern::Prefix(prefix.to_owned()),
            justification: None,
        }
    }

    fn approvers() -> Vec<ActorId> {
        vec![ActorId::new("security-officer"), ActorId::new("cto")]
    }

    #[test]
    fn deny_by_default() {
        let Ok(list) = EgressAllowlist::seal(vec![get_entry("/v1/accounts")], approvers()) else {
            unreachable!("seal must succeed");
        };
        assert_eq!(
            list.decide(&venue(), HttpMethod::Get, "/v1/accounts/123"),
            EgressDecision::Allow { entry_index: 0 }
        );
        // Different path, method, and venue all deny.
        assert_eq!(
            list.decide(&venue(), HttpMethod::Get, "/v1/orders"),
            EgressDecision::Deny
        );
        assert_eq!(
            list.decide(&venue(), HttpMethod::Post, "/v1/accounts/123"),
            EgressDecision::Deny
        );
        assert_eq!(
            list.decide(&VenueId::new("kraken"), HttpMethod::Get, "/v1/accounts/123"),
            EgressDecision::Deny
        );
    }

    #[test]
    fn unjustified_post_cannot_be_constructed() {
        let mut entry = get_entry("/private/history");
        entry.method = HttpMethod::Post;
        let result = EgressAllowlist::seal(vec![entry], approvers());
        assert_eq!(
            result,
            Err(AllowlistError::UnjustifiedNonReadMethod { index: 0 })
        );
    }

    #[test]
    fn justified_post_is_allowed() {
        let mut entry = get_entry("/private/history");
        entry.method = HttpMethod::Post;
        entry.justification = Some("venue serves account history reads over POST".to_owned());
        assert!(EgressAllowlist::seal(vec![entry], approvers()).is_ok());
    }

    #[test]
    fn unapproved_allowlist_rejected() {
        let result = EgressAllowlist::seal(vec![get_entry("/v1")], Vec::new());
        assert_eq!(result, Err(AllowlistError::NoApprovers));
    }

    #[test]
    fn hash_commits_to_entries() {
        let a = EgressAllowlist::seal(vec![get_entry("/v1/a")], approvers());
        let b = EgressAllowlist::seal(vec![get_entry("/v1/b")], approvers());
        let hashes = (
            a.map(|l| l.allowlist_hash()),
            b.map(|l| l.allowlist_hash()),
        );
        assert_ne!(hashes.0, hashes.1);
    }

    /// Golden vector — independently recomputed in Python.
    #[test]
    fn golden_hash_matches_independent_implementation() {
        let list = EgressAllowlist::seal(vec![get_entry("/v1/accounts")], approvers());
        let hash = list.map(|l| l.allowlist_hash().to_hex());
        assert_eq!(
            hash.as_deref().map_or("", |s| s),
            "615ade0947ab9f7e867ecffc876f1eb326f43e8646ef3f0f2d57e88e985ce375"
        );
    }
}
