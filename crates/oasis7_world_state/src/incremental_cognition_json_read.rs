//! Authentication of one exact Unit entry and its full JSON subtree. This
//! receipt does not exclude other outer entries or qualify a full field/family,
//! its siblings, history, proof ledger or whole Cognition.
use crate::{
    incremental_cognition_json::CognitionJsonFieldV1,
    incremental_field_member::{
        MemberPayloadCodecV1, MemberReadProfileV1, MemberRecordLimitsV1,
        read_member_record_profile_v1,
    },
    incremental_field_read::WorldFieldReadContextV1,
    incremental_json_records::{
        JsonReadWorkV1, JsonSubtreeQualificationV1, qualify_json_subtree_v1,
    },
    incremental_semantic_key::hash_semantic_key_v1,
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{MerkleNodeReader, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
#[derive(Debug)]
pub struct CognitionJsonEntryQualificationV1 {
    field: CognitionJsonFieldV1,
    root: StateRootV1,
    body: oasis7_proto::memory_budget::ReservedValue<JsonSubtreeQualificationV1>,
}
impl CognitionJsonEntryQualificationV1 {
    pub fn validate_binding(
        &self,
        field: CognitionJsonFieldV1,
        root: &StateRootV1,
    ) -> Result<(), ValueTreeError> {
        if self.field != field || &self.root != root {
            return Err(ValueTreeError::Invalid("cognition JSON cohort binding"));
        }
        self.body
            .validate_binding(field.profile(), self.body.root())
    }
    pub fn body_root(&self) -> &StateRootV1 {
        self.body.root()
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
) -> Result<
    Option<oasis7_proto::memory_budget::ReservedValue<CognitionJsonEntryQualificationV1>>,
    ValueTreeError,
> {
    context.check()?;
    let descriptor = field.descriptor();
    let metadata = WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let metadata_root = context.lookup(&metadata)?;
    let (key, semantic_bytes) = context.key(descriptor, WorldFieldRecordKindV1::Entry, &())?;
    let root = context.lookup(&key)?;
    let Some(metadata_root) = metadata_root else {
        if !field.optional() || root.is_some() {
            return Err(ValueTreeError::Invalid("cognition JSON missing metadata"));
        }
        context.check()?;
        return Ok(None);
    };
    context.records().presence(&metadata_root, &metadata, 0)?;
    let root = root.ok_or(ValueTreeError::Invalid("cognition JSON missing root"))?;
    let _scratch = context
        .values
        .memory
        .reserve(8192, context.values.class)
        .map_err(ValueTreeError::Memory)?;
    let mut bytes = [0u8; RECORD_ROOT_BYTES];
    let mut output = std::io::Cursor::new(&mut bytes[..]);
    let info = read_member_record_profile_v1(
        &root,
        &key,
        &mut context.values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::RecordRoot,
            semantic_bytes,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: context.semantic_bytes,
                payload_bytes: RECORD_ROOT_BYTES as u64,
                record_bytes: context.record_bytes,
            },
        },
        &mut std::io::sink(),
        &mut output,
        &mut context.cancelled,
    )?;
    if info.payload_bytes != RECORD_ROOT_BYTES as u64
        || output.position() != RECORD_ROOT_BYTES as u64
    {
        return Err(ValueTreeError::Invalid("cognition JSON root length"));
    }
    let semantic = hash_semantic_key_v1(
        &(),
        context.values.memory,
        context.values.class,
        context.key_work,
        &mut context.cancelled,
    )
    .map_err(|error| match error {
        crate::incremental_semantic_key::SemanticKeyError::Memory(error) => {
            ValueTreeError::Memory(error)
        }
        crate::incremental_semantic_key::SemanticKeyError::ResourceLimited => {
            ValueTreeError::ResourceLimited("cognition JSON key")
        }
        crate::incremental_semantic_key::SemanticKeyError::Cancelled => ValueTreeError::Cancelled,
        crate::incremental_semantic_key::SemanticKeyError::Invalid(error) => {
            ValueTreeError::Invalid(error)
        }
    })?;
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes)
        .map_err(|_| ValueTreeError::Invalid("cognition JSON root reference"))?;
    let child = reference
        .validate_binding(
            &field.profile().schema,
            &semantic.digest,
            context.root.namespace,
        )
        .map_err(|_| ValueTreeError::Invalid("cognition JSON root binding"))?;
    let body = qualify_json_subtree_v1(field.profile(), child, context, work)?;
    context.check()?;
    let retained = context
        .values
        .memory
        .reserve(1024, context.values.class)
        .map_err(ValueTreeError::Memory)?;
    Ok(Some(retained.retain(CognitionJsonEntryQualificationV1 {
        field,
        root: context.root.clone(),
        body,
    })))
}
