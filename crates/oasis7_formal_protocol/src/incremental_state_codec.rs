use super::*;
use crate::merkle::Node;
use std::sync::OnceLock;

const MAGIC: &[u8] = b"oasis7.iws.state-page.v1\0";

pub(super) struct StateCodec;

pub(super) fn validate_key(key: &[u8]) -> Result<(), String> {
    if key.len() > STATE_MAX_KEY_BYTES {
        return Err("state key byte bound".into());
    }
    Ok(())
}

pub(super) fn key_digest(namespace: StateNamespaceV1, key: &[u8]) -> Result<[u8; 32], String> {
    validate_key(key)?;
    let mut hash = blake3::Hasher::new();
    hash.update(b"oasis7.iws.state-key.v1\0");
    hash.update(&(namespace as u16).to_be_bytes());
    hash.update(&(key.len() as u16).to_be_bytes());
    hash.update(key);
    Ok(*hash.finalize().as_bytes())
}

pub(super) fn parse_digest(digest: &str) -> Result<[u8; 32], String> {
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("state digest encoding".into());
    }
    hex::decode(digest)
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| "state digest length".into())
}

impl Codec for StateCodec {
    type Leaf = StateEntryV1;
    const MAX_NODE_BYTES: usize = STATE_MAX_PAGE_BYTES;

    fn key(leaf: &Self::Leaf) -> Result<[u8; 32], String> {
        key_digest(leaf.namespace, &leaf.key)
    }

    fn encode(node: Node<Self::Leaf>) -> Result<Vec<u8>, String> {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&0u32.to_be_bytes());
        match node {
            Node::Branch { depth, left, right } => {
                if depth >= DEPTH {
                    return Err("state branch depth".into());
                }
                bytes.push(1);
                bytes.extend_from_slice(&(depth as u16).to_be_bytes());
                bytes.extend_from_slice(&parse_digest(&left)?);
                bytes.extend_from_slice(&parse_digest(&right)?);
            }
            Node::Leaf(leaf) => {
                validate_key(&leaf.key)?;
                bytes.push(0);
                bytes.extend_from_slice(&(leaf.namespace as u16).to_be_bytes());
                bytes.extend_from_slice(&(leaf.key.len() as u16).to_be_bytes());
                bytes.extend_from_slice(&leaf.key);
                bytes.extend_from_slice(&leaf.value);
            }
        }
        let length = (bytes.len() - MAGIC.len() - 4) as u32;
        bytes[MAGIC.len()..MAGIC.len() + 4].copy_from_slice(&length.to_be_bytes());
        Ok(bytes)
    }

    fn decode(bytes: &[u8]) -> Result<Node<Self::Leaf>, String> {
        if bytes.len() > STATE_MAX_PAGE_BYTES || !bytes.starts_with(MAGIC) {
            return Err("state page format/byte bound".into());
        }
        let encoded_len = bytes
            .get(MAGIC.len()..MAGIC.len() + 4)
            .ok_or("state page length truncated")?;
        let length =
            u32::from_be_bytes(encoded_len.try_into().map_err(|_| "state page length")?) as usize;
        let payload = bytes.get(MAGIC.len() + 4..).ok_or("state page truncated")?;
        if payload.len() != length {
            return Err("state page declared length mismatch".into());
        }
        let node = match payload.first() {
            Some(1) if payload.len() == 67 => Node::Branch {
                depth: usize::from(u16::from_be_bytes(
                    payload[1..3].try_into().map_err(|_| "state depth")?,
                )),
                left: hex::encode(&payload[3..35]),
                right: hex::encode(&payload[35..67]),
            },
            Some(0) if payload.len() >= 37 => {
                let namespace =
                    StateNamespaceV1::from_tag(u16::from_be_bytes([payload[1], payload[2]]))?;
                let len = usize::from(u16::from_be_bytes([payload[3], payload[4]]));
                if len > STATE_MAX_KEY_BYTES || payload.len() != len + 37 {
                    return Err("state leaf key/length bound".into());
                }
                Node::Leaf(StateEntryV1 {
                    namespace,
                    key: payload[5..5 + len].to_vec(),
                    value: payload[5 + len..]
                        .try_into()
                        .map_err(|_| "state value digest")?,
                })
            }
            _ => return Err("state page tag/length".into()),
        };
        // Encoding also rejects impossible branch depths.
        if Self::encode(match &node {
            Node::Branch { depth, left, right } => Node::Branch {
                depth: *depth,
                left: left.clone(),
                right: right.clone(),
            },
            Node::Leaf(leaf) => Node::Leaf(leaf.clone()),
        })? != bytes
        {
            return Err("state page canonical encoding".into());
        }
        Ok(node)
    }
}

pub(super) fn empty_hashes() -> Result<&'static [String], String> {
    static EMPTY: OnceLock<Result<Vec<String>, String>> = OnceLock::new();
    EMPTY
        .get_or_init(|| {
            let mut marker = MAGIC.to_vec();
            marker.extend_from_slice(&1u32.to_be_bytes());
            marker.push(2);
            crate::merkle::build_empty_hashes::<StateCodec>(&marker)
        })
        .as_ref()
        .map(Vec::as_slice)
        .map_err(Clone::clone)
}
