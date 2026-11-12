//! Original JsonValue grants, encoded by the shared bounded JSON graph.
use crate::{
    incremental_field_member::*,
    incremental_json_records::{JsonRecordProfileV1, JsonWriteWorkV1, prepare_json_subtree_v1},
    incremental_semantic_key::hash_semantic_key_v1,
};
use oasis7_formal_protocol::{
    incremental_record_root::RecordRootRefV1,
    incremental_state::{StateNamespaceV1, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
pub fn authorization_grant_profile_v1() -> JsonRecordProfileV1 {
    JsonRecordProfileV1 {
        field: FieldDescriptor {
            owner: FieldOwner::Runtime,
            name: "capability_grants_v2",
            shape: FieldShape::Map,
            destination: FieldDestination::State(StateNamespaceV1::Authorization),
        },
        schema: *blake3::hash(b"oasis7.iws.runtime-authorization.capability_grants_v2.v1\0")
            .as_bytes(),
    }
}
/// Produces one outer member bound to the original map key. Metadata and
/// atomic publication remain the owning family transaction's responsibility.
pub fn prepare_authorization_grant_member_v1<
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    key: &str,
    value: &serde_json::Value,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut JsonWriteWorkV1,
) -> Result<MemberRecordV1, MemberRecordError> {
    let profile = authorization_grant_profile_v1();
    let child = prepare_json_subtree_v1(profile, value, context, work)?;
    let identity = hash_semantic_key_v1(
        &key,
        context.memory,
        MemoryClass::Ordinary,
        context.key_work,
        &mut context.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    let reference = RecordRootRefV1::new(profile.schema, identity.digest, child)
        .map_err(|_| MemberRecordError::Invalid("authorization grant reference"))?;
    let row = write_member_record_v1(
        profile.field,
        WorldFieldRecordKindV1::Entry,
        &key,
        MemberPayloadRefV1::RecordRoot(&reference),
        context,
    )?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(row)
}

/// Authenticate an original grant's outer binding before validating its full
/// JSON graph. The resulting graph token is member-local, not family authority.
pub fn qualify_authorization_grant_member_v1<M, R, E, C>(
    original_key: &str,
    context: &mut crate::incremental_field_read::WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut crate::incremental_json_records::JsonReadWorkV1,
) -> Result<
    Option<
        oasis7_proto::memory_budget::ReservedValue<
            crate::incremental_json_records::JsonSubtreeQualificationV1,
        >,
    >,
    ValueTreeError,
>
where
    M: oasis7_formal_protocol::incremental_state::MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    context.check()?;
    if context.root.namespace != StateNamespaceV1::Authorization {
        return Err(ValueTreeError::Invalid("authorization grant namespace"));
    }
    let profile = authorization_grant_profile_v1();
    let metadata = WorldFieldKeyV1::new(profile.field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let metadata_value = context.lookup(&metadata)?.ok_or(ValueTreeError::Invalid(
        "authorization grant metadata missing",
    ))?;
    context.records().presence(&metadata_value, &metadata, 0)?;
    let (key, semantic_bytes) =
        context.key(profile.field, WorldFieldRecordKindV1::Entry, &original_key)?;
    let Some(value) = context.lookup(&key)? else {
        return Ok(None);
    };
    qualify_authorization_grant_value_v1(&key, semantic_bytes, &value, context, work).map(Some)
}

/// Fixed decoder shared by parent selection and transaction Current selection.
/// Only the original grant profile and schema are accepted.
pub(crate) fn qualify_authorization_grant_value_v1<M, R, E, C>(
    key: &WorldFieldKeyV1,
    semantic_bytes: u64,
    value: &[u8; 32],
    context: &mut crate::incremental_field_read::WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut crate::incremental_json_records::JsonReadWorkV1,
) -> Result<
    oasis7_proto::memory_budget::ReservedValue<
        crate::incremental_json_records::JsonSubtreeQualificationV1,
    >,
    ValueTreeError,
>
where
    M: oasis7_formal_protocol::incremental_state::MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    use oasis7_formal_protocol::incremental_record_root::RECORD_ROOT_BYTES;
    let profile = authorization_grant_profile_v1();
    let mut bytes = [0; RECORD_ROOT_BYTES];
    let info = read_member_record_v1(
        &value,
        &key,
        &mut context.values,
        MemberRecordLimitsV1 {
            semantic_bytes: context.semantic_bytes,
            payload_bytes: RECORD_ROOT_BYTES as u64,
            record_bytes: context.record_bytes,
        },
        &mut std::io::sink(),
        &mut &mut bytes[..],
        &mut context.cancelled,
    )?;
    if info.codec != MemberPayloadCodecV1::RecordRoot
        || info.payload_bytes != RECORD_ROOT_BYTES as u64
        || info.semantic_bytes != semantic_bytes
    {
        return Err(ValueTreeError::Invalid("authorization grant root profile"));
    }
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes)
        .map_err(|_| ValueTreeError::Invalid("authorization grant root framing"))?;
    let owner = WorldFieldKeyV1::from_bytes(key.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or(ValueTreeError::Invalid("authorization grant owner"))?;
    let root = reference
        .validate_binding(&profile.schema, &owner, StateNamespaceV1::Authorization)
        .map_err(|_| ValueTreeError::Invalid("authorization grant root binding"))?;
    let proof =
        crate::incremental_json_records::qualify_json_subtree_v1(profile, root, context, work)?;
    context.check()?;
    Ok(proof)
}

/// Borrowed complete map genesis. Every provisional write is handed to the
/// caller's shared transaction; no accepted root is installed here.
pub fn prepare_authorization_grants_genesis_v1<W, C, S>(
    source: &std::collections::BTreeMap<String, serde_json::Value>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut JsonWriteWorkV1,
    sink: &mut S,
) -> Result<(), MemberRecordError>
where
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    S: FnMut(
        oasis7_proto::memory_budget::ReservedValue<
            oasis7_formal_protocol::incremental_execution::ExecutionWriteV1,
        >,
    ) -> Result<(), ValueTreeError>,
{
    use crate::incremental_field_presence::*;
    work.cells_remaining =
        work.cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited(
                "authorization grants metadata",
            ))?;
    let meta = write_field_metadata_v1(
        authorization_grant_profile_v1().field,
        &mut PresenceRecordWriteContext {
            sink: &mut *ctx.sink,
            memory: ctx.memory,
            io: &mut *ctx.io,
            key_work: &mut *ctx.key_work,
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: ctx.limits.semantic_bytes,
                record_bytes: ctx.limits.record_bytes,
            },
            cancelled: &mut ctx.cancelled,
        },
    )?;
    crate::incremental_field_write::emit(meta.field_key, meta.value_root, ctx, sink)?;
    for (key, value) in source {
        work.cells_remaining =
            work.cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "authorization grants outer members",
                ))?;
        let row = prepare_authorization_grant_member_v1(key, value, ctx, work)?;
        crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
