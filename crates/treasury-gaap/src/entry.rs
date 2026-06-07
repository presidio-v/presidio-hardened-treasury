//! The journal-entry model: statement-line targets and structural
//! balance.

use serde::{Deserialize, Serialize};
use serde_json::json;
use treasury_core::{AssetAmount, AssetId, ContentHash};
use treasury_evidence::{canonical_bytes, sha256, CanonError};

/// Schema tag committed into every entry hash; bump on change.
pub const ENTRY_SCHEMA: &str = "treasury-gaap/journal-entry/v1";

/// Which financial statement a line lands on (the R11 routing target).
/// GAAP never emits `Oci` for crypto marks; the variant exists because
/// the *contract* is shared with the IFRS module (IAS 38 revaluation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Statement {
    /// Balance sheet.
    BalanceSheet,
    /// Income statement (net income).
    IncomeStatement,
    /// Other comprehensive income (IFRS lane; unused by GAAP crypto).
    Oci,
}

/// A statement line target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatementLine {
    /// Crypto assets at fair value (ASC 350-60 presentation).
    CryptoAssets,
    /// Unrealized fair-value change on crypto (ASU 2023-08 → NI).
    UnrealizedCryptoGainLoss,
    /// Realized gain/loss on crypto disposal.
    RealizedCryptoGainLoss,
    /// Transaction costs expensed as incurred (fee election: expense).
    TransactionCostsExpense,
    /// Cash / cash equivalents.
    Cash,
}

impl StatementLine {
    /// The statement this line belongs to under GAAP.
    #[must_use]
    pub fn statement(&self) -> Statement {
        match self {
            Self::CryptoAssets | Self::Cash => Statement::BalanceSheet,
            Self::UnrealizedCryptoGainLoss
            | Self::RealizedCryptoGainLoss
            | Self::TransactionCostsExpense => Statement::IncomeStatement,
        }
    }
}

/// Debit or credit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// Debit.
    Debit,
    /// Credit.
    Credit,
}

/// One line: a side, a target, an amount (always positive minor units).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalLine {
    /// Debit or credit.
    pub side: Side,
    /// Statement-line target.
    pub line: StatementLine,
    /// Positive amount in reporting-currency minor units.
    pub amount: AssetAmount,
}

/// A balanced, content-addressed journal entry. Constructible only
/// through [`JournalEntry::new`], which enforces balance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntry {
    /// What the entry records, e.g. `"asu2023_08_remeasurement"`.
    pub kind: String,
    /// The lines.
    pub lines: Vec<JournalLine>,
    /// Hash of the policy-module version that produced the entry.
    pub policy_hash: ContentHash,
}

impl JournalEntry {
    /// Construct, enforcing structural balance: at least two lines, all
    /// amounts positive and in one currency, Σdebits == Σcredits.
    ///
    /// # Errors
    /// See [`EntryError`].
    pub fn new(
        kind: impl Into<String>,
        lines: Vec<JournalLine>,
        policy_hash: ContentHash,
    ) -> Result<Self, EntryError> {
        if lines.len() < 2 {
            return Err(EntryError::TooFewLines);
        }
        let Some(first) = lines.first() else {
            return Err(EntryError::TooFewLines);
        };
        let currency: AssetId = first.amount.asset().clone();
        let mut debits: i128 = 0;
        let mut credits: i128 = 0;
        for line in &lines {
            if line.amount.asset() != &currency {
                return Err(EntryError::MixedCurrencies);
            }
            let amount = line.amount.atoms();
            if amount <= 0 {
                return Err(EntryError::NonPositiveAmount(amount));
            }
            match line.side {
                Side::Debit => {
                    debits = debits.checked_add(amount).ok_or(EntryError::Overflow)?;
                }
                Side::Credit => {
                    credits = credits.checked_add(amount).ok_or(EntryError::Overflow)?;
                }
            }
        }
        if debits != credits {
            return Err(EntryError::Unbalanced { debits, credits });
        }
        Ok(Self {
            kind: kind.into(),
            lines,
            policy_hash,
        })
    }

    /// The entry's content hash.
    ///
    /// # Errors
    /// [`EntryError::Canon`] on envelope failure (structurally
    /// unreachable).
    pub fn entry_hash(&self) -> Result<ContentHash, EntryError> {
        let envelope = json!({
            "schema": ENTRY_SCHEMA,
            "kind": self.kind.clone(),
            "lines": self.lines.clone(),
            "policy": self.policy_hash.to_hex(),
        });
        let bytes = canonical_bytes(&envelope)?;
        Ok(sha256(&bytes))
    }
}

/// Errors constructing entries.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EntryError {
    /// An entry needs at least a debit and a credit.
    #[error("journal entry needs at least two lines")]
    TooFewLines,
    /// All lines must share one currency.
    #[error("mixed currencies in one entry")]
    MixedCurrencies,
    /// Line amounts are strictly positive (direction lives in `Side`).
    #[error("non-positive line amount: {0}")]
    NonPositiveAmount(i128),
    /// Debits must equal credits — structurally.
    #[error("unbalanced entry: debits {debits}, credits {credits}")]
    Unbalanced {
        /// Sum of debit amounts.
        debits: i128,
        /// Sum of credit amounts.
        credits: i128,
    },
    /// 128-bit overflow.
    #[error("arithmetic overflow")]
    Overflow,
    /// Envelope canonicalization failure.
    #[error(transparent)]
    Canon(#[from] CanonError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usd(minor: i128) -> AssetAmount {
        AssetAmount::new(AssetId::new("USD"), minor)
    }

    fn line(side: Side, target: StatementLine, minor: i128) -> JournalLine {
        JournalLine {
            side,
            line: target,
            amount: usd(minor),
        }
    }

    #[test]
    fn unbalanced_entry_cannot_be_constructed() {
        let result = JournalEntry::new(
            "broken",
            vec![
                line(Side::Debit, StatementLine::CryptoAssets, 100),
                line(Side::Credit, StatementLine::Cash, 99),
            ],
            ContentHash([1; 32]),
        );
        assert_eq!(
            result,
            Err(EntryError::Unbalanced {
                debits: 100,
                credits: 99,
            })
        );
    }

    #[test]
    fn statement_routing_is_typed() {
        assert_eq!(
            StatementLine::UnrealizedCryptoGainLoss.statement(),
            Statement::IncomeStatement
        );
        assert_eq!(
            StatementLine::CryptoAssets.statement(),
            Statement::BalanceSheet
        );
    }

    /// Golden vector — independently recomputed in Python.
    #[test]
    fn golden_hash_matches_independent_implementation() {
        let Ok(entry) = JournalEntry::new(
            "asu2023_08_remeasurement",
            vec![
                line(Side::Debit, StatementLine::CryptoAssets, 30),
                line(Side::Credit, StatementLine::UnrealizedCryptoGainLoss, 30),
            ],
            ContentHash([9; 32]),
        ) else {
            unreachable!("entry is balanced");
        };
        let hash = entry.entry_hash().map(|h| h.to_hex());
        assert_eq!(
            hash.as_deref(),
            Ok("192b68f0ca2c6537238f0827bd356b81b8c1087dae35d620b531ca2bc0ecd015")
        );
    }
}
