//! Owner-neutral, charged record-local state-page bridge and one-write builder.
use crate::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_world_field_key::WorldFieldKeyV1,
};
use oasis7_formal_protocol::{
    incremental_state::{
        MerkleNodeReader, StateDeltaV1, StatePageWriter, StateRootV1, StateWriteV1,
    },
    incremental_value::{ValueIoBudget, ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{cell::RefCell, sync::Arc};
#[derive(Clone, Copy)]
pub(crate) struct RecordTreeLabelsV1 {
    pub read: &'static str,
    pub pending: &'static str,
    pub tree: &'static str,
    pub digest: &'static str,
    pub read_allowance: &'static str,
    pub admission: &'static str,
    pub bound: &'static str,
    pub producer_pool: &'static str,
    pub page: &'static str,
    pub write_allowance: &'static str,
}
#[derive(Clone, Copy)]
pub(crate) struct RecordTreeWritePolicyV1 {
    pub max_pending_node_bytes: usize,
    pub labels: RecordTreeLabelsV1,
}
// One StateWrite uses the same conservative construction phase as the
// formal transaction adapter: heap structures/capacity, not logical pending bytes.
const SINGLE_DELTA_HEAP_BOUND: usize = 512 * 1024 + 128 * 1024;
pub(crate) struct Page(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Page {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
pub(crate) struct Source<'a, 'io, R, C> {
    pub source: &'a R,
    pub memory: &'a MemoryBudget,
    pub class: MemoryClass,
    pub io: &'a RefCell<&'io mut ValueIoBudget>,
    pub cancelled: &'a RefCell<&'io mut C>,
    pub error: &'a RefCell<Option<ValueTreeError>>,
    pub labels: RecordTreeLabelsV1,
}
impl<R: ValueNodeReader, C: FnMut() -> bool> Source<'_, '_, R, C> {
    fn fail(&self, error: ValueTreeError) -> String {
        if self.error.borrow().is_none() {
            *self.error.borrow_mut() = Some(error);
        }
        self.labels.read.into()
    }
    pub fn finish<T>(&self, result: Result<T, String>) -> Result<T, ValueTreeError> {
        if let Some(error) = self.error.borrow_mut().take() {
            return Err(error);
        }
        if (self.cancelled.borrow_mut())() {
            return Err(ValueTreeError::Cancelled);
        }
        result.map_err(|error| {
            if error.starts_with("ResourceLimited:") {
                ValueTreeError::ResourceLimited(self.labels.pending)
            } else {
                ValueTreeError::Invalid(self.labels.tree)
            }
        })
    }
}
impl<R: ValueNodeReader, C: FnMut() -> bool> MerkleNodeReader for Source<'_, '_, R, C> {
    type Node = Page;
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(self.class)
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn read_node(&self, digest: &str, max_bytes: usize) -> Result<Option<Self::Node>, String> {
        if (self.cancelled.borrow_mut())() {
            return Err(self.fail(ValueTreeError::Cancelled));
        }
        let mut id = [0u8; 32];
        if digest.len() != 64 {
            return Err(self.fail(ValueTreeError::Invalid(self.labels.digest)));
        }
        for (index, pair) in digest.as_bytes().chunks_exact(2).enumerate() {
            let nibble = |b: u8| match b {
                b'0'..=b'9' => Some(b - b'0'),
                b'a'..=b'f' => Some(b - b'a' + 10),
                _ => None,
            };
            let (Some(a), Some(b)) = (nibble(pair[0]), nibble(pair[1])) else {
                return Err(self.fail(ValueTreeError::Invalid(self.labels.digest)));
            };
            id[index] = (a << 4) | b;
        }
        {
            let mut io = self.io.borrow_mut();
            if io.nodes_remaining == 0 || io.read_bytes_remaining < max_bytes as u64 {
                return Err(self.fail(ValueTreeError::ResourceLimited(self.labels.read_allowance)));
            }
            io.nodes_remaining -= 1;
            io.read_bytes_remaining -= max_bytes as u64;
        }
        let result = self
            .source
            .read_node(&id, max_bytes, self.memory, self.class);
        if (self.cancelled.borrow_mut())() {
            return Err(self.fail(ValueTreeError::Cancelled));
        }
        let bytes = result.map_err(|error| self.fail(error))?;
        if !bytes.uses_budget(self.memory) || bytes.reserved_bytes() < bytes.capacity() {
            return Err(self.fail(ValueTreeError::Invalid(self.labels.admission)));
        }
        if bytes.len() > max_bytes {
            return Err(self.fail(ValueTreeError::Invalid(self.labels.bound)));
        }
        self.io.borrow_mut().read_bytes_remaining += (max_bytes - bytes.len()) as u64;
        Ok(Some(Page(bytes)))
    }
}
pub(crate) fn emit_nodes<W: StatePageWriter, C: FnMut() -> bool>(
    delta: &ReservedValue<StateDeltaV1>,
    sink: &mut W,
    memory: &MemoryBudget,
    io: &mut ValueIoBudget,
    cancelled: &mut C,
    labels: RecordTreeLabelsV1,
) -> Result<(), ValueTreeError> {
    if !delta.uses_budget(memory) {
        return Err(ValueTreeError::Invalid(labels.producer_pool));
    }
    {
        let _validation = memory.reserve(64 << 10, MemoryClass::Ordinary)?;
        for page in &delta.new_nodes {
            if cancelled() {
                return Err(ValueTreeError::Cancelled);
            }
            page.verified_links()
                .map_err(|_| ValueTreeError::Invalid(labels.page))?;
            if io.nodes_remaining == 0 || io.write_bytes_remaining < page.bytes.len() as u64 {
                return Err(ValueTreeError::ResourceLimited(labels.write_allowance));
            }
            io.nodes_remaining -= 1;
            io.write_bytes_remaining -= page.bytes.len() as u64;
        }
    }
    sink.put_state_pages_cancelled(delta, memory, cancelled)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}
pub(crate) fn edit<
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
    policy: RecordTreeWritePolicyV1,
) -> Result<StateRootV1, ValueTreeError> {
    let RecordTreeWritePolicyV1 {
        max_pending_node_bytes,
        labels,
    } = policy;
    let _workspace = write.memory.reserve(8192, MemoryClass::Ordinary)?;
    let charge = write
        .memory
        .reserve(SINGLE_DELTA_HEAP_BOUND, MemoryClass::Ordinary)?;
    let mut combined = || cancelled() || (write.cancelled)();
    let delta = {
        let source = Source {
            source: values.source,
            memory: values.memory,
            class: values.class,
            io: &RefCell::new(&mut *values.io),
            cancelled: &RefCell::new(&mut combined),
            error: &RefCell::new(None),
            labels,
        };
        charge.retain(source.finish(StateDeltaV1::build(
            root,
            vec![StateWriteV1 {
                key: key.as_bytes().to_vec(),
                before: change.0,
                after: change.1,
            }],
            &source,
            max_pending_node_bytes,
        ))?)
    };
    emit_nodes(
        &delta,
        write.sink,
        write.memory,
        write.io,
        &mut combined,
        labels,
    )?;
    Ok(delta.after.clone())
}

/// Charged authenticated point lookup over a record-local StateRoot.
pub(crate) fn get<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    key: &WorldFieldKeyV1,
    values: &mut oasis7_formal_protocol::incremental_value::ValueReadContext<'_, R>,
    cancelled: &mut C,
    labels: RecordTreeLabelsV1,
) -> Result<Option<[u8; 32]>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let _workspace = match values.memory.reserve(8 << 10, values.class) {
        Ok(workspace) => workspace,
        Err(_) if cancelled() => return Err(ValueTreeError::Cancelled),
        Err(error) => return Err(ValueTreeError::Memory(error)),
    };
    let error = RefCell::new(None);
    let io = RefCell::new(&mut *values.io);
    let cancelled = RefCell::new(cancelled);
    let source = Source {
        source: values.source,
        memory: values.memory,
        class: values.class,
        io: &io,
        cancelled: &cancelled,
        error: &error,
        labels,
    };
    source.finish(root.get(key.as_bytes(), &source))
}

pub(crate) fn append<W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    root: StateRootV1,
    key: &WorldFieldKeyV1,
    value: [u8; 32],
    context: &mut MemberRecordWriteContext<'_, W, C>,
    policy: RecordTreeWritePolicyV1,
) -> Result<StateRootV1, ValueTreeError> {
    let RecordTreeWritePolicyV1 {
        max_pending_node_bytes,
        labels,
    } = policy;
    let _workspace = context.memory.reserve(8192, MemoryClass::Ordinary)?;
    let charge = context
        .memory
        .reserve(SINGLE_DELTA_HEAP_BOUND, MemoryClass::Ordinary)?;
    let delta = {
        let source = Source {
            source: &*context.sink,
            memory: context.memory,
            class: MemoryClass::Ordinary,
            io: &RefCell::new(&mut *context.io),
            cancelled: &RefCell::new(&mut context.cancelled),
            error: &RefCell::new(None),
            labels,
        };
        charge.retain(source.finish(StateDeltaV1::build(
            root,
            vec![StateWriteV1 {
                key: key.as_bytes().to_vec(),
                before: None,
                after: Some(value),
            }],
            &source,
            max_pending_node_bytes,
        ))?)
    };
    emit_nodes(
        &delta,
        context.sink,
        context.memory,
        context.io,
        &mut context.cancelled,
        labels,
    )?;
    Ok(delta.after.clone())
}

/// Charged authenticated point lookup over a record-local StateRoot.
pub(crate) fn lookup<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    key: &WorldFieldKeyV1,
    records: &mut crate::incremental_inventory_value_read::InventoryValueReader<'_, '_, R, C>,
    labels: RecordTreeLabelsV1,
) -> Result<Option<[u8; 32]>, ValueTreeError> {
    let _point = records
        .values
        .memory
        .reserve(8 << 10, records.values.class)?;
    let source = Source {
        source: records.values.source,
        memory: records.values.memory,
        class: records.values.class,
        io: &RefCell::new(&mut *records.values.io),
        cancelled: &RefCell::new(&mut *records.cancelled),
        error: &RefCell::new(None),
        labels,
    };
    source.finish(root.get(key.as_bytes(), &source))
}
