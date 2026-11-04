//! Borrowed producers for all three original Runtime rollback control fields.
use crate::{
    incremental_field_member::*,
    incremental_field_presence::*,
    incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape},
    incremental_record_tree::{self, RecordTreeWritePolicyV1},
    incremental_rollback_genesis::{
        LABELS, RollbackRecordRefV1, RollbackWorkV1, prepare_rollback_record_v1,
    },
    incremental_world_field_key::WorldFieldRecordKindV1,
    snapshot::{RollbackAuthorityRegistry, RollbackNonceOutcome},
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_record_root::RecordRootRefV1,
    incremental_state::{StateNamespaceV1, StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::collections::{BTreeMap, BTreeSet};
pub struct RollbackControlsRefV1<'a> {
    pub authority_registry: &'a RollbackAuthorityRegistry,
    pub consumed_nonces: &'a BTreeSet<String>,
    pub nonce_outcomes: &'a BTreeMap<String, RollbackNonceOutcome>,
}
pub(crate) fn field(name: &'static str) -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::Runtime,
        name,
        shape: match name {
            "rollback_authority_registry" => FieldShape::Value,
            "consumed_rollback_nonces" => FieldShape::Set,
            _ => FieldShape::Map,
        },
        destination: FieldDestination::State(StateNamespaceV1::Authorization),
    }
}
fn spend(work: &mut RollbackWorkV1) -> Result<(), ValueTreeError> {
    work.cells_remaining = work
        .cells_remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("rollback GEN cells"))?;
    Ok(())
}
fn metadata<W: ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
    field: FieldDescriptor,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<PresenceRecordV1, MemberRecordError> {
    write_field_metadata_v1(
        field,
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
    )
}
fn reference<W: ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
    name: &'static str,
    key: &str,
    root: StateRootV1,
    schema: [u8; 32],
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    let semantic = crate::incremental_semantic_key::hash_semantic_key_v1(
        key,
        ctx.memory,
        MemoryClass::Ordinary,
        ctx.key_work,
        &mut ctx.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    let reference = RecordRootRefV1::new(schema, semantic.digest, root)
        .map_err(|_| MemberRecordError::Invalid("rollback outer root"))?;
    write_member_record_v1(
        field(name),
        WorldFieldRecordKindV1::Entry,
        key,
        MemberPayloadRefV1::RecordRoot(&reference),
        ctx,
    )
}
pub fn prepare_rollback_controls_genesis_v1<
    W: ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &RollbackControlsRefV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RollbackWorkV1,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let validation_bytes = source
        .authority_registry
        .records()
        .try_fold(16384usize, |sum, record| {
            sum.checked_add(
                record
                    .authority_id
                    .len()
                    .checked_add(record.public_key_hex.len())?
                    .checked_mul(8)?,
            )
        })
        .ok_or(MemberRecordError::Invalid(
            "rollback registry admission overflow",
        ))?;
    let _charge = ctx
        .memory
        .reserve(validation_bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    // The source registry is an authority owner, never infer authority from bytes.
    source
        .authority_registry
        .validate()
        .map_err(|_| MemberRecordError::Invalid("rollback authority registry"))?;
    for name in [
        "rollback_authority_registry",
        "consumed_rollback_nonces",
        "rollback_nonce_outcomes",
    ] {
        spend(work)?;
        let meta = metadata(field(name), ctx)?;
        crate::incremental_field_write::emit(meta.field_key, meta.value_root, ctx, sink)?;
    }
    let policy = RecordTreeWritePolicyV1 {
        max_pending_node_bytes: work.max_pending_node_bytes,
        labels: LABELS,
    };
    let name = "rollback_authority_registry";
    let mut registry = StateRootV1::empty(StateNamespaceV1::Authorization)
        .map_err(|_| MemberRecordError::Invalid("rollback namespace"))?;
    let meta = metadata(field(name), ctx)?;
    spend(work)?;
    registry =
        incremental_record_tree::append(registry, &meta.field_key, meta.value_root, ctx, policy)?;
    spend(work)?;
    let marker = write_member_record_v1(
        field(name),
        WorldFieldRecordKindV1::Entry,
        &(),
        MemberPayloadRefV1::Unit,
        ctx,
    )?;
    registry = incremental_record_tree::append(
        registry,
        &marker.field_key,
        marker.value_root,
        ctx,
        policy,
    )?;
    for record in source.authority_registry.records() {
        let source = RollbackRecordRefV1::Authority(record);
        let child = prepare_rollback_record_v1(source, ctx, work)?;
        spend(work)?;
        let row = reference(name, &record.authority_id, child, source.schema(), ctx)?;
        registry =
            incremental_record_tree::append(registry, &row.field_key, row.value_root, ctx, policy)?;
    }
    // Unit semantic identity for the required global registry value.
    let semantic = crate::incremental_semantic_key::hash_semantic_key_v1(
        &(),
        ctx.memory,
        MemoryClass::Ordinary,
        ctx.key_work,
        &mut ctx.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    let registry_reference = RecordRootRefV1::new(
        *blake3::hash(b"runtime.rollback.registry.v1").as_bytes(),
        semantic.digest,
        registry,
    )
    .map_err(|_| MemberRecordError::Invalid("rollback registry root"))?;
    spend(work)?;
    let row = write_member_record_v1(
        field(name),
        WorldFieldRecordKindV1::Entry,
        &(),
        MemberPayloadRefV1::RecordRoot(&registry_reference),
        ctx,
    )?;
    crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
    for name in ["consumed_rollback_nonces", "rollback_nonce_outcomes"] {
        spend(work)?;
        let marker = write_member_record_v1(
            field(name),
            WorldFieldRecordKindV1::Entry,
            &(),
            MemberPayloadRefV1::Unit,
            ctx,
        )?;
        crate::incremental_field_write::emit(marker.field_key, marker.value_root, ctx, sink)?;
    }
    for nonce in source.consumed_nonces {
        spend(work)?;
        let row = write_member_record_v1(
            field("consumed_rollback_nonces"),
            WorldFieldRecordKindV1::Entry,
            nonce.as_str(),
            MemberPayloadRefV1::Bool(true),
            ctx,
        )?;
        crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
    }
    for (nonce, outcome) in source.nonce_outcomes {
        let source = RollbackRecordRefV1::Outcome(outcome);
        let root = prepare_rollback_record_v1(source, ctx, work)?;
        spend(work)?;
        let row = reference("rollback_nonce_outcomes", nonce, root, source.schema(), ctx)?;
        crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
