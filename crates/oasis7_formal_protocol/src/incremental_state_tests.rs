use super::*;
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

const QUOTA: usize = 16 * 1024 * 1024;

#[derive(Default)]
struct Pages {
    nodes: BTreeMap<String, Vec<u8>>,
    reads: Cell<usize>,
}
impl MerkleNodeReader for Pages {
    type Node = Vec<u8>;
    fn read_node(&self, digest: &str, max_bytes: usize) -> Result<Option<Vec<u8>>, String> {
        self.reads.set(self.reads.get() + 1);
        assert_eq!(max_bytes, STATE_MAX_PAGE_BYTES);
        if self
            .nodes
            .get(digest)
            .is_some_and(|bytes| bytes.len() > max_bytes)
        {
            return Err("page source byte bound".into());
        }
        Ok(self.nodes.get(digest).cloned())
    }
}
impl Pages {
    fn persist(&mut self, delta: &StateDeltaV1) {
        self.nodes.extend(
            delta
                .new_nodes
                .iter()
                .map(|n| (n.digest.clone(), n.bytes.clone())),
        );
    }
}

fn write(key: &[u8], before: Option<u8>, after: Option<u8>) -> StateWriteV1 {
    StateWriteV1 {
        key: key.to_vec(),
        before: before.map(|v| [v; 32]),
        after: after.map(|v| [v; 32]),
    }
}

/// Independent, full-rebuild oracle: start from the expected logical map,
/// encode leaves directly from the profile, then reduce a complete level at
/// a time. No production write-set, traversal, codec or updater supplies roots.
fn reference(
    namespace: StateNamespaceV1,
    values: &BTreeMap<Vec<u8>, [u8; 32]>,
) -> (String, BTreeSet<String>) {
    fn hash(bytes: &[u8]) -> String {
        blake3::hash(bytes).to_hex().to_string()
    }
    fn branch(depth: usize, left: &str, right: &str) -> Vec<u8> {
        let mut bytes = b"oasis7.iws.state-page.v1\0".to_vec();
        bytes.extend_from_slice(&67u32.to_be_bytes());
        bytes.push(1);
        bytes.extend_from_slice(&(depth as u16).to_be_bytes());
        bytes.extend(hex::decode(left).unwrap());
        bytes.extend(hex::decode(right).unwrap());
        bytes
    }
    let mut empty = vec![String::new(); 257];
    empty[256] = hash(b"oasis7.iws.state-page.v1\0\x00\x00\x00\x01\x02");
    for depth in (0..256).rev() {
        empty[depth] = hash(&branch(depth, &empty[depth + 1], &empty[depth + 1]));
    }
    let mut level = BTreeMap::new();
    let mut all_nodes = BTreeSet::new();
    for (key, value) in values {
        let mut key_bytes = b"oasis7.iws.state-key.v1\0".to_vec();
        key_bytes.extend_from_slice(&(namespace as u16).to_be_bytes());
        key_bytes.extend_from_slice(&(key.len() as u16).to_be_bytes());
        key_bytes.extend(key);
        let path = *blake3::hash(&key_bytes).as_bytes();
        let mut bytes = b"oasis7.iws.state-page.v1\0".to_vec();
        bytes.extend_from_slice(&((37 + key.len()) as u32).to_be_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&(namespace as u16).to_be_bytes());
        bytes.extend_from_slice(&(key.len() as u16).to_be_bytes());
        bytes.extend(key);
        bytes.extend(value);
        let digest = hash(&bytes);
        all_nodes.insert(digest.clone());
        level.insert(path, digest);
    }
    for depth in (0..256).rev() {
        let mut parents: BTreeMap<[u8; 32], (Option<String>, Option<String>)> = BTreeMap::new();
        for (mut path, digest) in level {
            let bit = 0x80 >> (depth % 8);
            let right = path[depth / 8] & bit != 0;
            path[depth / 8] &= !bit;
            let children = parents.entry(path).or_default();
            if right {
                children.1 = Some(digest);
            } else {
                children.0 = Some(digest);
            }
        }
        level = parents
            .into_iter()
            .map(|(path, (left, right))| {
                let digest = hash(&branch(
                    depth,
                    left.as_deref().unwrap_or(&empty[depth + 1]),
                    right.as_deref().unwrap_or(&empty[depth + 1]),
                ));
                all_nodes.insert(digest.clone());
                (path, digest)
            })
            .collect();
    }
    (
        level
            .into_values()
            .next()
            .unwrap_or_else(|| empty[0].clone()),
        all_nodes,
    )
}

#[test]
fn put_replace_delete_match_independent_logical_map_and_final_node_set() {
    let mut pages = Pages::default();
    let mut root = StateRootV1::empty(StateNamespaceV1::Business).unwrap();
    let mut values = BTreeMap::new();
    let mut old_nodes = BTreeSet::new();
    let mut seed = 77u64;
    for round in 0..48u8 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let key = vec![(seed >> 32) as u8 % 12];
        let before = values.get(&key).copied();
        let after = if round % 3 == 0 {
            None
        } else {
            Some([round; 32])
        };
        let delta = StateDeltaV1::build(
            root.clone(),
            vec![StateWriteV1 {
                key: key.clone(),
                before,
                after,
            }],
            &pages,
            QUOTA,
        )
        .unwrap();
        delta
            .verify(&root.digest().unwrap(), &pages, QUOTA)
            .unwrap();
        match after {
            Some(value) => {
                values.insert(key.clone(), value);
            }
            None => {
                values.remove(&key);
            }
        }
        let (expected, nodes) = reference(root.namespace, &values);
        assert_eq!(delta.after.node_root, expected);
        assert_eq!(delta.after.leaf_count, values.len() as u64);
        assert_eq!(
            delta
                .new_nodes
                .iter()
                .map(|node| node.digest.clone())
                .collect::<BTreeSet<_>>(),
            nodes.difference(&old_nodes).cloned().collect()
        );
        pages.persist(&delta);
        root = delta.after;
        assert_eq!(root.get(&key, &pages).unwrap(), after);
        old_nodes = nodes;
    }
}

#[test]
fn batch_and_sequential_updates_have_same_root_and_cas_hits_do_not_change_writes() {
    let root = StateRootV1::empty(StateNamespaceV1::Scheduler).unwrap();
    let mut pages = Pages::default();
    let updates = vec![
        write(b"a", None, Some(1)),
        write(b"b", None, Some(2)),
        write(b"c", None, Some(3)),
    ];
    let batch = StateDeltaV1::build(root.clone(), updates.clone(), &pages, QUOTA).unwrap();
    let mut sequential = root.clone();
    for change in updates.iter().rev() {
        let delta = StateDeltaV1::build(sequential, vec![change.clone()], &pages, QUOTA).unwrap();
        pages.persist(&delta);
        sequential = delta.after;
    }
    assert_eq!(batch.after, sequential);
    // These objects exist physically but are absent from the empty parent.
    assert_eq!(
        batch,
        StateDeltaV1::build(root, updates, &pages, QUOTA * 2).unwrap()
    );
    pages.persist(&batch);
    let noop = StateDeltaV1::build(
        batch.after.clone(),
        vec![write(b"b", Some(2), Some(2))],
        &pages,
        QUOTA,
    )
    .unwrap();
    assert!(noop.new_nodes.is_empty());
    assert_eq!(noop.before, noop.after);
    let deleted = StateDeltaV1::build(
        batch.after,
        vec![
            write(b"a", Some(1), None),
            write(b"b", Some(2), None),
            write(b"c", Some(3), None),
        ],
        &pages,
        QUOTA,
    )
    .unwrap();
    assert_eq!(
        deleted.after,
        StateRootV1::empty(StateNamespaceV1::Scheduler).unwrap()
    );
    assert!(deleted.new_nodes.is_empty());
}

#[test]
fn wrong_preconditions_parent_duplicate_and_noncanonical_deltas_are_rejected() {
    let root = StateRootV1::empty(StateNamespaceV1::Effects).unwrap();
    let pages = Pages::default();
    assert!(
        StateDeltaV1::build(
            root.clone(),
            vec![write(b"a", Some(1), Some(2))],
            &pages,
            QUOTA
        )
        .is_err()
    );
    assert!(
        StateDeltaV1::build(
            root.clone(),
            vec![write(b"a", None, Some(1)); 2],
            &pages,
            QUOTA
        )
        .is_err()
    );
    assert!(
        StateDeltaV1::build(
            root.clone(),
            vec![write(b"b", None, Some(1)), write(b"a", None, Some(1))],
            &pages,
            QUOTA
        )
        .is_err()
    );
    let delta = StateDeltaV1::build(
        root.clone(),
        vec![write(b"a", None, Some(1)), write(b"b", None, Some(2))],
        &pages,
        QUOTA,
    )
    .unwrap();
    assert!(delta.verify(&"00".repeat(32), &pages, QUOTA).is_err());
    for mutation in 0..5 {
        let mut bad = delta.clone();
        match mutation {
            0 => {
                bad.new_nodes.pop();
            }
            1 => bad.new_nodes.reverse(),
            2 => bad.new_nodes.push(bad.new_nodes[0].clone()),
            3 => bad.new_nodes[0].bytes[0] ^= 1,
            _ => bad.after.leaf_count += 1,
        }
        assert!(bad.verify(&root.digest().unwrap(), &pages, QUOTA).is_err());
    }
    assert!(
        StateDeltaV1::build(root, vec![write(b"a", None, Some(1))], &pages, 1)
            .unwrap_err()
            .contains("ResourceLimited")
    );
}

#[test]
fn authenticated_seek_and_absence_are_bounded_and_incomplete_proofs_fail() {
    let root = StateRootV1::empty(StateNamespaceV1::Authorization).unwrap();
    let mut pages = Pages::default();
    let mut head = root;
    for round in 0..32u8 {
        let delta = StateDeltaV1::build(
            head,
            vec![write(&[round], None, Some(round))],
            &pages,
            QUOTA,
        )
        .unwrap();
        pages.persist(&delta);
        head = delta.after;
        pages.reads.set(0);
        assert_eq!(head.get(&[0], &pages).unwrap(), Some([0; 32]));
        assert_eq!(pages.reads.get(), 257);
    }
    for key in [vec![0], vec![250]] {
        let path = head.path(&key, &pages).unwrap();
        assert_eq!(
            path.verify(&head, &key).unwrap(),
            head.get(&key, &pages).unwrap()
        );
        let mut truncated = path.clone();
        truncated.nodes.pop();
        assert!(truncated.verify(&head, &key).is_err());
        let mut extra = path.clone();
        extra.nodes.push(path.nodes[0].clone());
        assert!(extra.verify(&head, &key).is_err());
    }
    let mut damaged = pages;
    damaged.nodes.remove(&head.node_root);
    assert!(head.get(&[0], &damaged).is_err());
    assert!(head.get(&[250], &damaged).is_err());
}

#[test]
fn namespace_format_and_key_bounds_are_enforced() {
    let pages = Pages::default();
    let root = StateRootV1::empty(StateNamespaceV1::Business).unwrap();
    let other = StateRootV1::empty(StateNamespaceV1::Allocator).unwrap();
    assert_ne!(root.digest().unwrap(), other.digest().unwrap());
    let max_key = vec![0; STATE_MAX_KEY_BYTES];
    let delta = StateDeltaV1::build(
        root.clone(),
        vec![write(&max_key, None, Some(1))],
        &pages,
        QUOTA,
    )
    .unwrap();
    assert!(
        delta
            .new_nodes
            .iter()
            .all(|n| n.bytes.len() <= STATE_MAX_PAGE_BYTES)
    );
    let mut corrupt = delta
        .new_nodes
        .iter()
        .find(|n| n.bytes.len() > 1000)
        .unwrap()
        .bytes
        .clone();
    corrupt[0] ^= 1;
    assert!(StateCodec::decode(&corrupt).is_err());
    assert!(
        StateDeltaV1::build(
            root,
            vec![write(&vec![0; STATE_MAX_KEY_BYTES + 1], None, Some(1))],
            &pages,
            QUOTA
        )
        .is_err()
    );
}

#[test]
fn canonical_root_roundtrip_rejects_unknown_versions_types_lengths_and_counts() {
    let root = StateRootV1::empty(StateNamespaceV1::Effects).unwrap();
    let bytes = root.canonical_bytes().unwrap();
    assert_eq!(StateRootV1::from_canonical_bytes(&bytes).unwrap(), root);
    for index in [0, 20, 24, 28, 29, 31, 38] {
        let mut bad = bytes.clone();
        bad[index] ^= 1;
        assert!(
            StateRootV1::from_canonical_bytes(&bad).is_err(),
            "index={index}"
        );
    }
    assert!(StateRootV1::from_canonical_bytes(&bytes[..bytes.len() - 1]).is_err());
    let mut extra = bytes;
    extra.push(0);
    assert!(StateRootV1::from_canonical_bytes(&extra).is_err());
}

#[test]
fn state_profile_vectors() {
    let empty = StateRootV1::empty(StateNamespaceV1::Business).unwrap();
    let delta = StateDeltaV1::build(
        empty.clone(),
        vec![write(b"key", None, Some(7))],
        &Pages::default(),
        QUOTA,
    )
    .unwrap();
    assert_eq!(
        empty.node_root,
        "647e04d7deacf195db202f3836807908b9c7061687033623b3fa1da1e1a83d96"
    );
    assert_eq!(
        empty.digest().unwrap(),
        "bc4814d8083f28977dc477674cf030100c33a172692ffeb354ab728b092b9a4f"
    );
    assert_eq!(
        delta.after.node_root,
        "d545fdc3e4d75d072e83a272e5586e9151f39d4571cf401d0e7a012591803d24"
    );
    assert_eq!(
        delta.after.digest().unwrap(),
        "3819bf85ec86ae61164f77050244fd63257f1141d32e72ae70c8ebadf50eeb55"
    );
    let leaf = delta
        .new_nodes
        .iter()
        .find(|page| matches!(page.verified_links().unwrap(), StatePageLinksV1::Value(_)))
        .unwrap();
    assert_eq!(
        leaf.digest,
        "fe89ee702c63b3df56bb9e833cba62f88722e3f1fd3de8f7576a5b14b8caa11f"
    );
    assert_eq!(
        hex::encode(&leaf.bytes),
        "6f61736973372e6977732e73746174652d706167652e7631000000002800000100036b65790707070707070707070707070707070707070707070707070707070707070707"
    );
}

#[test]
fn typed_page_edges_bind_hash_and_reject_wrong_domain_length_or_unknown_namespace() {
    let delta = StateDeltaV1::build(
        StateRootV1::empty(StateNamespaceV1::Business).unwrap(),
        vec![write(b"key", None, Some(7))],
        &Pages::default(),
        QUOTA,
    )
    .unwrap();
    let mut values = 0;
    let mut branches = 0;
    for page in &delta.new_nodes {
        match page.verified_links().unwrap() {
            StatePageLinksV1::Value(entry) => {
                assert_eq!(entry.value, [7; 32]);
                values += 1;
            }
            StatePageLinksV1::Branch { children, .. } => {
                assert_eq!(children.into_iter().flatten().count(), 1);
                branches += 1;
            }
        }
    }
    assert_eq!((values, branches), (1, 256));
    let leaf = delta
        .new_nodes
        .iter()
        .find(|page| matches!(page.verified_links().unwrap(), StatePageLinksV1::Value(_)))
        .unwrap();
    for index in [0, 24, 28, 29] {
        let mut bad = leaf.clone();
        bad.bytes[index] ^= 0xff;
        // Rehash to ensure the format parser, rather than only hash binding,
        // detects corrupt domain, length, tag and namespace.
        bad.digest = blake3::hash(&bad.bytes).to_hex().to_string();
        assert!(bad.verified_links().is_err(), "index={index}");
    }
}

#[path = "incremental_state_scan_tests.rs"]
mod scan_tests;
