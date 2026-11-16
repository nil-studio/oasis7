//! Creation and deletion bind complete, qualified original owner identities.
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
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("material lifecycle binding")
}
pub struct MaterialRecordCreateV1<'a> {
    pub key: MaterialRecordKeyV1<'a>,
    pub source: MaterialRecordRefV1<'a>,
    pub family: &'a ReservedValue<MaterialFamilyQualificationV1>,
}
pub fn create_qualified_material_record_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
>(
    request: MaterialRecordCreateV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut MaterialWorkBudgetV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<MaterialRecordQualificationV1>, WorldFieldOverlayErrorV1> {
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        let _qualification_phase = crate::incremental_record_qualification_phase::bind_phase_v1(
            request.family,
            crate::incremental_record_qualification_phase::FamilyBindingV1::Digest(
                &tx.parent_fence().parent_roots,
            ),
            context.values.memory,
            MemoryClass::Ordinary,
            || cancel() || (write.cancelled)(),
        )?;
        if request.source.kind() != request.key.kind()
            || context.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(context.values.memory)
            || !tx.uses_budget(write.memory)
            || !request.family.uses_budget(write.memory)
            || crate::incremental_record_qualification_phase::check_class_v1(
                request.family,
                write.memory,
                oasis7_proto::memory_budget::MemoryClass::Ordinary,
            )
            .is_err()
        {
            return Err(invalid().into());
        }
        let outer = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            point::metadata(request.key, tx, &mut records, error)?;
            let key = read::outer_key(request.key, &mut records)?;
            if lookup(tx, &key.0, &mut records, error)?.is_some() {
                return Err(invalid().into());
            }
            key.0
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
            let mut record_cells = work.cells_remaining;
            let child = genesis::body(
                request.key,
                request.source,
                &mut combined,
                &mut record_cells,
                work,
            )?;
            work.cell()?;
            let row = genesis::write_root(request.key, child.clone(), &mut combined)?;
            (child, row)
        };
        if row.field_key != outer {
            return Err(invalid().into());
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(
            request.key.kind().namespace(),
            outer.as_bytes(),
            Some(row.value_root),
        )?;
        let owner = WorldFieldKeyV1::from_bytes(outer.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or_else(invalid)?;
        let proof = closure::mint(
            request.key.kind(),
            owner,
            child,
            write.memory,
            MemoryClass::Ordinary,
        )?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(proof)
    })
}
pub struct MaterialRecordDeleteV1<'a> {
    pub key: MaterialRecordKeyV1<'a>,
    pub qualification: &'a ReservedValue<MaterialRecordQualificationV1>,
}
/// The complete exact qualified child is the expected value. No sibling payload
/// is rescanned merely to delete its authenticated outer reference.
pub fn delete_qualified_material_record_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: MaterialRecordDeleteV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        let _workspace = context
            .values
            .memory
            .reserve(16 << 10, context.values.class)
            .map_err(ValueTreeError::Memory)?;
        let mut records = InventoryValueReader {
            values: &mut context.values,
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            cancelled: &mut context.cancelled,
        };
        let (outer, _) = point::current(
            request.key,
            request.qualification,
            tx,
            &mut records,
            &mut context.take_state_error,
        )?;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(request.key.kind().namespace(), outer.as_bytes(), None)?;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    })
}

/// Fixed MaterialProfile overwrite through the original complete seven-column
/// codec. The actual Current outer and closure are checked before publication.
#[expect(
    clippy::too_many_arguments,
    reason = "Bounded inputs are independently validated"
)]
pub fn upsert_current_material_profile_v1<M, P, R, E, C, W, CW>(
    profile: &MaterialProfileV1,
    parent: &ReservedValue<MaterialFamilyQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    state: &P,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut MaterialWorkBudgetV1,
    scan: &mut oasis7_formal_protocol::incremental_state::StateScanBudget,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
{
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        parent.validate_binding(&tx.parent_fence().parent_roots)?;
        if parent.allocation_class() != MemoryClass::Ordinary
            || ctx.values.class != MemoryClass::Ordinary
            || !parent.uses_budget(write.memory)
            || !tx.uses_budget(write.memory)
            || !write.memory.same_pool(ctx.values.memory)
        {
            return Err(invalid().into());
        }
        let key = MaterialRecordKeyV1::Profile(&profile.kind);
        let (outer, before, child) = {
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut ctx.cancelled,
            };
            point::metadata(key, tx, &mut records, &mut ctx.take_state_error)?;
            let outer = read::outer_key(key, &mut records)?;
            let before = lookup(tx, &outer.0, &mut records, &mut ctx.take_state_error)?;
            let child = before
                .map(|value| read::outer(key.kind(), &outer, &value, &mut records))
                .transpose()?;
            (outer.0, before, child)
        };
        let _phase = write
            .memory
            .reserve(64 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let _old = if let Some(child) = child {
            let owner = WorldFieldKeyV1::from_bytes(outer.as_bytes())
                .map_err(ValueTreeError::Invalid)?
                .semantic_key
                .ok_or_else(invalid)?;
            let mut read = crate::incremental_field_read::WorldFieldReadContextV1 {
                root: &child,
                state,
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
                cancelled: &mut ctx.cancelled,
            };
            Some(closure::qualify_material_body_v1(
                key.kind(),
                owner,
                &mut read,
                scan,
                &mut work.cells_remaining,
                ctx.record_bytes,
            )?)
        } else {
            None
        };
        let mut count = work.cells_remaining;
        let child = genesis::body(
            key,
            MaterialRecordRefV1::Profile(profile),
            write,
            &mut count,
            work,
        )?;
        work.cell()?;
        let row = genesis::write_root(key, child, write)?;
        if row.field_key != outer
            || lookup(
                tx,
                &outer,
                &mut InventoryValueReader {
                    values: &mut ctx.values,
                    key_work: &mut *ctx.key_work,
                    semantic_bytes: ctx.semantic_bytes,
                    record_bytes: ctx.record_bytes,
                    cancelled: &mut ctx.cancelled,
                },
                &mut ctx.take_state_error,
            )? != before
        {
            return Err(invalid().into());
        }
        if (ctx.cancelled)() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let ready = tx.guard();
        tx.validate_guard(&ready, live)?;
        tx.write(
            key.kind().namespace(),
            outer.as_bytes(),
            Some(row.value_root),
        )?;
        Ok(())
    })
}
