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
    let mut bytes = [0u8; 165];
    let mut output = Cursor::new(&mut bytes[..]);
    read_member_record_profile_v1(
        &id,
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
    Ok((id, root))
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
    let (expected, after) = change;
    read.check()?;
    if !parent.uses_budget(read.values.memory) {
        return Err(ValueTreeError::Invalid("JSON point parent pool").into());
    }
    parent.validate_binding(profile, parent.root())?;
    if read.values.class != MemoryClass::Ordinary || !read.values.memory.same_pool(write.memory) {
        return Err(ValueTreeError::Invalid("JSON point pool/class").into());
    }
    if path.len() > work.max_json_depth {
        return Err(ValueTreeError::ResourceLimited("JSON point depth").into());
    }
    let admitted = path
        .len()
        .checked_add(1)
        .and_then(|n| n.checked_mul(1024))
        .ok_or(ValueTreeError::ResourceLimited("JSON point path admission"))?;
    let _stack = write
        .memory
        .reserve(admitted, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let retained = write
        .memory
        .reserve(1024, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut parents = Vec::with_capacity(path.len());
    let original = parent.root().clone();
    let mut root = original.clone();
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
        values: &mut read.values,
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
    let expected_label = expected.label()?;
    let after_label = after.label()?;
    for segment in path {
        work.json_node()?;
        work.cell()?;
        let key = path_key(profile, *segment, &mut records)?;
        let (id, next) = child(profile, &root, &key, &mut records)?;
        parents.push((root, key.0, id, *segment));
        root = next;
    }
    work.json_node()?;
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
        &mut records,
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
    let value_id = compare(&root, &value, codec, bytes, &mut records)?;
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
    while let Some((parent, key, id, segment)) = parents.pop() {
        work.cell()?;
        let output = match segment {
            JsonPathSegmentV1::Name(name) => {
                root_row(profile, &(((), "members"), name), root, &mut write)?
            }
            JsonPathSegmentV1::Index(index) => {
                root_row(profile, &(((), "members"), index), root, &mut write)?
            }
        };
        root = tree::edit(
            parent,
            &key,
            (Some(id), Some(output.value_root)),
            records.values,
            &mut write,
            records.cancelled,
            policy,
        )?;
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(retained.retain(JsonScalarPointWriteV1 {
        before: original,
        after: root,
        changed: true,
    }))
}
fn root_for_noop(parent: &JsonSubtreeQualificationV1) -> StateRootV1 {
    parent.root().clone()
}
