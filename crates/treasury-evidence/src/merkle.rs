//! RFC 6962 Merkle Tree Head over evidence content hashes (spec v2 §3.3).
//!
//! Domain-separated hashing (`0x00` leaf prefix, `0x01` node prefix)
//! prevents second-preimage construction between leaves and interior
//! nodes. The tree head is the value that gets externally anchored
//! (public chain / RFC 3161 TSA), so its definition must never change:
//! it is RFC 6962 §2.1 verbatim, with each leaf's input being the 32
//! bytes of the evidence [`ContentHash`].

use sha2::{Digest, Sha256};
use treasury_core::ContentHash;

const LEAF_PREFIX: u8 = 0x00;
const NODE_PREFIX: u8 = 0x01;

/// Compute the RFC 6962 tree head over evidence hashes in insertion order.
///
/// The empty tree hashes to `SHA-256("")`, per RFC 6962.
#[must_use]
pub fn merkle_root(leaves: &[ContentHash]) -> ContentHash {
    match leaves {
        [] => {
            let digest = Sha256::digest([]);
            ContentHash(digest.into())
        }
        [single] => leaf_hash(single),
        many => {
            let k = largest_power_of_two_below(many.len());
            let (left, right) = many.split_at(k);
            node_hash(&merkle_root(left), &merkle_root(right))
        }
    }
}

fn leaf_hash(entry: &ContentHash) -> ContentHash {
    let mut hasher = Sha256::new();
    hasher.update([LEAF_PREFIX]);
    hasher.update(entry.as_bytes());
    ContentHash(hasher.finalize().into())
}

fn node_hash(left: &ContentHash, right: &ContentHash) -> ContentHash {
    let mut hasher = Sha256::new();
    hasher.update([NODE_PREFIX]);
    hasher.update(left.as_bytes());
    hasher.update(right.as_bytes());
    ContentHash(hasher.finalize().into())
}

/// Largest power of two strictly less than `n`. Only reachable with
/// `n >= 2` (the 0- and 1-leaf cases are handled before recursion).
fn largest_power_of_two_below(n: usize) -> usize {
    let mut k = 1usize;
    while k.saturating_mul(2) < n {
        k = k.saturating_mul(2);
    }
    k
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sha256;

    /// Leaves are `sha256([i])` for `i in 0..5`; expected heads computed by an
    /// independent Python implementation of RFC 6962 §2.1 (see repo history).
    fn leaves(n: usize) -> Vec<ContentHash> {
        (0..n).map(|i| sha256(&[u8::try_from(i).unwrap_or(0)])).collect()
    }

    #[test]
    fn empty_tree_is_sha256_of_empty_string() {
        assert_eq!(
            merkle_root(&[]).to_hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn single_leaf_golden() {
        assert_eq!(
            merkle_root(&leaves(1)).to_hex(),
            "d9de27625445003d8a9739a851e3ff8d41c0683630b4d63a88327a6aaa37c409"
        );
    }

    #[test]
    fn two_leaves_golden() {
        assert_eq!(
            merkle_root(&leaves(2)).to_hex(),
            "604d540f09268b91672ab011394d5266ccd7d4484d0d109411a55848126a1b2c"
        );
    }

    #[test]
    fn five_leaves_golden_unbalanced() {
        assert_eq!(
            merkle_root(&leaves(5)).to_hex(),
            "6b313b611b40676b9e1dfd70c4503f2379f88f0f1c2740fb7e1cacc32c113465"
        );
    }

    #[test]
    fn order_sensitivity() {
        let mut l = leaves(3);
        let head = merkle_root(&l);
        l.swap(0, 1);
        assert_ne!(merkle_root(&l), head, "tree head must commit to order");
    }
}
