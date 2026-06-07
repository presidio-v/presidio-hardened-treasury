//! Internal-transfer reconciliation (spec v2 §5, REQ-21).
//!
//! A wallet→exchange move is not a disposal; mis-booking it manufactures
//! phantom P&L in a public filer's income statement. This crate is the
//! decision core of the tiered matcher:
//!
//! | Tier | Signal | Authority |
//! |------|--------|-----------|
//! | 0 | Same tx hash on both venues' records | Auto-net, no human |
//! | 1 | Amount−fee + time window + address corroboration | Auto-net **below materiality**; queue above |
//! | 2 | Everything else (incl. ambiguity) | **Never auto-net**; always queue |
//!
//! Structural guarantees:
//! - **False-negative bias:** a leg the matcher cannot place does not
//!   book as disposal + acquisition — it lands in [`CloseBlockers`] and
//!   blocks period close until classified.
//! - **Determinism:** matching is a pure function of (legs, config); legs
//!   are processed in (event-time, leg-id) order, the config is a
//!   content-addressed artifact, and every decision envelope commits to
//!   the config hash — the auditor replays the versioned matcher.
//! - **No numeric confidence:** tiers are discrete corroboration classes,
//!   not floats (nothing in this crate can produce a float).
//! - **Dual control:** queued proposals need a preparer assertion and a
//!   confirmation from a *different* approver; the vendor never
//!   classifies. Terminal queue states are immutable.

#![forbid(unsafe_code)]

pub mod book;
pub mod config;
pub mod decision;
pub mod designate;
pub mod leg;
pub mod matcher;
pub mod queue;

pub use book::{draft_auto_net, draft_resolution, BookError};
pub use config::MatcherConfig;
pub use designate::{
    draft_designation, DesignateError, DesignationProposal, DesignationQueue, DesignationState,
    LegClassification, NonPurchaseKind,
};
pub use decision::{Disposition, MatchProposal, Tier};
pub use leg::{Direction, LegId, TransferLeg};
pub use matcher::{match_legs, CloseBlockers, MatchError, MatchOutcome};
pub use queue::{ConfirmationQueue, QueueError, QueueState};
