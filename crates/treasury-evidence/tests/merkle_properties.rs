//! Property tests (proptest): the evidence store's RFC 6962 tree heads
//! behave as anchoring requires. The headline invariant is anchored-prefix
//! immutability — once `entry_count` entries exist, their tree head can
//! never change under later appends, which is what makes a published anchor
//! tamper-evident.

use proptest::prelude::*;
use treasury_core::ContentHash;
use treasury_evidence::{merkle_root, sha256, EvidenceStore, InMemoryEvidenceStore};

proptest! {
    /// The tree head over the first `i` entries is invariant under any
    /// later appends. An auditor who anchored `tree_head_at(i)` can always
    /// recompute the same value; the operator cannot rewrite an anchored
    /// prefix.
    #[test]
    fn anchored_prefix_is_immutable_under_later_appends(
        first in prop::collection::vec(any::<u64>(), 0..30),
        more in prop::collection::vec(any::<u64>(), 0..30),
    ) {
        let mut store = InMemoryEvidenceStore::new();
        for n in &first {
            let _ = store.put(&n.to_le_bytes());
        }
        let k = store.len();
        let heads: Vec<Option<ContentHash>> = (0..=k).map(|i| store.tree_head_at(i)).collect();
        for n in &more {
            let _ = store.put(&n.to_le_bytes());
        }
        for (i, expected) in heads.iter().enumerate() {
            prop_assert_eq!(store.tree_head_at(i), *expected);
        }
    }

    /// The Merkle root is deterministic and commits to leaf order: a
    /// different ordering yields a different head (equal only for a
    /// palindrome).
    #[test]
    fn merkle_root_is_deterministic_and_order_committing(
        bytes in prop::collection::vec(any::<u8>(), 2..16),
    ) {
        let leaves: Vec<ContentHash> = bytes.iter().map(|b| sha256(&[*b])).collect();
        let root = merkle_root(&leaves);
        prop_assert_eq!(merkle_root(&leaves), root);
        let mut reversed = leaves.clone();
        reversed.reverse();
        if reversed != leaves {
            prop_assert_ne!(merkle_root(&reversed), root);
        }
    }
}
