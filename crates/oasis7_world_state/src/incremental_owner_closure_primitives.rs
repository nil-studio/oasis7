//! Complete primitive fields reuse the original owner validators at cold and Δ boundaries.
use super::*;
use crate::incremental_field_read::WorldFieldReadContextV1;
use oasis7_formal_protocol::{
    incremental_execution::{TransitionRecordV1, TransitionVerifier},
    incremental_state::{StateEntryV1, StateNamespaceV1, StateRootV1},
};
use oasis7_proto::memory_budget::MemoryClass;

const CARRIER: usize = 4096;
const FIELDS: usize = WORLD_STATE_FIELDS.len();
pub struct WorldPrimitiveFamilyWorkV1 {
    pub records_remaining: u64,
    pub max_record_bytes: u64,
}
/// Exact RootSet structural evidence for 29 flat primitive fields only.
pub struct WorldPrimitiveQualificationV1 {
    roots: [u8; 32],
    class: MemoryClass,
    counts: [u64; FIELDS],
}
impl WorldPrimitiveQualificationV1 {
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), WorldStateOwnerError> {
        if roots.digest().map_err(WorldStateOwnerError::RootSet)? != self.roots {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub fn field_count(&self) -> usize {
        WORLD_STATE_FIELDS.iter().filter(|f| supports(**f)).count()
    }
}
fn invalid() -> WorldStateOwnerError {
    WorldStateOwnerError::Invalid("primitive family closure")
}
fn supports(field: FieldDescriptor) -> bool {
    field.owner == FieldOwner::WorldState
        && (u64_field(field).is_some()
            || id_set_field(field).is_some()
            || number_map_field(field).is_some()
            || text_byte_field(field).is_some_and(|f| f != TextByteOwnerField::ModuleStates)
            || typed_maps::is_installed_target(field)
            || typed_maps::is_world_balance(field))
}
fn identify(
    ns: StateNamespaceV1,
    bytes: &[u8],
) -> Result<Option<(usize, FieldDescriptor, WorldFieldKeyV1)>, WorldStateOwnerError> {
    if !bytes.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(None);
    }
    let parsed = WorldFieldKeyV1::from_bytes(bytes).map_err(WorldStateOwnerError::Invalid)?;
    let Some((index, field)) = WORLD_STATE_FIELDS
        .iter()
        .copied()
        .enumerate()
        .find(|(_, f)| f.owner == parsed.owner && f.name == parsed.name && supports(*f))
    else {
        return Ok(None);
    };
    let key = WorldFieldKeyV1::new(field, parsed.kind, parsed.semantic_key)
        .map_err(WorldStateOwnerError::Invalid)?;
    if field.destination != FieldDestination::State(ns)
        || key.as_bytes() != bytes
        || !matches!(
            parsed.kind,
            WorldFieldRecordKindV1::Metadata | WorldFieldRecordKindV1::Entry
        )
    {
        return Err(invalid());
    }
    Ok(Some((index, field, key)))
}
fn spend(work: &mut WorldPrimitiveFamilyWorkV1) -> Result<(), WorldStateOwnerError> {
    work.records_remaining =
        work.records_remaining
            .checked_sub(1)
            .ok_or(WorldStateOwnerError::ResourceLimited(
                "primitive family records",
            ))?;
    Ok(())
}
fn check<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), WorldStateOwnerError> {
    ctx.check().map_err(WorldStateOwnerError::Value)?;
    if let Some(e) = (ctx.take_state_error)() {
        return Err(WorldStateOwnerError::Value(e));
    }
    Ok(())
}
fn member<R: ValueNodeReader>(
    field: FieldDescriptor,
    key: &WorldFieldKeyV1,
    value: &[u8; 32],
    root: &StateRootV1,
    values: &mut ValueReadContext<'_, R>,
    work: &WorldPrimitiveFamilyWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), WorldStateOwnerError> {
    values
        .verify_closure(value, work.max_record_bytes, &mut *cancelled)
        .map_err(WorldStateOwnerError::Value)?;
    if let Some(scalar) = u64_field(field) {
        return read_u64_entry(scalar, field, key.clone(), value, values, cancelled);
    }
    if let Some(set) = id_set_field(field) {
        return read_typed_member(
            value,
            key,
            OwnerMemberProfile {
                key_kind: id_set_key_kind(set),
                codec: MemberPayloadCodecV1::Unit,
                payload_bytes: 0,
                max_record_bytes: work.max_record_bytes,
            },
            values,
            cancelled,
        );
    }
    if let Some(map) = number_map_field(field) {
        let (key_kind, codec) = number_map_profile(map);
        return read_typed_member(
            value,
            key,
            OwnerMemberProfile {
                key_kind,
                codec,
                payload_bytes: 8,
                max_record_bytes: work.max_record_bytes,
            },
            values,
            cancelled,
        );
    }
    if let Some(owner) = text_byte_field(field) {
        return read_text_byte_member(
            owner,
            field,
            key,
            value,
            values,
            work.max_record_bytes,
            cancelled,
        );
    }
    if typed_maps::is_installed_target(field) || typed_maps::is_world_balance(field) {
        let _charge = values
            .memory
            .reserve(512 + key.as_bytes().len(), values.class)
            .map_err(WorldStateOwnerError::Memory)?;
        let entry = StateEntryV1 {
            namespace: root.namespace,
            key: key.as_bytes().to_vec(),
            value: *value,
        };
        if typed_maps::is_world_balance(field) {
            return typed_maps::validate_world_balance_member(
                root,
                field,
                &entry,
                WorldFieldKeyV1::from_bytes(key.as_bytes())
                    .map_err(WorldStateOwnerError::Invalid)?,
                values,
                work.max_record_bytes,
                cancelled,
            );
        }
        return typed_maps::validate_installed_target_member(
            field,
            &entry,
            WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(WorldStateOwnerError::Invalid)?,
            values,
            work.max_record_bytes,
            cancelled,
        );
    }
    Err(invalid())
}
fn metadata<R: ValueNodeReader>(
    key: &WorldFieldKeyV1,
    value: &[u8; 32],
    values: &mut ValueReadContext<'_, R>,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), WorldStateOwnerError> {
    if WorldFieldKeyV1::from_bytes(key.as_bytes())
        .map_err(WorldStateOwnerError::Invalid)?
        .semantic_key
        .is_some()
    {
        return Err(invalid());
    }
    read_field_presence_v1(
        value,
        key,
        values,
        PresenceRecordLimitsV1 {
            semantic_bytes: 0,
            record_bytes: 512,
        },
        &mut io::sink(),
        cancelled,
    )
    .map_err(WorldStateOwnerError::Value)?;
    Ok(())
}
pub fn qualify_world_primitive_fields_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldPrimitiveFamilyWorkV1,
) -> Result<ReservedValue<WorldPrimitiveQualificationV1>, WorldStateOwnerError> {
    check(ctx)?;
    roots.validate().map_err(WorldStateOwnerError::RootSet)?;
    if !roots.state.iter().any(|r| r == ctx.root) {
        return Err(invalid());
    }
    let retained = ctx
        .values
        .memory
        .reserve(CARRIER, ctx.values.class)
        .map_err(WorldStateOwnerError::Memory)?;
    let _phase = ctx
        .values
        .memory
        .reserve(32 << 10, ctx.values.class)
        .map_err(WorldStateOwnerError::Memory)?;
    let mut seen = [false; FIELDS];
    let mut counts = [0u64; FIELDS];
    {
        let cancelled = RefCell::new(&mut ctx.cancelled);
        let errors = RefCell::new(&mut ctx.take_state_error);
        for root in &roots.state {
            let result = StateScanContext {
                source: ctx.state,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *scan,
            }
            .visit_entries(
                root,
                |row| {
                    if let Some(e) = (errors.borrow_mut())() {
                        return Err(WorldStateOwnerError::Value(e));
                    }
                    spend(work)?;
                    let Some((index, field, key)) = identify(row.namespace, &row.key)? else {
                        return Ok(());
                    };
                    let mut poll = || (cancelled.borrow_mut())();
                    match WorldFieldKeyV1::from_bytes(key.as_bytes())
                        .map_err(WorldStateOwnerError::Invalid)?
                        .kind
                    {
                        WorldFieldRecordKindV1::Metadata => {
                            if std::mem::replace(&mut seen[index], true) {
                                return Err(invalid());
                            }
                            metadata(&key, &row.value, &mut ctx.values, &mut poll)?;
                        }
                        WorldFieldRecordKindV1::Entry => {
                            member(
                                field,
                                &key,
                                &row.value,
                                root,
                                &mut ctx.values,
                                work,
                                &mut poll,
                            )?;
                            counts[index] = counts[index].checked_add(1).ok_or_else(invalid)?;
                        }
                        _ => return Err(invalid()),
                    }
                    Ok(())
                },
                || (cancelled.borrow_mut())(),
            );
            match result {
                Ok(_) => {}
                Err(StateScanError::Visitor(e)) => return Err(e),
                Err(StateScanError::Cancelled) => {
                    return Err(WorldStateOwnerError::Value(ValueTreeError::Cancelled));
                }
                Err(StateScanError::Memory(e)) => return Err(WorldStateOwnerError::Memory(e)),
                Err(StateScanError::Tree(e)) => return Err(WorldStateOwnerError::Scan(e)),
                Err(StateScanError::Invalid(e)) => return Err(WorldStateOwnerError::Invalid(e)),
                Err(StateScanError::ResourceLimited) => {
                    return Err(WorldStateOwnerError::ResourceLimited("primitive scan"));
                }
            }
        }
    }
    for (i, field) in WORLD_STATE_FIELDS.iter().copied().enumerate() {
        if supports(field) && (!seen[i] || (u64_field(field).is_some() && counts[i] != 1)) {
            return Err(invalid());
        }
    }
    check(ctx)?;
    Ok(retained.retain(WorldPrimitiveQualificationV1 {
        roots: roots.digest().map_err(WorldStateOwnerError::RootSet)?,
        class: ctx.values.class,
        counts,
    }))
}
pub fn accept_world_primitive_transition_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<WorldPrimitiveQualificationV1>,
    verifier: &TransitionVerifier<'_, M>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldPrimitiveFamilyWorkV1,
) -> Result<ReservedValue<WorldPrimitiveQualificationV1>, WorldStateOwnerError> {
    check(ctx)?;
    if parent.class != MemoryClass::Ordinary
        || ctx.values.class != MemoryClass::Ordinary
        || verifier.class != MemoryClass::Ordinary
        || !parent.uses_budget(ctx.values.memory)
        || parent.reserved_bytes() < CARRIER
        || !verifier.memory.same_pool(ctx.values.memory)
    {
        return Err(invalid());
    }
    parent.validate_binding(&record.before)?;
    if !record.after.state.iter().any(|r| r == ctx.root) {
        return Err(invalid());
    }
    let checked = verifier
        .verify(record)
        .map_err(WorldStateOwnerError::RootSet)?;
    let record = &checked.record;
    let retained = ctx
        .values
        .memory
        .reserve(CARRIER, MemoryClass::Ordinary)
        .map_err(WorldStateOwnerError::Memory)?;
    let _phase = ctx
        .values
        .memory
        .reserve(32 << 10, MemoryClass::Ordinary)
        .map_err(WorldStateOwnerError::Memory)?;
    let mut counts = parent.counts;
    for write in &record.writes {
        check(ctx)?;
        spend(work)?;
        let Some((i, field, key)) = identify(write.namespace, &write.mutation.key)? else {
            continue;
        };
        match WorldFieldKeyV1::from_bytes(key.as_bytes())
            .map_err(WorldStateOwnerError::Invalid)?
            .kind
        {
            WorldFieldRecordKindV1::Metadata => {
                if write.mutation.before.is_none() || write.mutation.before != write.mutation.after
                {
                    return Err(invalid());
                }
            }
            WorldFieldRecordKindV1::Entry => {
                if write.mutation.before == write.mutation.after {
                    continue;
                }
                if let Some(after) = write.mutation.after {
                    member(
                        field,
                        &key,
                        &after,
                        record.after.state_root(write.namespace),
                        &mut ctx.values,
                        work,
                        &mut ctx.cancelled,
                    )?;
                }
                if write.mutation.before.is_none() {
                    counts[i] = counts[i].checked_add(1).ok_or_else(invalid)?;
                }
                if write.mutation.after.is_none() {
                    counts[i] = counts[i].checked_sub(1).ok_or_else(invalid)?;
                }
                if u64_field(field).is_some() && counts[i] != 1 {
                    return Err(invalid());
                }
            }
            _ => return Err(invalid()),
        }
    }
    check(ctx)?;
    Ok(retained.retain(WorldPrimitiveQualificationV1 {
        roots: record
            .after
            .digest()
            .map_err(WorldStateOwnerError::RootSet)?,
        class: MemoryClass::Ordinary,
        counts,
    }))
}
pub fn readmit_world_primitive_fields_ordinary_v1(
    parent: &ReservedValue<WorldPrimitiveQualificationV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<WorldPrimitiveQualificationV1>, WorldStateOwnerError> {
    if cancelled() {
        return Err(WorldStateOwnerError::Value(ValueTreeError::Cancelled));
    }
    if parent.class != MemoryClass::Recovery
        || !parent.uses_budget(memory)
        || parent.reserved_bytes() < CARRIER
    {
        return Err(invalid());
    }
    let retained = memory
        .reserve(CARRIER, MemoryClass::Ordinary)
        .map_err(WorldStateOwnerError::Memory)?;
    parent.validate_binding(roots)?;
    if cancelled() {
        return Err(WorldStateOwnerError::Value(ValueTreeError::Cancelled));
    }
    Ok(retained.retain(WorldPrimitiveQualificationV1 {
        roots: parent.roots,
        class: MemoryClass::Ordinary,
        counts: parent.counts,
    }))
}

#[cfg(test)]
#[path = "incremental_owner_closure_primitives_tests.rs"]
mod tests;
