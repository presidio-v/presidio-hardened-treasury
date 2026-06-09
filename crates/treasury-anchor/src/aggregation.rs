//! Merkle aggregation of tree heads for a single on-chain commitment
//! (spec v2 §3.3, ADR-0002).
//!
//! OpenTimestamps-style: many sealed evidence-store tree heads are
//! aggregated into one RFC 6962 tree, and only the **aggregation root**
//! is committed in a single Bitcoin transaction. Each head carries an
//! inclusion proof (audit path) into that root, so one transaction
//! anchors arbitrarily many heads at negligible per-head cost while each
//! head's membership remains independently verifiable.
//!
//! The aggregation root is, by construction, identical to
//! [`treasury_evidence::merkle_root`] over the same heads — the two
//! cannot drift, since both are RFC 6962 §2.1 over the 32-byte heads as
//! leaf entries. This module adds the audit paths `merkle_root` does
//! not return.

use treasury_core::ContentHash;
use treasury_evidence::{merkle_root, sha256};

const LEAF_PREFIX: u8 = 0x00;
const NODE_PREFIX: u8 = 0x01;

/// An RFC 6962 inclusion proof: the audit path proving one head is the
/// `leaf_index`-th leaf of a tree of `tree_size` leaves with a known
/// root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InclusionProof {
    /// Index of the proven head among the aggregated heads.
    pub leaf_index: usize,
    /// Total number of aggregated heads.
    pub tree_size: usize,
    /// The audit path (sibling hashes bottom to top).
    pub audit_path: Vec<ContentHash>,
}

/// The result of aggregating heads: the root to commit on-chain, and one
/// inclusion proof per input head in input order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aggregation {
    /// The aggregation root — the value committed in one transaction.
    pub root: ContentHash,
    /// Inclusion proofs, one per head, in input order.
    pub proofs: Vec<InclusionProof>,
}

/// Aggregate heads into a single committable root plus per-head proofs.
/// Returns `None` for an empty input (nothing to anchor).
#[must_use]
pub fn aggregate(heads: &[ContentHash]) -> Option<Aggregation> {
    if heads.is_empty() {
        return None;
    }
    let root = merkle_root(heads);
    let proofs = (0..heads.len())
        .map(|index| InclusionProof {
            leaf_index: index,
            tree_size: heads.len(),
            audit_path: audit_path(index, heads),
        })
        .collect();
    Some(Aggregation { root, proofs })
}

/// Verify that `head` is included in `root` via `proof` (RFC 6962 §2.1.1
/// reconstruction). A wrong head, index, or path fails.
#[must_use]
pub fn verify_inclusion(head: &ContentHash, proof: &InclusionProof, root: &ContentHash) -> bool {
    if proof.leaf_index >= proof.tree_size {
        return false;
    }
    let mut fn_idx = proof.leaf_index;
    let mut sn = proof.tree_size.saturating_sub(1);
    let mut hash = leaf_hash(head);
    for sibling in &proof.audit_path {
        if sn == 0 {
            return false;
        }
        if fn_idx & 1 == 1 || fn_idx == sn {
            hash = node_hash(sibling, &hash);
            while fn_idx != 0 && fn_idx & 1 == 0 {
                fn_idx >>= 1;
                sn >>= 1;
            }
        } else {
            hash = node_hash(&hash, sibling);
        }
        fn_idx >>= 1;
        sn >>= 1;
    }
    sn == 0 && &hash == root
}

/// RFC 6962 audit path PATH(m, D).
fn audit_path(m: usize, entries: &[ContentHash]) -> Vec<ContentHash> {
    let n = entries.len();
    if n <= 1 {
        return Vec::new();
    }
    let k = largest_power_of_two_below(n);
    let Some((left, right)) = split(entries, k) else {
        return Vec::new();
    };
    if m < k {
        let mut path = audit_path(m, left);
        path.push(subtree_root(right));
        path
    } else {
        let mut path = audit_path(m.saturating_sub(k), right);
        path.push(subtree_root(left));
        path
    }
}

fn subtree_root(entries: &[ContentHash]) -> ContentHash {
    merkle_root(entries)
}

fn split(entries: &[ContentHash], k: usize) -> Option<(&[ContentHash], &[ContentHash])> {
    let left = entries.get(..k)?;
    let right = entries.get(k..)?;
    Some((left, right))
}

fn leaf_hash(entry: &ContentHash) -> ContentHash {
    let mut bytes = Vec::with_capacity(33);
    bytes.push(LEAF_PREFIX);
    bytes.extend_from_slice(entry.as_bytes());
    sha256(&bytes)
}

fn node_hash(left: &ContentHash, right: &ContentHash) -> ContentHash {
    let mut bytes = Vec::with_capacity(65);
    bytes.push(NODE_PREFIX);
    bytes.extend_from_slice(left.as_bytes());
    bytes.extend_from_slice(right.as_bytes());
    sha256(&bytes)
}

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

    fn heads(n: usize) -> Vec<ContentHash> {
        (0..n)
            .map(|i| sha256(&[u8::try_from(i).unwrap_or(0)]))
            .collect()
    }

    #[test]
    fn root_matches_evidence_merkle_root() {
        let h = heads(5);
        let Some(agg) = aggregate(&h) else {
            unreachable!("non-empty");
        };
        assert_eq!(
            agg.root,
            merkle_root(&h),
            "aggregation must not drift from merkle_root"
        );
        // Golden (shared with treasury-evidence merkle tests).
        assert_eq!(
            agg.root.to_hex(),
            "6b313b611b40676b9e1dfd70c4503f2379f88f0f1c2740fb7e1cacc32c113465"
        );
    }

    #[test]
    fn every_head_verifies_in_its_aggregation() {
        let h = heads(7);
        let Some(agg) = aggregate(&h) else {
            unreachable!("non-empty");
        };
        for (index, head) in h.iter().enumerate() {
            let Some(proof) = agg.proofs.get(index) else {
                unreachable!("one proof per head");
            };
            assert!(
                verify_inclusion(head, proof, &agg.root),
                "head {index} must verify"
            );
        }
    }

    #[test]
    fn golden_audit_path_for_leaf_2_of_5() {
        let h = heads(5);
        let Some(agg) = aggregate(&h) else {
            unreachable!("non-empty");
        };
        let Some(proof) = agg.proofs.get(2) else {
            unreachable!("proof exists");
        };
        let path: Vec<String> = proof.audit_path.iter().map(ContentHash::to_hex).collect();
        assert_eq!(
            path,
            vec![
                "36e4970e7c84e559ed7290304c3c0672289a6918e7746d48397b0e122db7748f".to_owned(),
                "604d540f09268b91672ab011394d5266ccd7d4484d0d109411a55848126a1b2c".to_owned(),
                "12d24297164ffddfd8febbd02275c3c5ff24916dc71d5f3476931480854b3113".to_owned(),
            ]
        );
    }

    #[test]
    fn wrong_head_fails_verification() {
        let h = heads(5);
        let Some(agg) = aggregate(&h) else {
            unreachable!("non-empty");
        };
        let Some(proof) = agg.proofs.get(2) else {
            unreachable!("proof exists");
        };
        let imposter = sha256(&[9]);
        assert!(!verify_inclusion(&imposter, proof, &agg.root));
    }

    #[test]
    fn single_head_has_empty_path_and_verifies() {
        let h = heads(1);
        let Some(agg) = aggregate(&h) else {
            unreachable!("non-empty");
        };
        let Some(proof) = agg.proofs.first() else {
            unreachable!("one proof");
        };
        assert!(proof.audit_path.is_empty());
        let Some(only) = h.first() else {
            unreachable!("one head");
        };
        assert!(verify_inclusion(only, proof, &agg.root));
    }

    #[test]
    fn empty_input_aggregates_to_nothing() {
        assert_eq!(aggregate(&[]), None);
    }
}
