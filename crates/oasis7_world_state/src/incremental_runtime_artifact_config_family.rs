use super::*;
use crate::{
    incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_key_read::{OriginalKeyKind, OriginalSemanticKeyProbe},
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_record_root::RecordRootRefV1,
    incremental_state::{
        MerkleNodeReader, StateEntryV1, StateNamespaceV1, StateRootV1, StateScanBudget,
        StateScanContext, StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::{cell::RefCell, io::Cursor};

#[derive(Debug)]
pub struct ArtifactConfigFamilyQualificationV1 {
    roots: [u8; 32],
    class: oasis7_proto::memory_budget::MemoryClass,
}
impl ArtifactConfigFamilyQualificationV1 {
    pub fn allocation_class(&self) -> oasis7_proto::memory_budget::MemoryClass {
        self.class
    }
    pub fn validate_binding(&self, roots: &[u8; 32]) -> Result<(), ValueTreeError> {
        if self.roots == *roots {
            Ok(())
        } else {
            Err(read::invalid())
        }
    }
}
pub(super) fn mint(
    roots: [u8; 32],
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    class: oasis7_proto::memory_budget::MemoryClass,
) -> Result<ReservedValue<ArtifactConfigFamilyQualificationV1>, ValueTreeError> {
    Ok(memory
        .reserve(512, class)?
        .retain(ArtifactConfigFamilyQualificationV1 { roots, class }))
}

/// Re-admit this exact recovered scope using new Ordinary metadata only.
/// This reads no pages and does not select a head or grant execution authority.
pub fn readmit_artifact_config_family_ordinary_v1(
    source: &ReservedValue<ArtifactConfigFamilyQualificationV1>,
    roots: &RootSetV1,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<ArtifactConfigFamilyQualificationV1>, ValueTreeError> {
    use oasis7_proto::memory_budget::MemoryClass;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !source.uses_budget(memory)
        || source.reserved_bytes() < 512
        || source.class != MemoryClass::Recovery
    {
        return Err(read::invalid());
    }
    let _scratch = memory.reserve(128 << 10, MemoryClass::Ordinary)?;
    source.validate_binding(&roots.digest().map_err(|_| read::invalid())?)?;
    let output = mint(source.roots, memory, MemoryClass::Ordinary)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(output)
}

pub(super) fn outer<R: ValueNodeReader, C: FnMut() -> bool>(
    field: ArtifactConfigFieldV1,
    key: &WorldFieldKeyV1,
    id: &[u8; 32],
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<ReservedValue<StateRootV1>, ValueTreeError> {
    let retained = records.values.memory.reserve(8192, records.values.class)?;
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: records.semantic_bytes,
        payload_bytes: 165,
        record_bytes: records.record_bytes,
    };
    let header =
        read_member_record_header_v1(id, key, records.values, limits, &mut *records.cancelled)?;
    let mut bytes = [0; 165];
    let profile = MemberReadProfileV1 {
        codec: MemberPayloadCodecV1::RecordRoot,
        semantic_bytes: header.semantic_bytes,
        limits,
    };
    if field.global() {
        let expected = records.key(field.descriptor(), WorldFieldRecordKindV1::Entry, &())?;
        if expected.0 != *key || expected.1 != header.semantic_bytes {
            return Err(read::invalid());
        }
        read_member_record_profile_v1(
            id,
            key,
            records.values,
            profile,
            &mut std::io::sink(),
            &mut Cursor::new(bytes.as_mut_slice()),
            &mut *records.cancelled,
        )?;
    } else {
        let mut sink = std::io::sink();
        let mut probe = OriginalSemanticKeyProbe::new(OriginalKeyKind::Text, &mut sink);
        let result = read_member_record_profile_v1(
            id,
            key,
            records.values,
            profile,
            &mut probe,
            &mut Cursor::new(bytes.as_mut_slice()),
            &mut *records.cancelled,
        );
        if probe.error().is_some() {
            return Err(read::invalid());
        }
        result?;
        probe.finish()?;
    }
    let binding = WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| read::invalid())?;
    let root = reference
        .validate_binding(
            &field.schema(),
            &binding.semantic_key.ok_or_else(read::invalid)?,
            StateNamespaceV1::Artifacts,
        )
        .map_err(|_| read::invalid())?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(root.clone()))
}
pub(super) fn field(
    entry: &StateEntryV1,
) -> Result<Option<(ArtifactConfigFieldV1, WorldFieldKeyV1)>, ValueTreeError> {
    field_key(entry.namespace, &entry.key)
}
pub(super) fn field_key(
    namespace: StateNamespaceV1,
    bytes: &[u8],
) -> Result<Option<(ArtifactConfigFieldV1, WorldFieldKeyV1)>, ValueTreeError> {
    if !bytes.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(None);
    }
    let binding = WorldFieldKeyV1::from_bytes(bytes).map_err(ValueTreeError::Invalid)?;
    if binding.owner != FieldOwner::Runtime {
        return Ok(None);
    }
    let Some(field) = ArtifactConfigFieldV1::ALL
        .into_iter()
        .find(|field| field.name() == binding.name)
    else {
        return Ok(None);
    };
    let expected = WorldFieldKeyV1::new(field.descriptor(), binding.kind, binding.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if namespace != StateNamespaceV1::Artifacts || bytes != expected.as_bytes() {
        return Err(read::invalid());
    }
    Ok(Some((field, expected)))
}
pub(super) const RECORD_FIELDS: [ArtifactConfigFieldV1; 4] = [
    ArtifactConfigFieldV1::RegistryManifest,
    ArtifactConfigFieldV1::RegistryRegisteredAt,
    ArtifactConfigFieldV1::RegistryRegisteredBy,
    ArtifactConfigFieldV1::RegistryAuditEventId,
];

/// Complete eight-descriptor scope bound to the exact six-root RootSet. This
/// proves storage relationships only, not artifact bytes or execution policy.
pub fn qualify_artifact_config_family_v1<M, R, E, C>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    outer_scan: &mut StateScanBudget,
    inner_scan: &mut StateScanBudget,
    work: &mut ArtifactConfigReadWorkV1,
) -> Result<ReservedValue<ArtifactConfigFamilyQualificationV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    roots.validate().map_err(|_| read::invalid())?;
    ctx.check()?;
    if !roots.state.iter().any(|root| root == ctx.root) {
        return Err(read::invalid());
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let _scratch = ctx.values.memory.reserve(32 << 10, ctx.values.class)?;
    let mut metadata = [false; 8];
    let mut counts = [0u64; 8];
    let cancel = RefCell::new(&mut ctx.cancelled);
    for root in &roots.state {
        let mut local = WorldFieldReadContextV1 {
            root,
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
            cancelled: || (cancel.borrow_mut())(),
        };
        let result = StateScanContext {
            source: ctx.state,
            memory: local.values.memory,
            class: local.values.class,
            io: &mut *outer_scan,
        }
        .visit_entries(
            root,
            |entry| {
                work.cell()?;
                let Some((field, key)) = field(entry)? else {
                    return Ok(());
                };
                let index = ArtifactConfigFieldV1::ALL
                    .iter()
                    .position(|value| *value == field)
                    .ok_or_else(read::invalid)?;
                let binding =
                    WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
                if binding.kind == WorldFieldRecordKindV1::Metadata {
                    if metadata[index] {
                        return Err(read::invalid());
                    }
                    metadata[index] = true;
                    return local.records().presence(&entry.value, &key, 0);
                }
                if binding.kind != WorldFieldRecordKindV1::Entry {
                    return Err(read::invalid());
                }
                counts[index] = counts[index].checked_add(1).ok_or_else(read::invalid)?;
                let child = outer(field, &key, &entry.value, &mut local.records())?;
                if field == ArtifactConfigFieldV1::RegistryManifest {
                    for sibling in RECORD_FIELDS.into_iter().skip(1) {
                        let other_key = WorldFieldKeyV1::new(
                            sibling.descriptor(),
                            WorldFieldRecordKindV1::Entry,
                            binding.semantic_key,
                        )
                        .map_err(ValueTreeError::Invalid)?;
                        let id = local.lookup(&other_key)?.ok_or_else(read::invalid)?;
                        let other = outer(sibling, &other_key, &id, &mut local.records())?;
                        if *other != *child {
                            return Err(read::invalid());
                        }
                    }
                }
                if !RECORD_FIELDS
                    .into_iter()
                    .skip(1)
                    .any(|value| value == field)
                {
                    let mut body = WorldFieldReadContextV1 {
                        root: &child,
                        state: local.state,
                        values: ValueReadContext {
                            source: local.values.source,
                            memory: local.values.memory,
                            class: local.values.class,
                            io: &mut *local.values.io,
                        },
                        key_work: &mut *local.key_work,
                        semantic_bytes: local.semantic_bytes,
                        record_bytes: local.record_bytes,
                        take_state_error: &mut local.take_state_error,
                        cancelled: &mut local.cancelled,
                    };
                    qualify_artifact_config_body_v1(
                        field.body_kind(),
                        &mut body,
                        inner_scan,
                        work,
                    )?;
                }
                Ok(())
            },
            || (cancel.borrow_mut())(),
        );
        let error = (local.take_state_error)();
        if (cancel.borrow_mut())() {
            return Err(ValueTreeError::Cancelled);
        }
        if let Some(error) = error {
            return Err(error);
        }
        match result {
            Ok(_) => {}
            Err(StateScanError::Visitor(error)) => return Err(error),
            Err(StateScanError::Memory(error)) => return Err(error.into()),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(StateScanError::ResourceLimited) => {
                return Err(ValueTreeError::ResourceLimited(
                    "artifact config family scan",
                ));
            }
            Err(_) => return Err(read::invalid()),
        }
    }
    if metadata != [true; 8] {
        return Err(read::invalid());
    }
    for (index, field) in ArtifactConfigFieldV1::ALL.into_iter().enumerate() {
        if field.global() && counts[index] != 1 {
            return Err(read::invalid());
        }
    }
    if counts[1..5].iter().any(|count| *count != counts[1]) {
        return Err(read::invalid());
    }
    let digest = roots.digest().map_err(|_| read::invalid())?;
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(ArtifactConfigFamilyQualificationV1 {
        roots: digest,
        class: ctx.values.class,
    }))
}

pub(super) fn record_key<R: ValueNodeReader, C: FnMut() -> bool>(
    field: ArtifactConfigFieldV1,
    key: ArtifactConfigRecordKeyV1<'_>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    match (field.global(), key) {
        (true, ArtifactConfigRecordKeyV1::Global) => {
            records.key(field.descriptor(), WorldFieldRecordKindV1::Entry, &())
        }
        (false, ArtifactConfigRecordKeyV1::Member(key)) => {
            records.key(field.descriptor(), WorldFieldRecordKindV1::Entry, key)
        }
        _ => Err(read::invalid()),
    }
}
pub fn open_qualified_artifact_config_body_v1<M, R, E, C>(
    field: ArtifactConfigFieldV1,
    key: ArtifactConfigRecordKeyV1<'_>,
    roots: &RootSetV1,
    family: &ReservedValue<ArtifactConfigFamilyQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<ReservedValue<ArtifactConfigBodyQualificationV1>>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    if !family.uses_budget(ctx.values.memory)
        || family.allocation_class() != ctx.values.class
        || family.reserved_bytes() < std::mem::size_of::<ArtifactConfigFamilyQualificationV1>()
    {
        return Err(read::invalid());
    }
    family.validate_binding(&roots.digest().map_err(|_| read::invalid())?)?;
    if !roots
        .state
        .iter()
        .any(|root| root.namespace == StateNamespaceV1::Artifacts && root == ctx.root)
    {
        return Err(read::invalid());
    }
    let key = record_key(field, key, &mut ctx.records())?;
    let Some(id) = ctx.lookup(&key.0)? else {
        if field.global() {
            return Err(read::invalid());
        }
        ctx.check()?;
        return Ok(None);
    };
    let child = outer(field, &key.0, &id, &mut ctx.records())?;
    let result = closure::mint(
        field.body_kind(),
        &child,
        ctx.values.memory,
        ctx.values.class,
    )?;
    ctx.check()?;
    Ok(Some(result))
}
