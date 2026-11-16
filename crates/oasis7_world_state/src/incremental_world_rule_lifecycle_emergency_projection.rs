//! Paid Current emergency-lifecycle family index projection and source collection.
use super::*;

pub(super) fn map_root<M, R, E, C>(
    body: Option<&StateRootV1>,
    name: &'static str,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<ReservedValue<StateRootV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let Some(body) = body else {
        let empty = StateRootV1::empty(StateNamespaceV1::Authorization).map_err(|_| invalid())?;
        return Ok(d
            .ctx
            .values
            .memory
            .reserve(size_of::<StateRootV1>() + 512, d.ctx.values.class)?
            .retain(empty));
    };
    let p = emergency_profile();
    let heap = d.work.max_tree_heap;
    let row = point_typed_column_v1(&p, body, name, d, heap)?;
    let ColumnCellV1::Root(root) = &row.cell else {
        return Err(invalid());
    };
    let root = root.clone();
    Ok(d.ctx
        .values
        .memory
        .reserve(size_of::<StateRootV1>() + 512, d.ctx.values.class)?
        .retain(root))
}
fn charge_projection_insert<M, R, E, C, T>(
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    count: usize,
    text_bytes: usize,
) -> Result<(), ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let steps = count.checked_add(1).ok_or_else(limited)?;
    let compare = text_bytes.checked_mul(steps).ok_or_else(limited)?;
    let move_bytes = count
        .checked_mul(size_of::<Arc<ReservedValue<T>>>())
        .ok_or_else(limited)?;
    spend(
        d,
        steps,
        compare.checked_add(move_bytes).ok_or_else(limited)?,
    )
}
pub(super) fn outer_key<M, R, E, C>(
    p: &TypedRecordProfileV1,
    cell: &TypedCellRefV1<'_>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<Option<ReservedValue<ColumnKeyV1>>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let parsed = key(p, cell, d)?;
    match &*parsed {
        ColumnKeyV1::Text(name) if name == "_length" => Ok(None),
        ColumnKeyV1::Pair(kind, value) if matches!(&**kind,ColumnKeyV1::Text(name) if name=="entry") =>
        {
            let ColumnKeyV1::Text(_) = &**value else {
                return Err(invalid());
            };
            Ok(Some(parsed))
        }
        _ => Err(invalid()),
    }
}
pub(super) fn outer_text(key: &ColumnKeyV1) -> Result<&str, ValueTreeError> {
    let ColumnKeyV1::Pair(kind, value) = key else {
        return Err(invalid());
    };
    if !matches!(&**kind,ColumnKeyV1::Text(name) if name=="entry") {
        return Err(invalid());
    }
    let ColumnKeyV1::Text(outer) = &**value else {
        return Err(invalid());
    };
    Ok(outer)
}
fn index_authorizations<M, R, E, C>(
    builder: &mut EmergencyLifecycleBuilderV1,
    before: Option<&StateRootV1>,
    after: Option<&StateRootV1>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<(), ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let p = emergency_profile();
    let old = map_root(before, "authorizations", d)?;
    let new = map_root(after, "authorizations", d)?;
    visit_entries(&*old, &*new, d, |before, after, d| {
        let cell = after.or(before).ok_or_else(invalid)?;
        let Some(parsed_outer) = outer_key(&p, cell, d)? else {
            return d.check();
        };
        let outer = outer_text(&parsed_outer)?;
        if before.is_some() {
            builder.remove_authorization(&outer, d.scan, &mut d.ctx.cancelled)?;
        }
        if let Some(cell) = after {
            let record = root(&p, cell, d)?;
            let value = recover_typed_record_body_v1::<Authorization, _, _, _, _>(
                &*record, &p, d.ctx, d.scan, d.work,
            )?;
            let v = value.value();
            let text = outer
                .len()
                .checked_add(v.id.len())
                .and_then(|n| n.checked_add(v.provider_id.len()))
                .and_then(|n| n.checked_add(v.region_id.len()))
                .ok_or_else(limited)?;
            charge_projection_insert::<_, _, _, _, LifecycleEmergencyAuthorizationV1>(
                d,
                builder.authorizations.len(),
                text,
            )?;
            builder.insert_authorization(
                &outer,
                &v.id,
                &v.provider_id,
                &v.region_id,
                v.active,
                v.starts_at,
                v.expires_at,
            )?;
        }
        d.check()
    })
}
fn index_batches<M, R, E, C>(
    builder: &mut EmergencyLifecycleBuilderV1,
    before: Option<&StateRootV1>,
    after: Option<&StateRootV1>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<(), ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let p = emergency_profile();
    let old = map_root(before, "batches", d)?;
    let new = map_root(after, "batches", d)?;
    visit_entries(&*old, &*new, d, |before, after, d| {
        let cell = after.or(before).ok_or_else(invalid)?;
        let Some(parsed_outer) = outer_key(&p, cell, d)? else {
            return d.check();
        };
        let outer = outer_text(&parsed_outer)?;
        if before.is_some() {
            builder.remove_batch(&outer, d.scan, &mut d.ctx.cancelled)?;
        }
        if let Some(cell) = after {
            let record = root(&p, cell, d)?;
            let value = recover_typed_record_body_v1::<Batch, _, _, _, _>(
                &*record, &p, d.ctx, d.scan, d.work,
            )?;
            let v = value.value();
            let text = outer
                .len()
                .checked_add(v.authorization_id.len())
                .ok_or_else(limited)?;
            charge_projection_insert::<_, _, _, _, LifecycleEmergencyBatchV1>(
                d,
                builder.batches.len(),
                text,
            )?;
            builder.insert_batch(&outer, &v.authorization_id, v.committed_at)?;
        }
        d.check()
    })
}
fn index_observations<M, R, E, C>(
    builder: &mut EmergencyLifecycleBuilderV1,
    before: Option<&StateRootV1>,
    after: Option<&StateRootV1>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<(), ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let p = emergency_profile();
    let old = map_root(before, "observations", d)?;
    let new = map_root(after, "observations", d)?;
    visit_entries(&*old, &*new, d, |before, after, d| {
        let cell = after.or(before).ok_or_else(invalid)?;
        let Some(parsed_outer) = outer_key(&p, cell, d)? else {
            return d.check();
        };
        let outer = outer_text(&parsed_outer)?;
        if before.is_some() {
            builder.remove_observation(&outer, d.scan, &mut d.ctx.cancelled)?;
        }
        if let Some(cell) = after {
            let record = root(&p, cell, d)?;
            let _value = recover_typed_record_body_v1::<Observation, _, _, _, _>(
                &*record, &p, d.ctx, d.scan, d.work,
            )?;
            charge_projection_insert::<_, _, _, _, String>(
                d,
                builder.observations.len(),
                outer.len(),
            )?;
            builder.insert_observation(&outer)?;
        }
        d.check()
    })
}

fn index_minimum_services<M, R, E, C>(
    builder: &mut EmergencyLifecycleBuilderV1,
    before: Option<&StateRootV1>,
    after: Option<&StateRootV1>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<(), ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let p = emergency_profile();
    let old = map_root(before, "minimum_services", d)?;
    let new = map_root(after, "minimum_services", d)?;
    visit_entries(&*old, &*new, d, |before, after, d| {
        let cell = after.or(before).ok_or_else(invalid)?;
        let Some(parsed_outer) = outer_key(&p, cell, d)? else {
            return d.check();
        };
        let outer = outer_text(&parsed_outer)?;
        if before.is_some() {
            builder.remove_minimum_service(&outer, d.scan, &mut d.ctx.cancelled)?;
        }
        if let Some(cell) = after {
            let record = root(&p, cell, d)?;
            let value = recover_typed_record_body_v1::<MinimumService, _, _, _, _>(
                &*record, &p, d.ctx, d.scan, d.work,
            )?;
            let v = value.value();
            let text = outer
                .len()
                .checked_add(v.operator_agent_id.len())
                .and_then(|n| n.checked_add(v.registration.region_id.len()))
                .ok_or_else(limited)?;
            charge_projection_insert::<_, _, _, _, LifecycleMinimumServiceV1>(
                d,
                builder.minimum_services.len(),
                text,
            )?;
            builder.insert_minimum_service(
                &outer,
                &v.operator_agent_id,
                &v.registration.region_id,
                v.registered_at,
            )?;
        }
        d.check()
    })
}

fn index_changes<M, R, E, C>(
    parent: Option<&EmergencyLifecycleDirectoryV1>,
    before: Option<&StateRootV1>,
    after: Option<&StateRootV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
) -> Result<EmergencyLifecycleDirectoryV1, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut builder = EmergencyLifecycleBuilderV1::new(ctx.values.memory, ctx.values.class)?;
    if let Some(parent) = parent {
        parent.check_budget(ctx.values.memory, ctx.values.class)?;
        for row in &parent.minimum_services {
            builder.push_minimum_service_ref(row.clone(), scan, &mut ctx.cancelled)?;
        }
        for row in &parent.authorizations {
            builder.push_authorization_ref(row.clone(), scan, &mut ctx.cancelled)?;
        }
        for row in &parent.batches {
            builder.push_batch_ref(row.clone(), scan, &mut ctx.cancelled)?;
        }
        for row in &parent.observations {
            builder.push_observation_ref(row.clone(), scan, &mut ctx.cancelled)?;
        }
    }
    let p = emergency_profile();
    let trees = work.max_retained_trees;
    let mut d = ChangeDecoder {
        ctx,
        scan,
        work,
        profile: p,
        trees_remaining: trees,
        tree_depth: 0,
    };
    index_minimum_services(&mut builder, before, after, &mut d)?;
    index_authorizations(&mut builder, before, after, &mut d)?;
    index_batches(&mut builder, before, after, &mut d)?;
    index_observations(&mut builder, before, after, &mut d)?;
    let active_inputs = close_inputs::update(
        parent.map(|directory| &directory.active_inputs),
        before,
        after,
        &mut d,
    )?;
    d.check()?;
    builder.finish(active_inputs)
}

/// Update only the Emergency profile. `accept_body` has already verified the
/// complete original DTO; this child records the three maps consumed by the
/// fixed normal selector and shares unchanged rows on transitions.
pub(crate) fn update<T, M, R, E, C>(
    parent: Option<&EmergencyLifecycleDirectoryV1>,
    before: Option<&StateRootV1>,
    after: Option<&StateRootV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
) -> Result<Option<EmergencyLifecycleDirectoryV1>, ValueTreeError>
where
    T: CompositeBranchCodecV1,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    if <T as CompositeBranchCodecV1>::BRANCH != WorldRuleCompositeBranchV1::EmergencySupply {
        return Ok(None);
    }
    if before == after {
        if let Some(parent) = parent {
            return Ok(Some(parent.share(
                ctx.values.memory,
                ctx.values.class,
                scan,
                &mut ctx.cancelled,
            )?));
        }
        if before.is_some() {
            return Err(invalid());
        }
        let active_inputs =
            close_inputs::EmergencyActiveInputsV1::empty(ctx.values.memory, ctx.values.class)?;
        return Ok(Some(
            EmergencyLifecycleBuilderV1::new(ctx.values.memory, ctx.values.class)?
                .finish(active_inputs)?,
        ));
    }
    if before.is_some() && parent.is_none() {
        return Err(invalid());
    }
    Ok(Some(index_changes(parent, before, after, ctx, scan, work)?))
}

/// Fold the transaction's actual Emergency body into the accepted projection.
/// The caller must first bind/validate that body with the closed branch codec;
/// this helper only updates selector data and does not authorize a command.
pub(crate) fn project_current<M, R, E, C>(
    parent: &EmergencyLifecycleDirectoryV1,
    before: Option<&StateRootV1>,
    after: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
) -> Result<EmergencyLifecycleDirectoryV1, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    let directory = index_changes(Some(parent), before, Some(after), ctx, scan, work)?;
    ctx.check()?;
    Ok(directory)
}
