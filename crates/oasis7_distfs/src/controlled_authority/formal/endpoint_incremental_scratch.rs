//! Candidate-local, bounded scratch-counter CAS. No state/value page domain reuse.
use oasis7_formal_protocol::{
    incremental_scratch_counter::{CounterScratchStore, ScratchCounterDelta},
    incremental_state::MerkleNodeReader,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{cell::RefCell, collections::BTreeMap};
pub(super) struct ScratchNode(ReservedValue<Vec<u8>>);
impl AsRef<[u8]> for ScratchNode {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
pub(super) struct Scratch<'a> {
    memory: &'a MemoryBudget,
    class: MemoryClass,
    pages: RefCell<BTreeMap<String, ReservedValue<Vec<u8>>>>,
    max_pages: usize,
    _metadata: oasis7_proto::memory_budget::MemoryReservation,
}
impl<'a> Scratch<'a> {
    pub(super) fn new(
        memory: &'a MemoryBudget,
        class: MemoryClass,
        max_pages: usize,
    ) -> Result<Self, String> {
        let metadata = max_pages
            .checked_mul(256)
            .and_then(|n| n.checked_add(512))
            .ok_or("scratch metadata overflow")?;
        let charge = memory.reserve(metadata, class).map_err(|e| e.to_string())?;
        Ok(Self {
            memory,
            class,
            max_pages,
            pages: RefCell::new(BTreeMap::new()),
            _metadata: charge,
        })
    }
}
impl MerkleNodeReader for Scratch<'_> {
    type Node = ScratchNode;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(self.class)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<ScratchNode>, String> {
        let pages = self.pages.borrow();
        let Some(bytes) = pages.get(id) else {
            return Ok(None);
        };
        if bytes.len() > max {
            return Err("scratch page bound".into());
        }
        Ok(Some(ScratchNode(
            self.memory
                .reserve(bytes.len() + 128, self.class)
                .map_err(|e| e.to_string())?
                .retain(bytes.to_vec()),
        )))
    }
}
impl CounterScratchStore for Scratch<'_> {
    fn put_scratch_pages(&self, delta: &ReservedValue<ScratchCounterDelta>) -> Result<(), String> {
        if !delta.uses_budget(self.memory) {
            return Err("scratch pool".into());
        }
        delta.verify_pages()?;
        for page in &delta.new_nodes {
            let mut pages = self.pages.borrow_mut();
            if pages.len() >= self.max_pages && !pages.contains_key(&page.digest) {
                return Err("scratch finite pages".into());
            }
            let copy = self
                .memory
                .reserve(page.bytes.len() + 128, self.class)
                .map_err(|e| e.to_string())?
                .retain(page.bytes.clone());
            pages.insert(page.digest.clone(), copy);
        }
        Ok(())
    }
}
