use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayErrorV1;
pub fn accept_inventory_transition_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<InventoryFamilyQualificationV1>,
    verifier: &TransitionVerifier<'_, P>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut InventoryFamilyWorkV1,
) -> Result<ReservedValue<InventoryFamilyQualificationV1>, WorldFieldOverlayErrorV1> {
    {
        ctx.check()?;
        let memory = ctx.values.memory;
        if ctx.values.class != MemoryClass::Ordinary
            || !parent.uses_budget(memory)
            || parent.reserved_bytes() < std::mem::size_of::<InventoryFamilyQualificationV1>() + 128
            || verifier.class != MemoryClass::Ordinary
            || !verifier.memory.same_pool(memory)
        {
            return Err(invalid().into());
        }
        parent.check(memory, MemoryClass::Ordinary)?;
        if ctx.root != record.after.state_root(StateNamespaceV1::Business) {
            return Err(invalid().into());
        }
        let _phase = memory
            .reserve(8192, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        parent.validate_binding(&record.before)?;
    }
    let witness = crate::incremental_verified_world_transition::verify_world_transition_v1(
        verifier, record, ctx,
    )?;
    accept_inventory_verified_v1(parent, &witness, ctx, work)
}

pub(crate) fn accept_inventory_verified_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<InventoryFamilyQualificationV1>,
    witness: &crate::incremental_verified_world_transition::VerifiedWorldTransitionV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut InventoryFamilyWorkV1,
) -> Result<ReservedValue<InventoryFamilyQualificationV1>, WorldFieldOverlayErrorV1> {
    witness.check_context(ctx.values.memory, ctx.values.class, ctx.root)?;
    let record = witness.record();

    ctx.check()?;
    let memory = ctx.values.memory;
    if ctx.values.class != MemoryClass::Ordinary
        || !parent.uses_budget(memory)
        || parent.reserved_bytes() < std::mem::size_of::<InventoryFamilyQualificationV1>() + 128
    {
        return Err(invalid().into());
    }
    parent.check(memory, MemoryClass::Ordinary)?;
    if ctx.root != record.after.state_root(StateNamespaceV1::Business) {
        return Err(invalid().into());
    }
    let _phase = memory
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    parent.validate_binding(&record.before)?;
    let verified = witness.prepared();
    let charge = memory
        .reserve(
            std::mem::size_of::<InventoryFamilyQualificationV1>() + 128,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let bytes = verified
        .record
        .writes
        .len()
        .checked_mul(64)
        .and_then(|n| n.checked_add(128))
        .ok_or_else(invalid)?;
    let _changed = memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut touched = Vec::with_capacity(verified.record.writes.len());
    let mut groups = parent.groups.clone();
    let mut counts = parent.counts;
    let mut iw = InventoryIndexWorkV1 {
        steps_remaining: std::mem::take(&mut work.index_steps_remaining),
    };
    let result: Result<InventoryFamilyQualificationV1, WorldFieldOverlayErrorV1> = (|| {
        for write in &verified.record.writes {
            ctx.check()?;
            work.step(&mut ctx.cancelled)?;
            let Some(key) = key(&write.mutation.key)? else {
                continue;
            };
            if write.namespace != StateNamespaceV1::Business {
                return Err(invalid().into());
            }
            let mut group = None;
            for (id, add) in [(write.mutation.before, false), (write.mutation.after, true)] {
                let Some(id) = id else {
                    continue;
                };
                let decoded = row(&key, &id, ctx, work)?;
                if let Row::Meta(_) = decoded {
                    if write.mutation.after.is_none() {
                        return Err(invalid().into());
                    }
                    continue;
                }
                if let Some(k) = adjust(
                    &mut groups,
                    decoded,
                    add,
                    &mut counts,
                    &mut iw,
                    &mut ctx.cancelled,
                )? {
                    group = Some(k);
                }
            }
            if let Some(k) = group {
                touched.push(k);
            }
        }
        for k in touched {
            ctx.check()?;
            if let Some(meta) = groups.get(k, &mut iw, &mut ctx.cancelled)?
                && !meta.present
            {
                if meta.members != 0 {
                    return Err(invalid().into());
                }
                groups = groups.remove(k, &mut iw, &mut ctx.cancelled)?;
            }
        }
        if groups.len() as u64 != counts[2] {
            return Err(invalid().into());
        }
        ctx.check()?;
        Ok(InventoryFamilyQualificationV1 {
            roots: verified.record.after.digest().map_err(|_| invalid())?,
            class: MemoryClass::Ordinary,
            counts,
            groups: groups.clone(),
        })
    })();
    work.index_steps_remaining = iw.steps_remaining;
    ctx.check()?;
    Ok(charge.retain(result?))
}
