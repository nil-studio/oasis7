//! Bounded, admitted, non-serving CAS for real ephemeral counter pages.
use oasis7_formal_protocol::{
    incremental_scratch_counter::{CounterScratchStore, ScratchCounterDelta},
    incremental_state::MerkleNodeReader,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};
pub(super) struct Scratch {
    memory: MemoryBudget,
    nodes: RefCell<BTreeMap<String, Arc<ReservedValue<Vec<u8>>>>>,
    _metadata: MemoryReservation,
    max_nodes: usize,
}
pub(super) struct Node(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Node {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
impl Scratch {
    pub(super) fn new(memory: &MemoryBudget, max_nodes: usize) -> Self {
        Self {
            memory: memory.clone(),
            nodes: RefCell::new(BTreeMap::new()),
            _metadata: memory
                .reserve(
                    max_nodes.checked_mul(4096).unwrap() + 512,
                    MemoryClass::Ordinary,
                )
                .unwrap(),
            max_nodes,
        }
    }
}
impl MerkleNodeReader for Scratch {
    type Node = Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Node>, String> {
        let nodes = self.nodes.borrow();
        let Some(bytes) = nodes.get(id) else {
            return Ok(None);
        };
        if bytes.len() > max {
            return Err("ResourceLimited: scratch page bound".into());
        }
        Ok(Some(Node(bytes.clone())))
    }
}
impl CounterScratchStore for Scratch {
    fn put_scratch_pages(&self, delta: &ReservedValue<ScratchCounterDelta>) -> Result<(), String> {
        if !delta.uses_budget(&self.memory) {
            return Err("scratch foreign pool".into());
        }
        delta.verify_pages()?;
        let mut nodes = self.nodes.borrow_mut();
        for node in &delta.new_nodes {
            if !node
                .bytes
                .starts_with(b"oasis7.iws.recovery-counter-page.v1\0")
                || node.bytes.len() > 128
                || node.digest != blake3::hash(&node.bytes).to_hex().as_str()
            {
                return Err("scratch original page domain/hash/bound".into());
            }
            if !nodes.contains_key(&node.digest) && nodes.len() >= self.max_nodes {
                return Err("ResourceLimited: scratch metadata".into());
            }
            let charge = self
                .memory
                .reserve(node.bytes.len() + 128, MemoryClass::Ordinary)
                .map_err(|_| "ResourceLimited: scratch page charge")?;
            nodes.insert(
                node.digest.clone(),
                Arc::new(charge.retain(node.bytes.clone())),
            );
        }
        Ok(())
    }
}
