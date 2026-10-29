//! The previous recursive algorithm is a test-only compatibility oracle.
use super::*;

// Keep the old test oracle independent of the shared production updater.
struct Updater<'a, R> {
    reader: &'a R,
    pending: BTreeMap<String, Vec<u8>>,
    empty: &'a [String],
}

impl<R: BusinessIndexNodeReader> Updater<'_, R> {
    fn read_node(&self, digest: &str, depth: usize) -> Result<Option<BusinessIndexNodeV1>, String> {
        if digest == self.empty[depth] {
            return Ok(None);
        }
        if let Some(bytes) = self.pending.get(digest) {
            return BusinessIndexNodeV1::decode(bytes).map(Some);
        }
        read_node(self.reader, digest, depth)
    }
}

fn recursive_insert<R: BusinessIndexNodeReader>(
    updater: &mut Updater<'_, R>,
    root: &str,
    depth: usize,
    leaf: &BusinessIndexLeafV1,
) -> Result<(String, bool), String> {
    if depth == BUSINESS_INDEX_DEPTH {
        let existing = updater.read_node(root, depth)?;
        let inserted = match existing.as_ref().map(|node| &node.kind) {
            Some(BusinessIndexNodeKindV1::Leaf(old)) if old.key_digest == leaf.key_digest => false,
            Some(_) => return Err("reference leaf collision".into()),
            None => true,
        };
        let bytes = BusinessIndexNodeV1::leaf(leaf.clone())?.encode()?;
        let digest = blake3_hex(&bytes);
        updater.pending.insert(digest.clone(), bytes);
        return Ok((digest, inserted));
    }
    let (mut left, mut right) = match updater.read_node(root, depth)? {
        Some(node) => match node.kind {
            BusinessIndexNodeKindV1::Branch {
                left_hash,
                right_hash,
                ..
            } => (left_hash, right_hash),
            _ => return Err("reference early leaf".into()),
        },
        None => (
            updater.empty[depth + 1].clone(),
            updater.empty[depth + 1].clone(),
        ),
    };
    let key = digest_bytes(&leaf.key_digest)?;
    let (next, inserted) = recursive_insert(
        updater,
        if key_bit(&key, depth) { &right } else { &left },
        depth + 1,
        leaf,
    )?;
    if key_bit(&key, depth) {
        right = next;
    } else {
        left = next;
    }
    let bytes = BusinessIndexNodeV1::branch(depth, left, right)?.encode()?;
    let digest = blake3_hex(&bytes);
    updater.pending.insert(digest.clone(), bytes);
    Ok((digest, inserted))
}
fn reference_delta(
    base: BusinessIndexRootV1,
    updates: Vec<BusinessIndexLeafV1>,
    nodes: &MemoryNodes,
) -> BusinessIndexDeltaV1 {
    let empty = empty_hashes().unwrap();
    let mut updater = Updater {
        reader: nodes,
        pending: BTreeMap::new(),
        empty: &empty,
    };
    let mut root = base.node_root.clone();
    let mut leaf_count = base.leaf_count;
    for leaf in &updates {
        let (next, inserted) = recursive_insert(&mut updater, &root, 0, leaf).unwrap();
        root = next;
        if inserted {
            leaf_count += 1;
        }
    }
    let new = BusinessIndexRootV1 {
        node_root: root,
        leaf_count,
    };
    BusinessIndexDeltaV1 {
        schema_version: BUSINESS_INDEX_SCHEMA_V1,
        base_root: base.canonical_result_root().unwrap(),
        base,
        new_root: new.canonical_result_root().unwrap(),
        new,
        updates,
        node_writes: updater
            .pending
            .into_iter()
            .map(|(digest, bytes)| BusinessIndexNodeWriteV1 { digest, bytes })
            .collect(),
    }
}

#[test]
fn sparse_update_on_two_mib_stack_matches_recursive_canonical_bytes() {
    let first_updates = updates(1, &(1..=24).map(|n| (digest(n), n)).collect::<Vec<_>>());
    let second_updates = updates(2, &[(digest(25), 25), (digest(26), 26)]);
    let first_input = first_updates.clone();
    let second_input = second_updates.clone();
    // Only the compatibility oracle receives a large stack. The production
    // build and re-verification below run at the actual worker-stack budget.
    let (first_reference, second_reference) = std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            let mut nodes = MemoryNodes::default();
            let first = reference_delta(BusinessIndexRootV1::empty().unwrap(), first_input, &nodes);
            nodes.persist(&first.node_writes);
            let second = reference_delta(first.new.clone(), second_input, &nodes);
            (first, second)
        })
        .unwrap()
        .join()
        .unwrap();
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            let mut nodes = MemoryNodes::default();
            let first = BusinessIndexDeltaV1::build(
                BusinessIndexRootV1::empty().unwrap(),
                first_updates,
                &nodes,
            )
            .unwrap();
            assert_eq!(first, first_reference);
            assert_eq!(
                serde_cbor::to_vec(&first).unwrap(),
                serde_cbor::to_vec(&first_reference).unwrap()
            );
            first.verify(&first.base_root, &nodes).unwrap();
            nodes.persist(&first.node_writes);
            let second =
                BusinessIndexDeltaV1::build(first.new, second_updates.clone(), &nodes).unwrap();
            assert_eq!(second, second_reference);
            assert_eq!(
                serde_cbor::to_vec(&second).unwrap(),
                serde_cbor::to_vec(&second_reference).unwrap()
            );
            second.verify(&second.base_root, &nodes).unwrap();
            nodes.persist(&second.node_writes);
            let repeat =
                BusinessIndexDeltaV1::build(second.new.clone(), second_updates, &nodes).unwrap();
            assert_eq!(repeat.new, second.new);
            repeat.verify(&repeat.base_root, &nodes).unwrap();
            for n in 1..=26 {
                assert!(second.new.prove(&digest(n), &nodes).is_ok());
            }
            let mut corrupt = nodes;
            corrupt.0.get_mut(&second.new.node_root).unwrap().push(0);
            assert!(BusinessIndexDeltaV1::build(second.new, updates(3, &[]), &corrupt).is_err());
        })
        .unwrap()
        .join()
        .unwrap();
}
