//! Original scalar contents in one transaction; no whole-pending-group qualification.
use super::*;
use crate::{
    incremental_expected_bytes::Compare,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_world_field_key::WorldFieldKeyV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueNodeWriter},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("runtime scheduler scalar expected/profile/binding")
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeSchedulerControlValueRefV1<'a> {
    Unit,
    U64(u64),
    Text(&'a str),
}
impl<'a> RuntimeSchedulerControlValueRefV1<'a> {
    pub(super) fn payload(self) -> MemberPayloadRefV1<'a> {
        match self {
            Self::Unit => MemberPayloadRefV1::Unit,
            Self::U64(n) => MemberPayloadRefV1::U64(n),
            Self::Text(s) => MemberPayloadRefV1::Text(s),
        }
    }
    pub(super) fn text(self) -> &'a str {
        if let Self::Text(s) = self { s } else { "" }
    }
    pub(super) fn validate(
        self,
        profile: (MemberPayloadCodecV1, bool, u64),
    ) -> Result<(), ValueTreeError> {
        let valid = match self {
            Self::Unit => profile.1,
            Self::U64(n) => profile.0 == MemberPayloadCodecV1::U64 && n <= profile.2,
            Self::Text(_) => profile.0 == MemberPayloadCodecV1::Text,
        };
        if valid { Ok(()) } else { Err(invalid()) }
    }
    pub(super) fn matches(self, value: RuntimeSchedulerControlValueV1) -> bool {
        match (self, value) {
            (Self::Unit, RuntimeSchedulerControlValueV1::Unit) => true,
            (Self::U64(a), RuntimeSchedulerControlValueV1::Unsigned(b)) => a == b,
            (Self::Text(a), RuntimeSchedulerControlValueV1::Text { utf8_bytes }) => {
                a.len() as u64 == utf8_bytes
            }
            _ => false,
        }
    }
}
pub struct RuntimeSchedulerScalarRequestV1<'a> {
    pub field: RuntimeSchedulerScalarV1,
    pub qualification: &'a ReservedValue<RuntimeSchedulerControlsQualificationV1>,
}
pub(super) fn parent_binding<M: MerkleNodeReader, R: ValueNodeReader, C: FnMut() -> bool>(
    proof: &ReservedValue<RuntimeSchedulerControlsQualificationV1>,
    tx: &ExecutionTransaction<'_, M>,
    records: &InventoryValueReader<'_, '_, R, C>,
) -> Result<(), ValueTreeError> {
    if records.values.class != MemoryClass::Ordinary
        || !tx.uses_budget(records.values.memory)
        || !proof.uses_budget(records.values.memory)
    {
        return Err(invalid());
    }
    proof.validate_binding(&tx.parent_fence().parent_roots)
}
pub(super) fn metadata<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    name: &'static str,
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<(), WorldFieldOverlayErrorV1> {
    let key = WorldFieldKeyV1::new(descriptor(name), WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let value = lookup(tx, &key, records, error)?.ok_or_else(invalid)?;
    records.presence(&value, &key, 0)?;
    Ok(())
}
fn resolve<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: RuntimeSchedulerScalarRequestV1<'_>,
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
    max: u64,
    output: &mut impl std::io::Write,
) -> Result<(WorldFieldKeyV1, RuntimeSchedulerControlValueV1), WorldFieldOverlayErrorV1> {
    parent_binding(request.qualification, tx, records)?;
    metadata(request.field.name(), tx, records, error)?;
    let key = read::outer_key(request.field.name(), records)?;
    let root = lookup(tx, &key.0, records, error)?.ok_or_else(invalid)?;
    let value = read::value(request.field.profile(), &root, &key, records, max, output)?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok((key.0, value))
}
pub fn read_qualified_runtime_scheduler_scalar_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: RuntimeSchedulerScalarRequestV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max_payload_bytes: u64,
    output: &mut impl std::io::Write,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<RuntimeSchedulerControlValueV1, WorldFieldOverlayErrorV1> {
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        let mut records = InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        parent_binding(request.qualification, tx, &records)?;
        let _admission = records
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(resolve(
            request,
            tx,
            &mut records,
            &mut ctx.take_state_error,
            max_payload_bytes,
            output,
        )?
        .1)
    })
}
pub struct RuntimeSchedulerScalarChangeV1<'a> {
    pub field: RuntimeSchedulerScalarV1,
    pub qualification: &'a ReservedValue<RuntimeSchedulerControlsQualificationV1>,
    pub expected: RuntimeSchedulerControlValueRefV1<'a>,
    pub after: RuntimeSchedulerControlValueRefV1<'a>,
    pub max_current_payload_bytes: u64,
}
/// The parent token bounds inheritance, not arbitrary untyped pending writes.
/// A full typed receiver still validates the final after-root before acceptance.
pub fn change_qualified_runtime_scheduler_scalar_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
>(
    change: RuntimeSchedulerScalarChangeV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let cancel = &mut ctx.cancelled;
    let error = &mut ctx.take_state_error;
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if !tx.uses_budget(write.memory) {
            return Err(invalid().into());
        }
        change.expected.validate(change.field.profile())?;
        change.after.validate(change.field.profile())?;
        let _admission = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let (key, current) = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut combined,
            };
            let mut compare = Compare::new(change.expected.text().as_bytes());
            let (key, current) = resolve(
                RuntimeSchedulerScalarRequestV1 {
                    field: change.field,
                    qualification: change.qualification,
                },
                tx,
                &mut records,
                error,
                change.max_current_payload_bytes,
                &mut compare,
            )?;
            if !change.expected.matches(current) || !compare.is_equal() {
                return Err(invalid().into());
            }
            (key, current)
        };
        let _ = current;
        let unchanged = change.expected == change.after;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if unchanged {
            return Ok(false);
        }
        let row = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            write_member_record_v1(
                descriptor(change.field.name()),
                WorldFieldRecordKindV1::Entry,
                &(),
                change.after.payload(),
                &mut combined,
            )?
        };
        if row.field_key != key {
            return Err(invalid().into());
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(
            StateNamespaceV1::Scheduler,
            key.as_bytes(),
            Some(row.value_root),
        )?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}
