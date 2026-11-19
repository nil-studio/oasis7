//! Borrowed complete audit/invocation map genesis into one owning transaction.
use crate::{
    CapabilityAuthorizationAuditReceipt, CapabilityInvocationContext, incremental_field_member::*,
    incremental_field_presence::*, incremental_json_records::JsonWriteWorkV1,
    incremental_runtime_authorization_genesis::*,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::{StateNamespaceV1, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::collections::BTreeMap;
fn spend(
    work: &mut AuthorizationWriteWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    work.cells_remaining =
        work.cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited(
                "authorization family GEN cells",
            ))?;
    Ok(())
}
fn metadata<W, C, S>(
    name: &'static str,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut AuthorizationWriteWorkV1,
    sink: &mut S,
) -> Result<(), MemberRecordError>
where
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
{
    spend(work, &mut ctx.cancelled)?;
    let descriptor = FieldDescriptor {
        owner: FieldOwner::Runtime,
        name,
        shape: FieldShape::Map,
        destination: FieldDestination::State(StateNamespaceV1::Authorization),
    };
    let row = write_field_metadata_v1(
        descriptor,
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
    crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)
}
pub fn prepare_authorization_record_families_genesis_v1<W, C, S>(
    audits: &BTreeMap<String, CapabilityAuthorizationAuditReceipt>,
    invocations: &BTreeMap<String, CapabilityInvocationContext>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut AuthorizationWriteWorkV1,
    json: &mut JsonWriteWorkV1,
    sink: &mut S,
) -> Result<(), MemberRecordError>
where
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
{
    metadata("capability_authorization_receipts", ctx, work, sink)?;
    for (key, source) in audits {
        spend(work, &mut ctx.cancelled)?;
        let row = prepare_authorization_audit_member_v1(key, source, ctx, work, json)?;
        crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
    }
    metadata("capability_invocation_contexts", ctx, work, sink)?;
    for (key, source) in invocations {
        spend(work, &mut ctx.cancelled)?;
        let row = prepare_authorization_invocation_member_v1(key, source, ctx, work, json)?;
        crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
