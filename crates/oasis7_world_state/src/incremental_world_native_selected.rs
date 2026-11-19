//! Current policy reads use the original closed native-record decoder.
use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayErrorV1;
use crate::incremental_world_native_gameplay::GameplayPolicyFieldV1;
use oasis7_formal_protocol::incremental_execution::{
    ExecutionError, ExecutionFence, ExecutionTransaction,
};

/// Read the actual policy entry in the sole transaction, including a policy
/// changed earlier in that transaction. No parent-value fallback is allowed.
pub fn recover_gameplay_policy_overlay_v1(
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
    work: &mut TypedRecordReadWorkV1,
) -> Result<
    RecoveredTypedRecordV1<WorldNativeRecordV1<crate::GameplayPolicyState>>,
    WorldFieldOverlayErrorV1,
> {
    tx.contribute(|tx| {
        tx.validate_guard(&tx.guard(), live)?;
        read::check(ctx)?;
        let _phase = ctx
            .values
            .memory
            .reserve(4096, MemoryClass::Ordinary)
            .map_err(ExecutionError::Memory)?;
        let field = GameplayPolicyFieldV1::descriptor()?;
        let profile = GameplayPolicyFieldV1::profile()?;
        let namespace = profile.namespace()?;
        if !tx.uses_budget(ctx.values.memory)
            || ctx.values.class != MemoryClass::Ordinary
            || tx.parent_state_root(namespace) != ctx.root
        {
            return Err(ValueTreeError::Invalid("current gameplay policy scope").into());
        }
        let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let value = tx
            .read(namespace, metadata.as_bytes())?
            .ok_or(ValueTreeError::Invalid("current gameplay policy metadata"))?;
        ctx.records().presence(&value, &metadata, 0)?;
        let (key, _) = ctx.key(field, WorldFieldRecordKindV1::Entry, &())?;
        let value = tx
            .read(namespace, key.as_bytes())?
            .ok_or(ValueTreeError::Invalid("current gameplay policy entry"))?;
        let body =
            read::read_body_at_entry_v1::<GameplayPolicyFieldV1, _, _, _, _>(&key, &value, ctx)?;
        let recovered = recover_typed_record_body_v1(&body, &profile, ctx, scan, work)?;
        GameplayPolicyFieldV1::validate_record_binding(
            WorldNativeKeyRefV1::Unit,
            &body,
            ctx,
            scan,
            work,
        )?;
        read::check(ctx)?;
        Ok(recovered)
    })
}
