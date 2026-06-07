//! The entry builders — the pure functions of the GAAP lens — and the
//! L4 booking bridge.

use crate::entry::{EntryError, JournalEntry, JournalLine, Side, StatementLine};
use serde::{Deserialize, Serialize};
use serde_json::json;
use treasury_core::{AssetAmount, ContentHash, TenantId, TimestampNs};
use treasury_ledger::{ClaimLayer, EventDraft, Provenance};

/// The tenant's election for acquisition transaction costs (G-3): ASU
/// 2023-08 is deliberately silent, so this is a recorded per-tenant
/// policy election, not a module default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeeTreatment {
    /// Expense as incurred (income statement).
    Expense,
    /// Capitalize into the asset's carrying amount.
    Capitalize,
}

/// Acquisition: Dr crypto (basis, plus fee when capitalized), Dr fee
/// expense (when expensed), Cr cash for the full outlay.
///
/// # Errors
/// [`EntryError`] on imbalance (structurally unreachable for valid
/// inputs), mixed currencies, or non-positive amounts.
pub fn acquisition_entry(
    basis: &AssetAmount,
    fee: &AssetAmount,
    treatment: FeeTreatment,
    policy_hash: ContentHash,
) -> Result<JournalEntry, EntryError> {
    let currency = basis.asset().clone();
    let outlay = basis
        .atoms()
        .checked_add(fee.atoms())
        .ok_or(EntryError::Overflow)?;
    let mut lines = Vec::new();
    match treatment {
        FeeTreatment::Capitalize => {
            lines.push(JournalLine {
                side: Side::Debit,
                line: StatementLine::CryptoAssets,
                amount: AssetAmount::new(currency.clone(), outlay),
            });
        }
        FeeTreatment::Expense => {
            lines.push(JournalLine {
                side: Side::Debit,
                line: StatementLine::CryptoAssets,
                amount: basis.clone(),
            });
            if fee.atoms() > 0 {
                lines.push(JournalLine {
                    side: Side::Debit,
                    line: StatementLine::TransactionCostsExpense,
                    amount: fee.clone(),
                });
            }
        }
    }
    lines.push(JournalLine {
        side: Side::Credit,
        line: StatementLine::Cash,
        amount: AssetAmount::new(currency, outlay),
    });
    JournalEntry::new("acquisition", lines, policy_hash)
}

/// Periodic ASU 2023-08 remeasurement: carrying moves to fair value,
/// the change routes to **net income**. Returns `None` when the mark is
/// zero (no entry to book).
///
/// # Errors
/// [`EntryError`] on currency mismatch or overflow.
pub fn remeasurement_entry(
    carrying_before: &AssetAmount,
    fair_value_now: &AssetAmount,
    policy_hash: ContentHash,
) -> Result<Option<JournalEntry>, EntryError> {
    if carrying_before.asset() != fair_value_now.asset() {
        return Err(EntryError::MixedCurrencies);
    }
    let currency = fair_value_now.asset().clone();
    let delta = fair_value_now
        .atoms()
        .checked_sub(carrying_before.atoms())
        .ok_or(EntryError::Overflow)?;
    if delta == 0 {
        return Ok(None);
    }
    let magnitude = delta.checked_abs().ok_or(EntryError::Overflow)?;
    let amount = AssetAmount::new(currency, magnitude);
    let (asset_side, pnl_side) = if delta > 0 {
        (Side::Debit, Side::Credit)
    } else {
        (Side::Credit, Side::Debit)
    };
    let entry = JournalEntry::new(
        "asu2023_08_remeasurement",
        vec![
            JournalLine {
                side: asset_side,
                line: StatementLine::CryptoAssets,
                amount: amount.clone(),
            },
            JournalLine {
                side: pnl_side,
                line: StatementLine::UnrealizedCryptoGainLoss,
                amount,
            },
        ],
        policy_hash,
    )?;
    Ok(Some(entry))
}

/// Disposal: Dr cash for proceeds, Cr crypto for the carrying amount
/// derecognized, plug to realized gain/loss (net income).
///
/// # Errors
/// [`EntryError`] on currency mismatch, overflow, or non-positive
/// proceeds/carrying.
pub fn disposal_entry(
    proceeds: &AssetAmount,
    carrying_relieved: &AssetAmount,
    policy_hash: ContentHash,
) -> Result<JournalEntry, EntryError> {
    if proceeds.asset() != carrying_relieved.asset() {
        return Err(EntryError::MixedCurrencies);
    }
    let currency = proceeds.asset().clone();
    let gain = proceeds
        .atoms()
        .checked_sub(carrying_relieved.atoms())
        .ok_or(EntryError::Overflow)?;
    let mut lines = vec![
        JournalLine {
            side: Side::Debit,
            line: StatementLine::Cash,
            amount: proceeds.clone(),
        },
        JournalLine {
            side: Side::Credit,
            line: StatementLine::CryptoAssets,
            amount: carrying_relieved.clone(),
        },
    ];
    if gain != 0 {
        let magnitude = gain.checked_abs().ok_or(EntryError::Overflow)?;
        let side = if gain > 0 { Side::Credit } else { Side::Debit };
        lines.push(JournalLine {
            side,
            line: StatementLine::RealizedCryptoGainLoss,
            amount: AssetAmount::new(currency, magnitude),
        });
    }
    JournalEntry::new("disposal", lines, policy_hash)
}

/// Bridge an entry into the ledger as an L4 policy output whose
/// provenance names the input events it was computed from.
///
/// # Errors
/// Propagates [`EntryError`] from hashing.
pub fn draft_policy_output(
    entry: &JournalEntry,
    tenant: TenantId,
    event_time: TimestampNs,
    inputs: Vec<ContentHash>,
) -> Result<EventDraft, EntryError> {
    let entry_hash = entry.entry_hash()?;
    let payload = json!({
        "schema": crate::entry::ENTRY_SCHEMA,
        "booking": "journal_entry",
        "kind": entry.kind.clone(),
        "lines": entry.lines.clone(),
        "entry": entry_hash.to_hex(),
    });
    Ok(EventDraft {
        tenant,
        layer: ClaimLayer::PolicyOutput,
        event_time,
        supersedes: None,
        provenance: Provenance::PolicyOutput {
            policy_hash: entry.policy_hash,
            inputs,
        },
        payload,
    })
}
