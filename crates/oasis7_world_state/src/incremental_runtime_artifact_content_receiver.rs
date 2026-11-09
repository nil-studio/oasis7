//! Actual changed paths only; unchanged raw originals inherit the exact
//! qualified parent. This is a logical receiver, not a durability/head receipt.
use super::read::{check_entry, identify, invalid};
use super::*;
use crate::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_field_key::WorldFieldRecordKindV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{TransitionRecordV1, TransitionVerifier},
    incremental_state::{MerkleNodeReader, StateEntryV1},
    incremental_value::ValueNodeReader,
};
pub fn accept_runtime_artifact_content_transition_v1<M, P, R, E, C, O>(
    parent: &ReservedValue<RuntimeArtifactContentQualificationV1>,
    verifier: &TransitionVerifier<'_, P>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    originals: &mut O,
    work: &mut RuntimeArtifactOriginalWorkV1,
) -> Result<
    ReservedValue<RuntimeArtifactContentQualificationV1>,
    RuntimeArtifactOriginalErrorV1<O::Error>,
>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
{
    ctx.check()?;
    if !parent.uses_budget(ctx.values.memory)
        || parent.reserved_bytes() < std::mem::size_of::<RuntimeArtifactContentQualificationV1>()
        || parent.allocation_class() != ctx.values.class
        || verifier.class != ctx.values.class
        || !verifier.memory.same_pool(ctx.values.memory)
        || !originals.allocation_budget().same_pool(ctx.values.memory)
        || originals.allocation_class() != ctx.values.class
    {
        return Err(invalid().into());
    }
    parent.validate_binding(&record.before)?;
    let retained = ctx
        .values
        .memory
        .reserve(512, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    let result = verifier.verify(record);
    let original = (ctx.take_state_error)();
    ctx.check()?;
    if let Some(error) = original {
        return Err(error.into());
    }
    let verified = result?;
    let record = &verified.record;
    if !record.after.state.iter().any(|r| r == ctx.root) {
        return Err(invalid().into());
    }
    for write in &record.writes {
        ctx.check()?;
        let Some((field, kind)) = identify(write.namespace, &write.mutation.key)? else {
            continue;
        };
        if kind == WorldFieldRecordKindV1::Metadata
            && (write.mutation.before.is_none() || write.mutation.after != write.mutation.before)
        {
            return Err(invalid().into());
        }
        let Some(value) = write.mutation.after else {
            continue;
        };
        // Entry key copies are bounded and admitted before allocation.
        let _scratch = ctx
            .values
            .memory
            .reserve(8192, ctx.values.class)
            .map_err(ValueTreeError::Memory)?;
        let entry = StateEntryV1 {
            namespace: write.namespace,
            key: write.mutation.key.clone(),
            value,
        };
        if let Some(original) = check_entry(field, kind, &entry, &mut ctx.records())? {
            verify_runtime_artifact_original_v1(
                original,
                originals,
                ctx.values.memory,
                ctx.values.class,
                work,
                &mut ctx.cancelled,
            )?;
        }
    }
    let roots = record.after.digest().map_err(|_| invalid())?;
    ctx.check()?;
    Ok(retained.retain(RuntimeArtifactContentQualificationV1 {
        roots,
        class: ctx.values.class,
    }))
}
