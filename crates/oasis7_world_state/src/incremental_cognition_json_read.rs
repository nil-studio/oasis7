//! One closed JSON child graph bound to the original Cognition Unit identity.
//! This is not namespace/RootSet/family or history/proof-ledger qualification.
use crate::{
    incremental_cognition_json::CognitionJsonFieldV1,
    incremental_field_member::{
        MemberPayloadCodecV1, MemberReadProfileV1, MemberRecordLimitsV1,
        read_member_record_profile_v1,
    },
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_json_records::{
        JsonReadWorkV1, JsonSubtreeQualificationV1, qualify_json_subtree_v1,
    },
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{MerkleNodeReader, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
#[derive(Debug)]
pub struct CognitionJsonEntryQualificationV1 {
    field: CognitionJsonFieldV1,
    body: ReservedValue<JsonSubtreeQualificationV1>,
}
impl CognitionJsonEntryQualificationV1 {
    pub fn validate_binding(
        &self,
        field: CognitionJsonFieldV1,
        child: &StateRootV1,
    ) -> Result<(), ValueTreeError> {
        if self.field != field {
            return Err(ValueTreeError::Invalid("cognition JSON entry binding"));
        }
        self.body.validate_binding(field.profile(), child)
    }
    pub fn body_root(&self) -> &StateRootV1 {
        self.body.root()
    }
    pub(crate) fn json(&self) -> &ReservedValue<JsonSubtreeQualificationV1> {
        &self.body
    }
}
pub fn qualify_cognition_json_entry_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: CognitionJsonFieldV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut JsonReadWorkV1,
) -> Result<Option<ReservedValue<CognitionJsonEntryQualificationV1>>, ValueTreeError> {
    context.check()?;
    let metadata = WorldFieldKeyV1::new(field.descriptor(), WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let metadata_root = context.lookup(&metadata)?;
    let key = context.key(field.descriptor(), WorldFieldRecordKindV1::Entry, &())?;
    let root = context.lookup(&key.0)?;
    let Some(metadata_root) = metadata_root else {
        if !field.optional() || root.is_some() {
            return Err(ValueTreeError::Invalid("cognition JSON missing metadata"));
        }
        context.check()?;
        return Ok(None);
    };
    context.records().presence(&metadata_root, &metadata, 0)?;
    let root = root.ok_or(ValueTreeError::Invalid("cognition JSON missing root"))?;
    let child = outer(field, &key, &root, &mut context.records())?;
    let proof = qualify_body(field, &child, context, work)?;
    context.check()?;
    Ok(Some(proof))
}
pub(crate) fn outer<R: ValueNodeReader, C: FnMut() -> bool>(
    field: CognitionJsonFieldV1,
    key: &(WorldFieldKeyV1, u64),
    value: &[u8; 32],
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<ReservedValue<StateRootV1>, ValueTreeError> {
    let _scratch = records.values.memory.reserve(8192, records.values.class)?;
    let retained = records.values.memory.reserve(1024, records.values.class)?;
    let mut bytes = [0u8; RECORD_ROOT_BYTES];
    let mut output = std::io::Cursor::new(&mut bytes[..]);
    records
        .values
        .verify_closure(value, records.record_bytes, &mut *records.cancelled)?;
    let info = read_member_record_profile_v1(
        value,
        &key.0,
        records.values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::RecordRoot,
            semantic_bytes: key.1,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: records.semantic_bytes,
                payload_bytes: RECORD_ROOT_BYTES as u64,
                record_bytes: records.record_bytes,
            },
        },
        &mut std::io::sink(),
        &mut output,
        &mut *records.cancelled,
    )?;
    if info.payload_bytes != RECORD_ROOT_BYTES as u64
        || output.position() != RECORD_ROOT_BYTES as u64
    {
        return Err(ValueTreeError::Invalid("cognition JSON root length"));
    }
    let binding = WorldFieldKeyV1::from_bytes(key.0.as_bytes()).map_err(ValueTreeError::Invalid)?;
    let semantic = binding
        .semantic_key
        .ok_or(ValueTreeError::Invalid("cognition JSON Unit key"))?;
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes)
        .map_err(|_| ValueTreeError::Invalid("cognition JSON root reference"))?;
    let child = reference
        .validate_binding(&field.profile().schema, &semantic, binding.namespace)
        .map_err(|_| ValueTreeError::Invalid("cognition JSON root binding"))?
        .clone();
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(child))
}
pub(crate) fn mint(
    field: CognitionJsonFieldV1,
    body: ReservedValue<JsonSubtreeQualificationV1>,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> Result<ReservedValue<CognitionJsonEntryQualificationV1>, ValueTreeError> {
    if !body.uses_budget(memory) {
        return Err(ValueTreeError::Invalid("cognition JSON proof pool"));
    }
    body.validate_binding(field.profile(), body.root())?;
    let retained = memory.reserve(1024, class)?;
    Ok(retained.retain(CognitionJsonEntryQualificationV1 { field, body }))
}

pub(crate) fn qualify_body<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: CognitionJsonFieldV1,
    child: &StateRootV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut JsonReadWorkV1,
) -> Result<ReservedValue<CognitionJsonEntryQualificationV1>, ValueTreeError> {
    let body = qualify_json_subtree_v1(field.profile(), child, context, work)?;
    require_owner_body(field, &body, &mut context.records(), work)?;
    mint(field, body, context.values.memory, context.values.class)
}

/// A changed-path proof already validates immediate rows. This fixed root
/// shape check must not rescan unchanged members to repeat that obligation.
pub(crate) fn require_root_shape<R: ValueNodeReader, C: FnMut() -> bool>(
    field: CognitionJsonFieldV1,
    body: &ReservedValue<JsonSubtreeQualificationV1>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut JsonReadWorkV1,
) -> Result<(), ValueTreeError> {
    if field != CognitionJsonFieldV1::SchedulerState {
        crate::incremental_json_records::require_json_container_records_v1(
            field.profile(),
            body,
            field == CognitionJsonFieldV1::Continuations,
            records,
            work,
        )?;
    }
    Ok(())
}

pub(crate) fn qualify_body_records<R: ValueNodeReader, C: FnMut() -> bool>(
    field: CognitionJsonFieldV1,
    child: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut JsonReadWorkV1,
) -> Result<ReservedValue<CognitionJsonEntryQualificationV1>, ValueTreeError> {
    let body = crate::incremental_json_records::qualify_json_subtree_records_v1(
        field.profile(),
        child,
        records,
        work,
    )?;
    require_owner_body(field, &body, records, work)?;
    mint(field, body, records.values.memory, records.values.class)
}
fn require_owner_body<R: ValueNodeReader, C: FnMut() -> bool>(
    field: CognitionJsonFieldV1,
    body: &ReservedValue<JsonSubtreeQualificationV1>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut JsonReadWorkV1,
) -> Result<(), ValueTreeError> {
    if let Some(shape) = field.immediate() {
        crate::incremental_json_records::require_json_immediate_records_v1(
            field.profile(),
            body,
            shape,
            records,
            work,
        )?;
    } else if field != CognitionJsonFieldV1::SchedulerState {
        crate::incremental_json_records::require_json_container_records_v1(
            field.profile(),
            body,
            field == CognitionJsonFieldV1::Continuations,
            records,
            work,
        )?;
    }
    Ok(())
}
