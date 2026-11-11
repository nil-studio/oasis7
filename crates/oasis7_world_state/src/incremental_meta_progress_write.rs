//! Fresh Current CAS of a privately prepared Meta value. No action approval.
use super::*;
use oasis7_formal_protocol::incremental_state::StatePageWriter;
/// Preparation may precede an Accepted event, but publication never refreshes
/// its old guard. `current` must be genuinely re-opened after that event. Its
/// exact source must equal the immutable pre-preparation metadata/outer roots.
#[allow(clippy::too_many_arguments)]
pub fn write_prepared_current_meta_progress_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    prepared: &ReservedValue<PreparedCurrentMetaProgressV1>,
    current: &ReservedValue<CurrentMetaProgressRecordV1>,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    records: &mut u64,
    work: &mut crate::incremental_typed_record_codec::TypedRecordWriteWorkV1,
    mutation: &mut MetaProgressMutationWorkV1,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        current.validate(tx, ctx.values.memory, live)?;
        read::check(ctx)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !prepared.uses_budget(ctx.values.memory)
            || !current.uses_budget(ctx.values.memory)
            || !ctx.values.memory.same_pool(write.memory)
            || !tx.uses_budget(write.memory)
        {
            return Err(invalid().into());
        }
        tx.validate_transaction_identity(&prepared.origin.guard)?;
        let _phase = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        mutation.visit(
            prepared.origin.comparison_bytes(current)?,
            &mut ctx.cancelled,
        )?;
        if !prepared.origin.same_origin(current) {
            return Err(invalid().into());
        }
        let ns = MetaProgressFieldV1::profile()?.namespace()?;
        if ctx.root.namespace != ns {
            return Err(invalid().into());
        }
        let metadata = tx
            .read(ns, current.metadata.as_bytes())?
            .ok_or_else(invalid)?;
        read::check(ctx)?;
        ctx.records().presence(&metadata, &current.metadata, 0)?;
        let before = tx.read(ns, current.key.as_bytes())?;
        read::check(ctx)?;
        if metadata != current.metadata_root || before != current.before {
            return Err(invalid().into());
        }
        mutation.visit(0, &mut ctx.cancelled)?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let mut output = None;
        prepare_world_native_record_v1::<MetaProgressFieldV1>(
            &prepared.origin.target,
            prepared.value(),
            write,
            records,
            work,
            &mut |row| {
                if output.is_some() {
                    return Err(invalid());
                }
                output = Some(row);
                Ok(())
            },
        )?;
        let row = output.ok_or_else(invalid)?;
        if row.namespace != ns
            || row.mutation.key != current.key.as_bytes()
            || row.mutation.after.is_none()
        {
            return Err(invalid().into());
        }
        read::check(ctx)?;
        mutation.visit(0, &mut ctx.cancelled)?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.validate_guard(&current.guard, live)?;
        tx.write(ns, current.key.as_bytes(), row.mutation.after)?;
        read::check(ctx)?;
        mutation.visit(0, &mut ctx.cancelled)?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    })
}
