//! The golden close (spec v2 §2, whole-pipeline): this crate carries no
//! production code — it exists for `tests/golden_close.rs`, which runs
//! every stage of the close pipeline against every Phase 0 guarantee in
//! one quarter:
//!
//! ingestion (L1 observations over evidence-store payloads) →
//! reconciliation (tier-0 auto-net + booked L2) → designation (staking
//! reward, dual control, booked L3) → scope gate (confirmed in-scope,
//! booked L3) → lots (acquisition, basis-preserving transfer along the
//! netted movement) → valuation (the `(lots, price-snapshot, policy)`
//! key) → GAAP entries (booked L4 — all four claim layers) → posting
//! protocol (released, posted, read-back verified) → checkpoint (sealed
//! over the ledger's `as_of` state root) → external anchor (verified
//! against the live evidence store) → disclosure pack (tied to the
//! valuation, manifest closed over every referenced hash).
//!
//! The flagship assertion is **whole-close determinism**: the entire
//! close runs twice and produces the identical pack hash — the
//! full-system version of the Phase 0 exit criterion.

#![forbid(unsafe_code)]
