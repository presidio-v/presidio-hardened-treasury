//! The chain-agnostic, content-addressed normalized address history.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use treasury_core::{AssetAmount, ContentHash};
use treasury_evidence::{canonical_bytes, sha256, CanonError};

/// Schema tag committed into every history hash; bump on change.
pub const HISTORY_SCHEMA: &str = "treasury-chainsource/address-history/v1";

/// The chains Phase 1 ingests (ADR-0004).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Chain {
    /// Bitcoin (Core consensus; electrs / Fulcrum indexers).
    Bitcoin,
    /// Ethereum (reth / Erigon execution clients).
    Ethereum,
}

/// Direction of a movement relative to the address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Value arrived at the address.
    Inflow,
    /// Value left the address.
    Outflow,
}

/// One normalized movement touching an address. Integer-only money; the
/// raw indexer payload is hashed separately into the evidence store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainMovement {
    /// On-chain transaction reference (txid / tx hash).
    pub tx_ref: String,
    /// Inflow or outflow.
    pub direction: Direction,
    /// Amount in integer base units (no floats anywhere).
    pub amount: AssetAmount,
    /// Block height the movement was included at.
    pub block_height: u64,
}

/// A normalized address history up to a settled height. Two independent
/// sources producing the same settled history must produce the same
/// [`AddressHistory::history_hash`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressHistory {
    /// The chain.
    pub chain: Chain,
    /// The address (derived address; opaque here).
    pub address: String,
    /// Settled-to height: movements at or below this are final under the
    /// governing [`crate::FinalityPolicy`].
    pub settled_to_height: u64,
    /// Movements (any order on input; canonicalized for hashing).
    pub movements: Vec<ChainMovement>,
}

impl AddressHistory {
    /// Canonical ordering of movements: by (block height, tx ref,
    /// direction). Deterministic regardless of source ingestion order.
    fn ordered_movements(&self) -> Vec<&ChainMovement> {
        let mut ordered: Vec<&ChainMovement> = self.movements.iter().collect();
        ordered.sort_by(|a, b| {
            a.block_height
                .cmp(&b.block_height)
                .then_with(|| a.tx_ref.cmp(&b.tx_ref))
                .then_with(|| a.direction.cmp(&b.direction))
        });
        ordered
    }

    /// The history's content hash — the value two sources compare. Only
    /// movements at or below `settled_to_height` are included; unsettled
    /// (reorg-prone) movements never enter the hash.
    ///
    /// # Errors
    /// [`HistoryError::Canon`] when an amount cannot canonicalize
    /// (structurally unreachable: amounts are integer base units).
    pub fn history_hash(&self) -> Result<ContentHash, HistoryError> {
        let mut movements: Vec<Value> = Vec::new();
        for movement in self.ordered_movements() {
            if movement.block_height > self.settled_to_height {
                continue;
            }
            movements.push(json!({
                "tx_ref": movement.tx_ref.clone(),
                "direction": movement.direction,
                "amount": movement.amount.clone(),
                "block_height": movement.block_height.to_string(),
            }));
        }
        let envelope = json!({
            "schema": HISTORY_SCHEMA,
            "chain": self.chain,
            "address": self.address.clone(),
            "settled_to_height": self.settled_to_height.to_string(),
            "movements": movements,
        });
        let bytes = canonical_bytes(&envelope)?;
        Ok(sha256(&bytes))
    }
}

/// Errors from history operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum HistoryError {
    /// Envelope canonicalization failure.
    #[error(transparent)]
    Canon(#[from] CanonError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use treasury_core::AssetId;

    fn btc(atoms: i128) -> AssetAmount {
        AssetAmount::new(AssetId::new("BTC"), atoms)
    }

    fn movement(tx: &str, dir: Direction, atoms: i128, height: u64) -> ChainMovement {
        ChainMovement {
            tx_ref: tx.to_owned(),
            direction: dir,
            amount: btc(atoms),
            block_height: height,
        }
    }

    #[test]
    fn hash_is_order_independent() {
        let forward = AddressHistory {
            chain: Chain::Bitcoin,
            address: "bc1q-acme".to_owned(),
            settled_to_height: 100,
            movements: vec![
                movement("t1", Direction::Inflow, 500, 10),
                movement("t2", Direction::Outflow, 200, 20),
            ],
        };
        let reversed = AddressHistory {
            movements: vec![
                movement("t2", Direction::Outflow, 200, 20),
                movement("t1", Direction::Inflow, 500, 10),
            ],
            ..forward.clone()
        };
        assert_eq!(forward.history_hash(), reversed.history_hash());
    }

    #[test]
    fn unsettled_movements_are_excluded() {
        let base = AddressHistory {
            chain: Chain::Bitcoin,
            address: "bc1q-acme".to_owned(),
            settled_to_height: 100,
            movements: vec![movement("t1", Direction::Inflow, 500, 10)],
        };
        let mut with_unsettled = base.clone();
        with_unsettled
            .movements
            .push(movement("t9", Direction::Inflow, 999, 101));
        // The unsettled movement (height 101 > 100) does not change the hash.
        assert_eq!(base.history_hash(), with_unsettled.history_hash());
    }

    #[test]
    fn settled_movement_changes_the_hash() {
        let base = AddressHistory {
            chain: Chain::Bitcoin,
            address: "bc1q-acme".to_owned(),
            settled_to_height: 100,
            movements: vec![movement("t1", Direction::Inflow, 500, 10)],
        };
        let mut with_extra = base.clone();
        with_extra
            .movements
            .push(movement("t2", Direction::Outflow, 50, 90));
        assert_ne!(base.history_hash(), with_extra.history_hash());
    }

    /// Golden vector — independently recomputed in Python.
    #[test]
    fn golden_history_hash_matches_independent_implementation() {
        let history = AddressHistory {
            chain: Chain::Bitcoin,
            address: "bc1q-acme".to_owned(),
            settled_to_height: 100,
            movements: vec![
                movement("t2", Direction::Outflow, 200, 20),
                movement("t1", Direction::Inflow, 500, 10),
            ],
        };
        let hash = history.history_hash().map(|h| h.to_hex());
        assert_eq!(
            hash.as_deref(),
            Ok("07f7d3d8ad71bfa5b6f214266b54e3011595062a794885a845c86d2eb576669e"),
        );
    }
}
