//! Closed changed-column inheritance for an already qualified child.
use super::*;
use oasis7_formal_protocol::incremental_state::StateRootV1;
fn read_at<M, R, E, C>(
    field: ModuleAuthorizationFieldV1,
    name: &'static str,
    root: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut ModuleAuthorizationReadWorkV1,
) -> Result<ModuleAuthorizationValueV1, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut local = WorldFieldReadContextV1 {
        root,
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
    };
    read::cell(
        field,
        ModuleAuthorizationAddressV1::fixed(name),
        &mut local,
        work,
        &mut std::io::sink(),
    )
}
fn decoded<R: ValueNodeReader, C: FnMut() -> bool>(
    field: ModuleAuthorizationFieldV1,
    entry: &StateEntryV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<ModuleAuthorizationAddressV1, ValueTreeError> {
    let binding = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    if binding.kind != WorldFieldRecordKindV1::Entry {
        return Err(invalid());
    }
    let expected = WorldFieldKeyV1::new(
        genesis::body_descriptor(),
        binding.kind,
        binding.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    if entry.namespace != StateNamespaceV1::Authorization || expected.as_bytes() != entry.key {
        return Err(invalid());
    }
    let mut bytes = [0; 256];
    let mut out = Cursor::new(bytes.as_mut_slice());
    let info = read_member_record_key_v1(
        &entry.value,
        &expected,
        records.values,
        MemberRecordLimitsV1 {
            semantic_bytes: records.semantic_bytes.min(256),
            payload_bytes: u64::MAX,
            record_bytes: records.record_bytes,
        },
        &mut out,
        &mut *records.cancelled,
    )?;
    let n = out.position() as usize;
    if info.semantic_bytes != n as u64 {
        return Err(invalid());
    }
    let address = address(field, &bytes[..n])?;
    let key = read::key(field, address, records)?;
    if key.0 != expected || key.1 != info.semantic_bytes {
        return Err(invalid());
    }
    Ok(address)
}
pub(crate) fn advance_module_authorization_body_v1<M, R, E, C>(
    field: ModuleAuthorizationFieldV1,
    before: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut ModuleAuthorizationReadWorkV1,
) -> Result<(), ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    let after_charge = ctx.values.memory.reserve(512, ctx.values.class)?;
    let after = after_charge.retain(ctx.root.clone());
    if before == &*after {
        return ctx.check();
    }
    let (old_length, length) = if field == ModuleAuthorizationFieldV1::Capabilities {
        let old = read_at(field, "effect_kinds.length", before, ctx, work)?;
        let new = read_at(field, "effect_kinds.length", &after, ctx, work)?;
        match (old, new) {
            (ModuleAuthorizationValueV1::Unsigned(a), ModuleAuthorizationValueV1::Unsigned(b)) => {
                (a, b)
            }
            _ => return Err(invalid()),
        }
    } else {
        (0, 0)
    };
    if after.leaf_count
        != length
            .checked_add(fixed(field).len() as u64 + 2)
            .ok_or_else(invalid)?
    {
        return Err(invalid());
    }
    let mut count = old_length;
    let marker = ctx
        .records()
        .key(
            genesis::body_descriptor(),
            WorldFieldRecordKindV1::Entry,
            &(),
        )?
        .0;
    crate::incremental_record_diff::visit(
        before,
        &after,
        &mut ctx.records(),
        &mut scan.leaves_remaining,
        crate::incremental_record_diff::RecordDiffLabelsV1 {
            invalid: "module authorization changed child",
            cells: "module authorization changed leaves",
            stack: "module authorization changed stack",
            tree: genesis::LABELS,
        },
        |old, new, records| {
            let entry = new.or(old).ok_or_else(invalid)?;
            let binding =
                WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
            if binding.kind == WorldFieldRecordKindV1::Metadata || entry.key == marker.as_bytes() {
                if old.map(|x| x.value) != new.map(|x| x.value) {
                    return Err(invalid());
                }
                return Ok(());
            }
            work.cell()?;
            let address = decoded(field, entry, records)?;
            if let Some(index) = address.index {
                match (old.is_some(), new.is_some()) {
                    (false, true) if index >= old_length && index < length => {}
                    (true, false) if index >= length => {}
                    (true, true) if index < length => {}
                    _ => return Err(invalid()),
                }
                match (old.is_some(), new.is_some()) {
                    (false, true) => count = count.checked_add(1).ok_or_else(invalid)?,
                    (true, false) => count = count.checked_sub(1).ok_or_else(invalid)?,
                    _ => {}
                }
            } else if old.is_none() || new.is_none() {
                return Err(invalid());
            }
            if let Some(new) = new {
                let key = read::key(field, address, records)?;
                let max = work.max_payload_bytes;
                read::budget_value(
                    field,
                    address,
                    &new.value,
                    &key,
                    records,
                    work,
                    max,
                    &mut std::io::sink(),
                )?;
            }
            Ok(())
        },
    )?;
    ctx.check()?;
    if count != length {
        return Err(invalid());
    }
    if field == ModuleAuthorizationFieldV1::PolicyRules {
        let decision = read_at(field, "decision.kind", &after, ctx, work)?;
        let key = read::key(
            field,
            ModuleAuthorizationAddressV1::fixed("decision.reason"),
            &mut ctx.records(),
        )?;
        let id = ctx.lookup(&key.0)?.ok_or_else(invalid)?;
        let limits = MemberRecordLimitsV1 {
            semantic_bytes: ctx.semantic_bytes,
            payload_bytes: u64::MAX,
            record_bytes: ctx.record_bytes,
        };
        let header =
            read_member_record_header_v1(&id, &key.0, &mut ctx.values, limits, &mut ctx.cancelled)?;
        let reason = match header.codec {
            MemberPayloadCodecV1::Unit if header.payload_bytes == 0 => false,
            MemberPayloadCodecV1::Text => true,
            _ => return Err(invalid()),
        };
        if matches!(decision, ModuleAuthorizationValueV1::Label("deny")) != reason {
            return Err(invalid());
        }
    }
    ctx.check()
}
