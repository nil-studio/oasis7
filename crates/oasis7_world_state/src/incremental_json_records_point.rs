//! Provisional scalar replacement along one authenticated JSON path. Container
//! structure is unchanged; this output is not an owner/RootSet qualification.
use super::*;
use crate::{
    incremental_expected_bytes::Compare, incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::incremental_state::MerkleNodeReader;
use std::{cell::RefCell, io::Cursor};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonPathSegmentV1<'a> {
    Name(&'a str),
    Index(u64),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonScalarRefV1<'a> {
    Null,
    Bool(bool),
    Signed(i64),
    Unsigned(u64),
    FloatBits(u64),
    Text(&'a str),
}
impl<'a> JsonScalarRefV1<'a> {
    fn label(self) -> Result<&'static str, ValueTreeError> {
        Ok(match self {
            Self::Null => "null",
            Self::Bool(_) => "bool",
            Self::Signed(v) if v < 0 => "i64",
            Self::Unsigned(_) => "u64",
            Self::FloatBits(v) if f64::from_bits(v).is_finite() => "f64",
            Self::Text(_) => "string",
            _ => return Err(ValueTreeError::Invalid("JSON scalar representation")),
        })
    }
    fn payload(self) -> MemberPayloadRefV1<'a> {
        match self {
            Self::Null => MemberPayloadRefV1::Unit,
            Self::Bool(v) => MemberPayloadRefV1::Bool(v),
            Self::Signed(v) => MemberPayloadRefV1::I64(v),
            Self::Unsigned(v) | Self::FloatBits(v) => MemberPayloadRefV1::U64(v),
            Self::Text(value) => MemberPayloadRefV1::Text(value),
        }
    }
}
#[derive(Debug)]
pub struct JsonScalarPointWriteV1 {
    before: StateRootV1,
    after: StateRootV1,
    changed: bool,
}
impl JsonScalarPointWriteV1 {
    pub fn before(&self) -> &StateRootV1 {
        &self.before
    }
    pub fn after(&self) -> &StateRootV1 {
        &self.after
    }
    pub fn changed(&self) -> bool {
        self.changed
    }
}
fn required<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    key: &WorldFieldKeyV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<[u8; 32], ValueTreeError> {
    tree::lookup(root, key, records, LABELS)?
        .ok_or(ValueTreeError::Invalid("JSON point missing cell"))
}
fn compare<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    key: &(WorldFieldKeyV1, u64),
    codec: MemberPayloadCodecV1,
    expected: &[u8],
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<[u8; 32], ValueTreeError> {
    let id = required(root, &key.0, records)?;
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: records.semantic_bytes,
        payload_bytes: records.record_bytes,
        record_bytes: records.record_bytes,
    };
    let header =
        read_member_record_header_v1(&id, &key.0, records.values, limits, &mut *records.cancelled)?;
    if header.codec != codec
        || header.semantic_bytes != key.1
        || header.payload_bytes != expected.len() as u64
    {
        return Err(ValueTreeError::Invalid("JSON point expected value"));
    }
    let mut output = Compare::new(expected);
    read_member_record_profile_v1(
        &id,
        &key.0,
        records.values,
        MemberReadProfileV1 {
            codec,
            semantic_bytes: key.1,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: records.semantic_bytes,
                payload_bytes: expected.len() as u64,
                record_bytes: records.record_bytes,
            },
        },
        &mut std::io::sink(),
        &mut output,
        &mut *records.cancelled,
    )?;
    if !output.is_equal() {
        return Err(ValueTreeError::Invalid("JSON point expected value"));
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(id)
}
fn path_key<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    segment: JsonPathSegmentV1<'_>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    match segment {
        JsonPathSegmentV1::Name(name) => records.key(
            profile.field,
            WorldFieldRecordKindV1::Entry,
            &(((), "members"), name),
        ),
        JsonPathSegmentV1::Index(index) => records.key(
            profile.field,
            WorldFieldRecordKindV1::Entry,
            &(((), "members"), index),
        ),
    }
}
fn child<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    root: &StateRootV1,
    key: &(WorldFieldKeyV1, u64),
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<([u8; 32], StateRootV1), ValueTreeError> {
    let _scratch = records.values.memory.reserve(8192, records.values.class)?;
    let id = required(root, &key.0, records)?;
    let child = child_reference_admitted(profile, &id, key, records)?;
    Ok((id, child))
}
fn child_reference<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    id: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<StateRootV1, ValueTreeError> {
    let _scratch = records.values.memory.reserve(8192, records.values.class)?;
    child_reference_admitted(profile, id, key, records)
}
fn child_reference_admitted<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    id: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<StateRootV1, ValueTreeError> {
    let mut bytes = [0u8; 165];
    let mut output = Cursor::new(&mut bytes[..]);
    read_member_record_profile_v1(
        id,
        &key.0,
        records.values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::RecordRoot,
            semantic_bytes: key.1,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: records.semantic_bytes,
                payload_bytes: 165,
                record_bytes: records.record_bytes,
            },
        },
        &mut std::io::sink(),
        &mut output,
        &mut *records.cancelled,
    )?;
    if output.position() != 165 {
        return Err(ValueTreeError::Invalid("JSON point child length"));
    }
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes)
        .map_err(|_| ValueTreeError::Invalid("JSON point child reference"))?;
    let semantic = WorldFieldKeyV1::from_bytes(key.0.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or(ValueTreeError::Invalid("JSON point child key"))?;
    let root = reference
        .validate_binding(&profile.schema, &semantic, profile.namespace()?)
        .map_err(|_| ValueTreeError::Invalid("JSON point child binding"))?
        .clone();
    Ok(root)
}
/// Caller must wrap this entire provisional operation and its outer-root update
/// in the same ExecutionTransaction contribution. No returned object authorizes
/// a commit, propagates owner qualification, or certifies a physical sink.
pub fn prepare_json_scalar_path_change_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    profile: JsonRecordProfileV1,
    parent: &oasis7_proto::memory_budget::ReservedValue<JsonSubtreeQualificationV1>,
    path: &[JsonPathSegmentV1<'_>],
    change: (JsonScalarRefV1<'_>, JsonScalarRefV1<'_>),
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut JsonWriteWorkV1,
) -> Result<oasis7_proto::memory_budget::ReservedValue<JsonScalarPointWriteV1>, MemberRecordError> {
    read.check()?;
    if (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    prepare_json_scalar_path_change_records_v1(
        profile,
        parent,
        path,
        change,
        &mut read.records(),
        write,
        work,
    )
}

pub(crate) fn prepare_json_scalar_path_change_records_v1<
    R: ValueNodeReader,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    profile: JsonRecordProfileV1,
    parent: &oasis7_proto::memory_budget::ReservedValue<JsonSubtreeQualificationV1>,
    path: &[JsonPathSegmentV1<'_>],
    change: (JsonScalarRefV1<'_>, JsonScalarRefV1<'_>),
    read: &mut InventoryValueReader<'_, '_, R, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut JsonWriteWorkV1,
) -> Result<oasis7_proto::memory_budget::ReservedValue<JsonScalarPointWriteV1>, MemberRecordError> {
    let (expected, after) = change;
    if (read.cancelled)() || (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if !parent.uses_budget(read.values.memory) {
        return Err(ValueTreeError::Invalid("JSON point parent pool").into());
    }
    parent.validate_binding(profile, parent.root())?;
    if read.values.class != MemoryClass::Ordinary || !read.values.memory.same_pool(write.memory) {
        return Err(ValueTreeError::Invalid("JSON point pool/class").into());
    }
    let retained = write
        .memory
        .reserve(1024, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let original = parent.root().clone();
    let cancels = RefCell::new((&mut read.cancelled, &mut write.cancelled));
    let mut read_cancel = || {
        let mut c = cancels.borrow_mut();
        (c.0)() || (c.1)()
    };
    let mut write_cancel = || {
        let mut c = cancels.borrow_mut();
        (c.0)() || (c.1)()
    };
    let mut records = InventoryValueReader {
        values: read.values,
        key_work: read.key_work,
        semantic_bytes: read.semantic_bytes,
        record_bytes: read.record_bytes,
        cancelled: &mut read_cancel,
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut *write.sink,
        memory: write.memory,
        io: &mut *write.io,
        key_work: &mut *write.key_work,
        limits: write.limits,
        cancelled: &mut write_cancel,
    };
    let after_label = after.label()?;
    let Resolved {
        mut root,
        mut parents,
        label,
        label_id,
        value,
        value_id,
        expected_label,
        _admission,
    } = resolve_expected(
        profile,
        parent.root(),
        path,
        expected,
        &mut records,
        PathWork {
            cells: &mut work.cells_remaining,
            nodes: &mut work.json_nodes_remaining,
            max_depth: work.max_json_depth,
        },
    )?;
    let unchanged = expected == after;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if unchanged {
        return Ok(retained.retain(JsonScalarPointWriteV1 {
            before: original,
            after: root_for_noop(parent),
            changed: false,
        }));
    }
    let policy = RecordTreeWritePolicyV1 {
        max_pending_node_bytes: work.max_pending_node_bytes,
        labels: LABELS,
    };
    let payload = match after {
        JsonScalarRefV1::Text(v) => MemberPayloadRefV1::Text(v),
        _ => after.payload(),
    };
    work.cell()?;
    let output = row(profile, &((), "json.value"), payload, &mut write)?;
    root = tree::edit(
        root,
        &value.0,
        (Some(value_id), Some(output.value_root)),
        records.values,
        &mut write,
        records.cancelled,
        policy,
    )?;
    if expected_label != after_label {
        work.cell()?;
        let output = row(
            profile,
            &((), "json.kind"),
            MemberPayloadRefV1::Text(after_label),
            &mut write,
        )?;
        root = tree::edit(
            root,
            &label.0,
            (Some(label_id), Some(output.value_root)),
            records.values,
            &mut write,
            records.cancelled,
            policy,
        )?;
    }
    root = rewrite_parents(profile, root, &mut parents, &mut records, &mut write, work)?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(retained.retain(JsonScalarPointWriteV1 {
        before: original,
        after: root,
        changed: true,
    }))
}
fn rewrite_parents<
    W: StatePageWriter,
    R: ValueNodeReader,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
>(
    profile: JsonRecordProfileV1,
    mut root: StateRootV1,
    parents: &mut Parents<'_>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut JsonWriteWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let policy = RecordTreeWritePolicyV1 {
        max_pending_node_bytes: work.max_pending_node_bytes,
        labels: LABELS,
    };
    while let Some((parent, key, id, segment)) = parents.pop() {
        work.cell()?;
        let output = match segment {
            JsonPathSegmentV1::Name(name) => {
                root_row(profile, &(((), "members"), name), root, write)?
            }
            JsonPathSegmentV1::Index(index) => {
                root_row(profile, &(((), "members"), index), root, write)?
            }
        };
        root = tree::edit(
            parent,
            &key,
            (Some(id), Some(output.value_root)),
            records.values,
            write,
            records.cancelled,
            policy,
        )?;
    }
    Ok(root)
}
fn root_for_noop(parent: &JsonSubtreeQualificationV1) -> StateRootV1 {
    parent.root().clone()
}

/// Read-only exact-value query. It does not mutate a transaction or mint any
/// qualification; a mismatch is Invalid, not a successful approximate match.
pub struct JsonScalarPathReadWorkV1 {
    pub cells_remaining: u64,
    pub json_nodes_remaining: u64,
    pub max_depth: usize,
}
struct PathWork<'a> {
    cells: &'a mut u64,
    nodes: &'a mut u64,
    max_depth: usize,
}
impl PathWork<'_> {
    fn cell(&mut self) -> Result<(), MemberRecordError> {
        *self.cells = self
            .cells
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("JSON path cells"))?;
        Ok(())
    }
    fn node(&mut self) -> Result<(), MemberRecordError> {
        *self.nodes = self
            .nodes
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("JSON path nodes"))?;
        Ok(())
    }
}
type Parents<'a> = Vec<(
    StateRootV1,
    WorldFieldKeyV1,
    [u8; 32],
    JsonPathSegmentV1<'a>,
)>;
struct ResolvedPath<'a> {
    root: StateRootV1,
    parents: Parents<'a>,
    _admission: oasis7_proto::memory_budget::MemoryReservation,
}
impl<'a> ResolvedPath<'a> {
    fn new(
        parent: &StateRootV1,
        steps: usize,
        max: usize,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        class: MemoryClass,
    ) -> Result<Self, MemberRecordError> {
        if steps > max {
            return Err(ValueTreeError::ResourceLimited("JSON point depth").into());
        }
        let bytes = steps
            .checked_add(1)
            .and_then(|n| n.checked_mul(1024))
            .ok_or(ValueTreeError::ResourceLimited("JSON point path admission"))?;
        let admission = memory
            .reserve(bytes, class)
            .map_err(ValueTreeError::Memory)?;
        let parents = Vec::with_capacity(steps);
        let root = parent.clone();
        Ok(Self {
            root,
            parents,
            _admission: admission,
        })
    }
    fn walk<R: ValueNodeReader, C: FnMut() -> bool>(
        &mut self,
        profile: JsonRecordProfileV1,
        path: &[JsonPathSegmentV1<'a>],
        records: &mut InventoryValueReader<'_, '_, R, C>,
        work: &mut PathWork<'_>,
    ) -> Result<(), MemberRecordError> {
        for segment in path {
            work.node()?;
            work.cell()?;
            let key = path_key(profile, *segment, records)?;
            let (id, next) = child(profile, &self.root, &key, records)?;
            let root = std::mem::replace(&mut self.root, next);
            self.parents.push((root, key.0, id, *segment));
        }
        Ok(())
    }
}
struct Resolved<'a> {
    root: StateRootV1,
    parents: Vec<(
        StateRootV1,
        WorldFieldKeyV1,
        [u8; 32],
        JsonPathSegmentV1<'a>,
    )>,
    label: (WorldFieldKeyV1, u64),
    label_id: [u8; 32],
    value: (WorldFieldKeyV1, u64),
    value_id: [u8; 32],
    expected_label: &'static str,
    _admission: oasis7_proto::memory_budget::MemoryReservation,
}
fn resolve_expected<'a, R: ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    parent: &StateRootV1,
    path: &[JsonPathSegmentV1<'a>],
    expected: JsonScalarRefV1<'_>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    mut work: PathWork<'_>,
) -> Result<Resolved<'a>, MemberRecordError> {
    let mut resolved = ResolvedPath::new(
        parent,
        path.len(),
        work.max_depth,
        records.values.memory,
        records.values.class,
    )?;
    let expected_label = expected.label()?;
    resolved.walk(profile, path, records, &mut work)?;
    let ResolvedPath {
        root,
        parents,
        _admission: admission,
    } = resolved;
    work.node()?;
    work.cell()?;
    let label = records.key(
        profile.field,
        WorldFieldRecordKindV1::Entry,
        &((), "json.kind"),
    )?;
    let label_id = compare(
        &root,
        &label,
        MemberPayloadCodecV1::Text,
        expected_label.as_bytes(),
        records,
    )?;
    work.cell()?;
    let value = records.key(
        profile.field,
        WorldFieldRecordKindV1::Entry,
        &((), "json.value"),
    )?;
    let fixed;
    let (codec, bytes): (_, &[u8]) = match expected {
        JsonScalarRefV1::Null => (MemberPayloadCodecV1::Unit, &[]),
        JsonScalarRefV1::Bool(v) => {
            fixed = [u8::from(v); 8];
            (MemberPayloadCodecV1::Bool, &fixed[..1])
        }
        JsonScalarRefV1::Signed(v) => {
            fixed = v.to_be_bytes();
            (MemberPayloadCodecV1::I64, &fixed)
        }
        JsonScalarRefV1::Unsigned(v) | JsonScalarRefV1::FloatBits(v) => {
            fixed = v.to_be_bytes();
            (MemberPayloadCodecV1::U64, &fixed)
        }
        JsonScalarRefV1::Text(v) => (MemberPayloadCodecV1::Text, v.as_bytes()),
    };
    let value_id = compare(&root, &value, codec, bytes, records)?;
    Ok(Resolved {
        root,
        parents,
        label,
        label_id,
        value,
        value_id,
        expected_label,
        _admission: admission,
    })
}
pub fn check_json_scalar_path_expected_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: JsonRecordProfileV1,
    parent: &oasis7_proto::memory_budget::ReservedValue<JsonSubtreeQualificationV1>,
    path: &[JsonPathSegmentV1<'_>],
    expected: JsonScalarRefV1<'_>,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut JsonScalarPathReadWorkV1,
) -> Result<(), MemberRecordError> {
    context.check()?;
    check_json_scalar_path_expected_records_v1(
        profile,
        parent,
        path,
        expected,
        &mut context.records(),
        work,
    )
}
pub(crate) fn check_json_scalar_path_expected_records_v1<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    parent: &oasis7_proto::memory_budget::ReservedValue<JsonSubtreeQualificationV1>,
    path: &[JsonPathSegmentV1<'_>],
    expected: JsonScalarRefV1<'_>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut JsonScalarPathReadWorkV1,
) -> Result<(), MemberRecordError> {
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if !parent.uses_budget(records.values.memory) {
        return Err(ValueTreeError::Invalid("JSON point parent pool").into());
    }
    parent.validate_binding(profile, parent.root())?;
    resolve_expected(
        profile,
        parent.root(),
        path,
        expected,
        records,
        PathWork {
            cells: &mut work.cells_remaining,
            nodes: &mut work.json_nodes_remaining,
            max_depth: work.max_depth,
        },
    )?;
    Ok(())
}

/// Whole-node replacement is explicit; member edits never compare a whole DTO.
pub enum JsonSubtreeEditV1<'a> {
    ObjectMember {
        name: &'a str,
        expected: Option<&'a StateRootV1>,
        after: Option<&'a serde_json::Value>,
    },
    Append {
        expected_len: u64,
        values: &'a [serde_json::Value],
    },
    Truncate {
        expected_len: u64,
        len: u64,
    },
    Replace {
        expected: &'a StateRootV1,
        after: &'a serde_json::Value,
    },
}
pub type JsonPointWriteV1 = JsonScalarPointWriteV1;
/// Derive only the requested descendant from an already closed graph. This
/// inherits closure, not a RootSet/owner permission; a nested count is a bound.
pub fn qualify_json_path_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: JsonRecordProfileV1,
    parent: &oasis7_proto::memory_budget::ReservedValue<JsonSubtreeQualificationV1>,
    path: &[JsonPathSegmentV1<'_>],
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut JsonScalarPathReadWorkV1,
) -> Result<oasis7_proto::memory_budget::ReservedValue<JsonSubtreeQualificationV1>, MemberRecordError>
{
    context.check()?;
    qualify_json_path_records_v1(profile, parent, path, &mut context.records(), work)
}
pub(crate) fn qualify_json_path_records_v1<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    parent: &oasis7_proto::memory_budget::ReservedValue<JsonSubtreeQualificationV1>,
    path: &[JsonPathSegmentV1<'_>],
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut JsonScalarPathReadWorkV1,
) -> Result<oasis7_proto::memory_budget::ReservedValue<JsonSubtreeQualificationV1>, MemberRecordError>
{
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if !parent.uses_budget(records.values.memory) {
        return Err(ValueTreeError::Invalid("JSON path parent pool").into());
    }
    parent.validate_binding(profile, parent.root())?;
    let retained = records
        .values
        .memory
        .reserve(1024, records.values.class)
        .map_err(ValueTreeError::Memory)?;
    let mut resolved = ResolvedPath::new(
        parent.root(),
        path.len(),
        work.max_depth,
        records.values.memory,
        records.values.class,
    )?;
    let mut path_work = PathWork {
        cells: &mut work.cells_remaining,
        nodes: &mut work.json_nodes_remaining,
        max_depth: work.max_depth,
    };
    resolved.walk(profile, path, records, &mut path_work)?;
    path_work.node()?;
    let output = retained.retain(JsonSubtreeQualificationV1 {
        root: resolved.root,
        profile,
        nodes: parent.node_count_upper_bound(),
        exact: path.is_empty() && parent.exact,
    });
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(output)
}
pub fn prepare_json_subtree_change_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
>(
    profile: JsonRecordProfileV1,
    parent: &oasis7_proto::memory_budget::ReservedValue<JsonSubtreeQualificationV1>,
    path: &[JsonPathSegmentV1<'_>],
    edit: JsonSubtreeEditV1<'_>,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut JsonWriteWorkV1,
) -> Result<oasis7_proto::memory_budget::ReservedValue<JsonPointWriteV1>, MemberRecordError> {
    context.check()?;
    prepare_json_subtree_change_records_v1(
        profile,
        parent,
        path,
        edit,
        &mut context.records(),
        write,
        work,
    )
}
pub(crate) fn prepare_json_subtree_change_records_v1<
    R: ValueNodeReader,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
>(
    profile: JsonRecordProfileV1,
    parent: &oasis7_proto::memory_budget::ReservedValue<JsonSubtreeQualificationV1>,
    path: &[JsonPathSegmentV1<'_>],
    edit: JsonSubtreeEditV1<'_>,
    read: &mut InventoryValueReader<'_, '_, R, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut JsonWriteWorkV1,
) -> Result<oasis7_proto::memory_budget::ReservedValue<JsonPointWriteV1>, MemberRecordError> {
    if (read.cancelled)() || (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if read.values.class != MemoryClass::Ordinary
        || !parent.uses_budget(read.values.memory)
        || !read.values.memory.same_pool(write.memory)
    {
        return Err(ValueTreeError::Invalid("JSON subtree point pool/class").into());
    }
    parent.validate_binding(profile, parent.root())?;
    let retained = write
        .memory
        .reserve(1024, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let original = parent.root().clone();
    let cancels = RefCell::new((&mut read.cancelled, &mut write.cancelled));
    let mut read_cancel = || {
        let mut c = cancels.borrow_mut();
        (c.0)() || (c.1)()
    };
    let mut write_cancel = || {
        let mut c = cancels.borrow_mut();
        (c.0)() || (c.1)()
    };
    let mut records = InventoryValueReader {
        values: read.values,
        key_work: read.key_work,
        semantic_bytes: read.semantic_bytes,
        record_bytes: read.record_bytes,
        cancelled: &mut read_cancel,
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut *write.sink,
        memory: write.memory,
        io: &mut *write.io,
        key_work: &mut *write.key_work,
        limits: write.limits,
        cancelled: &mut write_cancel,
    };
    let mut resolved = ResolvedPath::new(
        parent.root(),
        path.len(),
        work.max_json_depth,
        records.values.memory,
        records.values.class,
    )?;
    resolved.walk(
        profile,
        path,
        &mut records,
        &mut PathWork {
            cells: &mut work.cells_remaining,
            nodes: &mut work.json_nodes_remaining,
            max_depth: work.max_json_depth,
        },
    )?;
    let policy = RecordTreeWritePolicyV1 {
        max_pending_node_bytes: work.max_pending_node_bytes,
        labels: LABELS,
    };
    let before = resolved.root.clone();
    let mut root = resolved.root;
    match edit {
        JsonSubtreeEditV1::Replace { expected, after } => {
            if expected != &root {
                return Err(ValueTreeError::Invalid("JSON subtree expected root").into());
            }
            root = prepare_at(profile, after, path.len(), &mut write, work)?;
        }
        JsonSubtreeEditV1::ObjectMember {
            name,
            expected,
            after,
        } => {
            let old_len = container_len(profile, &root, false, &mut records, work)?;
            let key = path_key(profile, JsonPathSegmentV1::Name(name), &mut records)?;
            let old = tree::lookup(&root, &key.0, &mut records, LABELS)?;
            let old_child = old
                .map(|id| child_reference(profile, &id, &key, &mut records))
                .transpose()?;
            if old_child.as_ref() != expected {
                return Err(ValueTreeError::Invalid("JSON member expected root").into());
            }
            let new_child = after
                .map(|value| {
                    prepare_at(
                        profile,
                        value,
                        path.len()
                            .checked_add(1)
                            .ok_or(ValueTreeError::ResourceLimited("JSON subtree depth"))?,
                        &mut write,
                        work,
                    )
                })
                .transpose()?;
            if new_child != old_child {
                let value = if let Some(child) = new_child {
                    work.cell()?;
                    Some(root_row(profile, &(((), "members"), name), child, &mut write)?.value_root)
                } else {
                    None
                };
                work.cell()?;
                root = tree::edit(
                    root,
                    &key.0,
                    (old, value),
                    records.values,
                    &mut write,
                    records.cancelled,
                    policy,
                )?;
                let new_len = match (old.is_some(), value.is_some()) {
                    (false, true) => old_len.checked_add(1),
                    (true, false) => old_len.checked_sub(1),
                    _ => Some(old_len),
                }
                .ok_or(ValueTreeError::Invalid("JSON member count"))?;
                if new_len != old_len {
                    root = write_len(
                        profile,
                        root,
                        old_len,
                        new_len,
                        &mut records,
                        &mut write,
                        work,
                    )?;
                }
            }
        }
        JsonSubtreeEditV1::Append {
            expected_len,
            values,
        } => {
            let len = container_len(profile, &root, true, &mut records, work)?;
            if len != expected_len {
                return Err(ValueTreeError::Invalid("JSON array expected length").into());
            }
            let new_len = len
                .checked_add(
                    u64::try_from(values.len())
                        .map_err(|_| ValueTreeError::ResourceLimited("JSON append length"))?,
                )
                .ok_or(ValueTreeError::ResourceLimited("JSON append length"))?;
            for (offset, value) in values.iter().enumerate() {
                let index = len
                    .checked_add(offset as u64)
                    .ok_or(ValueTreeError::ResourceLimited("JSON append index"))?;
                let child = prepare_at(
                    profile,
                    value,
                    path.len()
                        .checked_add(1)
                        .ok_or(ValueTreeError::ResourceLimited("JSON subtree depth"))?,
                    &mut write,
                    work,
                )?;
                work.cell()?;
                let row = root_row(profile, &(((), "members"), index), child, &mut write)?;
                root = tree::edit(
                    root,
                    &row.field_key,
                    (None, Some(row.value_root)),
                    records.values,
                    &mut write,
                    records.cancelled,
                    policy,
                )?;
            }
            if new_len != len {
                root = write_len(profile, root, len, new_len, &mut records, &mut write, work)?;
            }
        }
        JsonSubtreeEditV1::Truncate { expected_len, len } => {
            let old = container_len(profile, &root, true, &mut records, work)?;
            if old != expected_len || len > old {
                return Err(ValueTreeError::Invalid("JSON truncate expected length").into());
            }
            for index in len..old {
                work.cell()?;
                let key = path_key(profile, JsonPathSegmentV1::Index(index), &mut records)?;
                let id = required(&root, &key.0, &mut records)?;
                root = tree::edit(
                    root,
                    &key.0,
                    (Some(id), None),
                    records.values,
                    &mut write,
                    records.cancelled,
                    policy,
                )?;
            }
            if len != old {
                root = write_len(profile, root, old, len, &mut records, &mut write, work)?;
            }
        }
    }
    if root != before {
        root = rewrite_parents(
            profile,
            root,
            &mut resolved.parents,
            &mut records,
            &mut write,
            work,
        )?;
    } else {
        root = original.clone();
    }
    let changed = root != original;
    let output = retained.retain(JsonPointWriteV1 {
        before: original,
        after: root,
        changed,
    });
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(output)
}
fn prepare_at<W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    value: &serde_json::Value,
    depth: usize,
    write: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut JsonWriteWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let max = work.max_json_depth;
    work.max_json_depth = max
        .checked_sub(depth)
        .ok_or(ValueTreeError::ResourceLimited("JSON subtree depth"))?;
    let result = super::prepare_json_subtree_v1(profile, value, write, work);
    work.max_json_depth = max;
    result
}
fn container_len<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    root: &StateRootV1,
    array: bool,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut JsonWriteWorkV1,
) -> Result<u64, MemberRecordError> {
    work.cell()?;
    work.json_node()?;
    let key = records.key(
        profile.field,
        WorldFieldRecordKindV1::Entry,
        &((), "json.kind"),
    )?;
    compare(
        root,
        &key,
        MemberPayloadCodecV1::Text,
        if array { b"array" } else { b"object" },
        records,
    )?;
    root.leaf_count
        .checked_sub(5)
        .ok_or(MemberRecordError::Invalid("JSON container count"))
}
fn write_len<W: StatePageWriter, R: ValueNodeReader, C: FnMut() -> bool, CW: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    root: StateRootV1,
    old: u64,
    new: u64,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut JsonWriteWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    work.cell()?;
    let key = records.key(
        profile.field,
        WorldFieldRecordKindV1::Entry,
        &((), "json.length"),
    )?;
    let before = compare(
        &root,
        &key,
        MemberPayloadCodecV1::U64,
        &old.to_be_bytes(),
        records,
    )?;
    let row = row(
        profile,
        &((), "json.length"),
        MemberPayloadRefV1::U64(new),
        write,
    )?;
    Ok(tree::edit(
        root,
        &key.0,
        (Some(before), Some(row.value_root)),
        records.values,
        write,
        records.cancelled,
        RecordTreeWritePolicyV1 {
            max_pending_node_bytes: work.max_pending_node_bytes,
            labels: LABELS,
        },
    )?)
}
