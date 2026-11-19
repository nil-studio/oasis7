//! Runtime ordered original locators and bounded hot windows. Structural only.
use crate::{
    incremental_field_member::*,
    incremental_field_presence::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape},
    incremental_record_tree as tree,
    incremental_runtime_history_manifest::*,
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_material::FactLocationV1,
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
const LABELS: tree::RecordTreeLabelsV1 = tree::RecordTreeLabelsV1 {
    read: "Runtime History index read",
    pending: "Runtime History pending",
    tree: "Runtime History tree",
    digest: "Runtime History digest",
    read_allowance: "Runtime History read allowance",
    admission: "Runtime History source charge",
    bound: "Runtime History bound",
    producer_pool: "Runtime History producer pool",
    page: "Runtime History page",
    write_allowance: "Runtime History write allowance",
};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Runtime History index")
}
fn descriptor(lane: RuntimeHistoryLaneV1) -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: lane.name(),
        destination: FieldDestination::State(StateNamespaceV1::Artifacts),
        shape: FieldShape::ByteMap,
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeHistoryIndexRowV1 {
    pub location: FactLocationV1,
    pub encoded_bytes: u64,
}
impl RuntimeHistoryIndexRowV1 {
    fn bytes(self) -> [u8; 56] {
        let mut out = [0; 56];
        out[..32].copy_from_slice(&self.location.original);
        out[32..40].copy_from_slice(&self.encoded_bytes.to_be_bytes());
        out[40..48].copy_from_slice(&self.location.height.to_be_bytes());
        out[48..56].copy_from_slice(&self.location.ordinal.to_be_bytes());
        out
    }
    fn decode(b: [u8; 56]) -> Result<Self, ValueTreeError> {
        let result = Self {
            location: FactLocationV1 {
                original: b[..32].try_into().map_err(|_| invalid())?,
                height: u64::from_be_bytes(b[40..48].try_into().map_err(|_| invalid())?),
                ordinal: u64::from_be_bytes(b[48..56].try_into().map_err(|_| invalid())?),
            },
            encoded_bytes: u64::from_be_bytes(b[32..40].try_into().map_err(|_| invalid())?),
        };
        if result.encoded_bytes == 0 {
            return Err(invalid());
        }
        Ok(result)
    }
}
pub struct RuntimeHistoryIndexWorkV1 {
    pub cells_remaining: u64,
    pub max_pending_node_bytes: usize,
}
impl RuntimeHistoryIndexWorkV1 {
    fn step(&mut self) -> Result<(), ValueTreeError> {
        self.cells_remaining =
            self.cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "Runtime History index cells",
                ))?;
        Ok(())
    }
    fn policy(&self) -> tree::RecordTreeWritePolicyV1 {
        tree::RecordTreeWritePolicyV1 {
            max_pending_node_bytes: self.max_pending_node_bytes,
            labels: LABELS,
        }
    }
}
pub struct RuntimeHistoryIndexQualificationV1 {
    lane: RuntimeHistoryLaneV1,
    window: RuntimeHistoryWindowV1,
    class: MemoryClass,
    lineage: Option<(StateRootV1, u64)>,
}
impl RuntimeHistoryIndexQualificationV1 {
    pub fn lineage(&self) -> Option<(&StateRootV1, u64)> {
        self.lineage.as_ref().map(|(r, n)| (r, *n))
    }
    pub fn lane(&self) -> RuntimeHistoryLaneV1 {
        self.lane
    }
    pub fn window(&self) -> &RuntimeHistoryWindowV1 {
        &self.window
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
}
const CARRIER: usize = std::mem::size_of::<RuntimeHistoryIndexQualificationV1>() + 512;
fn poll<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    ctx.check()?;
    if let Some(e) = (ctx.take_state_error)() {
        return Err(e);
    }
    Ok(())
}
/// Point IO authenticates one fixed locator but does not establish provenance.
pub fn read_runtime_history_index_point_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    lane: RuntimeHistoryLaneV1,
    window: &RuntimeHistoryWindowV1,
    ordinal: u64,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeHistoryIndexWorkV1,
) -> Result<RuntimeHistoryIndexRowV1, ValueTreeError> {
    poll(ctx)?;
    window.validate()?;
    if ordinal >= window.total {
        return Err(invalid());
    }
    work.step()?;
    let _phase = ctx.values.memory.reserve(8192, ctx.values.class)?;
    let mut records = ctx.records();
    let (key, semantic) = records.key(descriptor(lane), WorldFieldRecordKindV1::Bytes, &ordinal)?;
    let id = tree::lookup(&window.root, &key, &mut records, LABELS)?.ok_or_else(invalid)?;
    let mut bytes = [0; 56];
    let info = read_member_record_profile_v1(
        &id,
        &key,
        &mut *records.values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::Bytes,
            semantic_bytes: semantic,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: records.semantic_bytes,
                payload_bytes: 56,
                record_bytes: records.record_bytes,
            },
        },
        &mut std::io::sink(),
        &mut &mut bytes[..],
        &mut *records.cancelled,
    )?;
    if info.payload_bytes != 56 {
        return Err(invalid());
    }
    let result = RuntimeHistoryIndexRowV1::decode(bytes)?;
    poll(ctx)?;
    Ok(result)
}
/// Full structural bootstrap. This deliberately does not mint original source
/// evidence from an observer callback. Candidate fact/type validation is separate.
pub fn qualify_runtime_history_index_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    lane: RuntimeHistoryLaneV1,
    window: &RuntimeHistoryWindowV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeHistoryIndexWorkV1,
) -> Result<ReservedValue<RuntimeHistoryIndexQualificationV1>, ValueTreeError> {
    poll(ctx)?;
    window.validate()?;
    let charge = ctx.values.memory.reserve(CARRIER, ctx.values.class)?;
    {
        let mut records = ctx.records();
        work.step()?;
        let key = WorldFieldKeyV1::new(descriptor(lane), WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let id = tree::lookup(&window.root, &key, &mut records, LABELS)?.ok_or_else(invalid)?;
        records.presence(&id, &key, 0)?;
        work.step()?;
        let (key, semantic) = records.key(descriptor(lane), WorldFieldRecordKindV1::Entry, &())?;
        let id = tree::lookup(&window.root, &key, &mut records, LABELS)?.ok_or_else(invalid)?;
        if records.unsigned(&id, &key, semantic)? != window.total {
            return Err(invalid());
        }
    }
    for ordinal in 0..window.total {
        read_runtime_history_index_point_v1(lane, window, ordinal, ctx, work)?;
    }
    {
        let mut records = ctx.records();
        let mut cursor =
            crate::incremental_json_records::cursor::Cursor::new(&window.root, &mut records)?;
        while cursor.next(&mut records)?.is_some() {
            work.step()?;
        }
    }
    poll(ctx)?;
    Ok(charge.retain(RuntimeHistoryIndexQualificationV1 {
        lane,
        window: window.clone(),
        class: ctx.values.class,
        lineage: None,
    }))
}
/// The window may evict a prefix or truncate a suffix without editing any old
/// page. A later append requires its end to reach the immutable accumulated tail;
/// rollback-and-append instead starts from the authenticated prefix branch root.
pub fn select_runtime_history_window_v1(
    source: &ReservedValue<RuntimeHistoryIndexQualificationV1>,
    start: u64,
    count: u64,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<RuntimeHistoryIndexQualificationV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if source.class != MemoryClass::Ordinary
        || !source.uses_budget(memory)
        || source.reserved_bytes() < CARRIER
    {
        return Err(invalid());
    }
    let charge = memory.reserve(CARRIER, MemoryClass::Ordinary)?;
    let mut window = source.window.clone();
    window.start = start;
    window.count = count;
    window.validate()?;
    if start < source.window.start || window.end()? > source.window.end()? {
        return Err(invalid());
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(RuntimeHistoryIndexQualificationV1 {
        lane: source.lane,
        window,
        class: MemoryClass::Ordinary,
        lineage: source.lineage.clone(),
    }))
}
pub fn readmit_runtime_history_index_ordinary_v1(
    source: &ReservedValue<RuntimeHistoryIndexQualificationV1>,
    expected: &RuntimeHistoryWindowV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<RuntimeHistoryIndexQualificationV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if source.class != MemoryClass::Recovery
        || !source.uses_budget(memory)
        || source.reserved_bytes() < CARRIER
        || &source.window != expected
    {
        return Err(invalid());
    }
    expected.validate()?;
    let charge = memory.reserve(CARRIER, MemoryClass::Ordinary)?;
    let window = source.window.clone();
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(RuntimeHistoryIndexQualificationV1 {
        lane: source.lane,
        window,
        class: MemoryClass::Ordinary,
        lineage: source.lineage.clone(),
    }))
}
/// Explicit new GEN builds the two control leaves plus streamed original rows.
pub fn prepare_runtime_history_index_v1<
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    I: IntoIterator<Item = Result<RuntimeHistoryIndexRowV1, ValueTreeError>>,
>(
    lane: RuntimeHistoryLaneV1,
    count: u64,
    rows: I,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeHistoryIndexWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    count.checked_add(2).ok_or_else(invalid)?;
    let _phase = ctx
        .memory
        .reserve(16384, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut root = StateRootV1::empty(StateNamespaceV1::Artifacts).map_err(|_| invalid())?;
    work.step()?;
    let meta = write_field_metadata_v1(
        descriptor(lane),
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
    root = tree::append(root, &meta.field_key, meta.value_root, ctx, work.policy())?;
    work.step()?;
    let control = write_member_record_v1(
        descriptor(lane),
        WorldFieldRecordKindV1::Entry,
        &(),
        MemberPayloadRefV1::U64(count),
        ctx,
    )?;
    root = tree::append(
        root,
        &control.field_key,
        control.value_root,
        ctx,
        work.policy(),
    )?;
    let mut emitted = 0;
    for row in rows {
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        work.step()?;
        if emitted >= count {
            return Err(invalid().into());
        }
        let row = row?;
        if row.encoded_bytes == 0 {
            return Err(invalid().into());
        }
        let bytes = row.bytes();
        let record = write_member_record_v1(
            descriptor(lane),
            WorldFieldRecordKindV1::Bytes,
            &emitted,
            MemberPayloadRefV1::Bytes(&bytes),
            ctx,
        )?;
        root = tree::append(
            root,
            &record.field_key,
            record.value_root,
            ctx,
            work.policy(),
        )?;
        emitted += 1;
    }
    if emitted != count {
        return Err(invalid().into());
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(root)
}

/// Rebuild one append from the qualified parent and authenticated old count.
/// Returned evidence concerns structure only; the caller must independently
/// check the new row's actual candidate occurrence and type before acceptance.
pub fn prepare_runtime_history_index_append_v1<M, R, E, C, W, CW>(
    parent: &ReservedValue<RuntimeHistoryIndexQualificationV1>,
    row: RuntimeHistoryIndexRowV1,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut RuntimeHistoryIndexWorkV1,
) -> Result<ReservedValue<RuntimeHistoryIndexQualificationV1>, MemberRecordError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
{
    poll(read)?;
    if (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if parent.class != MemoryClass::Ordinary
        || read.values.class != MemoryClass::Ordinary
        || !parent.uses_budget(write.memory)
        || parent.reserved_bytes() < CARRIER
        || !write.memory.same_pool(read.values.memory)
        || row.encoded_bytes == 0
        || parent.window.end()? != parent.window.total
    {
        return Err(invalid().into());
    }
    let retained = write
        .memory
        .reserve(CARRIER, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let _phase = write
        .memory
        .reserve(16384, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let count = parent.window.total.checked_add(1).ok_or_else(invalid)?;
    let (key, old) = {
        let mut records = read.records();
        work.step()?;
        let (key, semantic) =
            records.key(descriptor(parent.lane), WorldFieldRecordKindV1::Entry, &())?;
        let old =
            tree::lookup(&parent.window.root, &key, &mut records, LABELS)?.ok_or_else(invalid)?;
        if records.unsigned(&old, &key, semantic)? != parent.window.total {
            return Err(invalid().into());
        }
        let (next, _) = records.key(
            descriptor(parent.lane),
            WorldFieldRecordKindV1::Bytes,
            &parent.window.total,
        )?;
        if tree::lookup(&parent.window.root, &next, &mut records, LABELS)?.is_some() {
            return Err(invalid().into());
        }
        (key, old)
    };
    work.step()?;
    let control = write_member_record_v1(
        descriptor(parent.lane),
        WorldFieldRecordKindV1::Entry,
        &(),
        MemberPayloadRefV1::U64(count),
        write,
    )?;
    if key != control.field_key {
        return Err(invalid().into());
    }
    let root = tree::edit(
        parent.window.root.clone(),
        &key,
        (Some(old), Some(control.value_root)),
        &mut read.values,
        write,
        &mut read.cancelled,
        work.policy(),
    )?;
    work.step()?;
    let bytes = row.bytes();
    let record = write_member_record_v1(
        descriptor(parent.lane),
        WorldFieldRecordKindV1::Bytes,
        &parent.window.total,
        MemberPayloadRefV1::Bytes(&bytes),
        write,
    )?;
    let root = tree::append(
        root,
        &record.field_key,
        record.value_root,
        write,
        work.policy(),
    )?;
    let window = RuntimeHistoryWindowV1 {
        root,
        total: count,
        start: parent.window.start,
        count: parent.window.count.checked_add(1).ok_or_else(invalid)?,
    };
    window.validate()?;
    poll(read)?;
    if (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(retained.retain(RuntimeHistoryIndexQualificationV1 {
        lane: parent.lane,
        window,
        class: MemoryClass::Ordinary,
        lineage: Some(
            parent
                .lineage
                .clone()
                .unwrap_or_else(|| (parent.window.root.clone(), parent.window.total)),
        ),
    }))
}

/// Recover only the logical index accepted in an exact current selected Result.
/// Its identity is recomputed from the complete fixed manifest bytes; a decoded
/// manifest or an unrelated accepted occurrence cannot authorize this fast path.
/// This does not assert physical closure or original availability.
pub fn recover_runtime_history_index_attested_v1<M, R, E, C>(
    manifest: &RuntimeHistoryManifestV1,
    occurrence: &ReservedValue<
        oasis7_formal_protocol::incremental_material::VerifiedFactOccurrenceV1,
    >,
    lane: RuntimeHistoryLaneV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeHistoryIndexWorkV1,
) -> Result<ReservedValue<RuntimeHistoryIndexQualificationV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    use oasis7_formal_protocol::incremental_execution::ExecutionFactKindV1;
    poll(ctx)?;
    let bytes = manifest.canonical_bytes()?;
    if !occurrence.uses_budget(ctx.values.memory)
        || occurrence.reserved_bytes()
            < std::mem::size_of::<
                oasis7_formal_protocol::incremental_material::VerifiedFactOccurrenceV1,
            >()
        || occurrence.kind() != ExecutionFactKindV1::Result
        || occurrence.allocation_class() != ctx.values.class
        || occurrence.roots_id() != manifest.roots
        || occurrence.head_body_id() != occurrence.fact_body_id()
        || occurrence.original_bytes() != bytes.len() as u64
        || occurrence.location().original != *blake3::hash(&bytes).as_bytes()
    {
        return Err(invalid());
    }
    let charge = ctx.values.memory.reserve(CARRIER, ctx.values.class)?;
    let window = &manifest.lanes[lane.index()];
    {
        let mut records = ctx.records();
        work.step()?;
        let key = WorldFieldKeyV1::new(descriptor(lane), WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let id = tree::lookup(&window.root, &key, &mut records, LABELS)?.ok_or_else(invalid)?;
        records.presence(&id, &key, 0)?;
        work.step()?;
        let (key, semantic) = records.key(descriptor(lane), WorldFieldRecordKindV1::Entry, &())?;
        let id = tree::lookup(&window.root, &key, &mut records, LABELS)?.ok_or_else(invalid)?;
        if records.unsigned(&id, &key, semantic)? != window.total {
            return Err(invalid());
        }
    }
    if window.total > 0 {
        read_runtime_history_index_point_v1(lane, window, 0, ctx, work)?;
        read_runtime_history_index_point_v1(lane, window, window.total - 1, ctx, work)?;
    }
    if window.count > 0 {
        read_runtime_history_index_point_v1(lane, window, window.start, ctx, work)?;
        read_runtime_history_index_point_v1(lane, window, window.end()? - 1, ctx, work)?;
    }
    poll(ctx)?;
    Ok(charge.retain(RuntimeHistoryIndexQualificationV1 {
        lane,
        window: window.clone(),
        class: ctx.values.class,
        lineage: None,
    }))
}

#[cfg(test)]
#[path = "incremental_runtime_history_index_tests.rs"]
mod tests;

#[path = "incremental_runtime_history_index_prepared.rs"]
mod prepared;
pub(crate) use prepared::prepare_runtime_history_index_promotion_v1;
