use super::*;
use std::collections::BTreeMap;

#[test]
fn embedded_business_node_cache_rejects_corrupt_bytes() {
    use crate::controlled_authority::formal::business_index::BusinessIndexNodeReader;

    let mut cache = EmbeddedBusinessNodes::default();
    let bytes = b"canonical node bytes";
    let digest = blake3::hash(bytes).to_hex().to_string();
    cache.insert_write(&digest, bytes).unwrap();
    assert_eq!(cache.read_node(&digest).unwrap(), Some(bytes.to_vec()));
    assert!(cache.insert_write(&"ab".repeat(32), b"bad bytes").is_err());
    cache
        .nodes
        .insert(digest.clone(), b"tampered bytes".to_vec());
    assert!(cache.read_node(&digest).is_err());
}

fn put<T: serde::Serialize>(objects: &mut BTreeMap<String, Vec<u8>>, value: &T) -> String {
    let bytes = serde_cbor::to_vec(value).unwrap();
    let id = blake3::hash(&bytes).to_hex().to_string();
    objects.insert(id.clone(), bytes);
    id
}
#[test]
fn historical_skip_path_is_bounded_exact_and_rejects_fork_missing_and_corrupt_nodes() {
    let mut objects = BTreeMap::new();
    let mut nodes: Vec<(String, HistoryNode)> = Vec::new();
    for position in 1..=1024u64 {
        let mut ancestors = Vec::new();
        for level in 0..64 {
            let step = 1u64 << level;
            if position <= step {
                break;
            }
            let (id, node) = &nodes[(position - step - 1) as usize];
            ancestors.push(super::super::store::HistoryLink {
                head: node.head.clone(),
                node: id.clone(),
            });
        }
        let node = HistoryNode {
            head: FormalHeadRefV1 {
                position,
                record_id: format!("{position:064x}"),
            },
            request: format!("{:064x}", 2000 + position),
            entry: format!("{:064x}", 4000 + position),
            ancestors,
        };
        let id = put(&mut objects, &node);
        nodes.push((id, node));
    }
    let body = HotBody {
        version: 1,
        instance_digest: "00".repeat(32),
        authority_id: "00".repeat(32),
        activation_ref: "00".repeat(32),
        epoch: 1,
        revision: 1024,
        head: nodes[1023].1.head.clone(),
        head_node: Some(nodes[1023].0.clone()),
        imported_head_node: None,
        import_digest: None,
        base_parent: None,
        index: None,
        pending: None,
        decision_head: None,
        seal: None,
        durable_set: None,
    };
    let reads = std::cell::Cell::new(0);
    let mut load = |id: &str| {
        reads.set(reads.get() + 1);
        objects
            .get(id)
            .cloned()
            .ok_or_else(|| invalid("missing fixture node"))
    };
    let found = descend(&body, 33, Some(&nodes[32].1.head.record_id), &mut load).unwrap();
    assert_eq!(found.head, nodes[32].1.head);
    assert!(reads.get() <= 11);
    assert!(descend(&body, 33, Some(&"ff".repeat(32)), &mut load).is_err());
    assert_eq!(
        descend(&body, 33, None, &mut load).unwrap().head,
        nodes[32].1.head
    );
    objects.remove(&nodes[32].0);
    assert!(
        descend(&body, 33, None, &mut |id| objects
            .get(id)
            .cloned()
            .ok_or_else(|| invalid("missing")))
        .is_err()
    );
    objects.insert(nodes[32].0.clone(), b"corrupted node".to_vec());
    assert!(
        descend(&body, 33, None, &mut |id| objects
            .get(id)
            .cloned()
            .ok_or_else(|| invalid("missing")))
        .is_err()
    );
}

#[test]
fn historical_business_tree_verification_preserves_load_error_provenance() {
    use crate::controlled_authority::formal::business_index::BusinessIndexRootV1;
    let embedded = EmbeddedBusinessNodes::default();
    let root = BusinessIndexRootV1 {
        node_root: "ab".repeat(32),
        leaf_count: 1,
    };
    let diagnostic = "HistoryUnavailable grant node not ready".to_owned();
    for source in [
        FormalError::Io(diagnostic.clone()),
        FormalError::Invalid(diagnostic),
    ] {
        let mut load = |_: &str| Err(source.clone());
        let reader = HistoryBusinessReader {
            load: std::cell::RefCell::new(&mut load),
            embedded: &embedded,
            source_error: std::cell::RefCell::new(None),
        };
        let rejected = root
            .get(&"cd".repeat(32), &reader)
            .map_err(invalid)
            .map_err(|error| reader.preserve_source(error));
        assert_eq!(rejected.unwrap_err(), source);
        // Error capture belongs to this one tree computation, not later work.
        assert_eq!(
            reader.preserve_source(invalid("pure proof failure")),
            invalid("pure proof failure")
        );
    }
}
