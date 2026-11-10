use super::*;
use crate::{
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_verified_world_transition::VerifiedWorldTransitionV1,
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
#[allow(clippy::too_many_arguments)]
pub(crate) fn accept_factory_lifecycle_index_verified_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<FactoryLifecycleIndexV1>,
    old_factories: &ReservedValue<FactoryFamilyQualificationV1>,
    old_industry: &ReservedValue<IndustryFamilyQualificationV1>,
    new_factories: &ReservedValue<FactoryFamilyQualificationV1>,
    new_industry: &ReservedValue<IndustryFamilyQualificationV1>,
    witness: &VerifiedWorldTransitionV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut FactoryLifecycleIndexWorkV1,
) -> Result<ReservedValue<FactoryLifecycleIndexV1>, WorldFieldOverlayErrorV1> {
    ctx.check()?;
    let memory = ctx.values.memory;
    let class = ctx.values.class;
    let _phase = memory
        .reserve(32 << 10, class)
        .map_err(ValueTreeError::Memory)?;
    if class != MemoryClass::Ordinary || !parent.uses_budget(memory) {
        return Err(invalid().into());
    }
    witness.check_context(memory, class, ctx.root)?;
    let record = witness.record();
    parent.validate_binding(old_factories, old_industry, &record.before, memory, class)?;
    let digest = validate_joint_parent(new_factories, new_industry, &record.after, memory, class)?;
    if ctx.root != record.after.state_root(StateNamespaceV1::Business) {
        return Err(invalid().into());
    }
    let nf = count(new_factories.record_counts()[0], work.max_factories)?;
    let nj = count(new_industry.record_counts()[0], work.max_recipe_jobs)?;
    let (mut added_factories, mut added_jobs, mut relevant) = (0usize, 0usize, 0usize);
    for write in &record.writes {
        ctx.check()?;
        work.cell()?;
        if !write
            .mutation
            .key
            .starts_with(b"oasis7.iws.world-field-key.v1\0")
        {
            continue;
        }
        let key =
            WorldFieldKeyV1::from_bytes(&write.mutation.key).map_err(ValueTreeError::Invalid)?;
        if !matches!(key.name, "factories" | "pending_recipe_jobs") {
            continue;
        }
        relevant = relevant.checked_add(1).ok_or_else(invalid)?;
        if relevant > work.max_pending_records {
            return Err(
                ValueTreeError::ResourceLimited("factory lifecycle verified changes").into(),
            );
        }
        if write.mutation.before.is_none() && write.mutation.after.is_some() {
            if key.name == "factories" {
                added_factories = added_factories.checked_add(1).ok_or_else(invalid)?;
            } else {
                added_jobs = added_jobs.checked_add(1).ok_or_else(invalid)?;
            }
        }
    }
    let fc = parent
        .factories
        .len()
        .checked_add(added_factories)
        .ok_or_else(invalid)?;
    let jc = parent
        .jobs
        .len()
        .checked_add(added_jobs)
        .ok_or_else(invalid)?;
    let bytes = size(fc, std::mem::size_of::<HeldFactory>())?
        .checked_add(size(jc, std::mem::size_of::<HeldJob>())?)
        .and_then(|n| n.checked_add(nj.checked_mul(std::mem::size_of::<HeldJob>())?))
        .ok_or_else(invalid)?;
    let vectors = memory
        .reserve(bytes, class)
        .map_err(ValueTreeError::Memory)?;
    let owner = memory
        .reserve(std::mem::size_of::<FactoryLifecycleIndexV1>(), class)
        .map_err(ValueTreeError::Memory)?;
    let mut fs = Vec::with_capacity(fc);
    let mut js = Vec::with_capacity(jc);
    for row in &parent.factories {
        ctx.check()?;
        work.cell()?;
        fs.push(row.clone());
    }
    for row in &parent.jobs {
        ctx.check()?;
        work.cell()?;
        js.push(row.clone());
    }
    for write in &record.writes {
        ctx.check()?;
        work.cell()?;
        if !write
            .mutation
            .key
            .starts_with(b"oasis7.iws.world-field-key.v1\0")
        {
            continue;
        }
        let parsed =
            WorldFieldKeyV1::from_bytes(&write.mutation.key).map_err(ValueTreeError::Invalid)?;
        let factory = match parsed.name {
            "factories" => true,
            "pending_recipe_jobs" => false,
            _ => continue,
        };
        let descriptor = if factory {
            FactoryRecordKindV1::Factory.descriptor()?
        } else {
            IndustryRecordKindV1::Recipe.descriptor()?
        };
        let key = WorldFieldKeyV1::new(descriptor, parsed.kind, parsed.semantic_key)
            .map_err(ValueTreeError::Invalid)?;
        if parsed.kind != WorldFieldRecordKindV1::Entry
            || key.as_bytes() != write.mutation.key
            || write.namespace
                != if factory {
                    StateNamespaceV1::Business
                } else {
                    StateNamespaceV1::Scheduler
                }
        {
            return Err(invalid().into());
        }
        if factory {
            let mut decoded = Vec::with_capacity(2); // fixed, covered by phase
            for value in [write.mutation.before, write.mutation.after] {
                let row=value.map(|value|{
                    let mut context=FactoryTransitionReadContextV1{values:ValueReadContext{source:ctx.values.source,memory,class,io:&mut *ctx.values.io},key_work:&mut *ctx.key_work,
                        semantic_bytes:ctx.semantic_bytes,record_bytes:ctx.record_bytes,max_payload_bytes:work.max_text_bytes as u64,cancelled:&mut ctx.cancelled};
                    crate::incremental_factory_records::decode_factory_lifecycle_outer_record_v1(&value,&key,&mut context)
                }).transpose()?;
                decoded.push(row);
            }
            let identity = decoded[1]
                .as_ref()
                .or(decoded[0].as_ref())
                .ok_or_else(invalid)?;
            let FactoryQualifiedOuterIdentityV1::Record { id, .. } = &**identity else {
                return Err(invalid().into());
            };
            let mut pos = None;
            for (i, row) in fs.iter().enumerate() {
                ctx.check()?;
                if work.comparison(&row.id, id)?.is_eq() {
                    pos = Some(i);
                    break;
                }
            }
            match (&decoded[0], pos) {
                (Some(before), Some(i)) => {
                    let FactoryQualifiedOuterIdentityV1::Record {
                        id,
                        value_root,
                        record_root,
                    } = &**before
                    else {
                        return Err(invalid().into());
                    };
                    if fs[i].id != *id || fs[i].value != *value_root || fs[i].child != *record_root
                    {
                        return Err(invalid().into());
                    }
                }
                (None, None) => {}
                _ => return Err(invalid().into()),
            }
            let next = if let Some(after) = &decoded[1] {
                let FactoryQualifiedOuterIdentityV1::Record { id: after_id, .. } = &**after else {
                    return Err(invalid().into());
                };
                if after_id != id {
                    return Err(invalid().into());
                }
                let mut local = WorldFieldReadContextV1 {
                    root: record.after.state_root(StateNamespaceV1::Business),
                    state: ctx.state,
                    values: ValueReadContext {
                        source: ctx.values.source,
                        memory,
                        class,
                        io: &mut *ctx.values.io,
                    },
                    key_work: &mut *ctx.key_work,
                    semantic_bytes: ctx.semantic_bytes,
                    record_bytes: ctx.record_bytes,
                    take_state_error: &mut ctx.take_state_error,
                    cancelled: &mut ctx.cancelled,
                };
                project_factory(after, &mut local, work)?
            } else {
                None
            };
            match (pos, next) {
                (Some(i), Some(row)) => fs[i] = row,
                (Some(i), None) => {
                    for _ in i..fs.len() {
                        ctx.check()?;
                        work.cell()?;
                    }
                    fs.remove(i);
                }
                (None, Some(row)) => fs.push(row),
                _ => return Err(invalid().into()),
            }
        } else {
            let mut decoded = Vec::with_capacity(2);
            for value in [write.mutation.before, write.mutation.after] {
                let row = value
                    .map(|value| {
                        let mut context = IndustryTransitionReadContextV1 {
                            values: ValueReadContext {
                                source: ctx.values.source,
                                memory,
                                class,
                                io: &mut *ctx.values.io,
                            },
                            key_work: &mut *ctx.key_work,
                            semantic_bytes: ctx.semantic_bytes,
                            record_bytes: ctx.record_bytes,
                            max_payload_bytes: work.max_text_bytes as u64,
                            cancelled: &mut ctx.cancelled,
                        };
                        crate::incremental_industry_jobs::decode_industry_lifecycle_outer_record_v1(
                            &value,
                            &key,
                            &mut context,
                        )
                    })
                    .transpose()?;
                decoded.push(row);
            }
            let identity = decoded[1]
                .as_ref()
                .or(decoded[0].as_ref())
                .ok_or_else(invalid)?;
            let IndustryQualifiedOuterIdentityV1::Record { id, .. } = &**identity else {
                return Err(invalid().into());
            };
            let mut pos = None;
            for (i, row) in js.iter().enumerate() {
                ctx.check()?;
                work.cell()?;
                if row.id == *id {
                    pos = Some(i);
                    break;
                }
            }
            match (&decoded[0], pos) {
                (Some(before), Some(i)) => {
                    let IndustryQualifiedOuterIdentityV1::Record {
                        id,
                        value_root,
                        record_root,
                    } = &**before
                    else {
                        return Err(invalid().into());
                    };
                    if js[i].id != *id || js[i].value != *value_root || js[i].child != *record_root
                    {
                        return Err(invalid().into());
                    }
                }
                (None, None) => {}
                _ => return Err(invalid().into()),
            }
            let next = if let Some(after) = &decoded[1] {
                let IndustryQualifiedOuterIdentityV1::Record { id: after_id, .. } = &**after else {
                    return Err(invalid().into());
                };
                if after_id != id {
                    return Err(invalid().into());
                }
                let mut local = WorldFieldReadContextV1 {
                    root: record.after.state_root(StateNamespaceV1::Scheduler),
                    state: ctx.state,
                    values: ValueReadContext {
                        source: ctx.values.source,
                        memory,
                        class,
                        io: &mut *ctx.values.io,
                    },
                    key_work: &mut *ctx.key_work,
                    semantic_bytes: ctx.semantic_bytes,
                    record_bytes: ctx.record_bytes,
                    take_state_error: &mut ctx.take_state_error,
                    cancelled: &mut ctx.cancelled,
                };
                project_job(after, &mut local, work)?
            } else {
                None
            };
            match (pos, next) {
                (Some(i), Some(row)) => js[i] = row,
                (Some(i), None) => {
                    for _ in i..js.len() {
                        ctx.check()?;
                        work.cell()?;
                    }
                    js.remove(i);
                }
                (None, Some(row)) => js.push(row),
                _ => return Err(invalid().into()),
            }
        }
    }
    if fs.len() != nf || js.len() != nj {
        return Err(invalid().into());
    }
    sort(&mut fs, work, &mut ctx.cancelled, |a, b, w| {
        w.comparison(&a.id, &b.id)
    })?;
    sort(&mut js, work, &mut ctx.cancelled, |a, b, w| {
        w.cell()?;
        Ok(a.id.cmp(&b.id))
    })?;
    let mut by_factory = Vec::with_capacity(nj);
    for row in &js {
        ctx.check()?;
        work.cell()?;
        by_factory.push(row.clone());
    }
    sort(&mut by_factory, work, &mut ctx.cancelled, |a, b, w| {
        Ok(w.comparison(&a.factory, &b.factory)?.then(a.id.cmp(&b.id)))
    })?;
    let (mut completed_sum, mut stable_count) = (0u128, 0u64);
    for row in &fs {
        ctx.check()?;
        work.cell()?;
        completed_sum = completed_sum
            .checked_add(u128::from(row.completed))
            .ok_or_else(invalid)?;
        stable_count = stable_count
            .checked_add(u64::from(row.stable))
            .ok_or_else(invalid)?;
    }
    ctx.check()?;
    Ok(owner.retain(FactoryLifecycleIndexV1 {
        roots: digest,
        class,
        factory_counts: new_factories.record_counts(),
        industry_counts: new_industry.record_counts(),
        factories: fs,
        jobs: js,
        by_factory,
        completed_sum,
        stable_count,
        _vectors: vectors,
    }))
}
