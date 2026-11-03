use super::*;
use std::cell::RefCell;

// Deliberately ordered raw keys: these exercise prefix arithmetic independently
// of the production codecs' hashes and of Updater's path construction.
struct Raw;
impl Codec for Raw {
    type Leaf = [u8; 32];
    const MAX_NODE_BYTES: usize = 67;
    fn key(leaf: &Self::Leaf) -> Result<[u8; 32], String> {
        Ok(*leaf)
    }
    fn encode(node: Node<Self::Leaf>) -> Result<Vec<u8>, String> {
        match node {
            Node::Leaf(key) => Ok([&[0][..], &key].concat()),
            Node::Branch { depth, left, right } => {
                let mut bytes = vec![1];
                bytes.extend_from_slice(&(depth as u16).to_be_bytes());
                bytes.extend(hex::decode(left).unwrap());
                bytes.extend(hex::decode(right).unwrap());
                Ok(bytes)
            }
        }
    }
    fn decode(bytes: &[u8]) -> Result<Node<Self::Leaf>, String> {
        match bytes {
            [0, rest @ ..] if rest.len() == 32 => Ok(Node::Leaf(rest.try_into().unwrap())),
            [1, a, b, rest @ ..] if rest.len() == 64 => Ok(Node::Branch {
                depth: u16::from_be_bytes([*a, *b]) as usize,
                left: hex::encode(&rest[..32]),
                right: hex::encode(&rest[32..]),
            }),
            _ => Err("raw test codec".into()),
        }
    }
}
#[derive(Default)]
struct Pages {
    nodes: BTreeMap<String, Vec<u8>>,
    reads: RefCell<Vec<String>>,
}
impl MerkleNodeReader for Pages {
    type Node = Vec<u8>;
    fn read_node(&self, id: &str, _: usize) -> Result<Option<Self::Node>, String> {
        self.reads.borrow_mut().push(id.to_owned());
        Ok(self.nodes.get(id).cloned())
    }
}
fn store(pages: &mut Pages, node: Node<[u8; 32]>) -> String {
    let bytes = Raw::encode(node).unwrap();
    let id = blake3::hash(&bytes).to_hex().to_string();
    pages.nodes.insert(id.clone(), bytes);
    id
}
// Independent slow reference constructor: partition every level, no Updater.
fn tree(pages: &mut Pages, empty: &[String], keys: &[[u8; 32]], depth: usize) -> String {
    if keys.is_empty() {
        return empty[depth].clone();
    }
    if depth == DEPTH {
        assert_eq!(keys.len(), 1);
        return store(pages, Node::Leaf(keys[0]));
    }
    let split = keys.partition_point(|key| key[depth / 8] & (128 >> (depth % 8)) == 0);
    let left = tree(pages, empty, &keys[..split], depth + 1);
    let right = tree(pages, empty, &keys[split..], depth + 1);
    store(pages, Node::Branch { depth, left, right })
}
fn scan(
    pages: &Pages,
    empty: &[String],
    root: &str,
    lower: [u8; 32],
    upper: [u8; 32],
) -> Result<Vec<[u8; 32]>, VisitError<()>> {
    let mut result = Vec::new();
    let count =
        Updater::<Raw, _>::new(pages, empty, 0).visit_key_range(root, &lower, &upper, |leaf| {
            result.push(*leaf);
            Ok(())
        })?;
    assert_eq!(count as usize, result.len());
    Ok(result)
}
fn fixture() -> (Pages, Vec<String>, String, Vec<[u8; 32]>) {
    let empty = build_empty_hashes::<Raw>(b"raw-empty").unwrap();
    let mut keys = vec![[0; 32], [255; 32]];
    for bit in 0..256 {
        let mut key = [0; 32];
        key[bit / 8] = 128 >> (bit % 8);
        keys.push(key);
    }
    keys.sort();
    let mut pages = Pages::default();
    let root = tree(&mut pages, &empty, &keys, 0);
    (pages, empty, root, keys)
}

#[test]
fn inclusive_ranges_match_independent_oracle_at_every_bit_boundary() {
    let (pages, empty, root, keys) = fixture();
    assert_eq!(
        scan(&pages, &empty, &root, [0; 32], [255; 32]).unwrap(),
        keys
    );
    for (index, key) in keys.iter().enumerate() {
        assert_eq!(scan(&pages, &empty, &root, *key, *key).unwrap(), vec![*key]);
        if index % 7 == 0 {
            let end = (index + 13).min(keys.len() - 1);
            assert_eq!(
                scan(&pages, &empty, &root, *key, keys[end]).unwrap(),
                keys[index..=end]
            );
        }
    }
    let mut absent = [0; 32];
    absent[31] = 3;
    assert!(
        scan(&pages, &empty, &root, absent, absent)
            .unwrap()
            .is_empty()
    );
    assert!(
        scan(&pages, &empty, &empty[0], [0; 32], [255; 32])
            .unwrap()
            .is_empty()
    );
    pages.reads.borrow_mut().clear();
    assert!(matches!(
        scan(&pages, &empty, &root, [255; 32], [0; 32]),
        Err(VisitError::Tree(_))
    ));
    assert!(pages.reads.borrow().is_empty());
}

#[test]
fn future_subtree_is_not_read_but_requested_missing_nodes_fail() {
    let (mut pages, empty, root, _) = fixture();
    let Node::Branch { right, .. } = Raw::decode(&pages.nodes[&root]).unwrap() else {
        panic!()
    };
    pages.nodes.remove(&right);
    let lower = [0; 32];
    let mut upper = [255; 32];
    upper[0] = 127;
    assert!(scan(&pages, &empty, &root, lower, upper).is_ok());
    assert!(!pages.reads.borrow().contains(&right));
    assert!(matches!(
        scan(&pages, &empty, &root, [0; 32], [255; 32]),
        Err(VisitError::Tree(_))
    ));
    // A single requested leaf reads exactly its 257 authenticated path pages.
    pages.reads.borrow_mut().clear();
    assert_eq!(
        scan(&pages, &empty, &root, lower, lower).unwrap(),
        vec![lower]
    );
    assert_eq!(pages.reads.borrow().len(), DEPTH + 1);
}

#[test]
fn range_does_not_bypass_hash_depth_or_leaf_path_validation() {
    let empty = build_empty_hashes::<Raw>(b"raw-empty").unwrap();
    for mode in 0..3 {
        let mut pages = Pages::default();
        let mut wrong = [0; 32];
        wrong[31] = 1;
        let key = [0; 32];
        let mut root = store(&mut pages, Node::Leaf(if mode == 0 { wrong } else { key }));
        for depth in (0..DEPTH).rev() {
            root = store(
                &mut pages,
                Node::Branch {
                    depth: if mode == 1 && depth == 7 { 8 } else { depth },
                    left: root,
                    right: empty[depth + 1].clone(),
                },
            );
        }
        if mode == 2 {
            pages.nodes.get_mut(&root).unwrap()[3] ^= 1;
        }
        assert!(matches!(
            scan(&pages, &empty, &root, key, key),
            Err(VisitError::Tree(_))
        ));
    }
}

#[test]
fn visitor_failure_stops_before_later_leaves() {
    let (pages, empty, root, _) = fixture();
    let mut calls = 0;
    let result = Updater::<Raw, _>::new(&pages, &empty, 0).visit_key_range(
        &root,
        &[0; 32],
        &[255; 32],
        |_| {
            calls += 1;
            Err("stop")
        },
    );
    assert!(matches!(result, Err(VisitError::Visitor("stop"))));
    assert_eq!(calls, 1);
    assert_eq!(pages.reads.borrow().len(), DEPTH + 1);
}
