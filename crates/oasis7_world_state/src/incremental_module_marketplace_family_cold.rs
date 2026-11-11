use super::*;
pub fn qualify_module_marketplace_family_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut ModuleMarketplaceFamilyWorkV1,
) -> Result<ReservedValue<ModuleMarketplaceFamilyQualificationV1>, WorldStateOwnerError> {
    let result = (|| {
        check(ctx)?;
        let memory = ctx.values.memory;
        let class = ctx.values.class;
        let _phase = memory
            .reserve(8192, class)
            .map_err(WorldStateOwnerError::Memory)?;
        roots.validate().map_err(WorldStateOwnerError::RootSet)?;
        if ctx.state.allocation_class() != Some(class)
            || ctx
                .state
                .allocation_budget()
                .is_none_or(|m| !m.same_pool(memory))
        {
            return Err(bad());
        }
        let charge = memory
            .reserve(512, class)
            .map_err(WorldStateOwnerError::Memory)?;
        let mut metadata = [false; 3];
        let mut counts = [0u64; 3];
        {
            let stop = RefCell::new(&mut ctx.cancelled);
            let mut local = WorldFieldReadContextV1 {
                root: roots.state_root(StateNamespaceV1::Business),
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
                cancelled: || stop.borrow_mut()(),
            };
            for root in &roots.state {
                let scanned = StateScanContext {
                    source: local.state,
                    memory,
                    class,
                    io: &mut *scan,
                }
                .visit_entries_with_reader(
                    root,
                    |entry, guard| {
                        check(&mut local)?;
                        let Some((i, field, key)) = identify(&entry.key)? else {
                            return Ok(());
                        };
                        work.step(&mut local.cancelled)?;
                        if entry.namespace != StateNamespaceV1::Business {
                            return Err(bad());
                        }
                        let parsed = WorldFieldKeyV1::from_bytes(key.as_bytes())
                            .map_err(WorldStateOwnerError::Invalid)?;
                        if parsed.kind == WorldFieldRecordKindV1::Metadata {
                            if metadata[i] {
                                return Err(bad());
                            }
                            local
                                .records()
                                .presence(&entry.value, &key, 0)
                                .map_err(WorldStateOwnerError::Value)?;
                            metadata[i] = true;
                        } else {
                            let max = work.max_record_bytes.min(local.record_bytes);
                            let child = bridge::child_root(
                                field,
                                entry,
                                &mut local.values,
                                local.semantic_bytes,
                                max,
                                &mut local.cancelled,
                            )?;
                            bridge::full_child(
                                field,
                                &child,
                                guard,
                                &mut local.values,
                                local.key_work,
                                max,
                                &mut local.cancelled,
                            )?;
                            counts[i] = counts[i].checked_add(1).ok_or_else(bad)?;
                        }
                        check(&mut local)
                    },
                    || stop.borrow_mut()(),
                );
                check(&mut local)?;
                map_scan(scanned)?;
            }
        }
        if metadata != [true; 3] {
            return Err(bad());
        }
        check(ctx)?;
        let roots = roots.digest().map_err(WorldStateOwnerError::RootSet)?;
        check(ctx)?;
        Ok(charge.retain(ModuleMarketplaceFamilyQualificationV1 {
            roots,
            class,
            counts,
        }))
    })();
    check(ctx)?;
    result
}
pub fn readmit_module_marketplace_family_ordinary_v1(
    source: &ReservedValue<ModuleMarketplaceFamilyQualificationV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<ModuleMarketplaceFamilyQualificationV1>, WorldStateOwnerError> {
    if cancelled() {
        return Err(WorldStateOwnerError::Value(ValueTreeError::Cancelled));
    }
    if source.class != MemoryClass::Recovery
        || !source.uses_budget(memory)
        || source.reserved_bytes() < 512
    {
        return Err(bad());
    }
    let _phase = memory
        .reserve(4096, MemoryClass::Ordinary)
        .map_err(WorldStateOwnerError::Memory)?;
    roots.validate().map_err(WorldStateOwnerError::RootSet)?;
    source.validate_binding(roots)?;
    let charge = memory
        .reserve(512, MemoryClass::Ordinary)
        .map_err(WorldStateOwnerError::Memory)?;
    if cancelled() {
        return Err(WorldStateOwnerError::Value(ValueTreeError::Cancelled));
    }
    Ok(charge.retain(ModuleMarketplaceFamilyQualificationV1 {
        roots: source.roots,
        class: MemoryClass::Ordinary,
        counts: source.counts,
    }))
}
