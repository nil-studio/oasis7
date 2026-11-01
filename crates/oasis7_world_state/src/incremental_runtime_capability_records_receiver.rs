//! Generic replay plus this exact three-family changed closure.
use super::*;
use crate::{
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_diff::{self, RecordDiffLabelsV1},
};
use oasis7_formal_protocol::{
    incremental_execution::{TransitionRecordV1, TransitionVerifier},
    incremental_state::{MerkleNodeReader, StateRootV1, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use oasis7_proto::memory_budget::ReservedValue;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Runtime capability changed closure")
}
fn branch<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<u8, ValueTreeError> {
    let (key, id) = point::entry(
        RuntimeCapabilityRecordKindV1::Budget,
        RuntimeCapabilityColumnV1::SubjectKind,
        root,
        records,
    )?;
    let info = read_member_record_header_v1(
        &id,
        &key.0,
        records.values,
        MemberRecordLimitsV1 {
            semantic_bytes: records.semantic_bytes,
            payload_bytes: records.record_bytes,
            record_bytes: records.record_bytes,
        },
        &mut *records.cancelled,
    )?;
    if info.codec != MemberPayloadCodecV1::Text || info.payload_bytes > 6 {
        return Err(invalid());
    }
    let mut text = [0; 6];
    let RuntimeCapabilityValueV1::Text { utf8_bytes } = read::value(
        Some(RuntimeCapabilityColumnV1::SubjectKind),
        &id,
        &key,
        records,
        6,
        &mut std::io::Cursor::new(text.as_mut_slice()),
    )?
    else {
        return Err(invalid());
    };
    closure::subject_kind(&text[..usize::try_from(utf8_bytes).map_err(|_| invalid())?])
}
fn changed<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: RuntimeCapabilityRecordKindV1,
    before: &StateRootV1,
    after: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    cells: &mut u64,
    max: u64,
) -> Result<(), ValueTreeError> {
    if before.namespace != kind.namespace()
        || after.namespace != kind.namespace()
        || before.leaf_count != kind.leaf_count()
        || after.leaf_count != kind.leaf_count()
    {
        return Err(invalid());
    }
    let _workspace = records
        .values
        .memory
        .reserve(16 << 10, records.values.class)?;
    let mut keys = Vec::with_capacity(kind.leaf_count() as usize);
    keys.push((
        WorldFieldKeyV1::new(kind.descriptor(), WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?,
        None,
    ));
    keys.push((read::cell_key(kind, None, records)?.0, None));
    for column in kind.columns() {
        keys.push((
            read::cell_key(kind, Some(*column), records)?.0,
            Some(*column),
        ));
    }
    let mut changed_subject = 0u16;
    let mut nonunit = 0u16;
    incremental_record_diff::visit(
        before,
        after,
        records,
        cells,
        RecordDiffLabelsV1 {
            invalid: "Runtime capability diff",
            cells: "Runtime capability diff cells",
            stack: "Runtime capability diff stack",
            tree: genesis::LABELS,
        },
        |old, new, records| {
            let entry = new.or(old).ok_or_else(invalid)?;
            if old.is_none() || new.is_none() || entry.namespace != kind.namespace() {
                return Err(invalid());
            }
            let index = keys
                .iter()
                .position(|(key, _)| key.as_bytes() == entry.key)
                .ok_or_else(invalid)?;
            records.values.verify_closure(
                &entry.value,
                records.record_bytes,
                &mut *records.cancelled,
            )?;
            if index == 0 {
                return records.presence(&entry.value, &keys[index].0, 0);
            }
            let column = keys[index].1;
            let key = read::cell_key(kind, column, records)?;
            if column == Some(RuntimeCapabilityColumnV1::SubjectKind) {
                let info = read_member_record_header_v1(
                    &entry.value,
                    &key.0,
                    records.values,
                    MemberRecordLimitsV1 {
                        semantic_bytes: records.semantic_bytes,
                        payload_bytes: records.record_bytes,
                        record_bytes: records.record_bytes,
                    },
                    &mut *records.cancelled,
                )?;
                if info.codec != MemberPayloadCodecV1::Text || info.payload_bytes > 6 {
                    return Err(invalid());
                }
            }
            let value = read::value(
                column,
                &entry.value,
                &key,
                records,
                if column == Some(RuntimeCapabilityColumnV1::SubjectKind) {
                    6
                } else {
                    max
                },
                &mut std::io::sink(),
            )?;
            if let Some(column) = column
                && let Some(slot) = RUNTIME_CAPABILITY_SUBJECT_COLUMNS_V1
                    .iter()
                    .position(|c| *c == column)
            {
                let bit = 1u16 << slot;
                changed_subject |= bit;
                if value != RuntimeCapabilityValueV1::Unit {
                    nonunit |= bit;
                }
            }
            Ok(())
        },
    )?;
    if changed_subject != 0 {
        let after_branch = branch(after, records)?;
        let before_branch = if changed_subject & 1 != 0 {
            branch(before, records)?
        } else {
            after_branch
        };
        for (slot, column) in RUNTIME_CAPABILITY_SUBJECT_COLUMNS_V1
            .into_iter()
            .enumerate()
            .skip(1)
        {
            let bit = 1u16 << slot;
            let active = column.subject_mask() == after_branch;
            if changed_subject & bit != 0 && ((nonunit & bit != 0) != active) {
                return Err(invalid());
            }
            // Variants have disjoint original payload fields. A real variant change
            // must therefore replace every old/new active field; unchanged long
            // payloads need no header or body reread to prove the inherited relation.
            if before_branch != after_branch
                && (column.subject_mask() == before_branch || active)
                && changed_subject & bit == 0
            {
                return Err(invalid());
            }
        }
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}
pub fn accept_runtime_capability_transition_v1<
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<RuntimeCapabilityFamilyQualificationV1>,
    verifier: &TransitionVerifier<'_, S>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    inner_scan: &mut StateScanBudget,
    cells: &mut u64,
    max: u64,
) -> Result<ReservedValue<RuntimeCapabilityFamilyQualificationV1>, WorldFieldOverlayErrorV1> {
    ctx.check()?;
    if !parent.uses_budget(ctx.values.memory)
        || !verifier.memory.same_pool(ctx.values.memory)
        || verifier.class != ctx.values.class
    {
        return Err(invalid().into());
    }
    parent.validate_binding(&record.before.digest().map_err(|_| invalid())?)?;
    let verified = verifier.verify(record);
    let error = (ctx.take_state_error)();
    ctx.check()?;
    if let Some(e) = error {
        return Err(e.into());
    }
    let verified = verified?;
    if !verified.record.after.state.iter().any(|r| r == ctx.root) {
        return Err(invalid().into());
    }
    let mut counts = parent.record_counts();
    for write in &verified.record.writes {
        closure::spend(cells)?;
        if !write
            .mutation
            .key
            .starts_with(b"oasis7.iws.world-field-key.v1\0")
        {
            continue;
        }
        let binding =
            WorldFieldKeyV1::from_bytes(&write.mutation.key).map_err(ValueTreeError::Invalid)?;
        if binding.owner != FieldOwner::Runtime {
            continue;
        }
        let Some(kind) = RuntimeCapabilityRecordKindV1::ALL
            .into_iter()
            .find(|k| k.name() == binding.name)
        else {
            continue;
        };
        let key = WorldFieldKeyV1::new(kind.descriptor(), binding.kind, binding.semantic_key)
            .map_err(ValueTreeError::Invalid)?;
        if key.as_bytes() != write.mutation.key || write.namespace != kind.namespace() {
            return Err(invalid().into());
        }
        let meta = WorldFieldKeyV1::new(kind.descriptor(), WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let format = read::marker(kind, &mut ctx.records())?;
        if binding.kind == WorldFieldRecordKindV1::Metadata || key == format.0 {
            if write.mutation.before.is_none() {
                return Err(invalid().into());
            }
            let value = write.mutation.after.ok_or_else(invalid)?;
            ctx.values
                .verify_closure(&value, ctx.record_bytes, &mut ctx.cancelled)?;
            if key == meta {
                ctx.records().presence(&value, &meta, 0)?;
            } else {
                read::value(
                    None,
                    &value,
                    &format,
                    &mut ctx.records(),
                    0,
                    &mut std::io::sink(),
                )?;
            }
            continue;
        }
        if binding.kind != WorldFieldRecordKindV1::Entry {
            return Err(invalid().into());
        }
        let mut roots = [None, None];
        for (index, value) in [write.mutation.before, write.mutation.after]
            .into_iter()
            .enumerate()
        {
            if let Some(value) = value {
                let info = read_member_record_header_v1(
                    &value,
                    &key,
                    &mut ctx.values,
                    MemberRecordLimitsV1 {
                        semantic_bytes: ctx.semantic_bytes,
                        payload_bytes: 165,
                        record_bytes: ctx.record_bytes,
                    },
                    &mut ctx.cancelled,
                )?;
                roots[index] = Some(read::outer(
                    kind,
                    &(key.clone(), info.semantic_bytes),
                    &value,
                    &mut ctx.records(),
                )?);
            }
        }
        match (&roots[0], &roots[1]) {
            (Some(before), Some(after)) => {
                changed(kind, before, after, &mut ctx.records(), cells, max)?
            }
            (None, Some(after)) => {
                let mut body = WorldFieldReadContextV1 {
                    root: after,
                    state: ctx.state,
                    values: ValueReadContext {
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
                };
                closure::body(kind, &mut body, inner_scan, cells, max)?;
                let slot = family::index(kind);
                counts[slot] = counts[slot].checked_add(1).ok_or_else(invalid)?;
            }
            (Some(_), None) => {
                let slot = family::index(kind);
                counts[slot] = counts[slot].checked_sub(1).ok_or_else(invalid)?;
            }
            (None, None) => return Err(invalid().into()),
        }
        ctx.check()?;
    }
    let token = family::mint(
        verified.record.after.digest().map_err(|_| invalid())?,
        counts,
        ctx.values.memory,
        ctx.values.class,
    )?;
    ctx.check()?;
    Ok(token)
}
