use crate::{
    incremental_field_member::*,
    incremental_field_presence::{
        PresenceRecordLimitsV1, PresenceRecordWriteContext, write_field_metadata_v1,
    },
    incremental_record_tree::{self, RecordTreeLabelsV1, RecordTreeWritePolicyV1},
    incremental_semantic_key::{SemanticKeyV1, hash_semantic_key_v1},
};
use oasis7_formal_protocol::{
    incremental_record_root::RecordRootRefV1,
    incremental_state::{StateNamespaceV1, StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
#[derive(Clone, Copy)]
pub(super) enum AuthorizationRecordV1 {
    Audit,
    Invocation,
}
impl AuthorizationRecordV1 {
    pub(super) fn descriptor(self) -> FieldDescriptor {
        FieldDescriptor {
            owner: FieldOwner::Runtime,
            name: match self {
                Self::Audit => "capability_authorization_receipts.columns",
                Self::Invocation => "capability_invocation_contexts.columns",
            },
            destination: FieldDestination::State(StateNamespaceV1::Authorization),
            shape: FieldShape::Map,
        }
    }
    pub(super) fn schema(self) -> [u8; 32] {
        let mut hash = blake3::Hasher::new();
        hash.update(b"oasis7.iws.runtime-authorization.columns.v1\0");
        hash.update(self.descriptor().name.as_bytes());
        *hash.finalize().as_bytes()
    }
}
const LABELS: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "runtime authorization child state read",
    pending: "runtime authorization pending node allowance",
    tree: "runtime authorization typed child state tree",
    digest: "runtime authorization state node digest",
    read_allowance: "runtime authorization child read allowance",
    admission: "runtime authorization child source admission",
    bound: "runtime authorization child source bound",
    producer_pool: "runtime authorization producer pool",
    page: "runtime authorization state page",
    write_allowance: "runtime authorization write allowance",
};

/// Caller-owned logical work budget. Memory and I/O remain charged through the
/// shared contexts; this counter limits semantic rows and record framing work.
pub struct AuthorizationWriteWorkV1 {
    pub cells_remaining: u64,
    pub max_pending_node_bytes: usize,
}
impl AuthorizationWriteWorkV1 {
    pub(super) fn spend(
        &mut self,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        self.cells_remaining =
            self.cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "runtime authorization resource cells",
                ))?;
        Ok(())
    }
}

pub(super) fn append<W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    root: &mut StateRootV1,
    field_key: &WorldFieldKeyV1,
    value_root: [u8; 32],
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &AuthorizationWriteWorkV1,
) -> Result<(), MemberRecordError> {
    *root = incremental_record_tree::append(
        root.clone(),
        field_key,
        value_root,
        context,
        RecordTreeWritePolicyV1 {
            max_pending_node_bytes: work.max_pending_node_bytes,
            labels: LABELS,
        },
    )?;
    Ok(())
}

pub(super) fn child_root() -> Result<StateRootV1, ValueTreeError> {
    StateRootV1::empty(StateNamespaceV1::Authorization)
        .map_err(|_| ValueTreeError::Invalid("runtime authorization child namespace"))
}

pub(super) fn presence_context<'a, W, C: FnMut() -> bool>(
    context: &'a mut MemberRecordWriteContext<'_, W, C>,
) -> PresenceRecordWriteContext<'a, W, &'a mut C> {
    PresenceRecordWriteContext {
        sink: &mut *context.sink,
        memory: context.memory,
        io: &mut *context.io,
        key_work: &mut *context.key_work,
        limits: PresenceRecordLimitsV1 {
            semantic_bytes: context.limits.semantic_bytes,
            record_bytes: context.limits.record_bytes,
        },
        cancelled: &mut context.cancelled,
    }
}

pub(super) fn metadata<
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    root: &mut StateRootV1,
    record: AuthorizationRecordV1,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut AuthorizationWriteWorkV1,
) -> Result<(), MemberRecordError> {
    work.spend(&mut context.cancelled)?;
    let field = record.descriptor();
    let record = write_field_metadata_v1(field, &mut presence_context(context))?;
    append(root, &record.field_key, record.value_root, context, work)
}

pub(super) fn member<
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    K: SemanticKeyV1 + ?Sized,
>(
    root: &mut StateRootV1,
    record: AuthorizationRecordV1,
    key: &K,
    payload: MemberPayloadRefV1<'_>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut AuthorizationWriteWorkV1,
) -> Result<(), MemberRecordError> {
    work.spend(&mut context.cancelled)?;
    let cell = write_member_record_v1(
        record.descriptor(),
        WorldFieldRecordKindV1::Entry,
        key,
        payload,
        context,
    )?;
    append(root, &cell.field_key, cell.value_root, context, work)
}

pub(super) fn root_reference<
    K: SemanticKeyV1 + ?Sized,
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    record: AuthorizationRecordV1,
    key: &K,
    child: StateRootV1,
    context: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<RecordRootRefV1, MemberRecordError> {
    let owner = hash_semantic_key_v1(
        key,
        context.memory,
        MemoryClass::Ordinary,
        context.key_work,
        &mut context.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    RecordRootRefV1::new(record.schema(), owner.digest, child)
        .map_err(|_| MemberRecordError::Invalid("runtime authorization record root"))
}

pub(super) fn text<W: ValueNodeWriter + StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    root: &mut StateRootV1,
    record: AuthorizationRecordV1,
    key: &'static str,
    value: &str,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut AuthorizationWriteWorkV1,
) -> Result<(), MemberRecordError> {
    member(
        root,
        record,
        &key,
        MemberPayloadRefV1::Text(value),
        context,
        work,
    )
}

pub(super) fn u64_value<
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    root: &mut StateRootV1,
    record: AuthorizationRecordV1,
    key: &'static str,
    value: u64,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut AuthorizationWriteWorkV1,
) -> Result<(), MemberRecordError> {
    member(
        root,
        record,
        &key,
        MemberPayloadRefV1::U64(value),
        context,
        work,
    )
}

pub(super) fn i64_value<
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    root: &mut StateRootV1,
    record: AuthorizationRecordV1,
    key: &'static str,
    value: i64,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut AuthorizationWriteWorkV1,
) -> Result<(), MemberRecordError> {
    member(
        root,
        record,
        &key,
        MemberPayloadRefV1::I64(value),
        context,
        work,
    )
}

pub(super) fn option_text<
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    root: &mut StateRootV1,
    record: AuthorizationRecordV1,
    key: &'static str,
    value: Option<&str>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut AuthorizationWriteWorkV1,
) -> Result<(), MemberRecordError> {
    member(
        root,
        record,
        &(key, "present"),
        MemberPayloadRefV1::Bool(value.is_some()),
        context,
        work,
    )?;
    if let Some(value) = value {
        member(
            root,
            record,
            &(key, "value"),
            MemberPayloadRefV1::Text(value),
            context,
            work,
        )?;
    }
    Ok(())
}

pub(super) fn option_u64<
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    root: &mut StateRootV1,
    record: AuthorizationRecordV1,
    key: &'static str,
    value: Option<u64>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut AuthorizationWriteWorkV1,
) -> Result<(), MemberRecordError> {
    member(
        root,
        record,
        &(key, "present"),
        MemberPayloadRefV1::Bool(value.is_some()),
        context,
        work,
    )?;
    if let Some(value) = value {
        member(
            root,
            record,
            &(key, "value"),
            MemberPayloadRefV1::U64(value),
            context,
            work,
        )?;
    }
    Ok(())
}

pub(super) fn root_member<
    K: SemanticKeyV1 + ?Sized,
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    root: &mut StateRootV1,
    parent: AuthorizationRecordV1,
    key: &K,
    child_kind: AuthorizationRecordV1,
    child: StateRootV1,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut AuthorizationWriteWorkV1,
) -> Result<(), MemberRecordError> {
    let reference = root_reference(child_kind, key, child, context)?;
    member(
        root,
        parent,
        key,
        MemberPayloadRefV1::RecordRoot(&reference),
        context,
        work,
    )
}
