//! Full scheduler-owner consistency verification for genesis/recovery. Never
//! call this full scan for each ordinary wake mutation or due selection.
use super::{incremental_body::BodyMutationError, incremental_scheduler_index::IndexReader};
use oasis7_formal_protocol::{
    incremental_schedule::{
        SCHEDULE_ROOT_BYTES, ScheduleIndexV1, ScheduleScanBudget, ScheduleScanContext,
    },
    incremental_state::{
        MerkleNodeReader, StateNamespaceV1, StateRootV1, StateScanBudget, StateScanContext,
        StateScanError,
    },
    incremental_value::{ValueIoBudget, ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::incremental_scheduler_fields::{
    MODULE_WAKE_BYTES, MODULE_WAKE_INDEX_KEY_V1, ModuleWakeKeyV1, ModuleWakeV1,
};
use std::cell::RefCell;

pub struct ModuleScheduleReadContext<'a, R, V> {
    pub state_root: &'a StateRootV1,
    pub state_source: &'a R,
    pub values: &'a V,
    pub memory: &'a MemoryBudget,
    pub class: MemoryClass,
    pub state_io: &'a mut StateScanBudget,
    pub value_io: &'a mut ValueIoBudget,
    pub index_io: &'a mut ScheduleScanBudget,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedModuleScheduleContent {
    pub index: ScheduleIndexV1,
    pub scalar_entries: u64,
}
impl<R: MerkleNodeReader, V: ValueNodeReader> ModuleScheduleReadContext<'_, R, V> {
    /// Two bounded-memory passes over the exact Scheduler state: first find
    /// the descriptor and count scalar keys; then prove every scalar pair is
    /// in the fully verified index. Equal cardinality proves no extra pair.
    /// No cache, authorization, durable head or complete-world marker is made.
    pub fn verify(
        &mut self,
        cancelled: impl FnMut() -> bool,
    ) -> Result<ReservedValue<VerifiedModuleScheduleContent>, BodyMutationError> {
        let mut workspace = self
            .memory
            .reserve(256 * 1024, self.class)
            .map_err(ValueTreeError::Memory)?;
        if self.state_root.namespace != StateNamespaceV1::Scheduler {
            return Err(BodyMutationError::Invalid("module schedule namespace"));
        }
        let cancelled = RefCell::new(cancelled);
        let mut descriptor = None;
        let mut scalars = 0u64;
        StateScanContext {
            source: self.state_source,
            memory: self.memory,
            class: self.class,
            io: self.state_io,
        }
        .visit_entries(
            self.state_root,
            |entry| {
                if entry.key == MODULE_WAKE_INDEX_KEY_V1 {
                    descriptor = Some(entry.value);
                } else if ModuleWakeKeyV1::is_wake_key(&entry.key) {
                    ModuleWakeKeyV1::instance_from_bytes(&entry.key)
                        .map_err(BodyMutationError::Invalid)?;
                    scalars = scalars.checked_add(1).ok_or(BodyMutationError::Invalid(
                        "module schedule scalar overflow",
                    ))?;
                }
                Ok::<_, BodyMutationError>(())
            },
            || (cancelled.borrow_mut())(),
        )
        .map_err(scan_error)?;
        let descriptor =
            descriptor.ok_or(BodyMutationError::Invalid("module schedule index absent"))?;
        let bytes = fixed::<SCHEDULE_ROOT_BYTES, _>(
            &descriptor,
            self.values,
            self.memory,
            self.class,
            self.value_io,
            || (cancelled.borrow_mut())(),
        )?;
        let index = ScheduleIndexV1::from_canonical_bytes(&bytes)?;
        if index.entries != scalars {
            return Err(BodyMutationError::Invalid(
                "module schedule scalar/index count",
            ));
        }
        {
            let source = IndexReader::new(self.values, self.memory, self.class, self.value_io);
            let result = ScheduleScanContext {
                source: &source,
                memory: self.memory,
                class: self.class,
                io: self.index_io,
            }
            .visit_due(
                &index,
                u64::MAX,
                |_, _| Ok::<_, BodyMutationError>(()),
                || (cancelled.borrow_mut())(),
            );
            if let Some(error) = source.take_error() {
                return Err(error.into());
            }
            result.map_err(scan_error)?;
        }
        StateScanContext {
            source: self.state_source,
            memory: self.memory,
            class: self.class,
            io: self.state_io,
        }
        .visit_entries(
            self.state_root,
            |entry| {
                if ModuleWakeKeyV1::is_wake_key(&entry.key) {
                    let instance = ModuleWakeKeyV1::instance_from_bytes(&entry.key)
                        .map_err(BodyMutationError::Invalid)?;
                    let bytes = fixed::<MODULE_WAKE_BYTES, _>(
                        &entry.value,
                        self.values,
                        self.memory,
                        self.class,
                        self.value_io,
                        || (cancelled.borrow_mut())(),
                    )?;
                    let wake = ModuleWakeV1::from_canonical_bytes(&bytes)
                        .map_err(BodyMutationError::Invalid)?
                        .0;
                    let reader =
                        IndexReader::new(self.values, self.memory, self.class, self.value_io);
                    if !reader.result(index.contains(wake, instance, &reader))? {
                        return Err(BodyMutationError::Invalid(
                            "module schedule scalar/index pair",
                        ));
                    }
                }
                Ok::<_, BodyMutationError>(())
            },
            || (cancelled.borrow_mut())(),
        )
        .map_err(scan_error)?;
        if (cancelled.borrow_mut())() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let charge = workspace
            .split(512)
            .map_err(|_| BodyMutationError::Invalid("module schedule output admission"))?;
        Ok(charge.retain(VerifiedModuleScheduleContent {
            index,
            scalar_entries: scalars,
        }))
    }
}
fn scan_error(error: StateScanError<BodyMutationError>) -> BodyMutationError {
    match error {
        StateScanError::Visitor(error) => error,
        StateScanError::Memory(error) => ValueTreeError::Memory(error).into(),
        StateScanError::ResourceLimited => {
            ValueTreeError::ResourceLimited("module schedule scan").into()
        }
        StateScanError::Cancelled => ValueTreeError::Cancelled.into(),
        StateScanError::Tree(error) => {
            oasis7_formal_protocol::incremental_execution::ExecutionError::Tree(error).into()
        }
        StateScanError::Invalid(reason) => BodyMutationError::Invalid(reason),
    }
}
fn fixed<const N: usize, V: ValueNodeReader>(
    id: &[u8; 32],
    source: &V,
    memory: &MemoryBudget,
    class: MemoryClass,
    io: &mut ValueIoBudget,
    cancelled: impl FnMut() -> bool,
) -> Result<[u8; N], BodyMutationError> {
    let mut reader = ValueReadContext {
        source,
        memory,
        class,
        io,
    };
    let root = reader.load_root(id)?;
    if root.0.logical_len != N as u64 {
        return Err(BodyMutationError::Invalid(
            "module schedule fixed value length",
        ));
    }
    let mut bytes = [0; N];
    reader.read_range(&root, 0, N as u64, &mut bytes.as_mut_slice(), cancelled)?;
    Ok(bytes)
}
