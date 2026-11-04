use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayErrorV1;
use crate::incremental_verified_world_transition::{
    VerifiedWorldTransitionV1, verify_world_transition_v1,
};
fn parent_phase(
    parent: &ReservedValue<WorldCollectionControlsQualificationV1>,
    memory: &MemoryBudget,
    roots: &RootSetV1,
) -> Result<(), ValueTreeError> {
    if !parent.uses_budget(memory)
        || parent.reserved_bytes()
            < std::mem::size_of::<WorldCollectionControlsQualificationV1>() + 128
    {
        return Err(invalid());
    }
    parent.check(memory, MemoryClass::Ordinary)?;
    let _phase = memory.reserve(8192, MemoryClass::Ordinary)?;
    roots.validate().map_err(|_| invalid())?;
    parent.validate_binding(roots)
}
/// Verifies generic integrity exactly once; unrelated owners still require their own closures.
pub fn accept_world_collection_controls_transition_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<WorldCollectionControlsQualificationV1>,
    verifier: &TransitionVerifier<'_, P>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldCollectionControlsWorkV1,
) -> Result<ReservedValue<WorldCollectionControlsQualificationV1>, WorldFieldOverlayErrorV1> {
    ctx.check()?;
    if ctx.values.class != MemoryClass::Ordinary
        || verifier.class != MemoryClass::Ordinary
        || !verifier.memory.same_pool(ctx.values.memory)
        || !record.after.state.iter().any(|r| r == ctx.root)
    {
        return Err(invalid().into());
    }
    parent_phase(parent, ctx.values.memory, &record.before)?;
    let witness = verify_world_transition_v1(verifier, record, ctx)?;
    accept_world_collection_controls_verified_v1(parent, &witness, ctx, work)
}
/// One borrowed checked transition shared with the complete owner dispatcher.
pub(crate) fn accept_world_collection_controls_verified_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<WorldCollectionControlsQualificationV1>,
    witness: &VerifiedWorldTransitionV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldCollectionControlsWorkV1,
) -> Result<ReservedValue<WorldCollectionControlsQualificationV1>, WorldFieldOverlayErrorV1> {
    ctx.check()?;
    if ctx.values.class != MemoryClass::Ordinary {
        return Err(invalid().into());
    }
    witness.check_context(ctx.values.memory, MemoryClass::Ordinary, ctx.root)?;
    let record = witness.record();
    let memory = ctx.values.memory;
    parent_phase(parent, memory, &record.before)?;
    if ctx.state.allocation_class() != Some(MemoryClass::Ordinary)
        || ctx
            .state
            .allocation_budget()
            .is_none_or(|m| !m.same_pool(memory))
    {
        return Err(invalid().into());
    }
    let charge = memory
        .reserve(
            std::mem::size_of::<WorldCollectionControlsQualificationV1>() + 128,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let bytes = record
        .writes
        .len()
        .checked_mul(64)
        .and_then(|n| n.checked_add(128))
        .ok_or_else(invalid)?;
    let _changed = memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut touched = Vec::with_capacity(record.writes.len());
    let mut groups = parent.groups.clone();
    let mut counts = parent.counts;
    let mut cursor_entries = parent.cursor_entries;
    let mut iw = CollectionControlsIndexWorkV1 {
        steps_remaining: std::mem::take(&mut work.index_steps_remaining),
    };
    let result: Result<WorldCollectionControlsQualificationV1, WorldFieldOverlayErrorV1> = (|| {
        for write in &record.writes {
            ctx.check()?;
            work.step(&mut ctx.cancelled)?;
            let Some((field, key)) = key(&write.mutation.key)? else {
                continue;
            };
            if write.namespace != namespace(field) {
                return Err(invalid().into());
            }
            let parsed =
                WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
            if parsed.kind == WorldFieldRecordKindV1::Metadata {
                // A qualified parent has exactly one immutable descriptor for each field.
                if write.mutation.before.is_none() || write.mutation.before != write.mutation.after
                {
                    return Err(invalid().into());
                }
                continue;
            }
            let mut changed_group = None;
            for (id, add) in [(write.mutation.before, false), (write.mutation.after, true)] {
                let Some(id) = id else { continue };
                let decoded = row(field, &key, &id, !add, ctx, work)?;
                if let Row::Meta(_) = decoded {
                    return Err(invalid().into());
                }
                if let Some(k) = adjust(
                    &mut groups,
                    decoded,
                    add,
                    &mut counts,
                    &mut cursor_entries,
                    &mut iw,
                    &mut ctx.cancelled,
                )? {
                    changed_group = Some(k)
                }
            }
            if let Some(k) = changed_group {
                touched.push(k)
            }
        }
        for k in touched {
            ctx.check()?;
            if let Some(meta) = groups.get(k, &mut iw, &mut ctx.cancelled)? {
                if !meta.present {
                    if meta.members != 0 {
                        return Err(invalid().into());
                    }
                    groups = groups.remove(k, &mut iw, &mut ctx.cancelled)?;
                }
            }
        }
        final_counts(&groups, &counts, cursor_entries)?;
        ctx.check()?;
        Ok(WorldCollectionControlsQualificationV1 {
            roots: record.after.digest().map_err(|_| invalid())?,
            class: MemoryClass::Ordinary,
            counts,
            cursor_entries,
            groups: groups.clone(),
        })
    })();
    work.index_steps_remaining = iw.steps_remaining;
    ctx.check()?;
    Ok(charge.retain(result?))
}
