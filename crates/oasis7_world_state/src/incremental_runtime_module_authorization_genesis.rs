//! Provisional borrowed grant/rule production, without an authorization token.
use super::*;
use crate::{
    incremental_field_member::*,
    incremental_field_presence::*,
    incremental_record_tree::{self as tree, RecordTreeLabelsV1, RecordTreeWritePolicyV1},
    incremental_semantic_key::{SemanticKeyV1, hash_semantic_key_v1},
    incremental_world_field_key::WorldFieldRecordKindV1,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_record_root::RecordRootRefV1,
    incremental_state::{StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::collections::BTreeMap;
pub(super) const FORMAT: &str = "oasis7.iws.runtime-module-authorization.v1";
pub struct ModuleAuthorizationWriteWorkV1 {
    pub cells_remaining: u64,
    pub max_pending_node_bytes: usize,
}
impl ModuleAuthorizationWriteWorkV1 {
    fn cell(&mut self) -> Result<(), MemberRecordError> {
        self.cells_remaining =
            self.cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "module authorization cells",
                ))?;
        Ok(())
    }
}
pub(super) const LABELS: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "module authorization read",
    pending: "module authorization pending",
    tree: "module authorization tree",
    digest: "module authorization digest",
    read_allowance: "module authorization read allowance",
    admission: "module authorization admission",
    bound: "module authorization bound",
    producer_pool: "module authorization pool",
    page: "module authorization page",
    write_allowance: "module authorization write allowance",
};
pub(super) fn body_descriptor() -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: "runtime_module_authorization_body_v1",
        shape: FieldShape::Map,
        destination: FieldDestination::State(StateNamespaceV1::Authorization),
    }
}
fn metadata<W: ValueNodeWriter, C: FnMut() -> bool>(
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
fn row<K: SemanticKeyV1 + ?Sized, W: ValueNodeWriter, C: FnMut() -> bool>(
    field: FieldDescriptor,
    key: &K,
    payload: MemberPayloadRefV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    write_member_record_v1(field, WorldFieldRecordKindV1::Entry, key, payload, ctx)
}
pub(super) enum Body<'a> {
    Grant(&'a CapabilityGrant),
    Rule(&'a PolicyRule),
}
impl Body<'_> {
    fn field(&self) -> ModuleAuthorizationFieldV1 {
        match self {
            Self::Grant(_) => ModuleAuthorizationFieldV1::Capabilities,
            Self::Rule(_) => ModuleAuthorizationFieldV1::PolicyRules,
        }
    }
}
pub(super) fn body<W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    source: Body<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut ModuleAuthorizationWriteWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let policy = RecordTreeWritePolicyV1 {
        max_pending_node_bytes: work.max_pending_node_bytes,
        labels: LABELS,
    };
    let mut root = StateRootV1::empty(StateNamespaceV1::Authorization)
        .map_err(|_| MemberRecordError::Invalid("module authorization namespace"))?;
    work.cell()?;
    let meta = metadata(body_descriptor(), ctx)?;
    root = tree::append(root, &meta.field_key, meta.value_root, ctx, policy)?;
    work.cell()?;
    let field = source.field();
    let marker = row(
        body_descriptor(),
        &(),
        MemberPayloadRefV1::Text(field.label()),
        ctx,
    )?;
    root = tree::append(root, &marker.field_key, marker.value_root, ctx, policy)?;
    let mut emit = |address: ModuleAuthorizationAddressV1,
                    payload: MemberPayloadRefV1<'_>|
     -> Result<(), MemberRecordError> {
        validate(field, address, payload).map_err(MemberRecordError::Invalid)?;
        work.cell()?;
        let record = row(body_descriptor(), &address.semantic(field), payload, ctx)?;
        root = tree::append(
            root.clone(),
            &record.field_key,
            record.value_root,
            ctx,
            policy,
        )?;
        Ok(())
    };
    match source {
        Body::Grant(source) => visit_module_capability_grant_v1(source, &mut emit)?,
        Body::Rule(source) => visit_module_policy_rule_v1(source, &mut emit)?,
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(root)
}
fn root_row<K: SemanticKeyV1 + ?Sized, W: ValueNodeWriter, C: FnMut() -> bool>(
    field: ModuleAuthorizationFieldV1,
    key: &K,
    child: StateRootV1,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    let _scratch = ctx
        .memory
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let semantic = hash_semantic_key_v1(
        key,
        ctx.memory,
        MemoryClass::Ordinary,
        ctx.key_work,
        &mut ctx.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    let reference = RecordRootRefV1::new(field.schema(), semantic.digest, child)
        .map_err(|_| MemberRecordError::Invalid("module authorization outer binding"))?;
    row(
        field.descriptor(),
        key,
        MemberPayloadRefV1::RecordRoot(&reference),
        ctx,
    )
}
/// Complete source projection only. An owner qualifier must independently
/// establish the saved roots before any authorization consumer can trust them.
pub fn prepare_module_authorization_genesis_v1<W, C, S>(
    capabilities: &BTreeMap<String, CapabilityGrant>,
    policies: &crate::policy::PolicySet,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut ModuleAuthorizationWriteWorkV1,
    sink: &mut S,
) -> Result<(), MemberRecordError>
where
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
{
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let _operation = ctx
        .memory
        .reserve(32 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    for field in ModuleAuthorizationFieldV1::ALL {
        work.cell()?;
        let meta = metadata(field.descriptor(), ctx)?;
        crate::incremental_field_write::emit(meta.field_key, meta.value_root, ctx, sink)?;
    }
    for (key, grant) in capabilities {
        let child = body(Body::Grant(grant), ctx, work)?;
        work.cell()?;
        let record = root_row(ModuleAuthorizationFieldV1::Capabilities, key, child, ctx)?;
        crate::incremental_field_write::emit(record.field_key, record.value_root, ctx, sink)?;
    }
    let crate::policy::PolicySet { rules } = policies;
    work.cell()?;
    let length = row(
        ModuleAuthorizationFieldV1::PolicyRules.descriptor(),
        &(FORMAT, "length"),
        MemberPayloadRefV1::U64(rules.len() as u64),
        ctx,
    )?;
    crate::incremental_field_write::emit(length.field_key, length.value_root, ctx, sink)?;
    for (index, rule) in rules.iter().enumerate() {
        let child = body(Body::Rule(rule), ctx, work)?;
        work.cell()?;
        let record = root_row(
            ModuleAuthorizationFieldV1::PolicyRules,
            &(index as u64),
            child,
            ctx,
        )?;
        crate::incremental_field_write::emit(record.field_key, record.value_root, ctx, sink)?;
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
