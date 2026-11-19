//! Fresh, budgeted class admission over the shared finite physical page fixture.
use super::Pages;
use oasis7_formal_protocol::{
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::sync::Arc;
pub(crate) struct ClassedPages<'a> {
    pages: &'a Pages,
    memory: &'a MemoryBudget,
    class: MemoryClass,
}
pub(crate) struct ClassedNode(ReservedValue<Vec<u8>>);
impl AsRef<[u8]> for ClassedNode {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl<'a> ClassedPages<'a> {
    pub(crate) fn new(pages: &'a Pages, memory: &'a MemoryBudget, class: MemoryClass) -> Self {
        Self {
            pages,
            memory,
            class,
        }
    }
    fn copy(&self, id: &[u8; 32], max: usize) -> Result<ReservedValue<Vec<u8>>, ValueTreeError> {
        let cached =
            ValueNodeReader::read_node(self.pages, id, max, self.memory, MemoryClass::Ordinary)?;
        let bytes = cached
            .len()
            .checked_add(128)
            .ok_or(ValueTreeError::Invalid("classed fixture capacity"))?;
        let charge = self.memory.reserve(bytes, self.class)?;
        Ok(charge.retain(cached.as_slice().to_vec()))
    }
}
impl ValueNodeReader for ClassedPages<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if class != self.class || !memory.same_pool(self.memory) {
            return Err(ValueTreeError::Invalid("classed fixture pool/class"));
        }
        Ok(Arc::new(self.copy(id, max)?))
    }
}
impl MerkleNodeReader for ClassedPages<'_> {
    type Node = ClassedNode;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(self.class)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<ClassedNode>, String> {
        let id = *blake3::Hash::from_hex(id)
            .map_err(|e| e.to_string())?
            .as_bytes();
        self.copy(&id, max)
            .map(|p| Some(ClassedNode(p)))
            .map_err(|e| format!("{e:?}"))
    }
}
