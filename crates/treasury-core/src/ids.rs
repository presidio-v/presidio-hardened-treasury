//! Identifier newtypes. No bare strings cross subsystem boundaries.

use serde::{Deserialize, Serialize};

macro_rules! string_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Construct from any string-like value.
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            /// The identifier as a string slice.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl core::fmt::Display for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

string_id!(
    /// A tenant (one legal entity per tenant — spec v2 §6).
    TenantId
);
string_id!(
    /// An asset, e.g. `"BTC"` or a chain-qualified token reference.
    AssetId
);
string_id!(
    /// A venue: exchange, custodian, or self-custody wallet group.
    VenueId
);
string_id!(
    /// A human or service actor recorded in judgment provenance (spec v2 §3.1 L3).
    ActorId
);
string_id!(
    /// An ingestion source (venue API, chain node, price feed).
    SourceId
);
