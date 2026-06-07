//! GAAP module flow: NI routing, fee elections, balanced entries,
//! L4 booking (spec v2 §4, R11).

use treasury_core::{AssetAmount, AssetId, ContentHash, TenantId, TimestampNs};
use treasury_gaap::{
    acquisition_entry, disposal_entry, draft_policy_output, remeasurement_entry, FeeTreatment,
    Side, Statement, StatementLine,
};
use treasury_ledger::{ClaimLayer, InMemoryLedger, Ledger};

fn usd(minor: i128) -> AssetAmount {
    AssetAmount::new(AssetId::new("USD"), minor)
}

fn policy() -> ContentHash {
    ContentHash([9; 32])
}

#[test]
fn gain_remeasurement_routes_to_net_income() {
    let Ok(Some(entry)) = remeasurement_entry(&usd(70), &usd(100), policy()) else {
        unreachable!("non-zero mark must produce an entry");
    };
    // Dr CryptoAssets 30 / Cr UnrealizedCryptoGainLoss 30 — and the
    // credit line is income statement, by type.
    assert_eq!(entry.lines.len(), 2);
    let Some(pnl) = entry
        .lines
        .iter()
        .find(|l| l.line == StatementLine::UnrealizedCryptoGainLoss)
    else {
        unreachable!("P&L line exists");
    };
    assert_eq!(pnl.side, Side::Credit);
    assert_eq!(pnl.amount, usd(30));
    assert_eq!(pnl.line.statement(), Statement::IncomeStatement);
}

#[test]
fn loss_remeasurement_flips_sides() {
    let Ok(Some(entry)) = remeasurement_entry(&usd(100), &usd(70), policy()) else {
        unreachable!("non-zero mark must produce an entry");
    };
    let Some(pnl) = entry
        .lines
        .iter()
        .find(|l| l.line == StatementLine::UnrealizedCryptoGainLoss)
    else {
        unreachable!("P&L line exists");
    };
    assert_eq!(pnl.side, Side::Debit);
    assert_eq!(pnl.amount, usd(30));
}

#[test]
fn zero_mark_books_nothing() {
    assert_eq!(remeasurement_entry(&usd(100), &usd(100), policy()), Ok(None));
}

#[test]
fn fee_election_expense_hits_income_statement() {
    let Ok(entry) = acquisition_entry(&usd(1_000), &usd(50), FeeTreatment::Expense, policy())
    else {
        unreachable!("entry must build");
    };
    let Some(fee_line) = entry
        .lines
        .iter()
        .find(|l| l.line == StatementLine::TransactionCostsExpense)
    else {
        unreachable!("expensed fee line exists");
    };
    assert_eq!(fee_line.amount, usd(50));
    // Cash credit covers the full outlay either way.
    let Some(cash) = entry.lines.iter().find(|l| l.line == StatementLine::Cash) else {
        unreachable!("cash line exists");
    };
    assert_eq!(cash.amount, usd(1_050));
}

#[test]
fn fee_election_capitalize_lands_in_carrying_amount() {
    let Ok(entry) = acquisition_entry(&usd(1_000), &usd(50), FeeTreatment::Capitalize, policy())
    else {
        unreachable!("entry must build");
    };
    assert_eq!(entry.lines.len(), 2);
    let Some(asset) = entry
        .lines
        .iter()
        .find(|l| l.line == StatementLine::CryptoAssets)
    else {
        unreachable!("asset line exists");
    };
    assert_eq!(asset.amount, usd(1_050));
}

#[test]
fn disposal_plugs_realized_gain() {
    let Ok(entry) = disposal_entry(&usd(1_200), &usd(1_000), policy()) else {
        unreachable!("entry must build");
    };
    let Some(gain) = entry
        .lines
        .iter()
        .find(|l| l.line == StatementLine::RealizedCryptoGainLoss)
    else {
        unreachable!("gain line exists");
    };
    assert_eq!(gain.side, Side::Credit);
    assert_eq!(gain.amount, usd(200));
    assert_eq!(gain.line.statement(), Statement::IncomeStatement);
}

#[test]
fn entries_book_as_l4_policy_outputs() {
    let Ok(Some(entry)) = remeasurement_entry(&usd(70), &usd(100), policy()) else {
        unreachable!("entry must build");
    };
    let Ok(draft) = draft_policy_output(
        &entry,
        TenantId::new("acme"),
        TimestampNs::from_nanos(50),
        vec![ContentHash([1; 32]), ContentHash([2; 32])],
    ) else {
        unreachable!("draft must build");
    };
    assert_eq!(draft.layer, ClaimLayer::PolicyOutput);

    let mut ledger = InMemoryLedger::new();
    assert!(ledger.append(draft, TimestampNs::from_nanos(100)).is_ok());
    assert_eq!(ledger.verify_chain(&TenantId::new("acme")), Ok(()));
}
