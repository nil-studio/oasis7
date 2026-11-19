use super::*;
fn local<'a, M, R, E, C>(
    roots: &'a RootSetV1,
    ctx: &'a mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<WorldFieldReadContextV1<'a, M, R, &'a mut E, &'a mut C>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    Ok(WorldFieldReadContextV1 {
        root: roots.state_root(Field::profile()?.namespace()?),
        state: ctx.state,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: ctx.values.source,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: &mut ctx.cancelled,
    })
}
/// GEN/cold only: authenticate actual records once after the complete field proof.
pub(crate) fn build_starter_binding_index_v1(
    parent: &ReservedValue<WorldNativeFieldQualificationV1<Field>>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<
        '_,
        impl MerkleNodeReader,
        impl ValueNodeReader,
        impl FnMut() -> Option<ValueTreeError>,
        impl FnMut() -> bool,
    >,
    scan: &mut StateScanBudget,
    native: &mut WorldNativeQualificationWorkV1,
    work: &mut StarterBindingIndexWorkV1,
) -> Result<ReservedValue<StarterBindingIndexV1>, ValueTreeError> {
    read::check(ctx)?;
    let _phase = ctx.values.memory.reserve(32 << 10, ctx.values.class)?;
    if !parent.uses_budget(ctx.values.memory) || parent.allocation_class() != ctx.values.class {
        return Err(invalid());
    }
    parent.validate_binding(roots)?;
    let count = usize::try_from(parent.records()).map_err(|_| invalid())?;
    if count > work.max_rows || native.max_pending_records == 0 || native.max_pending_records > 256
    {
        return Err(ValueTreeError::ResourceLimited("starter index limits"));
    }
    let fee = index_fee(ctx.values.memory, ctx.values.class, count)?;
    let mut index = StarterBindingIndexV1 {
        roots: roots.digest().map_err(|_| invalid())?,
        class: ctx.values.class,
        metadata: [0; 32],
        by_binding: Vec::with_capacity(count),
        by_outer: Vec::with_capacity(count),
    };
    let page_bytes = native
        .max_pending_records
        .checked_mul(
            std::mem::size_of::<StateEntryV1>()
                + crate::incremental_world_field_key::WORLD_FIELD_KEY_MAX_BYTES,
        )
        .ok_or_else(invalid)?;
    let _page = ctx.values.memory.reserve(page_bytes, ctx.values.class)?;
    let mut pending = Vec::with_capacity(native.max_pending_records);
    let mut ctx = local(roots, ctx)?;
    let mut cursor = None;
    let mut metadata = false;
    loop {
        read::check(&mut ctx)?;
        let cancel = RefCell::new(&mut ctx.cancelled);
        let result = StateScanContext {
            source: ctx.state,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &mut *scan,
        }
        .visit_entries_page(
            ctx.root,
            cursor,
            native.max_pending_records,
            |entry| {
                native.records_remaining = native
                    .records_remaining
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited("starter index native rows"))?;
                work.spend(entry.key.len())?;
                if entry_kind(&entry.key, entry.namespace)?.is_some() {
                    if pending.len() == pending.capacity() {
                        return Err(ValueTreeError::ResourceLimited("starter index page"));
                    }
                    pending.push(entry.clone());
                }
                Ok(())
            },
            || (cancel.borrow_mut())(),
        );
        read::check(&mut ctx)?;
        let page = match result {
            Ok(page) => page,
            Err(StateScanError::Visitor(error)) => return Err(error),
            Err(StateScanError::Memory(error)) => return Err(error.into()),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(StateScanError::ResourceLimited) => {
                return Err(ValueTreeError::ResourceLimited("starter index scan"));
            }
            Err(_) => return Err(invalid()),
        };
        for entry in pending.drain(..) {
            read::check(&mut ctx)?;
            match entry_kind(&entry.key, entry.namespace)?.ok_or_else(invalid)? {
                WorldFieldRecordKindV1::Metadata => {
                    if metadata {
                        return Err(invalid());
                    }
                    metadata = true;
                    index.metadata = entry.value;
                    let key = WorldFieldKeyV1::new(
                        Field::descriptor()?,
                        WorldFieldRecordKindV1::Metadata,
                        None,
                    )
                    .map_err(ValueTreeError::Invalid)?;
                    ctx.values.verify_closure(
                        &entry.value,
                        ctx.record_bytes,
                        &mut ctx.cancelled,
                    )?;
                    ctx.records().presence(&entry.value, &key, 0)?;
                }
                WorldFieldRecordKindV1::Entry => {
                    let row = typed_row(&entry, &mut ctx, scan, native, work)?;
                    insert_row(&mut index, row, work, &mut ctx.cancelled)?;
                }
                _ => return Err(invalid()),
            }
        }
        if !page.has_more {
            break;
        }
        let next = page.last_key_digest.ok_or_else(invalid)?;
        if page.visited == 0 || cursor.is_some_and(|old| next <= old) {
            return Err(invalid());
        }
        cursor = Some(next);
    }
    read::check(&mut ctx)?;
    if !metadata || index.records() != count {
        return Err(invalid());
    }
    Ok(fee.retain(index))
}

/// Ordinary successor: reuse charged unchanged rows, decode only verified writes.
pub(crate) fn accept_starter_binding_index_verified_v1(
    parent: &ReservedValue<StarterBindingIndexV1>,
    after_field: &ReservedValue<WorldNativeFieldQualificationV1<Field>>,
    verified: &WorldNativeVerifiedTransitionV1,
    ctx: &mut WorldFieldReadContextV1<
        '_,
        impl MerkleNodeReader,
        impl ValueNodeReader,
        impl FnMut() -> Option<ValueTreeError>,
        impl FnMut() -> bool,
    >,
    scan: &mut StateScanBudget,
    native: &mut WorldNativeQualificationWorkV1,
    work: &mut StarterBindingIndexWorkV1,
) -> Result<ReservedValue<StarterBindingIndexV1>, WorldFieldOverlayErrorV1> {
    read::check(ctx)?;
    let _phase = ctx
        .values
        .memory
        .reserve(32 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    verified.check_context(ctx.values.memory, ctx.values.class, ctx.root)?;
    let record = verified.record();
    if !parent.uses_budget(ctx.values.memory)
        || !after_field.uses_budget(ctx.values.memory)
        || ctx.values.class != MemoryClass::Ordinary
        || after_field.allocation_class() != MemoryClass::Ordinary
    {
        return Err(invalid().into());
    }
    parent.validate(&record.before, ctx.values.memory, MemoryClass::Ordinary)?;
    after_field.validate_binding(&record.after)?;
    let count = usize::try_from(after_field.records()).map_err(|_| invalid())?;
    let mut inserts = 0usize;
    for write in &record.writes {
        read::check(ctx)?;
        work.spend(write.mutation.key.len())?;
        if entry_kind(&write.mutation.key, write.namespace)? == Some(WorldFieldRecordKindV1::Entry)
            && write.mutation.before.is_none()
            && write.mutation.after.is_some()
        {
            inserts = inserts.checked_add(1).ok_or_else(invalid)?;
        }
    }
    let capacity = parent.records().checked_add(inserts).ok_or_else(invalid)?;
    if count > work.max_rows || capacity > work.max_rows {
        return Err(ValueTreeError::ResourceLimited("starter successor rows").into());
    }
    let fee = index_fee(ctx.values.memory, MemoryClass::Ordinary, capacity)?;
    let mut index = StarterBindingIndexV1 {
        roots: record.after.digest().map_err(|_| invalid())?,
        class: MemoryClass::Ordinary,
        metadata: parent.metadata,
        by_binding: Vec::with_capacity(capacity),
        by_outer: Vec::with_capacity(capacity),
    };
    for row in &parent.by_outer {
        read::check(ctx)?;
        work.spend(std::mem::size_of::<HeldRow>())?;
        work.moved(1)?;
        index.by_outer.push(row.clone());
    }
    for row in &parent.by_binding {
        read::check(ctx)?;
        work.spend(std::mem::size_of::<HeldRow>())?;
        work.moved(1)?;
        index.by_binding.push(row.clone());
    }
    let mut ctx = local(&record.after, ctx)?;
    for write in &record.writes {
        read::check(&mut ctx)?;
        native.records_remaining =
            native
                .records_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "starter successor native rows",
                ))?;
        work.spend(write.mutation.key.len())?;
        let Some(kind) = entry_kind(&write.mutation.key, write.namespace)? else {
            continue;
        };
        if kind == WorldFieldRecordKindV1::Metadata {
            if write.mutation.before.is_none() || write.mutation.before != write.mutation.after {
                return Err(invalid().into());
            }
            continue;
        }
        if kind != WorldFieldRecordKindV1::Entry {
            return Err(invalid().into());
        }
        if write.mutation.before == write.mutation.after {
            continue;
        }
        let parsed =
            WorldFieldKeyV1::from_bytes(&write.mutation.key).map_err(ValueTreeError::Invalid)?;
        let key = WorldFieldKeyV1::new(Field::descriptor()?, kind, parsed.semantic_key)
            .map_err(ValueTreeError::Invalid)?;
        let found = outer_position(&index.by_outer, &key, work, &mut ctx.cancelled)?;
        match (found, write.mutation.before) {
            (Ok(pos), Some(before)) if index.by_outer[pos].member == before => {
                let row = index.by_outer.remove(pos);
                let mut binding = None;
                for (i, candidate) in index.by_binding.iter().enumerate() {
                    read::check(&mut ctx)?;
                    work.spend(std::mem::size_of::<HeldRow>())?;
                    if Arc::ptr_eq(candidate, &row) {
                        binding = Some(i);
                        break;
                    }
                }
                let binding = binding.ok_or_else(invalid)?;
                work.moved(index.by_outer.len() - pos + index.by_binding.len() - binding + 1)?;
                index.by_binding.remove(binding);
            }
            (Err(_), None) => {}
            _ => return Err(invalid().into()),
        }
        if let Some(after) = write.mutation.after {
            let _entry_fee = ctx
                .values
                .memory
                .reserve(
                    write.mutation.key.len() + std::mem::size_of::<StateEntryV1>(),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            let entry = StateEntryV1 {
                namespace: write.namespace,
                key: write.mutation.key.clone(),
                value: after,
            };
            let row = typed_row(&entry, &mut ctx, scan, native, work)?;
            insert_row(&mut index, row, work, &mut ctx.cancelled)?;
        }
    }
    read::check(&mut ctx)?;
    if index.records() != count {
        return Err(invalid().into());
    }
    Ok(fee.retain(index))
}
