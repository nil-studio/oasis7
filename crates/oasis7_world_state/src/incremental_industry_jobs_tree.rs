use super::*;
use crate::incremental_record_tree as shared;
use oasis7_formal_protocol::{
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueIoBudget, ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
pub(super) use shared::Page;
use std::cell::RefCell;
pub(super) struct Source<'a, R, C> {
    pub source: &'a R,
    pub memory: &'a MemoryBudget,
    pub class: MemoryClass,
    pub io: RefCell<&'a mut ValueIoBudget>,
    pub cancelled: RefCell<&'a mut C>,
    pub error: RefCell<Option<ValueTreeError>>,
}
impl<'io, R: ValueNodeReader, C: FnMut() -> bool> Source<'io, R, C> {
    fn adapter(&self) -> shared::Source<'_, 'io, R, C> {
        shared::Source {
            source: self.source,
            memory: self.memory,
            class: self.class,
            io: &self.io,
            cancelled: &self.cancelled,
            error: &self.error,
            labels: genesis::LABELS,
        }
    }
    pub fn finish<T>(&self, result: Result<T, String>) -> Result<T, ValueTreeError> {
        self.adapter().finish(result)
    }
}
impl<R: ValueNodeReader, C: FnMut() -> bool> MerkleNodeReader for Source<'_, R, C> {
    type Node = Page;
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(self.class)
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn read_node(&self, digest: &str, max_bytes: usize) -> Result<Option<Self::Node>, String> {
        self.adapter().read_node(digest, max_bytes)
    }
}
