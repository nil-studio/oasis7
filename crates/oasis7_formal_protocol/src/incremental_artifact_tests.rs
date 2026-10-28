use super::*;
use oasis7_proto::memory_budget::MemoryReservation;
use std::{collections::BTreeMap, sync::Arc};
struct Page(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Page {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
struct Source {
    memory: MemoryBudget,
    nodes: BTreeMap<String, Arc<ReservedValue<Vec<u8>>>>,
    _metadata: MemoryReservation,
}
impl MerkleNodeReader for Source {
    type Node = Page;
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.memory)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Page>, String> {
        self.nodes
            .get(id)
            .map(|bytes| {
                if bytes.len() <= max {
                    Ok(Page(bytes.clone()))
                } else {
                    Err("source bound".into())
                }
            })
            .transpose()
    }
}
fn source() -> Source {
    let memory = MemoryBudget::new(32 << 20, 0).unwrap();
    Source {
        _metadata: memory.reserve(512 * 1024, MemoryClass::Ordinary).unwrap(),
        memory,
        nodes: BTreeMap::new(),
    }
}
fn install(source: &mut Source, delta: &ArtifactCatalogDeltaV1) {
    for node in &delta.new_nodes {
        let charge = source
            .memory
            .reserve(node.bytes.len() + 128, MemoryClass::Ordinary)
            .unwrap();
        source.nodes.insert(
            node.digest.clone(),
            Arc::new(charge.retain(node.bytes.clone())),
        );
    }
}
fn entries() -> [ArtifactEntryV1; 2] {
    [1, 3].map(|n| ArtifactEntryV1 {
        kind: ArtifactKindV1::Wasm,
        original: OriginalArtifactRefV1 {
            id: [n; 32],
            byte_len: n as u64,
        },
    })
}
// Independent whole-tree profile encoder, without production Codec/Updater.
fn reference(entries: &[ArtifactEntryV1]) -> String {
    fn hash(payload: &[u8]) -> String {
        let mut bytes = b"oasis7.iws.artifact-page.v1\0".to_vec();
        bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        bytes.extend_from_slice(payload);
        blake3::hash(&bytes).to_hex().to_string()
    }
    fn branch(depth: usize, left: &str, right: &str) -> String {
        let mut bytes = vec![1];
        bytes.extend_from_slice(&(depth as u16).to_be_bytes());
        bytes.extend(hex::decode(left).unwrap());
        bytes.extend(hex::decode(right).unwrap());
        hash(&bytes)
    }
    let mut empty = vec![String::new(); 257];
    empty[256] = hash(&[2]);
    for d in (0..256).rev() {
        empty[d] = branch(d, &empty[d + 1], &empty[d + 1]);
    }
    let mut level = BTreeMap::new();
    for entry in entries {
        let mut bytes = vec![0, entry.kind as u8];
        bytes.extend(entry.original.id);
        bytes.extend(entry.original.byte_len.to_be_bytes());
        level.insert(entry.original.id, hash(&bytes));
    }
    for d in (0..256).rev() {
        let mut next: BTreeMap<[u8; 32], (Option<String>, Option<String>)> = BTreeMap::new();
        for (mut path, id) in level {
            let mask = 0x80 >> (d % 8);
            let right = path[d / 8] & mask != 0;
            path[d / 8] &= !mask;
            let pair = next.entry(path).or_default();
            if right {
                pair.1 = Some(id)
            } else {
                pair.0 = Some(id)
            }
        }
        level = next
            .into_iter()
            .map(|(path, (left, right))| {
                (
                    path,
                    branch(
                        d,
                        left.as_deref().unwrap_or(&empty[d + 1]),
                        right.as_deref().unwrap_or(&empty[d + 1]),
                    ),
                )
            })
            .collect();
    }
    level.into_values().next().unwrap_or(empty[0].clone())
}
#[test]
fn catalog_matches_independent_tree_and_idempotent_incremental_append() {
    let mut source = source();
    let empty = ArtifactCatalogV1::empty(ArtifactKindV1::Wasm).unwrap();
    let entries = entries();
    let all = empty
        .append(&entries, &source, &source.memory, 4 << 20)
        .unwrap();
    all.verify(empty.digest().unwrap(), &source, &source.memory, 4 << 20)
        .unwrap();
    let mut forged = (*all).clone();
    forged.new_nodes.pop();
    assert!(
        forged
            .verify(empty.digest().unwrap(), &source, &source.memory, 4 << 20)
            .is_err()
    );
    for node in &all.new_nodes {
        node.verified_links().unwrap();
    }
    assert_eq!(all.after.node_root, reference(&entries));
    assert_eq!(all.after.count, 2);
    let first = empty
        .append(&entries[..1], &source, &source.memory, 4 << 20)
        .unwrap();
    install(&mut source, &first);
    let second = first
        .after
        .append(&entries[1..], &source, &source.memory, 4 << 20)
        .unwrap();
    assert_eq!(second.after, all.after);
    install(&mut source, &second);
    assert_eq!(
        second.after.get(&[1; 32], &source).unwrap(),
        Some(entries[0])
    );
    let again = second
        .after
        .append(&entries, &source, &source.memory, 4 << 20)
        .unwrap();
    assert!(again.new_nodes.is_empty());
    assert_eq!(again.after, all.after);
    let mut actual = vec![];
    assert_eq!(
        all.after
            .visit(&source, |entry| {
                actual.push(*entry);
                Ok::<_, ()>(())
            })
            .unwrap(),
        2
    );
    assert_eq!(actual, entries);
}
#[test]
fn kind_length_order_count_missing_and_pool_are_checked() {
    let mut source = source();
    let root = ArtifactCatalogV1::empty(ArtifactKindV1::Wasm).unwrap();
    let entries = entries();
    let delta = root
        .append(&entries, &source, &source.memory, 4 << 20)
        .unwrap();
    install(&mut source, &delta);
    for mode in 0..3 {
        let mut wrong = entries;
        if mode == 0 {
            wrong.swap(0, 1);
        }
        if mode == 1 {
            wrong[0].kind = ArtifactKindV1::Rules;
        }
        if mode == 2 {
            wrong[0].original.byte_len += 1;
        }
        assert!(
            delta
                .after
                .append(&wrong, &source, &source.memory, 4 << 20)
                .is_err()
        );
    }
    for mode in 0..3 {
        let mut wrong = delta.after.clone();
        if mode == 0 {
            wrong.kind = ArtifactKindV1::Rules;
        }
        if mode == 1 {
            wrong.count += 1;
        }
        if mode == 2 {
            source.nodes.remove(&wrong.node_root);
        }
        assert!(wrong.visit(&source, |_| Ok::<_, ()>(())).is_err());
    }
    let foreign = MemoryBudget::new(4 << 20, 0).unwrap();
    assert!(root.append(&entries, &source, &foreign, 4 << 20).is_err());
}
#[test]
fn descriptor_and_manifest_have_exact_profile_bytes_and_reject_unknown_forms() {
    let root = ArtifactCatalogV1 {
        kind: ArtifactKindV1::Rules,
        count: 7,
        node_root: hex::encode([8; 32]),
    };
    let mut expected = b"oasis7.iws.artifact-root.v1\0".to_vec();
    expected.extend(41u32.to_be_bytes());
    expected.push(2);
    expected.extend(7u64.to_be_bytes());
    expected.extend([8; 32]);
    assert_eq!(root.canonical_bytes().unwrap(), expected);
    assert_eq!(
        ArtifactCatalogV1::from_canonical_bytes(&expected).unwrap(),
        root
    );
    let manifest = IwsExecutionManifestV1 {
        scope: WorldScopeV1 {
            world_id: "w".into(),
            genesis_config: [1; 32],
        },
        format_profile: [2; 32],
        native: [3; 32],
        executable: [4; 32],
        wasm: [5; 32],
        rules: [6; 32],
        policy: OriginalArtifactRefV1 {
            id: [7; 32],
            byte_len: 9,
        },
        mode: ExecutionManifestModeV1::Genesis,
    };
    let mut payload = vec![0, 1, b'w'];
    for n in 1..=7 {
        payload.extend([n; 32]);
    }
    payload.extend(9u64.to_be_bytes());
    payload.push(0);
    let mut expected = b"oasis7.iws.execution-manifest.v1\0".to_vec();
    expected.extend((payload.len() as u32).to_be_bytes());
    expected.extend(payload);
    assert_eq!(manifest.canonical_bytes().unwrap(), expected);
    assert_eq!(
        IwsExecutionManifestV1::from_canonical_bytes(&expected).unwrap(),
        manifest
    );
    for mode in 0..4 {
        let mut bad = expected.clone();
        if mode == 0 {
            bad[0] ^= 1;
        }
        if mode == 1 {
            bad.pop();
        }
        if mode == 2 {
            bad.push(0);
        }
        if mode == 3 {
            *bad.last_mut().unwrap() = 2;
        }
        assert!(IwsExecutionManifestV1::from_canonical_bytes(&bad).is_err());
    }
    let mut execute = manifest;
    execute.mode = ExecutionManifestModeV1::Execution {
        parent_commit: [10; 32],
        parent_roots: [11; 32],
        operation: [12; 32],
        input: [13; 32],
        continuation: OriginalArtifactRefV1 {
            id: [14; 32],
            byte_len: 17,
        },
    };
    assert_eq!(
        IwsExecutionManifestV1::from_canonical_bytes(&execute.canonical_bytes().unwrap()).unwrap(),
        execute
    );
}
