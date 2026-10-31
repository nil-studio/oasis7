//! Bounded record-local state trees over the caller's shared page source.
use super::*;
use crate::incremental_inventory_value_read::InventoryValueReader;
use oasis7_formal_protocol::{
    incremental_state::{MerkleNodeReader, StateRootV1, StateScanBudget},
    incremental_value::{ValueIoBudget, ValueNodeReader},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use std::cell::RefCell;
/// Continuous caller-owned work limits. No reader/writer resets these in a
/// record loop. The pending-node bound limits logical Merkle work; heap
/// construction uses a separate conservative phase admission.
pub struct FactoryWorkBudgetV1 {
    pub cells_remaining: u64,
    pub max_pending_node_bytes: usize,
    pub scan: StateScanBudget,
}
impl FactoryWorkBudgetV1 {
    pub(super) fn cell(&mut self) -> Result<(), ValueTreeError> {
        self.cells_remaining = self
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("factory cell work"))?;
        Ok(())
    }
}
use crate::incremental_record_tree as shared;
const LABELS: shared::RecordTreeLabelsV1 = shared::RecordTreeLabelsV1 {
    read: "factory child state read",
    pending: "factory pending node allowance",
    tree: "factory child state tree",
    digest: "factory state node digest",
    read_allowance: "factory child read allowance",
    admission: "factory child source admission",
    bound: "factory child source bound",
    producer_pool: "factory child producer pool",
    page: "factory child page",
    write_allowance: "factory child write allowance",
};
pub(super) use shared::Page;
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
            labels: LABELS,
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
pub(super) fn roots<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    keys: &read::Keys,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<read::Roots, ValueTreeError> {
    let _codec = records
        .values
        .memory
        .reserve(64 << 10, records.values.class)?;
    let source = Source {
        source: records.values.source,
        memory: records.values.memory,
        class: records.values.class,
        io: RefCell::new(&mut *records.values.io),
        cancelled: RefCell::new(&mut *records.cancelled),
        error: RefCell::new(None),
    };
    let mut roots = [None; 5];
    for (index, key) in keys.iter().enumerate() {
        if let Some((key, _)) = key {
            roots[index] = source.finish(root.get(key.as_bytes(), &source))?;
        }
    }
    Ok(roots)
}
pub(super) fn edit<
    R: ValueNodeReader,
    W: StatePageWriter,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
>(
    root: StateRootV1,
    key: &WorldFieldKeyV1,
    change: (Option<[u8; 32]>, Option<[u8; 32]>),
    values: &mut oasis7_formal_protocol::incremental_value::ValueReadContext<'_, R>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    cancelled: &mut C,
    work: &FactoryWorkBudgetV1,
) -> Result<StateRootV1, ValueTreeError> {
    shared::edit(
        root,
        key,
        change,
        values,
        write,
        cancelled,
        shared::RecordTreeWritePolicyV1 {
            max_pending_node_bytes: work.max_pending_node_bytes,
            labels: LABELS,
        },
    )
}
pub(super) fn append<W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    root: StateRootV1,
    key: &WorldFieldKeyV1,
    value: [u8; 32],
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &FactoryWorkBudgetV1,
) -> Result<StateRootV1, ValueTreeError> {
    shared::append(
        root,
        key,
        value,
        context,
        shared::RecordTreeWritePolicyV1 {
            max_pending_node_bytes: work.max_pending_node_bytes,
            labels: LABELS,
        },
    )
}
