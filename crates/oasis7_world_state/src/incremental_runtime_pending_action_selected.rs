//! Current-overlay lookup uses the same closed typed decoder as recovery.
use super::*;
use oasis7_formal_protocol::incremental_execution::{ExecutionFence, ExecutionTransaction};
/// The selected value reference is read from the actual transaction, never
/// supplied by a caller. Deletion and original qualification remain separate.
pub fn recover_current_pending_action_record_v1(
    position: PendingActionPositionV1,
    tx: &mut ExecutionTransaction<'_, impl MerkleNodeReader>,
    live: &ExecutionFence,
    ctx: &mut WorldFieldReadContextV1<
        '_,
        impl MerkleNodeReader,
        impl ValueNodeReader,
        impl FnMut() -> Option<ValueTreeError>,
        impl FnMut() -> bool,
    >,
    scan: &mut StateScanBudget,
    work: &mut PendingActionReadWorkV1,
) -> Result<
    Option<RecoveredPendingActionRecordV1>,
    crate::incremental_field_overlay::WorldFieldOverlayErrorV1,
> {
    tx.contribute(|tx| {
        tx.validate_guard(&tx.guard(), live)?;
        check(ctx)?;
        if !tx.uses_budget(ctx.values.memory) || ctx.values.class != MemoryClass::Ordinary {
            return Err(invalid().into());
        }
        let (key, _) = ctx.key(
            PENDING_ACTIONS_FIELD_V1,
            WorldFieldRecordKindV1::Entry,
            &position.key(),
        )?;
        let Some(id) = tx.read(StateNamespaceV1::Scheduler, key.as_bytes())? else {
            return Ok(None);
        };
        let record = recover_selected_value(position, id, &key, ctx, scan, work)?;
        check(ctx)?;
        Ok(Some(record))
    })
}
