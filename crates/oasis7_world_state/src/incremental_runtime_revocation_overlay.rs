//! Provisional exact replacement of an already qualified revocation body.
use super::*;
use crate::incremental_field_overlay::{
    WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup,
};
use crate::incremental_inventory_value_read::InventoryValueReader;
use oasis7_formal_protocol::incremental_execution::{ExecutionFence, PreparedGuard};
use oasis7_proto::memory_budget::ReservedValue;
/// Both current and replacement body references are exact original typed roots.
/// This adapter grants no revocation/business authorization or head installation.
pub fn change_runtime_revocation_overlay_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    ER: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    CR: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
>(
    qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
    expected: &StateRootV1,
    after: &StateRootV1,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let stop = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|transaction| {
        transaction.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || read.values.class != MemoryClass::Ordinary
            || qualification.allocation_class() != MemoryClass::Ordinary
            || !qualification.uses_budget(context.values.memory)
            || qualification.reserved_bytes() < 512
            || !transaction.uses_budget(context.values.memory)
            || !transaction.uses_budget(read.values.memory)
            || !transaction.uses_budget(write.memory)
        {
            return Err(invalid().into());
        }
        read.check()?;
        if let Some(error) = (read.take_state_error)() {
            return Err(error.into());
        }
        qualification.validate_digest(&transaction.parent_fence().parent_roots)?;
        let field = family::outer();
        let meta = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let (key, current) = {
            let mut combined = || stop() || (write.cancelled)() || (read.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            let metadata = lookup(transaction, &meta, &mut records, error)?.ok_or_else(invalid)?;
            records.presence(&metadata, &meta, 0)?;
            let (key, _) = records.key(field, WorldFieldRecordKindV1::Entry, &())?;
            let value = lookup(transaction, &key, &mut records, error)?.ok_or_else(invalid)?;
            (key, value)
        };
        let current = {
            let mut combined_read = WorldFieldReadContextV1 {
                root: read.root,
                state: read.state,
                values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: || stop() || (write.cancelled)() || (read.cancelled)(),
            };
            family::child(&key, &current, &mut combined_read)?
        };
        if current != *expected {
            return Err(ValueTreeError::Invalid("revocation exact before").into());
        }
        if stop() || (write.cancelled)() || (read.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if expected == after {
            read.check()?;
            return Ok(false);
        }
        {
            let mut combined_read = WorldFieldReadContextV1 {
                root: read.root,
                state: read.state,
                values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: || stop() || (write.cancelled)() || (read.cancelled)(),
            };
            recover_runtime_revocation_body_v1(after, &mut combined_read, scan, work)?;
        }
        if stop() || (write.cancelled)() || (read.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let owner = WorldFieldKeyV1::from_bytes(key.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or_else(invalid)?;
        let reference =
            RecordRootRefV1::new(schema(), owner, after.clone()).map_err(|_| invalid())?;
        let row = write_member_record_v1(
            field,
            WorldFieldRecordKindV1::Entry,
            &(),
            MemberPayloadRefV1::RecordRoot(&reference),
            &mut MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || stop() || (write.cancelled)() || (read.cancelled)(),
            },
        )?;
        if row.field_key != key {
            return Err(invalid().into());
        }
        transaction.write(
            StateNamespaceV1::Authorization,
            key.as_bytes(),
            Some(row.value_root),
        )?;
        if stop() || (write.cancelled)() || (read.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}
