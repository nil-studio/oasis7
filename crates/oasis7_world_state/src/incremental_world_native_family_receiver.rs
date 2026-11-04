//! Only a private complete before-field proof can authorize changed-path reuse.
use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayErrorV1;
fn accept_verified_inner<F: WorldNativeFieldSpecV1>(
    parent: &ReservedValue<WorldNativeFieldQualificationV1<F>>,
    verified: &WorldNativeVerifiedTransitionV1,
    ctx: &mut WorldFieldReadContextV1<
        '_,
        impl MerkleNodeReader,
        impl ValueNodeReader,
        impl FnMut() -> Option<ValueTreeError>,
        impl FnMut() -> bool,
    >,
    scan: &mut StateScanBudget,
    work: &mut WorldNativeFieldReadWorkV1,
) -> Result<ReservedValue<WorldNativeFieldQualificationV1<F>>, WorldFieldOverlayErrorV1> {
    read::check(ctx)?;
    verified.check_context(ctx.values.memory, ctx.values.class, ctx.root)?;
    let record = verified.record();
    parent.validate_binding(&record.before)?;
    record.after.validate().map_err(|_| invalid())?;
    let ns = target_namespace::<F>()?;
    if parent.class != MemoryClass::Ordinary
        || ctx.values.class != MemoryClass::Ordinary
        || verified.allocation_class() != MemoryClass::Ordinary
        || !parent.uses_budget(ctx.values.memory)
        || parent.reserved_bytes() < 512
        || verified
            .check_pool_class(ctx.values.memory, ctx.values.class)
            .is_err()
        || ctx.root != record.after.state_root(ns)
    {
        return Err(invalid().into());
    }
    check_count::<F>(parent.records)?;
    let retained = ctx
        .values
        .memory
        .reserve(512, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let _scratch = ctx
        .values
        .memory
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut count = i128::from(parent.records);
    for write in &record.writes {
        read::check(ctx)?;
        spend(work)?;
        if !matches_key::<F>(&write.mutation.key)? {
            continue;
        }
        if write.mutation.key.len() > crate::incremental_world_field_key::WORLD_FIELD_KEY_MAX_BYTES
        {
            return Err(invalid().into());
        }
        let probe = StateEntryV1 {
            namespace: write.namespace,
            key: write.mutation.key.clone(),
            value: write
                .mutation
                .after
                .or(write.mutation.before)
                .ok_or_else(invalid)?,
        };
        if write.namespace != ns {
            return Err(invalid().into());
        }
        let key = key::canonical::<F>(&probe)?;
        let parsed =
            WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
        match parsed.kind {
            WorldFieldRecordKindV1::Metadata => {
                if parsed.semantic_key.is_some()
                    || write.mutation.before.is_none()
                    || write.mutation.after != write.mutation.before
                {
                    return Err(invalid().into());
                }
            }
            WorldFieldRecordKindV1::Entry => {
                if write.mutation.before == write.mutation.after {
                    continue;
                }
                if write.mutation.after.is_none() {
                    if F::Key::UNIT {
                        return Err(invalid().into());
                    }
                    let _key = key::original::<F>(&probe, ctx, work.max_key_heap)?;
                    count -= 1;
                    continue;
                }
                let original = key::original::<F>(&probe, ctx, work.max_key_heap)?;
                let after = read::read_body_at_entry_v1::<F, _, _, _, _>(&key, &probe.value, ctx)?;
                let before = write
                    .mutation
                    .before
                    .map(|value| read::read_body_at_entry_v1::<F, _, _, _, _>(&key, &value, ctx))
                    .transpose()?;
                body::<F>(
                    key::original_ref::<F>(&original)?,
                    before.as_deref(),
                    &after,
                    ctx,
                    scan,
                    work,
                )?;
                if write.mutation.before.is_none() {
                    count += 1;
                }
            }
            _ => return Err(invalid().into()),
        }
        read::check(ctx)?;
    }
    let count = u64::try_from(count).map_err(|_| invalid())?;
    check_count::<F>(count)?;
    read::check(ctx)?;
    let proof = retained.retain(WorldNativeFieldQualificationV1 {
        roots: record.after.digest().map_err(|_| invalid())?,
        records: count,
        class: MemoryClass::Ordinary,
        _field: PhantomData,
    });
    read::check(ctx)?;
    Ok(proof)
}
#[cfg(test)]
pub(crate) fn accept_world_native_field_transition_v1<F: WorldNativeFieldSpecV1>(
    parent: &ReservedValue<WorldNativeFieldQualificationV1<F>>,
    verifier: &TransitionVerifier<'_, impl MerkleNodeReader>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<
        '_,
        impl MerkleNodeReader,
        impl ValueNodeReader,
        impl FnMut() -> Option<ValueTreeError>,
        impl FnMut() -> bool,
    >,
    scan: &mut StateScanBudget,
    work: &mut WorldNativeFieldReadWorkV1,
) -> Result<ReservedValue<WorldNativeFieldQualificationV1<F>>, WorldFieldOverlayErrorV1> {
    read::check(ctx)?;
    if parent.class != MemoryClass::Ordinary
        || ctx.values.class != MemoryClass::Ordinary
        || !parent.uses_budget(ctx.values.memory)
        || parent.reserved_bytes() < 512
        || verifier.class != ctx.values.class
        || !verifier.memory.same_pool(ctx.values.memory)
        || ctx.root != record.after.state_root(target_namespace::<F>()?)
    {
        return Err(invalid().into());
    }
    {
        let _phase = ctx
            .values
            .memory
            .reserve(128 << 10, ctx.values.class)
            .map_err(ValueTreeError::Memory)?;
        parent.validate_binding(&record.before)?;
    }
    let verified = verify_world_native_transition_v1(verifier, record, ctx)?;
    let result = accept_world_native_field_verified_v1::<F>(parent, &verified, ctx, scan, work);
    read::check(ctx)?;
    result
}

/// One shared generic replay can feed every independently typed native field.
pub(crate) type WorldNativeVerifiedTransitionV1 =
    crate::incremental_verified_world_transition::VerifiedWorldTransitionV1;
pub(crate) fn verify_world_native_transition_v1(
    verifier: &TransitionVerifier<'_, impl MerkleNodeReader>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<
        '_,
        impl MerkleNodeReader,
        impl ValueNodeReader,
        impl FnMut() -> Option<ValueTreeError>,
        impl FnMut() -> bool,
    >,
) -> Result<WorldNativeVerifiedTransitionV1, WorldFieldOverlayErrorV1> {
    read::check(ctx)?;
    if ctx.values.class != MemoryClass::Ordinary {
        return Err(invalid().into());
    }
    Ok(
        crate::incremental_verified_world_transition::verify_world_transition_v1(
            verifier, record, ctx,
        )?,
    )
}

pub(crate) fn accept_world_native_field_verified_v1<F: WorldNativeFieldSpecV1>(
    parent: &ReservedValue<WorldNativeFieldQualificationV1<F>>,
    verified: &WorldNativeVerifiedTransitionV1,
    ctx: &mut WorldFieldReadContextV1<
        '_,
        impl MerkleNodeReader,
        impl ValueNodeReader,
        impl FnMut() -> Option<ValueTreeError>,
        impl FnMut() -> bool,
    >,
    scan: &mut StateScanBudget,
    work: &mut WorldNativeFieldReadWorkV1,
) -> Result<ReservedValue<WorldNativeFieldQualificationV1<F>>, WorldFieldOverlayErrorV1> {
    let result = accept_verified_inner::<F>(parent, verified, ctx, scan, work);
    read::check(ctx)?;
    result
}
