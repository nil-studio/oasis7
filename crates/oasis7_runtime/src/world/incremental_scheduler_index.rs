//! Shared content-page bridge for the scheduler owner. Index nodes use the
//! same charged immutable page pool as values, with explicit logical IO work.
use super::{
    incremental_body::{BodyMutationContext, BodyMutationError, write_field_bytes},
    incremental_body_event::read_fixed,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionTransaction},
    incremental_schedule::{
        SCHEDULE_ROOT_BYTES, ScheduleDeltaV1, ScheduleIndexV1, SchedulePageWriter,
    },
    incremental_state::{MerkleNodeReader, StateNamespaceV1 as Ns},
    incremental_value::{ValueIoBudget, ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::incremental_scheduler_fields::{
    MODULE_WAKE_INDEX_KEY_V1, ModuleWakeKeyV1, ModuleWakeV1,
};
use std::{
    cell::{Cell, RefCell},
    sync::Arc,
};

pub(super) struct Charged(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Charged {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
pub(super) struct IndexReader<'a, V> {
    values: &'a V,
    memory: &'a MemoryBudget,
    class: MemoryClass,
    nodes: &'a Cell<u64>,
    bytes: &'a Cell<u64>,
    error: RefCell<Option<ValueTreeError>>,
}
impl<'a, V> IndexReader<'a, V> {
    pub(super) fn new(
        values: &'a V,
        memory: &'a MemoryBudget,
        class: MemoryClass,
        io: &'a mut ValueIoBudget,
    ) -> Self {
        Self {
            values,
            memory,
            class,
            nodes: Cell::from_mut(&mut io.nodes_remaining),
            bytes: Cell::from_mut(&mut io.read_bytes_remaining),
            error: RefCell::new(None),
        }
    }
    pub(super) fn take_error(&self) -> Option<ValueTreeError> {
        self.error.borrow_mut().take()
    }
    pub(super) fn result<T>(
        &self,
        result: Result<T, ExecutionError>,
    ) -> Result<T, BodyMutationError> {
        match result {
            Ok(value) => Ok(value),
            Err(error) => Err(self
                .error
                .borrow_mut()
                .take()
                .map(BodyMutationError::Value)
                .unwrap_or(BodyMutationError::Execution(error))),
        }
    }
}
impl<V: ValueNodeReader> MerkleNodeReader for IndexReader<'_, V> {
    type Node = Charged;
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(self.class)
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        let result = (|| {
            if self.nodes.get() == 0 || self.bytes.get() == 0 {
                return Err(ValueTreeError::ResourceLimited("scheduler index reads"));
            }
            self.nodes.set(self.nodes.get() - 1);
            let admitted = max.min(self.bytes.get().min(usize::MAX as u64) as usize);
            self.bytes.set(self.bytes.get() - admitted as u64);
            let id: [u8; 32] = hex::decode(id)
                .map_err(|_| ValueTreeError::Invalid("scheduler page ID"))?
                .try_into()
                .map_err(|_| ValueTreeError::Invalid("scheduler page ID length"))?;
            let bytes = self
                .values
                .read_node(&id, admitted, self.memory, self.class)?;
            if bytes.len() > admitted
                || !bytes.uses_budget(self.memory)
                || bytes.reserved_bytes() < bytes.capacity()
            {
                return Err(ValueTreeError::Invalid(
                    "scheduler source bound/charge/pool",
                ));
            }
            self.bytes
                .set(self.bytes.get() + (admitted - bytes.len()) as u64);
            Ok(Some(Charged(bytes)))
        })();
        result.map_err(|error| {
            let text = error.to_string();
            let mut first = self.error.borrow_mut();
            if first.is_none() {
                *first = Some(error);
            }
            text
        })
    }
}
pub(super) fn build<W: ValueNodeReader, C>(
    root: &ScheduleIndexV1,
    instance: &str,
    before: Option<u64>,
    after: Option<u64>,
    ctx: &mut BodyMutationContext<'_, W, C>,
) -> Result<ReservedValue<ScheduleDeltaV1>, BodyMutationError> {
    let reader = IndexReader::new(&*ctx.sink, ctx.memory, MemoryClass::Ordinary, ctx.io);
    reader.result(root.update(instance, before, after, &reader, ctx.memory, 256 * 1024))
}
pub(super) fn persist<W: SchedulePageWriter, C: FnMut() -> bool>(
    delta: &ReservedValue<ScheduleDeltaV1>,
    ctx: &mut BodyMutationContext<'_, W, C>,
) -> Result<(), BodyMutationError> {
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let nodes = delta.new_nodes.len() as u64;
    let bytes = delta
        .new_nodes
        .iter()
        .try_fold(0u64, |sum, page| sum.checked_add(page.bytes.len() as u64))
        .ok_or(BodyMutationError::Invalid("scheduler write byte overflow"))?;
    if ctx.io.nodes_remaining < nodes || ctx.io.write_bytes_remaining < bytes {
        return Err(ValueTreeError::ResourceLimited("scheduler index writes").into());
    }
    ctx.io.nodes_remaining -= nodes;
    ctx.io.write_bytes_remaining -= bytes;
    ctx.sink.put_schedule_pages(delta, ctx.memory)?;
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
pub(super) fn stage<
    R: MerkleNodeReader,
    V: ValueNodeReader,
    W: SchedulePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    instance: &str,
    before: Option<u64>,
    after: Option<u64>,
    txn: &mut ExecutionTransaction<'_, R>,
    values: &V,
    ctx: &mut BodyMutationContext<'_, W, C>,
) -> Result<(), BodyMutationError> {
    let key = ModuleWakeKeyV1::new(instance).map_err(BodyMutationError::Invalid)?;
    let bytes = read_fixed::<SCHEDULE_ROOT_BYTES, _, _, _, _>(
        txn,
        Ns::Scheduler,
        MODULE_WAKE_INDEX_KEY_V1,
        values,
        ctx,
    )?;
    let root = ScheduleIndexV1::from_canonical_bytes(&bytes)?;
    let delta = build(&root, instance, before, after, ctx)?;
    persist(&delta, ctx)?;
    if delta.before != delta.after {
        let id = write_field_bytes(&delta.after.canonical_bytes()?, ctx)?;
        txn.write(Ns::Scheduler, MODULE_WAKE_INDEX_KEY_V1, Some(id))?;
    }
    if before != after {
        let id = after
            .map(|wake| write_field_bytes(&ModuleWakeV1(wake).canonical_bytes(), ctx))
            .transpose()?;
        txn.write(Ns::Scheduler, key.as_bytes(), id)?;
    }
    Ok(())
}
