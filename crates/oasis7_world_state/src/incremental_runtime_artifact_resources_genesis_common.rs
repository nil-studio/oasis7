use super::*;
const LABELS: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "runtime artifact child state read",
    pending: "runtime artifact pending node allowance",
    tree: "runtime artifact typed child state tree",
    digest: "runtime artifact state node digest",
    read_allowance: "runtime artifact child read allowance",
    admission: "runtime artifact child source admission",
    bound: "runtime artifact child source bound",
    producer_pool: "runtime artifact producer pool",
    page: "runtime artifact state page",
    write_allowance: "runtime artifact write allowance",
};

/// Caller-owned logical work budget. Memory and I/O remain charged through the
/// shared contexts; this counter limits semantic rows and record framing work.
pub struct RuntimeArtifactResourcesWorkV1 {
    pub cells_remaining: u64,
    pub max_pending_node_bytes: usize,
}
impl RuntimeArtifactResourcesWorkV1 {
    fn spend(&mut self, cancelled: &mut impl FnMut() -> bool) -> Result<(), ValueTreeError> {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        self.cells_remaining =
            self.cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "runtime artifact resource cells",
                ))?;
        Ok(())
    }
}

pub(super) fn append<W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    root: &mut StateRootV1,
    field_key: &WorldFieldKeyV1,
    value_root: [u8; 32],
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &RuntimeArtifactResourcesWorkV1,
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
    StateRootV1::empty(StateNamespaceV1::Artifacts)
        .map_err(|_| ValueTreeError::Invalid("runtime artifact child namespace"))
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
    record: RuntimeArtifactRecordV1,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<(), MemberRecordError> {
    work.spend(&mut context.cancelled)?;
    let field = record.descriptor();
    let record = write_field_metadata_v1(field, &mut presence_context(context))?;
    append(root, &record.field_key, record.value_root, context, work)
}

pub(super) fn group<
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    K: SemanticKeyV1 + ?Sized,
>(
    root: &mut StateRootV1,
    record: RuntimeArtifactRecordV1,
    key: &K,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<(), MemberRecordError> {
    work.spend(&mut context.cancelled)?;
    let field = record.descriptor();
    let record = write_field_group_v1(field, key, &mut presence_context(context))?;
    append(root, &record.field_key, record.value_root, context, work)
}

pub(super) fn member<
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    K: SemanticKeyV1 + ?Sized,
>(
    root: &mut StateRootV1,
    record: RuntimeArtifactRecordV1,
    key: &K,
    payload: MemberPayloadRefV1<'_>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
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
    record: RuntimeArtifactRecordV1,
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
        .map_err(|_| MemberRecordError::Invalid("runtime artifact record root"))
}

pub(super) fn text<W: ValueNodeWriter + StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    root: &mut StateRootV1,
    record: RuntimeArtifactRecordV1,
    key: &'static str,
    value: &str,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
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
    record: RuntimeArtifactRecordV1,
    key: &'static str,
    value: u64,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
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
    record: RuntimeArtifactRecordV1,
    key: &'static str,
    value: i64,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
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
    record: RuntimeArtifactRecordV1,
    key: &'static str,
    value: Option<&str>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
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
    record: RuntimeArtifactRecordV1,
    key: &'static str,
    value: Option<u64>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
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
    parent: RuntimeArtifactRecordV1,
    key: &K,
    child_kind: RuntimeArtifactRecordV1,
    child: StateRootV1,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
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

pub(super) fn emit_top<
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    field: RuntimeArtifactFieldV1,
    child: Option<StateRootV1>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    sink: &mut S,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<(), MemberRecordError> {
    work.spend(&mut context.cancelled)?;
    let descriptor = field.descriptor();
    let meta = write_field_metadata_v1(descriptor, &mut presence_context(context))?;
    incremental_field_write::emit(meta.field_key, meta.value_root, context, sink)?;
    work.spend(&mut context.cancelled)?;
    let reference = match child {
        None => None,
        Some(child) => {
            let owner = hash_semantic_key_v1(
                &(),
                context.memory,
                MemoryClass::Ordinary,
                context.key_work,
                &mut context.cancelled,
            )
            .map_err(MemberRecordError::Key)?;
            Some(
                RecordRootRefV1::new(field.schema(), owner.digest, child)
                    .map_err(|_| MemberRecordError::Invalid("runtime artifact outer root"))?,
            )
        }
    };
    let payload = match reference.as_ref() {
        None => MemberPayloadRefV1::Unit,
        Some(reference) => MemberPayloadRefV1::RecordRoot(reference),
    };
    let outer = write_member_record_v1(
        descriptor,
        WorldFieldRecordKindV1::Entry,
        &(),
        payload,
        context,
    )?;
    incremental_field_write::emit(outer.field_key, outer.value_root, context, sink)
}
