//! Changed membership closure inherited from an exact qualified parent.
use super::*;
use crate::{
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_key_read::{OriginalKeyInfo, OriginalKeyKind, OriginalSemanticKeyProbe},
    incremental_record_value_read::RecordValueV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{TransitionRecordV1, TransitionVerifier},
    incremental_state::MerkleNodeReader,
};
use oasis7_proto::memory_budget::ReservedValue;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("effect changed membership closure")
}
fn spend(work: &mut RuntimeEffectJsonWorkV1) -> Result<(), ValueTreeError> {
    work.cells_remaining = work
        .cells_remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("effect changed cells"))?;
    Ok(())
}
fn lookup_after<R: ValueNodeReader, C: FnMut() -> bool>(
    record: &TransitionRecordV1,
    key: &WorldFieldKeyV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<Option<[u8; 32]>, ValueTreeError> {
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(write) = record
        .writes
        .iter()
        .find(|w| w.namespace == StateNamespaceV1::Effects && w.mutation.key == key.as_bytes())
    {
        return Ok(write.mutation.after);
    }
    tree::lookup(
        record.before.state_root(StateNamespaceV1::Effects),
        key,
        records,
        LABELS,
    )
}
fn control<R: ValueNodeReader, C: FnMut() -> bool>(
    record: &TransitionRecordV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<RuntimeEffectQueueFrontierV1, ValueTreeError> {
    let mut values = [None; 4];
    for (i, name) in ["head", "tail", "count", "next_ordinal"]
        .into_iter()
        .enumerate()
    {
        let key = records.key(
            RuntimeEffectKindV1::Pending.descriptor(),
            WorldFieldRecordKindV1::Entry,
            &((FORMAT, ()), name),
        )?;
        let id = lookup_after(record, &key.0, records)?.ok_or_else(invalid)?;
        values[i] = match family::read(
            &id,
            &key,
            MemberPayloadCodecV1::U64,
            i < 2,
            8,
            records,
            &mut std::io::sink(),
        )? {
            RecordValueV1::Unsigned(v) => Some(v),
            RecordValueV1::Unit if i < 2 => None,
            _ => return Err(invalid()),
        };
    }
    Ok(RuntimeEffectQueueFrontierV1 {
        head: values[0],
        tail: values[1],
        count: values[2].ok_or_else(invalid)?,
        next_ordinal: values[3].ok_or_else(invalid)?,
    })
}
fn reference<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: RuntimeEffectKindV1,
    key: &WorldFieldKeyV1,
    id: &[u8; 32],
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(StateRootV1, Option<u64>), ValueTreeError> {
    let header = read_member_record_header_v1(
        id,
        key,
        records.values,
        MemberRecordLimitsV1 {
            semantic_bytes: records.semantic_bytes,
            payload_bytes: 165,
            record_bytes: records.record_bytes,
        },
        &mut *records.cancelled,
    )?;
    let mut sink = std::io::sink();
    let mut probe = OriginalSemanticKeyProbe::new(
        if kind == RuntimeEffectKindV1::Pending {
            OriginalKeyKind::Unsigned
        } else {
            OriginalKeyKind::Text
        },
        &mut sink,
    );
    let result = family::reference(
        kind,
        id,
        &(key.clone(), header.semantic_bytes),
        false,
        records,
        &mut probe,
    );
    if let Some(error) = probe.error() {
        return Err(ValueTreeError::Invalid(error));
    }
    let root = result?;
    let ordinal = match probe.finish()? {
        OriginalKeyInfo::Unsigned(v) => Some(v),
        OriginalKeyInfo::Text { .. } => None,
    };
    Ok((root, ordinal))
}
type PendingItem = (StateRootV1, Option<u64>, Option<u64>);
fn pending_after<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ordinal: u64,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeEffectJsonWorkV1,
) -> Result<Option<PendingItem>, ValueTreeError> {
    let child = {
        let mut records = ctx.records();
        let key = records.key(
            RuntimeEffectKindV1::Pending.descriptor(),
            WorldFieldRecordKindV1::Entry,
            &ordinal,
        )?;
        lookup_after(record, &key.0, &mut records)?
            .map(|id| {
                family::reference(
                    RuntimeEffectKindV1::Pending,
                    &id,
                    &key,
                    false,
                    &mut records,
                    &mut std::io::sink(),
                )
            })
            .transpose()?
    };
    child
        .map(|root| family::pending_item(&root, ctx, work))
        .transpose()
}
fn check_pending<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ordinal: u64,
    record: &TransitionRecordV1,
    frontier: RuntimeEffectQueueFrontierV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeEffectJsonWorkV1,
) -> Result<(), ValueTreeError> {
    let Some((_, prev, next)) = pending_after(ordinal, record, ctx, work)? else {
        return Ok(());
    };
    if ordinal >= frontier.next_ordinal
        || prev.is_some_and(|v| v >= ordinal)
        || next.is_some_and(|v| v <= ordinal)
    {
        return Err(invalid());
    }
    match prev {
        Some(v) => {
            if pending_after(v, record, ctx, work)?.ok_or_else(invalid)?.2 != Some(ordinal) {
                return Err(invalid());
            }
        }
        None => {
            if frontier.head != Some(ordinal) {
                return Err(invalid());
            }
        }
    }
    match next {
        Some(v) => {
            if pending_after(v, record, ctx, work)?.ok_or_else(invalid)?.1 != Some(ordinal) {
                return Err(invalid());
            }
        }
        None => {
            if frontier.tail != Some(ordinal) {
                return Err(invalid());
            }
        }
    }
    Ok(())
}
/// Independent generic replay precedes every owner check. Only unchanged or
/// authenticated old bodies from actual changed outer records are inherited;
/// newly produced bodies receive complete Intent/JSON qualification.
/// No unrelated queue body or params subtree is scanned for a handoff.
pub fn accept_runtime_effect_transition_v1<
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<RuntimeEffectFamilyQualificationV1>,
    verifier: &TransitionVerifier<'_, S>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeEffectJsonWorkV1,
) -> Result<ReservedValue<RuntimeEffectFamilyQualificationV1>, WorldFieldOverlayErrorV1> {
    ctx.check()?;
    if !parent.uses_budget(ctx.values.memory)
        || !verifier.memory.same_pool(ctx.values.memory)
        || verifier.class != ctx.values.class
    {
        return Err(invalid().into());
    }
    parent.validate_binding(&record.before.digest().map_err(|_| invalid())?)?;
    let _operation = ctx
        .values
        .memory
        .reserve(32 << 10, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    let verified = verifier.verify(record);
    let original = (ctx.take_state_error)();
    ctx.check()?;
    if let Some(error) = original {
        return Err(error.into());
    }
    let verified = verified?;
    let record = &verified.record;
    if !record.after.state.iter().any(|root| root == ctx.root) {
        return Err(invalid().into());
    }
    // Capacity and each StateRoot's fixed digest allocation are admitted before
    // collecting at most D old bodies. There is no family-sized visited set.
    let bytes = record
        .writes
        .len()
        .checked_mul(std::mem::size_of::<StateRootV1>() + 128)
        .ok_or(ValueTreeError::ResourceLimited(
            "effect old bodies admission",
        ))?;
    let _old = ctx
        .values
        .memory
        .reserve(bytes, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    let mut inherited = Vec::with_capacity(record.writes.len());
    for write in &record.writes {
        let Some(id) = write.mutation.before else {
            continue;
        };
        if !write
            .mutation
            .key
            .starts_with(b"oasis7.iws.world-field-key.v1\0")
        {
            continue;
        }
        let binding =
            WorldFieldKeyV1::from_bytes(&write.mutation.key).map_err(ValueTreeError::Invalid)?;
        let Some(kind) = RuntimeEffectKindV1::ALL
            .into_iter()
            .find(|kind| binding.owner == FieldOwner::Runtime && kind.name() == binding.name)
        else {
            continue;
        };
        let key = WorldFieldKeyV1::new(kind.descriptor(), binding.kind, binding.semantic_key)
            .map_err(ValueTreeError::Invalid)?;
        if key.as_bytes() != write.mutation.key || write.namespace != StateNamespaceV1::Effects {
            return Err(invalid().into());
        }
        if reserved(kind, &key, &mut ctx.records())? {
            continue;
        }
        let (root, _) = reference(kind, &key, &id, &mut ctx.records())?;
        let body = if kind == RuntimeEffectKindV1::Pending {
            family::pending_item(&root, ctx, work)?.0
        } else {
            root
        };
        inherited.push(body);
    }
    let frontier = control(record, &mut ctx.records())?;
    if frontier.next_ordinal < parent.pending_frontier().next_ordinal {
        return Err(invalid().into());
    }
    let mut counts = parent.record_counts();
    for write in &record.writes {
        spend(work)?;
        if !write
            .mutation
            .key
            .starts_with(b"oasis7.iws.world-field-key.v1\0")
        {
            continue;
        }
        let binding =
            WorldFieldKeyV1::from_bytes(&write.mutation.key).map_err(ValueTreeError::Invalid)?;
        let Some(kind) = RuntimeEffectKindV1::ALL
            .into_iter()
            .find(|kind| binding.owner == FieldOwner::Runtime && kind.name() == binding.name)
        else {
            continue;
        };
        let key = WorldFieldKeyV1::new(kind.descriptor(), binding.kind, binding.semantic_key)
            .map_err(ValueTreeError::Invalid)?;
        if key.as_bytes() != write.mutation.key || write.namespace != StateNamespaceV1::Effects {
            return Err(invalid().into());
        }
        if reserved(kind, &key, &mut ctx.records())? {
            if write.mutation.before.is_none() || write.mutation.after.is_none() {
                return Err(invalid().into());
            }
            // Controls are checked below; metadata and format have a unique
            // canonical original value and may not be replaced by a new value.
            let mut records = ctx.records();
            let meta =
                WorldFieldKeyV1::new(kind.descriptor(), WorldFieldRecordKindV1::Metadata, None)
                    .map_err(ValueTreeError::Invalid)?;
            let marker = records.key(
                kind.descriptor(),
                WorldFieldRecordKindV1::Entry,
                &(FORMAT, ()),
            )?;
            if (key == meta || key == marker.0) && write.mutation.before != write.mutation.after {
                return Err(invalid().into());
            }
            continue;
        }
        let old = write
            .mutation
            .before
            .map(|id| reference(kind, &key, &id, &mut ctx.records()))
            .transpose()?;
        let new = write
            .mutation
            .after
            .map(|id| reference(kind, &key, &id, &mut ctx.records()))
            .transpose()?;
        let slot = usize::from(kind == RuntimeEffectKindV1::Inflight);
        match (&old, &new) {
            (None, Some(_)) => counts[slot] = counts[slot].checked_add(1).ok_or_else(invalid)?,
            (Some(_), None) => counts[slot] = counts[slot].checked_sub(1).ok_or_else(invalid)?,
            (None, None) => return Err(invalid().into()),
            _ => (),
        }
        if let Some((root, ordinal)) = new {
            let body = if let Some(ordinal) = ordinal {
                if old.is_none() && ordinal < parent.pending_frontier().next_ordinal {
                    return Err(invalid().into());
                }
                let body = family::pending_item(&root, ctx, work)?.0;
                check_pending(ordinal, record, frontier, ctx, work)?;
                body
            } else {
                root
            };
            if !inherited.iter().any(|old| old == &body) {
                family::body(RuntimeEffectKindV1::Inflight, &body, ctx, work)?;
            }
        }
        if let Some((root, Some(_))) = old {
            let (_, prev, next) = family::pending_item(&root, ctx, work)?;
            // Deletions cannot leave an unchanged neighbor pointing at a
            // removed ordinal, including a run of adjacent deletions.
            for ordinal in [prev, next].into_iter().flatten() {
                check_pending(ordinal, record, frontier, ctx, work)?;
            }
        }
        ctx.check()?;
    }
    if frontier.count != counts[0]
        || (frontier.count == 0) != frontier.head.is_none()
        || frontier.head.is_none() != frontier.tail.is_none()
    {
        return Err(invalid().into());
    }
    for (ordinal, column) in [(frontier.head, 0), (frontier.tail, 1)] {
        if let Some(ordinal) = ordinal {
            let (_, prev, next) = pending_after(ordinal, record, ctx, work)?.ok_or_else(invalid)?;
            if (column == 0 && prev.is_some()) || (column == 1 && next.is_some()) {
                return Err(invalid().into());
            }
            check_pending(ordinal, record, frontier, ctx, work)?;
        }
    }
    ctx.check()?;
    let result = family::mint(
        record.after.digest().map_err(|_| invalid())?,
        frontier,
        counts,
        ctx.values.memory,
        ctx.values.class,
    )?;
    ctx.check()?;
    Ok(result)
}
fn reserved<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: RuntimeEffectKindV1,
    key: &WorldFieldKeyV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<bool, ValueTreeError> {
    let meta = WorldFieldKeyV1::new(kind.descriptor(), WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    if key == &meta {
        return Ok(true);
    }
    let marker = records.key(
        kind.descriptor(),
        WorldFieldRecordKindV1::Entry,
        &(FORMAT, ()),
    )?;
    if key == &marker.0 {
        return Ok(true);
    }
    if kind == RuntimeEffectKindV1::Pending {
        for name in ["head", "tail", "count", "next_ordinal"] {
            let field = records.key(
                kind.descriptor(),
                WorldFieldRecordKindV1::Entry,
                &((FORMAT, ()), name),
            )?;
            if key == &field.0 {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
