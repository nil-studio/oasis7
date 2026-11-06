use super::*;
use std::cell::RefCell;
pub fn qualify_world_collection_controls_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldCollectionControlsWorkV1,
) -> Result<ReservedValue<WorldCollectionControlsQualificationV1>, ValueTreeError> {
    ctx.check()?;
    let memory = ctx.values.memory;
    let class = ctx.values.class;
    let _phase = memory.reserve(8192, class)?;
    if ctx.state.allocation_class() != Some(class)
        || ctx
            .state
            .allocation_budget()
            .is_none_or(|m| !m.same_pool(memory))
    {
        return Err(invalid());
    }
    roots.validate().map_err(|_| invalid())?;
    let charge = memory.reserve(
        std::mem::size_of::<WorldCollectionControlsQualificationV1>() + 128,
        class,
    )?;
    let mut groups = CollectionControlsIndexV1::new(memory, class);
    let mut iw = CollectionControlsIndexWorkV1 {
        steps_remaining: std::mem::take(&mut work.index_steps_remaining),
    };
    let result = (|| {
        let mut counts = [[0; 2]; 3];
        let mut cursor_entries = 0;
        let mut metadata = [false; 4];
        {
            let stop = RefCell::new(&mut ctx.cancelled);
            let mut local = WorldFieldReadContextV1 {
                root: roots.state_root(StateNamespaceV1::Business),
                state: ctx.state,
                values: oasis7_formal_protocol::incremental_value::ValueReadContext {
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
            for scanned in &roots.state {
                let result = StateScanContext {
                    source: local.state,
                    memory,
                    class,
                    io: &mut *scan,
                }
                .visit_entries(
                    scanned,
                    |entry| {
                        local.check()?;
                        let Some((field, key)) = key(&entry.key)? else {
                            return Ok(());
                        };
                        if entry.namespace != namespace(field) {
                            return Err(invalid());
                        }
                        let decoded = row(field, &key, &entry.value, false, &mut local, work)?;
                        if let Row::Meta(i) = decoded {
                            if metadata[i] {
                                return Err(invalid());
                            }
                            metadata[i] = true;
                        } else {
                            adjust(
                                &mut groups,
                                decoded,
                                true,
                                &mut counts,
                                &mut cursor_entries,
                                &mut iw,
                                &mut local.cancelled,
                            )?;
                        }
                        local.check()
                    },
                    || stop.borrow_mut()(),
                );
                local.check()?;
                match result {
                    Ok(_) => (),
                    Err(StateScanError::Visitor(e)) => return Err(e),
                    Err(StateScanError::Memory(e)) => return Err(e.into()),
                    Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
                    Err(StateScanError::ResourceLimited) => {
                        return Err(ValueTreeError::ResourceLimited("collection controls scan"));
                    }
                    Err(_) => return Err(invalid()),
                }
            }
        }
        if metadata != [true; 4] {
            return Err(invalid());
        }
        groups.visit_page(None, usize::MAX, &mut iw, &mut ctx.cancelled, |meta| {
            if !meta.present {
                return Err(invalid());
            }
            Ok(())
        })?;
        final_counts(&groups, &counts, cursor_entries)?;
        ctx.check()?;
        Ok(WorldCollectionControlsQualificationV1 {
            roots: roots.digest().map_err(|_| invalid())?,
            class,
            counts,
            cursor_entries,
            groups: groups.clone(),
        })
    })();
    work.index_steps_remaining = iw.steps_remaining;
    ctx.check()?;
    Ok(charge.retain(result?))
}
pub fn readmit_world_collection_controls_ordinary_v1(
    source: &ReservedValue<WorldCollectionControlsQualificationV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    work: &mut WorldCollectionControlsWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<WorldCollectionControlsQualificationV1>, ValueTreeError> {
    work.step(&mut cancelled)?;
    if !source.uses_budget(memory)
        || source.reserved_bytes()
            < std::mem::size_of::<WorldCollectionControlsQualificationV1>() + 128
    {
        return Err(invalid());
    }
    source.check(memory, MemoryClass::Recovery)?;
    let _phase = memory.reserve(4096, MemoryClass::Ordinary)?;
    roots.validate().map_err(|_| invalid())?;
    source.validate_binding(roots)?;
    let charge = memory.reserve(
        std::mem::size_of::<WorldCollectionControlsQualificationV1>() + 128,
        MemoryClass::Ordinary,
    )?;
    let mut iw = CollectionControlsIndexWorkV1 {
        steps_remaining: std::mem::take(&mut work.index_steps_remaining),
    };
    let result = source
        .groups
        .readmit(memory, MemoryClass::Ordinary, &mut iw, &mut cancelled);
    work.index_steps_remaining = iw.steps_remaining;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(WorldCollectionControlsQualificationV1 {
        roots: source.roots,
        class: MemoryClass::Ordinary,
        counts: source.counts,
        cursor_entries: source.cursor_entries,
        groups: result?,
    }))
}
