//! Non-serving, ephemeral recovery counters. Separate wire domain prevents
//! inline u64 payloads from ever masquerading as state value-tree references.
use crate::{
    incremental_state::{MerkleNodeReader, StateNodeWriteV1},
    merkle::{Codec, DEPTH, Node, Updater, build_empty_hashes},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::sync::OnceLock;
const MAGIC: &[u8] = b"oasis7.iws.recovery-counter-page.v1\0";
#[derive(Clone)]
struct Counter {
    key: [u8; 32],
    value: u64,
}
struct CounterCodec;
impl Codec for CounterCodec {
    type Leaf = Counter;
    const MAX_NODE_BYTES: usize = 128;
    fn key(leaf: &Counter) -> Result<[u8; 32], String> {
        if leaf.value == 0 {
            return Err("zero scratch counter".into());
        }
        Ok(leaf.key)
    }
    fn encode(node: Node<Counter>) -> Result<Vec<u8>, String> {
        let mut bytes = Vec::with_capacity(128);
        bytes.extend_from_slice(MAGIC);
        match node {
            Node::Leaf(leaf) => {
                Self::key(&leaf)?;
                bytes.push(0);
                bytes.extend_from_slice(&leaf.key);
                bytes.extend_from_slice(&leaf.value.to_be_bytes());
            }
            Node::Branch { depth, left, right } => {
                if depth >= DEPTH {
                    return Err("scratch branch depth".into());
                }
                bytes.push(1);
                bytes.extend_from_slice(&(depth as u16).to_be_bytes());
                for digest in [left, right] {
                    if digest.len() != 64
                        || digest
                            .bytes()
                            .any(|b| !b.is_ascii_digit() && !(b'a'..=b'f').contains(&b))
                    {
                        return Err("scratch branch digest".into());
                    }
                    bytes.extend_from_slice(&hex::decode(digest).map_err(|e| e.to_string())?);
                }
            }
        }
        Ok(bytes)
    }
    fn decode(bytes: &[u8]) -> Result<Node<Counter>, String> {
        let bytes = bytes.strip_prefix(MAGIC).ok_or("scratch page domain")?;
        match bytes {
            [0, rest @ ..] if rest.len() == 40 => {
                let leaf = Counter {
                    key: rest[..32].try_into().unwrap(),
                    value: u64::from_be_bytes(rest[32..].try_into().unwrap()),
                };
                Self::key(&leaf)?;
                Ok(Node::Leaf(leaf))
            }
            [1, rest @ ..] if rest.len() == 66 => {
                let depth = u16::from_be_bytes(rest[..2].try_into().unwrap()) as usize;
                if depth >= DEPTH {
                    return Err("scratch branch depth".into());
                }
                Ok(Node::Branch {
                    depth,
                    left: hex::encode(&rest[2..34]),
                    right: hex::encode(&rest[34..]),
                })
            }
            _ => Err("scratch page tag/length".into()),
        }
    }
}
fn empty() -> Result<&'static [String], String> {
    static EMPTY: OnceLock<Result<Vec<String>, String>> = OnceLock::new();
    EMPTY
        .get_or_init(|| {
            build_empty_hashes::<CounterCodec>(b"oasis7.iws.recovery-counter-empty.v1\0")
        })
        .as_ref()
        .map(Vec::as_slice)
        .map_err(Clone::clone)
}
fn key(bytes: &[u8]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"oasis7.iws.recovery-counter-key.v1\0");
    h.update(&(bytes.len() as u64).to_be_bytes());
    h.update(bytes);
    *h.finalize().as_bytes()
}
/// No canonical/root-registry encoding or conversion to a state root exists.
#[derive(Clone)]
pub struct ScratchCounterRoot {
    node: String,
    pub entries: u64,
}
pub struct ScratchCounterDelta {
    pub after: ScratchCounterRoot,
    pub new_nodes: Vec<StateNodeWriteV1>,
}
pub trait CounterScratchStore: MerkleNodeReader {
    fn put_scratch_pages(&self, delta: &ReservedValue<ScratchCounterDelta>) -> Result<(), String>;
}
impl ScratchCounterRoot {
    pub fn empty() -> Result<Self, String> {
        Ok(Self {
            node: empty()?[0].clone(),
            entries: 0,
        })
    }
    pub fn get(&self, bytes: &[u8], source: &impl MerkleNodeReader) -> Result<Option<u64>, String> {
        Ok(Updater::<CounterCodec, _>::new(source, empty()?, 0)
            .lookup(&self.node, &key(bytes))?
            .map(|leaf| leaf.value))
    }
    pub fn increment(
        &self,
        bytes: &[u8],
        source: &impl MerkleNodeReader,
        max_pending: usize,
    ) -> Result<ScratchCounterDelta, String> {
        let mut update =
            Updater::<CounterCodec, _>::new(source, empty()?, max_pending).track_parent_nodes();
        let key = key(bytes);
        let old = update.lookup(&self.node, &key)?;
        let value = old
            .as_ref()
            .map(|leaf| leaf.value)
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("scratch counter overflow")?;
        let (node, _) = update.replace(&self.node, 0, &key, Some(&Counter { key, value }))?;
        let entries = self
            .entries
            .checked_add(u64::from(old.is_none()))
            .ok_or("scratch entries overflow")?;
        let new_nodes = update
            .final_writes(&node)?
            .into_iter()
            .map(|(digest, bytes)| StateNodeWriteV1 { digest, bytes })
            .collect();
        Ok(ScratchCounterDelta {
            after: Self { node, entries },
            new_nodes,
        })
    }
}
impl ScratchCounterDelta {
    pub fn verify_pages(&self) -> Result<(), String> {
        for page in &self.new_nodes {
            if blake3::hash(&page.bytes).to_hex().as_str() != page.digest {
                return Err("scratch page digest".into());
            }
            CounterCodec::decode(&page.bytes)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    #[derive(Default)]
    struct Pages(BTreeMap<String, Vec<u8>>);
    impl MerkleNodeReader for Pages {
        type Node = Vec<u8>;
        fn read_node(&self, id: &str, max: usize) -> Result<Option<Vec<u8>>, String> {
            let output = self.0.get(id).cloned();
            if output.as_ref().is_some_and(|bytes| bytes.len() > max) {
                return Err("bound".into());
            }
            Ok(output)
        }
    }
    #[test]
    fn scratch_counter_matches_independent_counts_and_rejects_bad_pages() {
        let mut root = ScratchCounterRoot::empty().unwrap();
        let mut pages = Pages::default();
        let mut expected = BTreeMap::new();
        for i in 0u64..40 {
            let key = (i % 13).to_be_bytes();
            *expected.entry(key).or_insert(0) += 1;
            let delta = root.increment(&key, &pages, 256 << 10).unwrap();
            delta.verify_pages().unwrap();
            for node in &delta.new_nodes {
                pages.0.insert(node.digest.clone(), node.bytes.clone());
            }
            root = delta.after;
            assert_eq!(root.entries, expected.len() as u64);
            for (key, count) in &expected {
                assert_eq!(root.get(key, &pages).unwrap(), Some(*count));
            }
            assert_eq!(root.get(b"absent", &pages).unwrap(), None);
        }
        let mut corrupt = pages.0.get(&root.node).unwrap().clone();
        corrupt[0] ^= 1;
        pages.0.insert(root.node.clone(), corrupt);
        assert!(root.get(&0u64.to_be_bytes(), &pages).is_err());
        for bytes in [
            vec![],
            b"oasis7.iws.state-page.v1\0".to_vec(),
            {
                let mut bytes = CounterCodec::encode(Node::Leaf(Counter {
                    key: [0; 32],
                    value: 1,
                }))
                .unwrap();
                *bytes.last_mut().unwrap() = 0;
                bytes
            },
            {
                let mut bytes = CounterCodec::encode(Node::Leaf(Counter {
                    key: [0; 32],
                    value: 1,
                }))
                .unwrap();
                bytes.push(0);
                bytes
            },
        ] {
            assert!(CounterCodec::decode(&bytes).is_err());
        }
    }
}
