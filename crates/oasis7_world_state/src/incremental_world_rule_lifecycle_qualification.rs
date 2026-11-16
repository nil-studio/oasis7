//! Paid lifecycle-index qualification, receipt acceptance, and ordinary readmission.
use super::*;

fn debit_outer(work: &mut WorldRulesFamilyWorkV1, bytes: usize) -> Result<(), ValueTreeError> {
    work.outer_records_remaining = work
        .outer_records_remaining
        .checked_sub(1)
        .ok_or_else(limited)?;
    work.remainder.cells_remaining = work
        .remainder
        .cells_remaining
        .checked_sub(u64::try_from(bytes).map_err(|_| limited())?)
        .ok_or_else(limited)?;
    Ok(())
}

fn canonical_world_rules_key(
    raw: &[u8],
) -> Result<Option<(WorldFieldKeyV1, WorldFieldRecordKindV1)>, ValueTreeError> {
    const DOMAIN: &[u8] = b"oasis7.iws.world-field-key.v1\0";
    if !raw.starts_with(DOMAIN) {
        return Ok(None);
    }
    let binding = WorldFieldKeyV1::from_bytes(raw).map_err(ValueTreeError::Invalid)?;
    if binding.owner != FieldOwner::WorldState || binding.name != "world_rules" {
        return Ok(None);
    }
    if binding.namespace != StateNamespaceV1::Authorization {
        return Err(invalid());
    }
    let key = WorldFieldKeyV1::new(
        world_rules_descriptor().map_err(|_| invalid())?,
        binding.kind,
        binding.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != raw {
        return Err(invalid());
    }
    Ok(Some((key, binding.kind)))
}

fn read_member_semantic<M, R, E, C>(
    value: &[u8; 32],
    key: &WorldFieldKeyV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_semantic: usize,
) -> Result<(Vec<u8>, MemoryReservation), ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let reserve = max_semantic
        .checked_add(size_of::<Vec<u8>>() + 64)
        .ok_or_else(limited)?;
    let fee = ctx
        .values
        .memory
        .reserve(reserve, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    let mut semantic = Vec::new();
    semantic
        .try_reserve_exact(max_semantic)
        .map_err(|_| limited())?;
    if semantic.capacity() > max_semantic {
        return Err(invalid());
    }
    read_member_record_key_v1(
        value,
        key,
        &mut ctx.values,
        MemberRecordLimitsV1 {
            semantic_bytes: u64::try_from(max_semantic).map_err(|_| limited())?,
            payload_bytes: ctx.record_bytes,
            record_bytes: ctx.record_bytes,
        },
        &mut semantic,
        &mut ctx.cancelled,
    )?;
    ctx.check()?;
    Ok((semantic, fee))
}

fn sort_ids(
    ids: &mut Vec<u64>,
    work: &mut WorldRulesFamilyWorkV1,
    reject_duplicates: bool,
) -> Result<(), ValueTreeError> {
    let n = ids.len();
    let levels = if n <= 1 {
        0
    } else {
        usize::BITS as usize - (n - 1).leading_zeros() as usize
    };
    let comparisons = n.checked_mul(levels).ok_or_else(limited)?;
    let bytes = comparisons
        .checked_mul(size_of::<u64>())
        .ok_or_else(limited)?;
    work.remainder.cells_remaining = work
        .remainder
        .cells_remaining
        .checked_sub(u64::try_from(comparisons).map_err(|_| limited())?)
        .ok_or_else(limited)?;
    work.outer_records_remaining = work
        .outer_records_remaining
        .checked_sub(u64::try_from(comparisons).map_err(|_| limited())?)
        .ok_or_else(limited)?;
    let _ = bytes;
    ids.sort_unstable();
    if reject_duplicates {
        if ids.windows(2).any(|w| w[0] == w[1]) {
            return Err(invalid());
        }
    } else {
        ids.dedup();
    }
    Ok(())
}

fn collect_initial_shipment_ids<M, R, E, C>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldRulesFamilyWorkV1,
    expected_count: usize,
) -> Result<(Vec<u64>, MemoryReservation), ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let memory = ctx.values.memory.clone();
    let class = ctx.values.class;
    let bytes = expected_count
        .checked_mul(size_of::<u64>())
        .and_then(|n| n.checked_add(size_of::<Vec<u64>>() + 64))
        .ok_or_else(limited)?;
    let fee = memory
        .reserve(bytes, class)
        .map_err(ValueTreeError::Memory)?;
    let mut ids = Vec::new();
    ids.try_reserve_exact(expected_count)
        .map_err(|_| limited())?;
    if ids.capacity() > expected_count {
        return Err(invalid());
    }
    let stop = std::cell::RefCell::new(&mut ctx.cancelled);
    let mut local = WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Authorization),
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
        cancelled: || stop.borrow_mut()(),
    };
    let max_semantic = usize::try_from(local.semantic_bytes)
        .unwrap_or(usize::MAX)
        .min(usize::try_from(work.max_semantic_bytes).unwrap_or(usize::MAX));
    for root in roots
        .state
        .iter()
        .filter(|root| root.namespace == StateNamespaceV1::Authorization)
    {
        let result = StateScanContext {
            source: local.state,
            memory: &memory,
            class,
            io: scan,
        }
        .visit_entries(
            root,
            |entry| {
                local.check()?;
                if entry.namespace != StateNamespaceV1::Authorization {
                    return Err(invalid());
                }
                let Some((key, kind)) = canonical_world_rules_key(&entry.key)? else {
                    return Ok(());
                };
                if key.as_bytes() != entry.key {
                    return Err(invalid());
                }
                if kind != WorldFieldRecordKindV1::Entry {
                    return Ok(());
                }
                debit_outer(work, entry.key.len())?;
                let (semantic, _semantic_fee) =
                    read_member_semantic(&entry.value, &key, &mut local, max_semantic)?;
                if let Some((id, marker)) = shipment_key(&semantic)? {
                    if marker {
                        if ids.len() == expected_count {
                            return Err(invalid());
                        }
                        ids.push(id);
                    }
                }
                local.check()
            },
            || stop.borrow_mut()(),
        );
        match result {
            Ok(_) => (),
            Err(StateScanError::Visitor(e)) => return Err(e),
            Err(StateScanError::Memory(e)) => return Err(ValueTreeError::Memory(e)),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(StateScanError::ResourceLimited) => return Err(limited()),
            Err(_) => return Err(invalid()),
        }
        local.check()?;
    }
    if ids.len() != expected_count {
        return Err(invalid());
    }
    sort_ids(&mut ids, work, true)?;
    Ok((ids, fee))
}

fn collect_changed_shipment_ids<M, R, E, C>(
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldRulesFamilyWorkV1,
) -> Result<(Vec<u64>, MemoryReservation), ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let cap = record.writes.len();
    let bytes = cap
        .checked_mul(size_of::<u64>())
        .and_then(|n| n.checked_add(size_of::<Vec<u64>>() + 64))
        .ok_or_else(limited)?;
    let fee = ctx
        .values
        .memory
        .reserve(bytes, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    let mut ids = Vec::new();
    ids.try_reserve_exact(cap).map_err(|_| limited())?;
    if ids.capacity() > cap {
        return Err(invalid());
    }
    let max_semantic = usize::try_from(ctx.semantic_bytes)
        .unwrap_or(usize::MAX)
        .min(usize::try_from(work.max_semantic_bytes).unwrap_or(usize::MAX));
    for write in &record.writes {
        ctx.check()?;
        if write.namespace != StateNamespaceV1::Authorization {
            continue;
        }
        let Some((key, kind)) = canonical_world_rules_key(&write.mutation.key)? else {
            continue;
        };
        if kind != WorldFieldRecordKindV1::Entry {
            continue;
        }
        debit_outer(work, write.mutation.key.len())?;
        let value = write
            .mutation
            .after
            .or(write.mutation.before)
            .ok_or_else(invalid)?;
        let (semantic, _semantic_fee) = read_member_semantic(&value, &key, ctx, max_semantic)?;
        if let Some((id, _)) = shipment_key(&semantic)? {
            ids.push(id);
        }
    }
    sort_ids(&mut ids, work, false)?;
    Ok((ids, fee))
}

fn copy_supply_rows(
    builder: &mut WorldRuleLifecycleIndexBuilderV1,
    supply: &SupplyFamilyQualificationV1,
    work: &mut WorldRulesFamilyWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), ValueTreeError> {
    let counts = supply.record_counts();
    if usize::try_from(counts[0]).map_err(|_| limited())? != supply.lifecycle_authorizations().len()
        || usize::try_from(counts[1]).map_err(|_| limited())? != supply.lifecycle_batches().len()
    {
        return Err(invalid());
    }
    for row in supply.lifecycle_authorizations() {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        if !row.uses_budget(&builder.memory) {
            return Err(invalid());
        }
        let value = &***row;
        debit_outer(work, value.bytes()?)?;
        builder.push_supply_authorization_ref(Arc::clone(row))?;
    }
    for row in supply.lifecycle_batches() {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        if !row.uses_budget(&builder.memory) {
            return Err(invalid());
        }
        let value = &***row;
        debit_outer(work, value.bytes()?)?;
        builder.push_supply_batch_ref(Arc::clone(row))?;
    }
    Ok(())
}

fn copy_emergency_rows(
    builder: &mut WorldRuleLifecycleIndexBuilderV1,
    emergency: &WorldRuleCompositeQualificationV1<crate::world_rules::EmergencySupplyStateV1>,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut WorldRulesFamilyWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), ValueTreeError> {
    emergency.check_budget(memory, class)?;
    let directory = emergency
        .emergency_lifecycle_directory()
        .ok_or_else(invalid)?;
    for row in directory.minimum_service_rows() {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        debit_outer(work, row.bytes()?)?;
        builder.push_minimum_service_ref(Arc::clone(row))?;
    }
    for row in directory.authorization_rows() {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        debit_outer(work, row.bytes()?)?;
        builder.push_emergency_authorization_ref(Arc::clone(row))?;
    }
    // Keep every row, including committed batches. Original selectors decide
    // whether a selected batch is relevant to a particular command.
    for row in directory.batch_rows() {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        debit_outer(work, row.bytes()?)?;
        builder.push_emergency_batch_ref(Arc::clone(row))?;
    }
    for row in directory.observation_rows() {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        debit_outer(work, row.len())?;
        builder.push_observation_ref(Arc::clone(row))?;
    }
    Ok(())
}

fn validate_source_families<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    supply: &SupplyFamilyQualificationV1,
    remainder: &WorldRuleRemainderQualificationV1,
    emergency: &WorldRuleCompositeQualificationV1<crate::world_rules::EmergencySupplyStateV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    ctx.check()?;
    let digest = roots.digest().map_err(|_| invalid())?;
    if ctx.root != roots.state_root(StateNamespaceV1::Authorization)
        || supply.allocation_class() != ctx.values.class
        || remainder.allocation_class() != ctx.values.class
        || emergency.allocation_class() != ctx.values.class
    {
        return Err(invalid());
    }
    supply.validate_binding(&digest)?;
    if supply
        .lifecycle_authorizations()
        .iter()
        .any(|row| !row.uses_budget(&ctx.values.memory))
        || supply
            .lifecycle_batches()
            .iter()
            .any(|row| !row.uses_budget(&ctx.values.memory))
    {
        return Err(invalid());
    }
    remainder.validate_binding(roots)?;
    emergency.validate_binding(roots)?;
    emergency.check_budget(&ctx.values.memory, ctx.values.class)?;
    if supply.rules_present() != remainder.rules_present()
        || emergency.parent_present() != remainder.rules_present()
    {
        return Err(invalid());
    }
    ctx.check()
}

/// Build a fixed WorldRules lifecycle selector from the exact typed family
/// qualifications. It retains only selector data; actions still reopen their
/// selected records through the original Current codecs.
#[allow(clippy::too_many_arguments)]
pub fn qualify_world_rule_lifecycle_index_v1<M, R, E, C>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    outer_scan: &mut StateScanBudget,
    _inner_scan: &mut StateScanBudget,
    work: &mut WorldRulesFamilyWorkV1,
    supply: &SupplyFamilyQualificationV1,
    remainder: &WorldRuleRemainderQualificationV1,
    emergency: &WorldRuleCompositeQualificationV1<crate::world_rules::EmergencySupplyStateV1>,
) -> Result<ReservedValue<WorldRuleLifecycleIndexV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    validate_source_families(roots, supply, remainder, emergency, ctx)?;
    let digest = roots.digest().map_err(|_| invalid())?;
    let mut builder = WorldRuleLifecycleIndexBuilderV1::new(&ctx.values.memory, ctx.values.class)?;
    copy_supply_rows(&mut builder, supply, work, &mut ctx.cancelled)?;
    copy_emergency_rows(
        &mut builder,
        emergency,
        &ctx.values.memory,
        ctx.values.class,
        work,
        &mut ctx.cancelled,
    )?;

    let expected = usize::try_from(remainder.record_counts()[2]).map_err(|_| limited())?;
    let (ids, _ids_fee) = collect_initial_shipment_ids(roots, ctx, outer_scan, work, expected)?;
    for id in ids {
        ctx.check()?;
        let Some((value, text_fees)) =
            read_shipment_texts(id, ctx, work.remainder.max_payload_bytes)?
        else {
            return Err(invalid());
        };
        let row = LifecycleSupplyShipmentV1 {
            outer_id: id,
            value,
        };
        debit_outer(work, row.bytes()?)?;
        builder.push_shipment_paid(row, text_fees)?;
    }
    if builder.supply_shipments.len() != expected {
        return Err(invalid());
    }
    ctx.check()?;
    builder.finish(digest)
}

/// Rebuild the selector after one verified WorldRules transition. Only IDs
/// found in that transition's authenticated writes are reopened; unchanged
/// shipment rows are shared from the parent projection.
#[allow(clippy::too_many_arguments)]
pub fn accept_world_rule_lifecycle_index_v1<M, R, E, C>(
    parent_index: &ReservedValue<WorldRuleLifecycleIndexV1>,
    parent_supply: &SupplyFamilyQualificationV1,
    parent_remainder: &WorldRuleRemainderQualificationV1,
    parent_emergency: &WorldRuleCompositeQualificationV1<
        crate::world_rules::EmergencySupplyStateV1,
    >,
    supply: &SupplyFamilyQualificationV1,
    remainder: &WorldRuleRemainderQualificationV1,
    emergency: &WorldRuleCompositeQualificationV1<crate::world_rules::EmergencySupplyStateV1>,
    witness: &crate::incremental_verified_world_transition::VerifiedWorldTransitionV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldRulesFamilyWorkV1,
) -> Result<ReservedValue<WorldRuleLifecycleIndexV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let record = witness.record();
    let before_digest = record.before.digest().map_err(|_| invalid())?;
    let after_digest = record.after.digest().map_err(|_| invalid())?;
    if ctx.values.class != MemoryClass::Ordinary
        || parent_index.class != MemoryClass::Ordinary
        || !parent_index.uses_budget(&ctx.values.memory)
        || !ctx.values.memory.same_pool(&parent_index.memory)
        || ctx.root != record.after.state_root(StateNamespaceV1::Authorization)
    {
        return Err(invalid().into());
    }
    witness.check_context(&ctx.values.memory, ctx.values.class, ctx.root)?;
    parent_index.validate_binding(&record.before)?;
    parent_supply.validate_binding(&before_digest)?;
    parent_remainder.validate_binding(&record.before)?;
    parent_emergency.validate_binding(&record.before)?;
    supply.validate_binding(&after_digest)?;
    remainder.validate_binding(&record.after)?;
    emergency.validate_binding(&record.after)?;
    validate_source_families(&record.after, supply, remainder, emergency, ctx)?;

    let mut builder =
        WorldRuleLifecycleIndexBuilderV1::new(&ctx.values.memory, MemoryClass::Ordinary)?;
    copy_supply_rows(&mut builder, supply, work, &mut ctx.cancelled)?;
    copy_emergency_rows(
        &mut builder,
        emergency,
        &ctx.values.memory,
        MemoryClass::Ordinary,
        work,
        &mut ctx.cancelled,
    )?;

    let (changed, _changed_fee) = collect_changed_shipment_ids(record, ctx, work)?;
    let expected = usize::try_from(remainder.record_counts()[2]).map_err(|_| limited())?;
    if expected
        > parent_index
            .supply_shipments
            .len()
            .saturating_add(changed.len())
    {
        return Err(limited().into());
    }
    let mut old_at = 0usize;
    let mut changed_at = 0usize;
    while old_at < parent_index.supply_shipments.len() || changed_at < changed.len() {
        ctx.check()?;
        let old_id = parent_index
            .supply_shipments
            .get(old_at)
            .map(|row| row.outer_id);
        let changed_id = changed.get(changed_at).copied();
        match (old_id, changed_id) {
            (Some(old), Some(new)) if old < new => {
                let row = &parent_index.supply_shipments[old_at];
                if !row.uses_budget(&ctx.values.memory) {
                    return Err(invalid().into());
                }
                debit_outer(work, row.bytes()?)?;
                builder.push_supply_shipment_ref(Arc::clone(row))?;
                old_at += 1;
            }
            (Some(old), Some(new)) if old == new => {
                if let Some((value, text_fees)) =
                    read_shipment_texts(new, ctx, work.remainder.max_payload_bytes)?
                {
                    let row = LifecycleSupplyShipmentV1 {
                        outer_id: new,
                        value,
                    };
                    debit_outer(work, row.bytes()?)?;
                    builder.push_shipment_paid(row, text_fees)?;
                }
                old_at += 1;
                changed_at += 1;
            }
            (_, Some(new)) => {
                if let Some((value, text_fees)) =
                    read_shipment_texts(new, ctx, work.remainder.max_payload_bytes)?
                {
                    let row = LifecycleSupplyShipmentV1 {
                        outer_id: new,
                        value,
                    };
                    debit_outer(work, row.bytes()?)?;
                    builder.push_shipment_paid(row, text_fees)?;
                }
                changed_at += 1;
            }
            (Some(_), None) => {
                let row = &parent_index.supply_shipments[old_at];
                if !row.uses_budget(&ctx.values.memory) {
                    return Err(invalid().into());
                }
                debit_outer(work, row.bytes()?)?;
                builder.push_supply_shipment_ref(Arc::clone(row))?;
                old_at += 1;
            }
            (None, None) => break,
        }
    }
    if builder.supply_shipments.len() != expected {
        return Err(invalid().into());
    }
    ctx.check()?;
    Ok(builder.finish(after_digest)?)
}

/// Admit the immutable selector under Ordinary memory by copying each row
/// only after reserving its concrete storage. The source qualification is not
/// reinterpreted and cancellation is checked throughout the copy.
pub fn readmit_world_rule_lifecycle_index_ordinary_v1(
    source: &ReservedValue<WorldRuleLifecycleIndexV1>,
    memory: &MemoryBudget,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<ReservedValue<WorldRuleLifecycleIndexV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if source.class != MemoryClass::Recovery || !source.uses_budget(memory) {
        return Err(invalid());
    }
    source.check_budget(memory, MemoryClass::Recovery)?;
    let mut builder = WorldRuleLifecycleIndexBuilderV1::new(memory, MemoryClass::Ordinary)?;
    for row in &source.supply_authorizations {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        builder.copy_supply_authorization(&***row)?;
    }
    for row in &source.supply_batches {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        builder.copy_supply_batch(&***row)?;
    }
    for row in &source.supply_shipments {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        builder.copy_supply_shipment(&***row)?;
    }
    for row in &source.minimum_services {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        builder.copy_minimum_service(&***row)?;
    }
    for row in &source.emergency_authorizations {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        builder.copy_emergency_authorization(&***row)?;
    }
    for row in &source.emergency_batches {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        builder.copy_emergency_batch(&***row)?;
    }
    for row in &source.observation_keys {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        builder.copy_observation(&**row)?;
    }
    let output = builder.finish(source.roots)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(output)
}
