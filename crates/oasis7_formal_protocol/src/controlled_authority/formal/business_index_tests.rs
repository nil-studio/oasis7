use super::*;
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct MemoryNodes(BTreeMap<String, Vec<u8>>);

impl BusinessIndexNodeReader for MemoryNodes {
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.get(digest).cloned())
    }
}

impl MemoryNodes {
    pub(super) fn persist(&mut self, writes: &[BusinessIndexNodeWriteV1]) {
        for write in writes {
            self.0.insert(write.digest.clone(), write.bytes.clone());
        }
    }
}

pub(super) fn digest(byte: u8) -> String {
    hex::encode([byte; 32])
}

fn leaf(key_digest: String, kind: BusinessIndexLeafKindV1, value: u8) -> BusinessIndexLeafV1 {
    BusinessIndexLeafV1 {
        key_digest,
        kind,
        canonical_value: serde_cbor::to_vec(&value).unwrap(),
    }
}

pub(super) fn updates(metadata: u8, outcomes: &[(String, u8)]) -> Vec<BusinessIndexLeafV1> {
    let mut leaves = vec![leaf(
        metadata_key_digest(),
        BusinessIndexLeafKindV1::CommitMetadata,
        metadata,
    )];
    leaves.extend(
        outcomes
            .iter()
            .map(|(key, value)| leaf(key.clone(), BusinessIndexLeafKindV1::Outcome, *value)),
    );
    leaves.sort_by(|left, right| left.key_digest.cmp(&right.key_digest));
    leaves
}

#[test]
fn sparse_delta_is_parent_bound_append_only_and_proves_exact_outcomes() {
    let mut nodes = MemoryNodes::default();
    let empty = BusinessIndexRootV1::empty().unwrap();
    let empty_root = empty.canonical_result_root().unwrap();
    let key = digest(7);
    let first =
        BusinessIndexDeltaV1::build(empty, updates(1, &[(key.clone(), 2)]), &nodes).unwrap();
    assert_eq!(first.base_root, empty_root);
    assert_ne!(first.new_root, first.base_root);
    assert!(first.node_writes.len() <= 2 * BUSINESS_INDEX_DEPTH + 2);
    assert_eq!(
        first
            .verify(&first.base_root, &nodes)
            .unwrap()
            .canonical_result_root()
            .unwrap(),
        first.new_root
    );
    assert!(first.verify(&digest(88), &nodes).is_err());

    nodes.persist(&first.node_writes);
    let proof = first.new.prove(&key, &nodes).unwrap();
    assert_eq!(proof.leaf.key_digest, key);
    assert_eq!(proof.leaf.kind, BusinessIndexLeafKindV1::Outcome);
    assert_eq!(
        proof.leaf.canonical_value,
        serde_cbor::to_vec(&2u8).unwrap()
    );
    assert_eq!(proof.siblings.len(), BUSINESS_INDEX_DEPTH);
    assert_eq!(proof.leaf_count, 2);

    let second =
        BusinessIndexDeltaV1::build(first.new.clone(), updates(3, &[(digest(9), 4)]), &nodes)
            .unwrap();
    assert!(second.node_writes.len() <= 2 * BUSINESS_INDEX_DEPTH + 2);
    assert_eq!(
        second
            .verify(&first.new_root, &nodes)
            .unwrap()
            .canonical_result_root()
            .unwrap(),
        second.new_root
    );
    nodes.persist(&second.node_writes);
    assert_eq!(
        second
            .new
            .get(&key, &nodes)
            .unwrap()
            .unwrap()
            .canonical_value,
        serde_cbor::to_vec(&2u8).unwrap()
    );
    assert_eq!(
        second
            .new
            .get(&digest(9), &nodes)
            .unwrap()
            .unwrap()
            .canonical_value,
        serde_cbor::to_vec(&4u8).unwrap()
    );
}

#[test]
fn sparse_delta_rejects_mutation_missing_parent_nodes_and_tampered_writes() {
    let mut nodes = MemoryNodes::default();
    let empty = BusinessIndexRootV1::empty().unwrap();
    let key = digest(11);
    let first =
        BusinessIndexDeltaV1::build(empty, updates(1, &[(key.clone(), 2)]), &nodes).unwrap();
    nodes.persist(&first.node_writes);

    assert!(
        BusinessIndexDeltaV1::build(first.new.clone(), updates(3, &[(key.clone(), 99)]), &nodes,)
            .is_err()
    );
    // The first transition has an empty parent and needs no persisted nodes.
    assert!(
        first
            .verify(&first.base_root, &MemoryNodes::default())
            .is_ok()
    );
    let second =
        BusinessIndexDeltaV1::build(first.new.clone(), updates(3, &[(digest(12), 4)]), &nodes)
            .unwrap();
    assert!(
        second
            .verify(&second.base_root, &MemoryNodes::default())
            .is_err()
    );

    let mut tampered = first.clone();
    tampered.updates[0].canonical_value = serde_cbor::to_vec(&99u8).unwrap();
    assert!(
        tampered
            .verify(&first.base_root, &MemoryNodes::default())
            .is_err()
    );
    let mut invalid_writes = first.clone();
    invalid_writes.node_writes[0].bytes.push(0);
    assert!(invalid_writes.validate_shape().is_err());
}

#[test]
fn node_encoding_is_content_addressed_and_strict() {
    let node =
        BusinessIndexNodeV1::leaf(leaf(digest(5), BusinessIndexLeafKindV1::Outcome, 9)).unwrap();
    let bytes = node.encode().unwrap();
    assert_eq!(node.digest().unwrap(), blake3_hex(&bytes));
    assert_eq!(BusinessIndexNodeV1::decode(&bytes).unwrap(), node);
    let mut trailing = bytes;
    trailing.push(0);
    assert!(BusinessIndexNodeV1::decode(&trailing).is_err());
}

#[test]
fn long_term_count_has_no_fixed_total_cap_but_overflow_and_one_delta_are_bounded() {
    let mut nodes = MemoryNodes::default();
    let initial = BusinessIndexDeltaV1::build(
        BusinessIndexRootV1::empty().unwrap(),
        updates(1, &[]),
        &nodes,
    )
    .unwrap();
    nodes.persist(&initial.node_writes);
    // Model an already authenticated historical count without constructing a
    // million irrelevant fixtures. Count remains part of the committed root.
    let mut high = initial.new.clone();
    high.leaf_count = 1_048_576;
    let next =
        BusinessIndexDeltaV1::build(high.clone(), updates(2, &[(digest(7), 3)]), &nodes).unwrap();
    assert_eq!(next.new.leaf_count, 1_048_577);
    next.verify(&high.canonical_result_root().unwrap(), &nodes)
        .unwrap();
    high.leaf_count = u64::MAX;
    assert!(
        BusinessIndexDeltaV1::build(high, updates(2, &[(digest(7), 3)]), &nodes)
            .unwrap_err()
            .contains("overflow")
    );
    let mut too_many = initial.clone();
    too_many.updates = vec![too_many.updates[0].clone(); BUSINESS_INDEX_MAX_UPDATES + 1];
    let encoded = serde_cbor::to_vec(&too_many).unwrap();
    assert!(
        serde_cbor::from_slice::<BusinessIndexDeltaV1>(&encoded)
            .unwrap_err()
            .to_string()
            .contains("entry bound")
    );
}
#[test]
fn generic_archive_delta_allows_no_fixed_metadata_and_multiple_mutable_metadata_keys() {
    let mut nodes = MemoryNodes::default();
    let first = BusinessIndexDeltaV1::build(
        BusinessIndexRootV1::empty().unwrap(),
        vec![leaf(digest(3), BusinessIndexLeafKindV1::Outcome, 1)],
        &nodes,
    )
    .unwrap();
    first.verify(&first.base_root, &nodes).unwrap();
    nodes.persist(&first.node_writes);
    let second = BusinessIndexDeltaV1::build(
        first.new.clone(),
        vec![
            leaf(digest(4), BusinessIndexLeafKindV1::CommitMetadata, 2),
            leaf(digest(5), BusinessIndexLeafKindV1::CommitMetadata, 3),
        ],
        &nodes,
    )
    .unwrap();
    second.verify(&first.new_root, &nodes).unwrap();
    nodes.persist(&second.node_writes);
    assert!(
        BusinessIndexDeltaV1::build(
            second.new.clone(),
            vec![leaf(digest(4), BusinessIndexLeafKindV1::CommitMetadata, 4)],
            &nodes
        )
        .is_ok()
    );
    assert!(
        BusinessIndexDeltaV1::build(
            second.new,
            vec![leaf(digest(3), BusinessIndexLeafKindV1::CommitMetadata, 9)],
            &nodes
        )
        .is_err()
    );
}

#[path = "business_index_iterative_tests.rs"]
mod iterative_tests;

#[test]
fn bounded_empty_subtree_table_matches_canonical_digest_at_every_depth() {
    let table = empty_subtree_hashes().unwrap();
    assert_eq!(table.len(), BUSINESS_INDEX_DEPTH + 1);
    assert_eq!(table[0], BusinessIndexRootV1::empty().unwrap().node_root);
    for (depth, hash) in table.iter().enumerate() {
        assert_eq!(
            hash,
            &BusinessIndexRootV1::empty_subtree_hash(depth).unwrap()
        );
        if depth < BUSINESS_INDEX_DEPTH {
            let branch = BusinessIndexNodeV1::branch(
                depth,
                table[depth + 1].clone(),
                table[depth + 1].clone(),
            )
            .unwrap();
            assert_eq!(hash, &blake3_hex(&branch.encode().unwrap()));
            assert_ne!(
                hash,
                &table[depth + 1],
                "depth context cannot be interchanged"
            );
        }
    }
    assert!(BusinessIndexRootV1::empty_subtree_hash(BUSINESS_INDEX_DEPTH + 1).is_err());
    assert!(table.get(BUSINESS_INDEX_DEPTH + 1).is_none());
}
