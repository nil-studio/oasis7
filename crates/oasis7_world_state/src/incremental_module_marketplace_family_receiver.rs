use super::*;
pub fn accept_module_marketplace_family_transition_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<ModuleMarketplaceFamilyQualificationV1>,
    verifier: &TransitionVerifier<'_, M>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut ModuleMarketplaceFamilyWorkV1,
) -> Result<ReservedValue<ModuleMarketplaceFamilyQualificationV1>, WorldStateOwnerError> {
    let result = (|| {
        {
            check(ctx)?;
            let memory = ctx.values.memory;
            if ctx.values.class != MemoryClass::Ordinary
                || parent.class != MemoryClass::Ordinary
                || !parent.uses_budget(memory)
                || parent.reserved_bytes() < 512
                || verifier.class != MemoryClass::Ordinary
                || !verifier.memory.same_pool(memory)
                || ctx.root != record.after.state_root(StateNamespaceV1::Business)
            {
                return Err(bad());
            }
            let _phase = memory
                .reserve(8192, MemoryClass::Ordinary)
                .map_err(WorldStateOwnerError::Memory)?;
            parent.validate_binding(&record.before)?;
        }
        let witness=crate::incremental_verified_world_transition::verify_world_transition_v1(verifier,record,ctx) .map_err(|e|match e {crate::incremental_verified_world_transition::WorldTransitionVerificationErrorV1::Execution(e)=>WorldStateOwnerError::RootSet(e),crate::incremental_verified_world_transition::WorldTransitionVerificationErrorV1::Value(e)=>WorldStateOwnerError::Value(e)})?;
        accept_module_marketplace_family_verified_v1(parent, &witness, ctx, scan, work)
    })();
    check(ctx)?;
    result
}

pub(crate) fn accept_module_marketplace_family_verified_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<ModuleMarketplaceFamilyQualificationV1>,
    witness: &crate::incremental_verified_world_transition::VerifiedWorldTransitionV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut ModuleMarketplaceFamilyWorkV1,
) -> Result<ReservedValue<ModuleMarketplaceFamilyQualificationV1>, WorldStateOwnerError> {
    witness
        .check_context(ctx.values.memory, ctx.values.class, ctx.root)
        .map_err(WorldStateOwnerError::Value)?;
    let record = witness.record();
    let result = (|| {
        check(ctx)?;
        let memory = ctx.values.memory;
        if ctx.values.class != MemoryClass::Ordinary
            || parent.class != MemoryClass::Ordinary
            || !parent.uses_budget(memory)
            || parent.reserved_bytes() < 512
            || ctx.root != record.after.state_root(StateNamespaceV1::Business)
        {
            return Err(bad());
        }
        let _phase = memory
            .reserve(8192, MemoryClass::Ordinary)
            .map_err(WorldStateOwnerError::Memory)?;
        parent.validate_binding(&record.before)?;
        let checked = witness.prepared();
        let charge = memory
            .reserve(512, MemoryClass::Ordinary)
            .map_err(WorldStateOwnerError::Memory)?;
        let mut counts = parent.counts;
        {
            let stop = RefCell::new(&mut ctx.cancelled);
            let mut local = WorldFieldReadContextV1 {
                root: record.after.state_root(StateNamespaceV1::Business),
                state: ctx.state,
                values: ValueReadContext {
                    source: ctx.values.source,
                    memory,
                    class: MemoryClass::Ordinary,
                    io: &mut *ctx.values.io,
                },
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                take_state_error: &mut ctx.take_state_error,
                cancelled: || stop.borrow_mut()(),
            };
            let result = StateScanContext {
                source: local.state,
                memory,
                class: MemoryClass::Ordinary,
                io: scan,
            }
            .with_reader(
                || stop.borrow_mut()(),
                |guard| {
                    for write in &checked.record.writes {
                        check(&mut local)?;
                        work.step(&mut local.cancelled)?;
                        let Some((i, field, key)) = identify(&write.mutation.key)? else {
                            continue;
                        };
                        if write.namespace != StateNamespaceV1::Business {
                            return Err(bad());
                        }
                        let parsed = WorldFieldKeyV1::from_bytes(key.as_bytes())
                            .map_err(WorldStateOwnerError::Invalid)?;
                        if parsed.kind == WorldFieldRecordKindV1::Metadata {
                            if write.mutation.before.is_none()
                                || write.mutation.before != write.mutation.after
                            {
                                return Err(bad());
                            }
                            continue;
                        }
                        let max = work.max_record_bytes.min(local.record_bytes);
                        let _keys = memory
                            .reserve(2048 + key.as_bytes().len(), MemoryClass::Ordinary)
                            .map_err(WorldStateOwnerError::Memory)?;
                        let mut outer = StateEntryV1 {
                            namespace: StateNamespaceV1::Business,
                            key: key.as_bytes().to_vec(),
                            value: [0; 32],
                        };
                        let old = if let Some(id) = write.mutation.before {
                            outer.value = id;
                            Some(bridge::child_root(
                                field,
                                &outer,
                                &mut local.values,
                                local.semantic_bytes,
                                max,
                                &mut local.cancelled,
                            )?)
                        } else {
                            None
                        };
                        let new = if let Some(id) = write.mutation.after {
                            outer.value = id;
                            Some(bridge::child_root(
                                field,
                                &outer,
                                &mut local.values,
                                local.semantic_bytes,
                                max,
                                &mut local.cancelled,
                            )?)
                        } else {
                            None
                        };
                        match (old, new) {
                            (None, None) => return Err(bad()),
                            (None, Some(new)) => {
                                bridge::full_child(
                                    field,
                                    &new,
                                    guard,
                                    &mut local.values,
                                    local.key_work,
                                    max,
                                    &mut local.cancelled,
                                )?;
                                counts[i] = counts[i].checked_add(1).ok_or_else(bad)?;
                            }
                            (Some(_), None) => {
                                counts[i] = counts[i].checked_sub(1).ok_or_else(bad)?
                            }
                            (Some(old), Some(new)) => {
                                if *old != *new {
                                    let mut records = local.records();
                                    match ModuleMarketplaceFieldV1::from_name(field.name)
                                        .ok_or_else(bad)?
                                    {
                                        ModuleMarketplaceFieldV1::VisualEntities => {
                                            bridge::visual_delta(
                                                field,
                                                &old,
                                                &new,
                                                guard,
                                                &mut records,
                                                &mut work.changed_cells_remaining,
                                                max,
                                            )?
                                        }
                                        ModuleMarketplaceFieldV1::ArtifactListings => {
                                            bridge::listing_delta(
                                                field,
                                                &old,
                                                &new,
                                                &mut records,
                                                &mut work.changed_cells_remaining,
                                                max,
                                            )?
                                        }
                                        ModuleMarketplaceFieldV1::ArtifactBids => {
                                            bridge::bids_delta(
                                                field,
                                                &old,
                                                &new,
                                                guard,
                                                &mut records,
                                                &mut work.changed_cells_remaining,
                                                &mut work.membership_cells_remaining,
                                                max,
                                            )?
                                        }
                                    }
                                }
                            }
                        }
                        check(&mut local)?;
                    }
                    Ok(())
                },
            );
            check(&mut local)?;
            map_scan(result)?;
        }
        check(ctx)?;
        let roots = checked
            .record
            .after
            .digest()
            .map_err(WorldStateOwnerError::RootSet)?;
        check(ctx)?;
        Ok(charge.retain(ModuleMarketplaceFamilyQualificationV1 {
            roots,
            class: MemoryClass::Ordinary,
            counts,
        }))
    })();
    check(ctx)?;
    result
}
