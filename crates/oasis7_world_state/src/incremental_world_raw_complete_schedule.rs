//! Exact original module-wake scalars and ordered index; no lifecycle authority.
use crate::{
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_scheduler_fields::{
        MODULE_WAKE_BYTES, MODULE_WAKE_INDEX_KEY_V1, ModuleWakeKeyV1, ModuleWakeV1,
    },
    incremental_verified_world_transition::VerifiedWorldTransitionV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{RootSetV1, TransitionRecordV1},
    incremental_schedule::{SCHEDULE_ROOT_BYTES, ScheduleIndexV1, ScheduleScanBudget},
    incremental_state::{
        MerkleNodeReader, StateNamespaceV1, StateScanBudget, StateScanContext, StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{cell::RefCell, io::Cursor};
#[path = "incremental_world_raw_complete_schedule_diff.rs"]
mod diff;
pub struct WorldRawScheduleWorkV1 {
    pub cells_remaining: u64,
    pub index: ScheduleScanBudget,
}
pub struct WorldRawScheduleQualificationV1 {
    roots: [u8; 32],
    class: MemoryClass,
    index_value: [u8; 32],
    index: ScheduleIndexV1,
}
const CARRIER: usize = std::mem::size_of::<WorldRawScheduleQualificationV1>() + 512;
impl WorldRawScheduleQualificationV1 {
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), WorldFieldOverlayErrorV1> {
        if self.roots != roots.digest()? {
            return Err(invalid().into());
        }
        Ok(())
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub fn entries(&self) -> u64 {
        self.index.entries
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("raw module schedule scalar/index closure")
}
fn check<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    ctx.check()?;
    if let Some(e) = (ctx.take_state_error)() {
        return Err(e);
    }
    Ok(())
}
fn spend(work: &mut WorldRawScheduleWorkV1) -> Result<(), ValueTreeError> {
    work.cells_remaining = work
        .cells_remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("raw schedule cells"))?;
    Ok(())
}
pub(super) fn fixed<
    const N: usize,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &[u8; 32],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<[u8; N], ValueTreeError> {
    check(ctx)?;
    let root = ctx.values.load_root(id)?;
    if root.0.logical_len != N as u64 {
        return Err(invalid());
    }
    let mut bytes = [0; N];
    ctx.values.read_range(
        &root,
        0,
        N as u64,
        &mut Cursor::new(&mut bytes[..]),
        &mut ctx.cancelled,
    )?;
    ctx.values
        .verify_closure(id, N as u64, &mut ctx.cancelled)?;
    check(ctx)?;
    Ok(bytes)
}
fn wake<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &[u8; 32],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<u64, ValueTreeError> {
    ModuleWakeV1::from_canonical_bytes(&fixed::<MODULE_WAKE_BYTES, _, _, _, _>(id, ctx)?)
        .map(|v| v.0)
        .map_err(ValueTreeError::Invalid)
}
fn read_index<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &[u8; 32],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<ScheduleIndexV1, WorldFieldOverlayErrorV1> {
    Ok(ScheduleIndexV1::from_canonical_bytes(&fixed::<
        SCHEDULE_ROOT_BYTES,
        _,
        _,
        _,
        _,
    >(id, ctx)?)?)
}
fn after_value<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    record: Option<&TransitionRecordV1>,
    key: &[u8],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<[u8; 32]>, ValueTreeError> {
    check(ctx)?;
    if let Some(record) = record {
        if let Some(w) = record
            .writes
            .iter()
            .find(|w| w.namespace == StateNamespaceV1::Scheduler && w.mutation.key == key)
        {
            return Ok(w.mutation.after);
        }
    }
    ctx.lookup_raw(roots.state_root(StateNamespaceV1::Scheduler), key)
}
pub fn qualify_world_raw_schedule_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldRawScheduleWorkV1,
) -> Result<ReservedValue<WorldRawScheduleQualificationV1>, WorldFieldOverlayErrorV1> {
    let result = (|| {
        check(ctx)?;
        let _phase = ctx
            .values
            .memory
            .reserve(128 << 10, ctx.values.class)
            .map_err(ValueTreeError::Memory)?;
        roots.validate()?;
        if !roots.state.iter().any(|r| r == ctx.root) {
            return Err(invalid().into());
        }
        let charge = ctx
            .values
            .memory
            .reserve(CARRIER, ctx.values.class)
            .map_err(ValueTreeError::Memory)?;
        let index_value =
            after_value(roots, None, MODULE_WAKE_INDEX_KEY_V1, ctx)?.ok_or_else(invalid)?;
        let index = read_index(&index_value, ctx)?;
        let mut scalars = 0u64;
        let mut indexes = 0u64;
        {
            let stop = RefCell::new(&mut ctx.cancelled);
            let mut local = WorldFieldReadContextV1 {
                root: ctx.root,
                state: ctx.state,
                values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                    source: ctx.values.source,
                    memory: ctx.values.memory,
                    class: ctx.values.class,
                    io: &mut *ctx.values.io,
                },
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                take_state_error: &mut ctx.take_state_error,
                cancelled: || stop.borrow_mut()(),
            };
            for root in &roots.state {
                let result = StateScanContext {
                    source: local.state,
                    memory: local.values.memory,
                    class: local.values.class,
                    io: &mut *scan,
                }
                .visit_entries(
                    root,
                    |entry| {
                        check(&mut local)?;
                        spend(work)?;
                        if entry.key == MODULE_WAKE_INDEX_KEY_V1 {
                            if entry.namespace != StateNamespaceV1::Scheduler
                                || entry.value != index_value
                            {
                                return Err(invalid());
                            }
                            indexes = indexes.checked_add(1).ok_or_else(invalid)?;
                        } else if ModuleWakeKeyV1::is_wake_key(&entry.key) {
                            if entry.namespace != StateNamespaceV1::Scheduler {
                                return Err(invalid());
                            }
                            ModuleWakeKeyV1::instance_from_bytes(&entry.key)
                                .map_err(ValueTreeError::Invalid)?;
                            wake(&entry.value, &mut local)?;
                            scalars = scalars.checked_add(1).ok_or_else(invalid)?;
                        }
                        Ok(())
                    },
                    || stop.borrow_mut()(),
                );
                check(&mut local)?;
                match result {
                    Ok(_) => {}
                    Err(StateScanError::Visitor(e)) => return Err(e.into()),
                    Err(StateScanError::Memory(e)) => return Err(ValueTreeError::Memory(e).into()),
                    Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled.into()),
                    Err(StateScanError::ResourceLimited) => {
                        return Err(
                            ValueTreeError::ResourceLimited("raw schedule scalar scan").into()
                        );
                    }
                    Err(_) => return Err(invalid().into()),
                }
            }
        }
        if indexes != 1 || scalars != index.entries {
            return Err(invalid().into());
        }
        diff::advance(&ScheduleIndexV1::empty()?, &index, roots, None, ctx, work)?;
        check(ctx)?;
        Ok(charge.retain(WorldRawScheduleQualificationV1 {
            roots: roots.digest()?,
            class: ctx.values.class,
            index_value,
            index,
        }))
    })();
    check(ctx)?;
    result
}
pub(crate) fn accept_world_raw_schedule_verified_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<WorldRawScheduleQualificationV1>,
    witness: &VerifiedWorldTransitionV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldRawScheduleWorkV1,
) -> Result<ReservedValue<WorldRawScheduleQualificationV1>, WorldFieldOverlayErrorV1> {
    let result = (|| {
        check(ctx)?;
        witness.check_context(ctx.values.memory, ctx.values.class, ctx.root)?;
        if ctx.values.class != MemoryClass::Ordinary
            || parent.class != MemoryClass::Ordinary
            || !parent.uses_budget(ctx.values.memory)
            || parent.reserved_bytes() < CARRIER
        {
            return Err(invalid().into());
        }
        let _phase = ctx
            .values
            .memory
            .reserve(128 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let record = witness.record();
        parent.validate_binding(&record.before)?;
        let charge = ctx
            .values
            .memory
            .reserve(CARRIER, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut value = parent.index_value;
        let mut count = i128::from(parent.index.entries);
        for write in &record.writes {
            check(ctx)?;
            spend(work)?;
            if write.mutation.key == MODULE_WAKE_INDEX_KEY_V1 {
                if write.namespace != StateNamespaceV1::Scheduler
                    || write.mutation.before != Some(parent.index_value)
                {
                    return Err(invalid().into());
                }
                value = write.mutation.after.ok_or_else(invalid)?;
            } else if ModuleWakeKeyV1::is_wake_key(&write.mutation.key) {
                if write.namespace != StateNamespaceV1::Scheduler {
                    return Err(invalid().into());
                }
                ModuleWakeKeyV1::instance_from_bytes(&write.mutation.key)
                    .map_err(ValueTreeError::Invalid)?;
                if let Some(id) = write.mutation.before {
                    wake(&id, ctx)?;
                    count -= 1;
                }
                if let Some(id) = write.mutation.after {
                    wake(&id, ctx)?;
                    count += 1;
                }
            }
        }
        let index = if value == parent.index_value {
            parent.index.clone()
        } else {
            read_index(&value, ctx)?
        };
        if u64::try_from(count).map_err(|_| invalid())? != index.entries {
            return Err(invalid().into());
        }
        diff::advance(
            &parent.index,
            &index,
            &record.before,
            Some(record),
            ctx,
            work,
        )?;
        // A scalar mutation without an index mutation cannot silently change a wake.
        // Changed/new scalars must be witnessed in the exact after index as well.
        diff::check_changed_scalars(&index, record, ctx, work)?;
        check(ctx)?;
        Ok(charge.retain(WorldRawScheduleQualificationV1 {
            roots: *witness.after_digest(),
            class: MemoryClass::Ordinary,
            index_value: value,
            index,
        }))
    })();
    check(ctx)?;
    result
}
pub fn readmit_world_raw_schedule_ordinary_v1(
    source: &ReservedValue<WorldRawScheduleQualificationV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<WorldRawScheduleQualificationV1>, WorldFieldOverlayErrorV1> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if source.class != MemoryClass::Recovery
        || !source.uses_budget(memory)
        || source.reserved_bytes() < CARRIER
    {
        return Err(invalid().into());
    }
    let _phase = memory
        .reserve(128 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    source.validate_binding(roots)?;
    let charge = memory
        .reserve(CARRIER, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let output = charge.retain(WorldRawScheduleQualificationV1 {
        roots: source.roots,
        class: MemoryClass::Ordinary,
        index_value: source.index_value,
        index: source.index.clone(),
    });
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(output)
}
