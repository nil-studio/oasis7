//! Detached ordinal locator indexes. Qualification closes only this index;
//! original availability and accepted kind-bound occurrences are separate.
use super::*;
use crate::{
    incremental_field_member::*,
    incremental_field_presence::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape},
    incremental_record_tree as tree,
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_material::FactLocationV1,
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
const ROW_BYTES: usize = 32 + 8 + 8 + 8;
const LABELS: tree::RecordTreeLabelsV1 = tree::RecordTreeLabelsV1 {
    read: "Cognition History index read",
    pending: "Cognition History index pending",
    tree: "Cognition History index tree",
    digest: "Cognition History index digest",
    read_allowance: "Cognition History index read allowance",
    admission: "Cognition History index source admission",
    bound: "Cognition History index source bound",
    producer_pool: "Cognition History index producer pool",
    page: "Cognition History index page",
    write_allowance: "Cognition History index write allowance",
};
fn descriptor(field: CognitionHistoryFieldV1) -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::Cognition,
        name: match field {
            CognitionHistoryFieldV1::JournalEvents => "history.journal.events.index.v1",
            CognitionHistoryFieldV1::Responses => "history.responses.index.v1",
            CognitionHistoryFieldV1::EconomyJournal => "history.economy.journal.index.v1",
            CognitionHistoryFieldV1::EconomyProvisionJournal => {
                "history.economy.provision_journal.index.v1"
            }
        },
        destination: FieldDestination::State(StateNamespaceV1::Artifacts),
        shape: FieldShape::ByteMap,
    }
}
/// A locator carries an untrusted formal position hint. Neither these fields
/// nor the private index proof attest to the original's semantic validity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CognitionHistoryIndexRowV1 {
    pub location: FactLocationV1,
    pub encoded_bytes: u64,
}
impl CognitionHistoryIndexRowV1 {
    fn bytes(self) -> [u8; ROW_BYTES] {
        let mut out = [0; ROW_BYTES];
        out[..32].copy_from_slice(&self.location.original);
        out[32..40].copy_from_slice(&self.encoded_bytes.to_be_bytes());
        out[40..48].copy_from_slice(&self.location.height.to_be_bytes());
        out[48..56].copy_from_slice(&self.location.ordinal.to_be_bytes());
        out
    }
    fn decode(bytes: &[u8; ROW_BYTES]) -> Self {
        Self {
            location: FactLocationV1 {
                original: bytes[..32].try_into().expect("fixed row"),
                height: u64::from_be_bytes(bytes[40..48].try_into().expect("fixed row")),
                ordinal: u64::from_be_bytes(bytes[48..56].try_into().expect("fixed row")),
            },
            encoded_bytes: u64::from_be_bytes(bytes[32..40].try_into().expect("fixed row")),
        }
    }
}
pub struct CognitionHistoryIndexWorkV1 {
    pub cells_remaining: u64,
    pub max_pending_node_bytes: usize,
}
impl CognitionHistoryIndexWorkV1 {
    fn cell(&mut self) -> Result<(), ValueTreeError> {
        self.cells_remaining =
            self.cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "Cognition History index cells",
                ))?;
        Ok(())
    }
}
/// Build one explicit complete view, streaming rows in original view order.
/// This returns an unqualified root and does not append a formal fact.
pub fn prepare_cognition_history_index_v1<W, C, I>(
    field: CognitionHistoryFieldV1,
    count: u64,
    rows: I,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut CognitionHistoryIndexWorkV1,
) -> Result<StateRootV1, MemberRecordError>
where
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    I: IntoIterator<Item = Result<CognitionHistoryIndexRowV1, ValueTreeError>>,
{
    let _scratch = context
        .memory
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    count.checked_add(2).ok_or(MemberRecordError::Invalid(
        "Cognition History index count overflow",
    ))?;
    let mut root = StateRootV1::empty(StateNamespaceV1::Artifacts)
        .map_err(|_| MemberRecordError::Invalid("Cognition History index namespace"))?;
    let policy = tree::RecordTreeWritePolicyV1 {
        max_pending_node_bytes: work.max_pending_node_bytes,
        labels: LABELS,
    };
    work.cell()?;
    let meta = write_field_metadata_v1(
        descriptor(field),
        &mut PresenceRecordWriteContext {
            sink: &mut *context.sink,
            memory: context.memory,
            io: &mut *context.io,
            key_work: &mut *context.key_work,
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: context.limits.semantic_bytes,
                record_bytes: context.limits.record_bytes,
            },
            cancelled: &mut context.cancelled,
        },
    )?;
    root = tree::append(root, &meta.field_key, meta.value_root, context, policy)?;
    work.cell()?;
    let control = write_member_record_v1(
        descriptor(field),
        WorldFieldRecordKindV1::Entry,
        &(),
        MemberPayloadRefV1::U64(count),
        context,
    )?;
    root = tree::append(
        root,
        &control.field_key,
        control.value_root,
        context,
        policy,
    )?;
    let mut emitted = 0u64;
    for row in rows {
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        work.cell()?;
        if emitted >= count {
            return Err(MemberRecordError::Invalid(
                "Cognition History index excess row",
            ));
        }
        let bytes = row?.bytes();
        let row = write_member_record_v1(
            descriptor(field),
            WorldFieldRecordKindV1::Bytes,
            &emitted,
            MemberPayloadRefV1::Bytes(&bytes),
            context,
        )?;
        root = tree::append(root, &row.field_key, row.value_root, context, policy)?;
        emitted += 1;
    }
    if emitted != count {
        return Err(MemberRecordError::Invalid(
            "Cognition History index missing row",
        ));
    }
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(root)
}
#[derive(Debug)]
pub struct CognitionHistoryIndexQualificationV1 {
    root: StateRootV1,
    field: CognitionHistoryFieldV1,
    count: u64,
    class: MemoryClass,
}
impl CognitionHistoryIndexQualificationV1 {
    pub(crate) fn class(&self) -> MemoryClass {
        self.class
    }
    pub fn root(&self) -> &StateRootV1 {
        &self.root
    }
    pub fn field(&self) -> CognitionHistoryFieldV1 {
        self.field
    }
    pub fn validate_binding(
        &self,
        field: CognitionHistoryFieldV1,
        index: &CognitionHistoryIndexRefV1,
    ) -> Result<(), ValueTreeError> {
        if index.present
            && self.field == field
            && self.root == index.root
            && self.count == index.records
        {
            Ok(())
        } else {
            Err(ValueTreeError::Invalid(
                "Cognition History index proof binding",
            ))
        }
    }
    pub fn record_count(&self) -> u64 {
        self.count
    }
}

/// An actual authenticated point lookup, not a caller-supplied locator. This
/// closes only the row's relationship to this private index qualification.
pub struct CognitionHistoryIndexPointV1 {
    pub(super) root: StateRootV1,
    pub(super) field: CognitionHistoryFieldV1,
    pub(super) ordinal: u64,
    pub(super) row: CognitionHistoryIndexRowV1,
    pub(super) class: MemoryClass,
}
impl CognitionHistoryIndexPointV1 {
    pub fn row(&self) -> CognitionHistoryIndexRowV1 {
        self.row
    }
    pub fn ordinal(&self) -> u64 {
        self.ordinal
    }
}
/// Read one original ordinal without scanning another row or original.
pub fn read_cognition_history_index_point_v1<M, R, E, C>(
    parent: &ReservedValue<CognitionHistoryIndexQualificationV1>,
    ordinal: u64,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut CognitionHistoryIndexWorkV1,
) -> Result<ReservedValue<CognitionHistoryIndexPointV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    context.check()?;
    if !parent.uses_budget(context.values.memory)
        || parent.reserved_bytes() < 16 << 10
        || parent.class != context.values.class
        || ordinal >= parent.count
    {
        return Err(ValueTreeError::Invalid(
            "Cognition History index point parent",
        ));
    }
    read_cognition_history_index_point_parts_v1(&parent.root, parent.field, ordinal, context, work)
}
pub(super) fn read_cognition_history_index_point_parts_v1<M, R, E, C>(
    root: &StateRootV1,
    field: CognitionHistoryFieldV1,
    ordinal: u64,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut CognitionHistoryIndexWorkV1,
) -> Result<ReservedValue<CognitionHistoryIndexPointV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    context.check()?;
    let charge = context
        .values
        .memory
        .reserve(16 << 10, context.values.class)?;
    work.cell()?;
    let mut records = context.records();
    let (key, semantic) =
        records.key(descriptor(field), WorldFieldRecordKindV1::Bytes, &ordinal)?;
    let value = tree::lookup(root, &key, &mut records, LABELS)?.ok_or(ValueTreeError::Invalid(
        "Cognition History index point missing",
    ))?;
    let mut bytes = [0; ROW_BYTES];
    let info = read_member_record_profile_v1(
        &value,
        &key,
        &mut *records.values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::Bytes,
            semantic_bytes: semantic,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: records.semantic_bytes,
                payload_bytes: ROW_BYTES as u64,
                record_bytes: records.record_bytes,
            },
        },
        &mut std::io::sink(),
        &mut &mut bytes[..],
        &mut *records.cancelled,
    )?;
    if info.payload_bytes != ROW_BYTES as u64 {
        return Err(ValueTreeError::Invalid(
            "Cognition History index point width",
        ));
    }
    let point = CognitionHistoryIndexPointV1 {
        root: root.clone(),
        field: field,
        ordinal,
        row: CognitionHistoryIndexRowV1::decode(&bytes),
        class: context.values.class,
    };
    context.check()?;
    Ok(charge.retain(point))
}
/// Copy fixed authenticated index metadata into Ordinary admission, bound to
/// an already admitted exact accepted view. No H traversal or Rec Arc sharing.
pub fn readmit_cognition_history_index_ordinary_v1(
    source: &ReservedValue<CognitionHistoryIndexQualificationV1>,
    view: &ReservedValue<AcceptedCognitionHistoryViewV1>,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CognitionHistoryIndexQualificationV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !source.uses_budget(memory)
        || source.class != MemoryClass::Recovery
        || !view.uses_budget(memory)
        || view.class() != MemoryClass::Ordinary
        || view.occurrence().allocation_class() != MemoryClass::Ordinary
    {
        return Err(ValueTreeError::Invalid(
            "Cognition History index readmission source",
        ));
    }
    let charge = memory.reserve(16 << 10, MemoryClass::Ordinary)?;
    source.validate_binding(source.field, view.manifest().index_ref(source.field))?;
    let output = CognitionHistoryIndexQualificationV1 {
        root: source.root.clone(),
        field: source.field,
        count: source.count,
        class: MemoryClass::Ordinary,
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(output))
}

/// Append exactly one original view ordinal. Only count and the new tail row
/// are read/changed; no old row or original payload is scanned. The result is
/// an index-integrity proof, not an accepted History capability. Ordinary
/// callers must run this provisional producer inside transaction.contribute
/// (or the consuming final-fact seal callback) so errors discard that attempt.
pub fn prepare_cognition_history_index_append_v1<M, R, E, C, W, CW>(
    parent: &ReservedValue<CognitionHistoryIndexQualificationV1>,
    row: CognitionHistoryIndexRowV1,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut CognitionHistoryIndexWorkV1,
) -> Result<ReservedValue<CognitionHistoryIndexQualificationV1>, MemberRecordError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
{
    read.check()?;
    if read.values.class != MemoryClass::Ordinary
        || parent.class != MemoryClass::Ordinary
        || !write.memory.same_pool(read.values.memory)
        || !parent.uses_budget(read.values.memory)
        || parent.reserved_bytes() < std::mem::size_of::<CognitionHistoryIndexQualificationV1>()
    {
        return Err(MemberRecordError::Invalid(
            "Cognition History append pool/class",
        ));
    }
    let retained = write
        .memory
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let (root, count) = append_cognition_history_index_parts_v3(
        &parent.root,
        parent.field,
        parent.count,
        row,
        read,
        write,
        work,
    )?;
    Ok(retained.retain(CognitionHistoryIndexQualificationV1 {
        class: MemoryClass::Ordinary,
        root,
        field: parent.field,
        count,
    }))
}
pub(super) fn append_cognition_history_index_parts_v3<M, R, E, C, W, CW>(
    root: &StateRootV1,
    field: CognitionHistoryFieldV1,
    count: u64,
    row: CognitionHistoryIndexRowV1,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut CognitionHistoryIndexWorkV1,
) -> Result<(StateRootV1, u64), MemberRecordError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
{
    read.check()?;
    if read.values.class != MemoryClass::Ordinary
        || !write.memory.same_pool(read.values.memory)
        || root.namespace != StateNamespaceV1::Artifacts
        || count.checked_add(2) != Some(root.leaf_count)
    {
        return Err(MemberRecordError::Invalid(
            "Cognition History append parts binding",
        ));
    }
    if (read.cancelled)() || (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let next = count
        .checked_add(1)
        .and_then(|count| count.checked_add(2).map(|_| count))
        .ok_or(MemberRecordError::Invalid(
            "Cognition History append count overflow",
        ))?;
    let (count_key, old_count) = {
        let mut records = read.records();
        work.cell()?;
        let (key, semantic) = records.key(descriptor(field), WorldFieldRecordKindV1::Entry, &())?;
        let id = tree::lookup(root, &key, &mut records, LABELS)?.ok_or(ValueTreeError::Invalid(
            "Cognition History append count absent",
        ))?;
        if records.unsigned(&id, &key, semantic)? != count {
            return Err(MemberRecordError::Invalid(
                "Cognition History append expected count",
            ));
        }
        work.cell()?;
        let (tail, _) = records.key(descriptor(field), WorldFieldRecordKindV1::Bytes, &count)?;
        if tree::lookup(root, &tail, &mut records, LABELS)?.is_some() {
            return Err(MemberRecordError::Invalid(
                "Cognition History append tail present",
            ));
        }
        (key, id)
    };
    let policy = tree::RecordTreeWritePolicyV1 {
        max_pending_node_bytes: work.max_pending_node_bytes,
        labels: LABELS,
    };
    work.cell()?;
    let bytes = row.bytes();
    let tail = write_member_record_v1(
        descriptor(field),
        WorldFieldRecordKindV1::Bytes,
        &count,
        MemberPayloadRefV1::Bytes(&bytes),
        write,
    )?;
    let root = tree::edit(
        root.clone(),
        &tail.field_key,
        (None, Some(tail.value_root)),
        &mut read.values,
        write,
        &mut read.cancelled,
        policy,
    )?;
    work.cell()?;
    let count = write_member_record_v1(
        descriptor(field),
        WorldFieldRecordKindV1::Entry,
        &(),
        MemberPayloadRefV1::U64(next),
        write,
    )?;
    let root = tree::edit(
        root,
        &count_key,
        (Some(old_count), Some(count.value_root)),
        &mut read.values,
        write,
        &mut read.cancelled,
        policy,
    )?;
    if root.leaf_count != next + 2 {
        return Err(MemberRecordError::Invalid(
            "Cognition History append cardinality",
        ));
    }
    if (read.cancelled)() || (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok((root, next))
}

/// Complete dense-key/index-value closure with no allocation proportional to
/// count. The observer receives provisional locators, never original proofs.
pub fn qualify_cognition_history_index_v1<M, R, E, C, F>(
    field: CognitionHistoryFieldV1,
    index: &CognitionHistoryIndexRefV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut CognitionHistoryIndexWorkV1,
    mut observe: F,
) -> Result<ReservedValue<CognitionHistoryIndexQualificationV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    F: FnMut(u64, CognitionHistoryIndexRowV1) -> Result<(), ValueTreeError>,
{
    context.check()?;
    let charge = context
        .values
        .memory
        .reserve(16 << 10, context.values.class)?;
    if !index.present
        || index.root.namespace != StateNamespaceV1::Artifacts
        || index.records.checked_add(2) != Some(index.root.leaf_count)
    {
        return Err(ValueTreeError::Invalid("Cognition History index shape"));
    }
    let mut records = context.records();
    work.cell()?;
    let meta = WorldFieldKeyV1::new(descriptor(field), WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let id = tree::lookup(&index.root, &meta, &mut records, LABELS)?
        .ok_or(ValueTreeError::Invalid("Cognition History index metadata"))?;
    records
        .values
        .verify_closure(&id, records.record_bytes, &mut *records.cancelled)?;
    records.presence(&id, &meta, 0)?;
    work.cell()?;
    let (key, semantic) = records.key(descriptor(field), WorldFieldRecordKindV1::Entry, &())?;
    let id = tree::lookup(&index.root, &key, &mut records, LABELS)?
        .ok_or(ValueTreeError::Invalid("Cognition History index count"))?;
    records
        .values
        .verify_closure(&id, records.record_bytes, &mut *records.cancelled)?;
    if records.unsigned(&id, &key, semantic)? != index.records {
        return Err(ValueTreeError::Invalid(
            "Cognition History index count value",
        ));
    }
    for ordinal in 0..index.records {
        work.cell()?;
        let (key, semantic) =
            records.key(descriptor(field), WorldFieldRecordKindV1::Bytes, &ordinal)?;
        let id = tree::lookup(&index.root, &key, &mut records, LABELS)?.ok_or(
            ValueTreeError::Invalid("Cognition History index dense ordinal"),
        )?;
        records
            .values
            .verify_closure(&id, records.record_bytes, &mut *records.cancelled)?;
        let mut bytes = [0; ROW_BYTES];
        let info = read_member_record_profile_v1(
            &id,
            &key,
            &mut *records.values,
            MemberReadProfileV1 {
                codec: MemberPayloadCodecV1::Bytes,
                semantic_bytes: semantic,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: records.semantic_bytes,
                    payload_bytes: ROW_BYTES as u64,
                    record_bytes: records.record_bytes,
                },
            },
            &mut std::io::sink(),
            &mut &mut bytes[..],
            &mut *records.cancelled,
        )?;
        if info.payload_bytes != ROW_BYTES as u64 {
            return Err(ValueTreeError::Invalid("Cognition History index row width"));
        }
        observe(ordinal, CognitionHistoryIndexRowV1::decode(&bytes))?;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
    }
    // Known distinct keys above plus the authenticated actual cardinality
    // exclude unknown leaves and forged leaf_count/semantic-path claims.
    let mut cursor =
        crate::incremental_json_records::cursor::Cursor::new(&index.root, &mut records)?;
    while cursor.next(&mut records)?.is_some() {
        work.cell()?;
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(CognitionHistoryIndexQualificationV1 {
        class: context.values.class,
        root: index.root.clone(),
        field,
        count: index.records,
    }))
}
