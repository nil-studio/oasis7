use super::*;
use crate::merkle::{Codec, DEPTH, Node};
use std::sync::OnceLock;
use wire::{Reader, Writer, parse_hash};

const MAGIC: &[u8] = b"oasis7.iws.catalog-node.v1\0";

#[derive(Clone)]
pub(super) enum CatalogLeaf {
    Height(CatalogEntryV1),
    Commit {
        scope: WorldScopeV1,
        commit: [u8; 32],
        height: u64,
    },
}
pub(super) struct CatalogCodec;

pub(super) fn query_key(
    scope: &WorldScopeV1,
    query: CatalogQueryV1,
) -> Result<[u8; 32], ExecutionError> {
    query_key_with_work(scope, query, &mut |_, _| Ok(()))
}
pub(super) fn query_key_with_work(
    scope: &WorldScopeV1,
    query: CatalogQueryV1,
    work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
) -> Result<[u8; 32], ExecutionError> {
    work(1, scope.world_id.len() as u64)?;
    scope.validate()?;
    let work = std::cell::RefCell::new(work);
    let meter = |s, b| (work.borrow_mut())(s, b);
    let mut out = Writer::with_work(&meter);
    out.scope(scope);
    match query {
        CatalogQueryV1::Height(height) => {
            out.u8(1);
            out.u64(height);
        }
        CatalogQueryV1::Commit(commit) => {
            out.u8(2);
            out.hash(&commit);
        }
    }
    let bytes = out.finish(b"oasis7.iws.catalog-key.v1\0", HISTORY_MAX_HEADER_BYTES)?;
    meter(1, bytes.len() as u64)?;
    Ok(wire::digest(&bytes))
}
impl CatalogCodec {
    pub(super) fn key_digest(leaf: &CatalogLeaf) -> Result<[u8; 32], ExecutionError> {
        Self::key_digest_with_work(leaf, &mut |_, _| Ok(()))
    }
    pub(super) fn key_digest_with_work(
        leaf: &CatalogLeaf,
        work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    ) -> Result<[u8; 32], ExecutionError> {
        match leaf {
            CatalogLeaf::Height(entry) => {
                query_key_with_work(&entry.scope, CatalogQueryV1::Height(entry.height), work)
            }
            CatalogLeaf::Commit { scope, commit, .. } => {
                query_key_with_work(scope, CatalogQueryV1::Commit(*commit), work)
            }
        }
    }
    fn encode_node(node: Node<CatalogLeaf>) -> Result<Vec<u8>, ExecutionError> {
        let mut out = Writer::new();
        match node {
            Node::Branch { depth, left, right } => {
                if depth >= DEPTH {
                    return Err(ExecutionError::Invalid("catalog branch depth"));
                }
                out.u8(1);
                out.u16(depth as u16);
                out.hash(&parse_hash(&left)?);
                out.hash(&parse_hash(&right)?);
            }
            Node::Leaf(CatalogLeaf::Height(entry)) => {
                entry.validate()?;
                out.u8(0);
                out.u8(1);
                out.scope(&entry.scope);
                out.u64(entry.height);
                out.hash(&entry.commit);
                out.optional(&entry.parent_commit);
                out.checkpoint(&entry.checkpoint);
                out.checkpoint(&entry.latest_checkpoint);
            }
            Node::Leaf(CatalogLeaf::Commit {
                scope,
                commit,
                height,
            }) => {
                scope.validate()?;
                out.u8(0);
                out.u8(2);
                out.scope(&scope);
                out.hash(&commit);
                out.u64(height);
            }
        }
        out.finish(MAGIC, CATALOG_MAX_NODE_BYTES)
    }
    fn decode_node(bytes: &[u8]) -> Result<Node<CatalogLeaf>, ExecutionError> {
        let mut r = Reader::new(bytes, MAGIC, CATALOG_MAX_NODE_BYTES)?;
        let node = match r.u8()? {
            1 => {
                let depth = usize::from(r.u16()?);
                if depth >= DEPTH {
                    return Err(ExecutionError::Invalid("catalog branch depth"));
                }
                Node::Branch {
                    depth,
                    left: hex::encode(r.array::<32>()?),
                    right: hex::encode(r.array::<32>()?),
                }
            }
            0 => match r.u8()? {
                1 => {
                    let entry = CatalogEntryV1 {
                        scope: r.scope()?,
                        height: r.u64()?,
                        commit: r.array()?,
                        parent_commit: r.optional()?,
                        checkpoint: r.checkpoint()?,
                        latest_checkpoint: r.checkpoint()?,
                    };
                    entry.validate()?;
                    Node::Leaf(CatalogLeaf::Height(entry))
                }
                2 => Node::Leaf(CatalogLeaf::Commit {
                    scope: r.scope()?,
                    commit: r.array()?,
                    height: r.u64()?,
                }),
                _ => return Err(ExecutionError::Invalid("catalog leaf kind")),
            },
            _ => return Err(ExecutionError::Invalid("catalog node tag")),
        };
        r.finish()?;
        Ok(node)
    }
}
impl Codec for CatalogCodec {
    type Leaf = CatalogLeaf;
    const MAX_NODE_BYTES: usize = CATALOG_MAX_NODE_BYTES;
    fn key(leaf: &Self::Leaf) -> Result<[u8; 32], String> {
        Self::key_digest(leaf).map_err(|e| e.to_string())
    }
    fn encode(node: Node<Self::Leaf>) -> Result<Vec<u8>, String> {
        Self::encode_node(node).map_err(|e| e.to_string())
    }
    fn decode(bytes: &[u8]) -> Result<Node<Self::Leaf>, String> {
        Self::decode_node(bytes).map_err(|e| e.to_string())
    }
}
pub(super) fn empty_hashes() -> Result<&'static [String], ExecutionError> {
    empty_hashes_with_work(&mut |_, _| Ok(()))
}
pub(super) fn empty_hashes_with_work(
    work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
) -> Result<&'static [String], ExecutionError> {
    static EMPTY: OnceLock<Result<Vec<String>, String>> = OnceLock::new();
    if let Some(value) = EMPTY.get() {
        work(1, 0)?;
        return value
            .as_ref()
            .map(Vec::as_slice)
            .map_err(|e| ExecutionError::Tree(e.clone()));
    }
    let work = std::cell::RefCell::new(work);
    let first = std::cell::RefCell::new(None);
    let meter = |bytes: usize| {
        if first.borrow().is_some() {
            return Err("catalog meter stopped".to_owned());
        }
        match (work.borrow_mut())(1, bytes as u64) {
            Ok(()) => Ok(()),
            Err(error) => {
                *first.borrow_mut() = Some(error);
                Err("catalog meter stopped".to_owned())
            }
        }
    };
    let built = (|| {
        let mut out = Writer::new();
        meter(1).map_err(ExecutionError::Tree)?;
        out.u8(2);
        let marker = out.finish(MAGIC, CATALOG_MAX_NODE_BYTES)?;
        crate::merkle::build_empty_hashes_with_meter::<CatalogCodec>(&marker, Some(&meter))
            .map_err(ExecutionError::Tree)
    })();
    if let Some(error) = first.into_inner() {
        return Err(error);
    }
    let built = built?;
    let _ = EMPTY.set(Ok(built));
    EMPTY
        .get()
        .ok_or(ExecutionError::Invalid("catalog empty cache"))?
        .as_ref()
        .map(Vec::as_slice)
        .map_err(|e| ExecutionError::Tree(e.clone()))
}
