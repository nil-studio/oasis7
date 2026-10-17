use super::super::tests::{MemoryNodes, digest, updates};
use super::*;

#[test]
fn v2_removes_intermediate_paths_and_preserves_root_and_proofs() {
    let nodes = MemoryNodes::default();
    let base = BusinessIndexRootV1::empty().unwrap();
    let changes = updates(1, &[(digest(1), 2), (digest(2), 3), (digest(3), 4)]);
    let old = BusinessIndexDeltaV1::build(base.clone(), changes.clone(), &nodes).unwrap();
    let compact = BusinessIndexDeltaV1::build_v2(base.clone(), changes.clone(), &nodes).unwrap();
    assert_eq!(compact.schema_version, BUSINESS_INDEX_SCHEMA_V2);
    assert_eq!(old.new_root, compact.new_root);
    assert_eq!(old.new, compact.new);
    assert!(compact.node_writes.len() < old.node_writes.len());
    assert_eq!(
        compact,
        BusinessIndexDeltaV1::build_v2(base, changes, &nodes).unwrap()
    );
    compact.verify(&compact.base_root, &nodes).unwrap();
    let mut committed = MemoryNodes::default();
    committed.persist(&compact.node_writes);
    for update in &compact.updates {
        assert_eq!(
            compact
                .new
                .get(&update.key_digest, &committed)
                .unwrap()
                .as_ref(),
            Some(update)
        );
        let proof = compact.new.prove(&update.key_digest, &committed).unwrap();
        let key = digest_bytes(&proof.leaf.key_digest).unwrap();
        let mut hash = BusinessIndexNodeV1::leaf(proof.leaf)
            .unwrap()
            .digest()
            .unwrap();
        for (level, sibling) in (0..BUSINESS_INDEX_DEPTH).rev().zip(proof.siblings) {
            let (left, right) = if key_bit(&key, level) {
                (sibling, hash)
            } else {
                (hash, sibling)
            };
            hash = BusinessIndexNodeV1::branch(level, left, right)
                .unwrap()
                .digest()
                .unwrap();
        }
        assert_eq!(hash, compact.new.node_root);
    }
    // Pretending a compact delta is v1 must never rewrite historical v1 bytes.
    let mut wrong_version = compact.clone();
    wrong_version.schema_version = BUSINESS_INDEX_SCHEMA_V1;
    assert!(wrong_version.verify(&compact.base_root, &nodes).is_err());
    old.verify(&old.base_root, &nodes).unwrap();
}

#[test]
fn v2_ignores_unrelated_cas_contents_and_omits_parent_nodes() {
    let mut nodes = MemoryNodes::default();
    let first = BusinessIndexDeltaV1::build(
        BusinessIndexRootV1::empty().unwrap(),
        updates(1, &[(digest(1), 2)]),
        &nodes,
    )
    .unwrap();
    nodes.persist(&first.node_writes);
    let noop =
        BusinessIndexDeltaV1::build_v2(first.new.clone(), first.updates.clone(), &nodes).unwrap();
    assert_eq!(noop.new, first.new);
    assert!(noop.node_writes.is_empty());
    noop.verify(&first.new_root, &nodes).unwrap();
    let changes = updates(3, &[(digest(2), 4)]);
    let second =
        BusinessIndexDeltaV1::build_v2(first.new.clone(), changes.clone(), &nodes).unwrap();
    nodes.persist(&second.node_writes);
    assert_eq!(
        second,
        BusinessIndexDeltaV1::build_v2(first.new.clone(), changes, &nodes).unwrap()
    );
    second.verify(&first.new_root, &nodes).unwrap();
    // Mixed v1 -> v2 -> v1 history uses the unchanged node/root codec.
    let third =
        BusinessIndexDeltaV1::build(second.new.clone(), updates(5, &[(digest(3), 6)]), &nodes)
            .unwrap();
    third.verify(&second.new_root, &nodes).unwrap();
    nodes.persist(&third.node_writes);
    assert!(third.new.prove(&digest(1), &nodes).is_ok());
}

#[test]
fn v2_verifier_rejects_missing_extra_reordered_corrupt_and_wrong_parent_writes() {
    let nodes = MemoryNodes::default();
    let base = BusinessIndexRootV1::empty().unwrap();
    let changes = updates(1, &[(digest(1), 2), (digest(2), 3)]);
    let old = BusinessIndexDeltaV1::build(base.clone(), changes.clone(), &nodes).unwrap();
    let delta = BusinessIndexDeltaV1::build_v2(base, changes, &nodes).unwrap();
    let mut missing = delta.clone();
    missing.node_writes.pop();
    assert!(missing.verify(&delta.base_root, &nodes).is_err());
    let mut extra = delta.clone();
    let old_only = old
        .node_writes
        .iter()
        .find(|write| {
            !extra
                .node_writes
                .iter()
                .any(|new| new.digest == write.digest)
        })
        .unwrap();
    extra.node_writes.push(old_only.clone());
    extra.node_writes.sort_by(|a, b| a.digest.cmp(&b.digest));
    assert!(extra.verify(&delta.base_root, &nodes).is_err());
    let mut duplicate = delta.clone();
    duplicate.node_writes.push(duplicate.node_writes[0].clone());
    assert!(duplicate.verify(&delta.base_root, &nodes).is_err());
    let mut reordered = delta.clone();
    reordered.node_writes.swap(0, 1);
    assert!(reordered.verify(&delta.base_root, &nodes).is_err());
    let mut corrupt = delta.clone();
    corrupt.node_writes[0].bytes.push(0);
    assert!(corrupt.verify(&delta.base_root, &nodes).is_err());
    assert!(delta.verify(&digest(99), &nodes).is_err());
    let mut unknown = delta.clone();
    unknown.schema_version = 3;
    assert!(unknown.verify(&delta.base_root, &nodes).is_err());
    let bytes = serde_cbor::to_vec(&delta).unwrap();
    let decoded: BusinessIndexDeltaV1 = serde_cbor::from_slice(&bytes).unwrap();
    assert_eq!(decoded, delta);
    decoded.verify(&delta.base_root, &nodes).unwrap();
}

/// Independent test oracle traverses entire small trees, unlike the producer's
/// bounded updated-path algorithm. It proves exact set difference, not a size
/// inequality or merely equality of the final root.
fn all_reachable(root: &BusinessIndexRootV1, nodes: &MemoryNodes) -> BTreeSet<String> {
    let empty = empty_hashes().unwrap();
    let mut result = BTreeSet::new();
    let mut stack = vec![(root.node_root.clone(), 0)];
    while let Some((hash, depth)) = stack.pop() {
        if hash == empty[depth] || !result.insert(hash.clone()) {
            continue;
        }
        let node = read_node(nodes, &hash, depth).unwrap().unwrap();
        if let BusinessIndexNodeKindV1::Branch {
            left_hash,
            right_hash,
            ..
        } = node.kind
        {
            stack.push((left_hash, depth + 1));
            stack.push((right_hash, depth + 1));
        }
    }
    result
}

#[test]
fn v2_exact_write_set_matches_independent_whole_tree_difference_across_mixed_history() {
    let mut nodes = MemoryNodes::default();
    let mut base = BusinessIndexRootV1::empty().unwrap();
    for round in 0u8..8 {
        let changes = updates(
            round,
            &[
                (digest(round * 2), round),
                (digest(round * 2 + 1), round + 1),
            ],
        );
        let parent = all_reachable(&base, &nodes);
        let delta = BusinessIndexDeltaV1::build_v2(base.clone(), changes.clone(), &nodes).unwrap();
        delta
            .verify(&base.canonical_result_root().unwrap(), &nodes)
            .unwrap();
        let v1 = BusinessIndexDeltaV1::build(base, changes, &nodes).unwrap();
        assert_eq!(delta.new_root, v1.new_root);
        // Alternate persisted history versions while checking the v2 set.
        let mut final_nodes = MemoryNodes::default();
        final_nodes.persist(&v1.node_writes);
        // Copy old dependencies without treating unrelated CAS as parent nodes.
        for hash in &parent {
            let bytes = nodes.read_node(hash).unwrap().unwrap();
            final_nodes.persist(&[BusinessIndexNodeWriteV1 {
                digest: hash.clone(),
                bytes,
            }]);
        }
        let final_set = all_reachable(&delta.new, &final_nodes);
        let expected: BTreeSet<_> = final_set.difference(&parent).cloned().collect();
        let actual: BTreeSet<_> = delta
            .node_writes
            .iter()
            .map(|write| write.digest.clone())
            .collect();
        assert_eq!(actual, expected);
        nodes.persist(if round % 2 == 0 {
            &delta.node_writes
        } else {
            &v1.node_writes
        });
        base = delta.new;
    }
}

#[test]
fn v2_bounds_input_before_tree_materialization() {
    let changes = (0u8..17)
        .map(|key| BusinessIndexLeafV1 {
            key_digest: digest(key),
            kind: BusinessIndexLeafKindV1::Outcome,
            canonical_value: vec![0; BUSINESS_INDEX_MAX_LEAF_BYTES],
        })
        .collect();
    assert!(
        BusinessIndexDeltaV1::build_v2(
            BusinessIndexRootV1::empty().unwrap(),
            changes,
            &MemoryNodes::default()
        )
        .unwrap_err()
        .contains("input byte bound")
    );
}
