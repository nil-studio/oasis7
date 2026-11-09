//! Charged single-leaf cursor over authenticated sparse State pages.
use super::*;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("JSON authenticated State cursor")
}
use oasis7_formal_protocol::incremental_state::{
    MerkleNodeReader, STATE_MAX_PAGE_BYTES, StateEntryV1, StateNodeWriteV1, StatePageLinksV1,
};
use oasis7_proto::memory_budget::MemoryReservation;
use std::cell::RefCell;
struct Node {
    id: String,
    depth: u16,
    path: [u8; 32],
}
pub(crate) struct Cursor {
    stack: Vec<Node>,
    leaves: u64,
    expected: u64,
    namespace: StateNamespaceV1,
    _scratch: MemoryReservation,
}
impl Cursor {
    pub(crate) fn new<R: ValueNodeReader, C: FnMut() -> bool>(
        root: &StateRootV1,
        records: &mut crate::incremental_inventory_value_read::InventoryValueReader<'_, '_, R, C>,
    ) -> Result<Self, ValueTreeError> {
        root.validate().map_err(|_| invalid())?;
        let scratch = records
            .values
            .memory
            .reserve(96 << 10, records.values.class)?;
        let mut stack = Vec::with_capacity(257);
        if root.node_root
            != StateRootV1::empty(root.namespace)
                .map_err(|_| invalid())?
                .node_root
        {
            stack.push(Node {
                id: root.node_root.clone(),
                depth: 0,
                path: [0; 32],
            });
        }
        Ok(Self {
            stack,
            leaves: 0,
            expected: root.leaf_count,
            namespace: root.namespace,
            _scratch: scratch,
        })
    }
    pub(crate) fn next<R: ValueNodeReader, C: FnMut() -> bool>(
        &mut self,
        records: &mut crate::incremental_inventory_value_read::InventoryValueReader<'_, '_, R, C>,
    ) -> Result<Option<StateEntryV1>, ValueTreeError> {
        while let Some(node) = self.stack.pop() {
            if (records.cancelled)() {
                return Err(ValueTreeError::Cancelled);
            }
            let page = {
                let source = tree::Source {
                    source: records.values.source,
                    memory: records.values.memory,
                    class: records.values.class,
                    io: &RefCell::new(&mut *records.values.io),
                    cancelled: &RefCell::new(&mut *records.cancelled),
                    error: &RefCell::new(None),
                    labels: LABELS,
                };
                let bytes = source
                    .finish(source.read_node(&node.id, STATE_MAX_PAGE_BYTES))?
                    .ok_or_else(invalid)?;
                StateNodeWriteV1 {
                    digest: node.id,
                    bytes: bytes.as_ref().to_vec(),
                }
                .verified_links()
                .map_err(|_| invalid())?
            };
            match page {
                StatePageLinksV1::Value(entry) if node.depth == 256 => {
                    if entry.namespace != self.namespace
                        || entry.key_digest().map_err(|_| invalid())? != node.path
                    {
                        return Err(invalid());
                    }
                    self.leaves = self.leaves.checked_add(1).ok_or_else(invalid)?;
                    if self.leaves > self.expected {
                        return Err(invalid());
                    }
                    if (records.cancelled)() {
                        return Err(ValueTreeError::Cancelled);
                    }
                    return Ok(Some(entry));
                }
                StatePageLinksV1::Branch { depth, children }
                    if depth == node.depth && depth < 256 =>
                {
                    for (index, id) in children.into_iter().enumerate().rev() {
                        if let Some(id) = id {
                            if self.stack.len() == 257 {
                                return Err(ValueTreeError::ResourceLimited(
                                    "effect State cursor stack",
                                ));
                            }
                            let mut path = node.path;
                            if index == 1 {
                                path[usize::from(depth) / 8] |= 0x80 >> (depth % 8);
                            }
                            self.stack.push(Node {
                                id,
                                depth: depth + 1,
                                path,
                            });
                        }
                    }
                }
                _ => return Err(invalid()),
            }
        }
        if self.leaves != self.expected {
            return Err(invalid());
        }
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(None)
    }
}
