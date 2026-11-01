//! Isolated fixed records: qualified create and closed whole-record deletion.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
pub struct RuntimeCapabilityCreateV1<'a> {
    pub key: RuntimeCapabilityRecordKeyV1<'a>,
    pub family: &'a ReservedValue<RuntimeCapabilityFamilyQualificationV1>,
    pub source: RuntimeCapabilityRecordRefV1<'a>,
}
pub fn create_qualified_runtime_capability_record_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
>(
    request: RuntimeCapabilityCreateV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut RuntimeCapabilityRecordsWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<RuntimeCapabilityRecordQualificationV1>, WorldFieldOverlayErrorV1> {
    let cancel = &mut ctx.cancelled;
    let error = &mut ctx.take_state_error;
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if request.key.kind != request.source.kind()
            || ctx.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(ctx.values.memory)
            || !tx.uses_budget(write.memory)
            || !request.family.uses_budget(write.memory)
        {
            return Err(read::invalid().into());
        }
        request
            .family
            .validate_binding(&tx.parent_fence().parent_roots)?;
        let _admission = write
            .memory
            .reserve(64 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let (outer, owner) = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut combined,
            };
            let meta = WorldFieldKeyV1::new(
                request.key.kind.descriptor(),
                WorldFieldRecordKindV1::Metadata,
                None,
            )
            .map_err(ValueTreeError::Invalid)?;
            let format = read::marker(request.key.kind, &mut records)?;
            let outer = read::outer_key(request.key, &mut records)?;
            let m = lookup(tx, &meta, &mut records, error)?.ok_or_else(read::invalid)?;
            let f = lookup(tx, &format.0, &mut records, error)?.ok_or_else(read::invalid)?;
            if lookup(tx, &outer.0, &mut records, error)?.is_some() {
                return Err(read::invalid().into());
            }
            records.presence(&m, &meta, 0)?;
            read::value(None, &f, &format, &mut records, 0, &mut std::io::sink())?;
            let owner = WorldFieldKeyV1::from_bytes(outer.0.as_bytes())
                .map_err(ValueTreeError::Invalid)?
                .semantic_key
                .ok_or_else(read::invalid)?;
            if (records.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            (outer.0, owner)
        };
        let (child, row) = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            let child = genesis::body(request.source, &mut combined, work)?;
            work.spend()?;
            let row = genesis::write_root(request.key, child.clone(), &mut combined)?;
            (child, row)
        };
        if row.field_key != outer {
            return Err(read::invalid().into());
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(
            request.key.kind.namespace(),
            outer.as_bytes(),
            Some(row.value_root),
        )?;
        let token = closure::mint(
            request.key.kind,
            owner,
            child,
            tx.parent_fence().parent_roots,
            write.memory,
            MemoryClass::Ordinary,
        )?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(token)
    })
}
pub struct RuntimeCapabilityDeleteV1<'a> {
    pub key: RuntimeCapabilityRecordKeyV1<'a>,
    pub qualification: &'a ReservedValue<RuntimeCapabilityRecordQualificationV1>,
}
pub fn delete_qualified_runtime_capability_record_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: RuntimeCapabilityDeleteV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        let _admission = ctx
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut records = InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        let resolved = point::resolve(
            request.key,
            request.qualification,
            tx,
            &mut records,
            &mut ctx.take_state_error,
        )?;
        tx.write(
            request.key.kind.namespace(),
            resolved.outer.as_bytes(),
            None,
        )?;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    })
}
