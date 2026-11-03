//! Complete immutable JSON-body scope. This is not a Runtime/World receipt.
use super::*;
use crate::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_value_read::{
        RecordReadLabelsV1, RecordReadProfileV1, RecordValueV1, read_record_value_v1,
    },
    incremental_semantic_key::SEMANTIC_KEY_DOMAIN,
    incremental_utf8_output::Utf8Output,
};
use oasis7_formal_protocol::incremental_state::MerkleNodeReader;
use oasis7_proto::memory_budget::ReservedValue;
use std::io::{Cursor as Bytes, Write};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("effect JSON closed profile")
}
use super::cursor;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Null,
    Bool,
    I64,
    U64,
    F64,
    String,
    Array,
    Object,
}
impl Kind {
    pub(super) fn container(self) -> bool {
        matches!(self, Self::Array | Self::Object)
    }
}
pub struct JsonReadWorkV1 {
    pub cells_remaining: u64,
    pub json_nodes_remaining: u64,
    pub max_depth: usize,
    pub max_payload_bytes: u64,
}
impl JsonReadWorkV1 {
    fn cell(&mut self) -> Result<(), ValueTreeError> {
        self.cells_remaining = self
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("effect JSON closure cells"))?;
        Ok(())
    }
}
#[derive(Debug)]
pub struct JsonSubtreeQualificationV1 {
    pub(super) root: StateRootV1,
    pub(super) profile: JsonRecordProfileV1,
    pub(super) nodes: u64,
    pub(super) exact: bool,
}
impl JsonSubtreeQualificationV1 {
    pub fn root(&self) -> &StateRootV1 {
        &self.root
    }
    pub fn validate_binding(
        &self,
        profile: JsonRecordProfileV1,
        root: &StateRootV1,
    ) -> Result<(), ValueTreeError> {
        if self.profile == profile && &self.root == root {
            Ok(())
        } else {
            Err(invalid())
        }
    }
    /// Diagnostic bound, never an allowance to reset or skip actual work.
    pub fn node_count_upper_bound(&self) -> u64 {
        self.nodes
    }
    /// Raw qualification counts exactly; changed paths may retain only a bound.
    pub fn exact_node_count(&self) -> Option<u64> {
        self.exact.then_some(self.nodes)
    }
}
const READ_LABELS: RecordReadLabelsV1 = RecordReadLabelsV1 {
    utf8: "effect JSON UTF8",
    ledger_utf8: "effect JSON unused ledger",
    ledger_frame: "effect JSON unused ledger frame",
    boolean: "effect JSON bool",
    fixed: "effect JSON fixed",
    profile: "effect JSON profile",
    integer: "effect JSON integer",
};
fn value<R: ValueNodeReader, C: FnMut() -> bool>(
    id: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    codec: MemberPayloadCodecV1,
    max: u64,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    output: &mut impl Write,
) -> Result<RecordValueV1, ValueTreeError> {
    records
        .values
        .verify_closure(id, records.record_bytes, &mut *records.cancelled)?;
    read_record_value_v1(
        id,
        key,
        records,
        max,
        output,
        RecordReadProfileV1 {
            codec,
            nullable: false,
            integer_max: u64::MAX,
            labels: READ_LABELS,
        },
    )
}
fn cell_key<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    name: &str,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    records.key(profile.field, WorldFieldRecordKindV1::Entry, &((), name))
}
fn get<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    key: &WorldFieldKeyV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<[u8; 32], ValueTreeError> {
    tree::lookup(root, key, records, LABELS)?.ok_or_else(invalid)
}
pub(super) struct Frame {
    pub(super) cursor: cursor::Cursor,
    pub(super) keys: [WorldFieldKeyV1; 5],
    pub(super) ids: [[u8; 32]; 5],
    pub(super) kind: Kind,
    pub(super) length: u64,
    members: u64,
}
pub(super) fn kind<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    root: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<Kind, ValueTreeError> {
    if root.namespace != profile.namespace()? {
        return Err(invalid());
    }
    let key = cell_key(profile, "json.kind", records)?;
    let id = get(root, &key.0, records)?;
    kind_value(&id, &key, records)
}
fn kind_value<R: ValueNodeReader, C: FnMut() -> bool>(
    id: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<Kind, ValueTreeError> {
    let mut text = [0; 8];
    let observed = value(
        id,
        key,
        MemberPayloadCodecV1::Text,
        8,
        records,
        &mut Bytes::new(text.as_mut_slice()),
    )?;
    let RecordValueV1::Text { utf8_bytes } = observed else {
        return Err(invalid());
    };
    let node = match text.get(..utf8_bytes as usize).ok_or_else(invalid)? {
        b"null" => Kind::Null,
        b"bool" => Kind::Bool,
        b"i64" => Kind::I64,
        b"u64" => Kind::U64,
        b"f64" => Kind::F64,
        b"string" => Kind::String,
        b"array" => Kind::Array,
        b"object" => Kind::Object,
        _ => return Err(invalid()),
    };
    Ok(node)
}
pub(super) fn frame<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    root: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut JsonReadWorkV1,
) -> Result<Frame, ValueTreeError> {
    if root.namespace != profile.namespace()? {
        return Err(invalid());
    }
    work.json_nodes_remaining = work
        .json_nodes_remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("effect JSON closure nodes"))?;
    let cursor = cursor::Cursor::new(root, records)?;
    let meta = WorldFieldKeyV1::new(profile.field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let marker = records.key(profile.field, WorldFieldRecordKindV1::Entry, &())?;
    let label = cell_key(profile, "json.kind", records)?;
    let payload = cell_key(profile, "json.value", records)?;
    let length = cell_key(profile, "json.length", records)?;
    let ids = [
        get(root, &meta, records)?,
        get(root, &marker.0, records)?,
        get(root, &label.0, records)?,
        get(root, &payload.0, records)?,
        get(root, &length.0, records)?,
    ];
    records
        .values
        .verify_closure(&ids[0], records.record_bytes, &mut *records.cancelled)?;
    records.presence(&ids[0], &meta, 0)?;
    value(
        &ids[1],
        &marker,
        MemberPayloadCodecV1::Unit,
        0,
        records,
        &mut std::io::sink(),
    )?;
    let node = kind_value(&ids[2], &label, records)?;
    let codec = match node {
        Kind::Null | Kind::Array | Kind::Object => MemberPayloadCodecV1::Unit,
        Kind::Bool => MemberPayloadCodecV1::Bool,
        Kind::I64 => MemberPayloadCodecV1::I64,
        Kind::U64 | Kind::F64 => MemberPayloadCodecV1::U64,
        Kind::String => MemberPayloadCodecV1::Text,
    };
    let observed = value(
        &ids[3],
        &payload,
        codec,
        work.max_payload_bytes,
        records,
        &mut std::io::sink(),
    )?;
    if matches!((node,observed),(Kind::I64,RecordValueV1::Signed(v)) if v>=0)
        || matches!((node,observed),(Kind::F64,RecordValueV1::Unsigned(v)) if !f64::from_bits(v).is_finite())
    {
        return Err(invalid());
    }
    let count = if node.container() {
        match value(
            &ids[4],
            &length,
            MemberPayloadCodecV1::U64,
            8,
            records,
            &mut std::io::sink(),
        )? {
            RecordValueV1::Unsigned(v) => v,
            _ => return Err(invalid()),
        }
    } else {
        value(
            &ids[4],
            &length,
            MemberPayloadCodecV1::Unit,
            0,
            records,
            &mut std::io::sink(),
        )?;
        0
    };
    if root.leaf_count != count.checked_add(5).ok_or_else(invalid)? {
        return Err(invalid());
    }
    Ok(Frame {
        cursor,
        keys: [meta, marker.0, label.0, payload.0, length.0],
        ids,
        kind: node,
        length: count,
        members: 0,
    })
}
const PREFIX: usize = SEMANTIC_KEY_DOMAIN.len() + 4 + 8 + 7 + 9;
struct Probe<'a> {
    header: [u8; PREFIX],
    used: usize,
    remaining: Option<u64>,
    index: bool,
    text: Utf8Output<'a, std::io::Sink>,
    bad: bool,
}
impl<'a> Probe<'a> {
    fn new(index: bool, sink: &'a mut std::io::Sink) -> Self {
        Self {
            header: [0; PREFIX],
            used: 0,
            remaining: None,
            index,
            text: Utf8Output::new(sink, "effect JSON member key UTF8"),
            bad: false,
        }
    }
    fn fail(&mut self) -> std::io::Error {
        self.bad = true;
        std::io::Error::new(std::io::ErrorKind::InvalidData, "effect JSON member key")
    }
    fn head(&mut self) -> std::io::Result<()> {
        let n = SEMANTIC_KEY_DOMAIN.len();
        if &self.header[..n] != SEMANTIC_KEY_DOMAIN
            || self.header[n..n + 4] != [5, 5, 0, 1]
            || self.header[n + 4..n + 12] != 7u64.to_be_bytes()
            || &self.header[n + 12..n + 19] != b"members"
            || self.header[n + 19] != if self.index { 2 } else { 1 }
        {
            return Err(self.fail());
        }
        self.remaining = Some(if self.index {
            0
        } else {
            u64::from_be_bytes(self.header[PREFIX - 8..].try_into().unwrap())
        });
        Ok(())
    }
    fn finish(&self) -> Result<Option<u64>, ValueTreeError> {
        if self.bad
            || self.used != PREFIX
            || self.remaining != Some(0)
            || !self.text.is_complete()
            || self.text.is_invalid()
        {
            return Err(invalid());
        }
        Ok(self
            .index
            .then(|| u64::from_be_bytes(self.header[PREFIX - 8..].try_into().unwrap())))
    }
}
impl Write for Probe<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.bad {
            return Err(self.fail());
        }
        if self.used < PREFIX {
            let n = bytes.len().min(PREFIX - self.used);
            self.header[self.used..self.used + n].copy_from_slice(&bytes[..n]);
            self.used += n;
            if self.used == PREFIX {
                self.head()?;
            }
            return Ok(n);
        }
        let remaining = self.remaining.unwrap();
        if remaining == 0 && !bytes.is_empty() {
            return Err(self.fail());
        }
        let n = (remaining as u128).min(bytes.len() as u128) as usize;
        let n = self.text.write(&bytes[..n])?;
        self.remaining = Some(remaining - n as u64);
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.text.flush()
    }
}
pub(super) fn child<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    entry: &oasis7_formal_protocol::incremental_state::StateEntryV1,
    index: bool,
    length: u64,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<StateRootV1, ValueTreeError> {
    child_with_ordinal(profile, entry, index, length, records).map(|(root, _)| root)
}
pub(super) fn child_with_ordinal<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    entry: &oasis7_formal_protocol::incremental_state::StateEntryV1,
    index: bool,
    length: u64,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(StateRootV1, Option<u64>), ValueTreeError> {
    let _scratch = records.values.memory.reserve(8192, records.values.class)?;
    let binding = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    if binding.owner != profile.field.owner
        || binding.name != profile.field.name
        || binding.shape != profile.field.shape
        || binding.kind != WorldFieldRecordKindV1::Entry
    {
        return Err(invalid());
    }
    let key = WorldFieldKeyV1::new(profile.field, binding.kind, binding.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if entry.namespace != profile.namespace()? || entry.key != key.as_bytes() {
        return Err(invalid());
    }
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: records.semantic_bytes,
        payload_bytes: 165,
        record_bytes: records.record_bytes,
    };
    let header = read_member_record_header_v1(
        &entry.value,
        &key,
        records.values,
        limits,
        &mut *records.cancelled,
    )?;
    let mut sink = std::io::sink();
    let mut probe = Probe::new(index, &mut sink);
    let mut bytes = [0; 165];
    records
        .values
        .verify_closure(&entry.value, records.record_bytes, &mut *records.cancelled)?;
    let result = read_member_record_profile_v1(
        &entry.value,
        &key,
        records.values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::RecordRoot,
            semantic_bytes: header.semantic_bytes,
            limits,
        },
        &mut probe,
        &mut Bytes::new(bytes.as_mut_slice()),
        &mut *records.cancelled,
    );
    if probe.bad || probe.text.is_invalid() {
        return Err(invalid());
    }
    result?;
    let ordinal = probe.finish()?;
    if ordinal.is_some_and(|ordinal| ordinal >= length) {
        return Err(invalid());
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
    let root = reference
        .validate_binding(
            &profile.schema,
            &binding.semantic_key.ok_or_else(invalid)?,
            profile.namespace()?,
        )
        .map_err(|_| invalid())?
        .clone();
    Ok((root, ordinal))
}
/// Standalone complete JSON-body proof only. Family/parent RootSet binding and
/// all pending/inflight relations must be checked by their separate owner gate.
pub fn qualify_json_subtree_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: JsonRecordProfileV1,
    root: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut JsonReadWorkV1,
) -> Result<ReservedValue<JsonSubtreeQualificationV1>, ValueTreeError> {
    ctx.check()?;
    qualify_records(profile, root, &mut ctx.records(), work)
}
/// The caller establishes the phase/source context; all JSON reads and work
/// remain on the same borrowed continuous record context.
pub(crate) fn qualify_records<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    root: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut JsonReadWorkV1,
) -> Result<ReservedValue<JsonSubtreeQualificationV1>, ValueTreeError> {
    profile.namespace()?;
    let retained = records.values.memory.reserve(512, records.values.class)?;
    let capacity = work
        .max_depth
        .checked_add(1)
        .ok_or(ValueTreeError::ResourceLimited(
            "effect JSON closure depth admission",
        ))?;
    let _stack = records.values.memory.reserve(
        capacity.checked_mul(std::mem::size_of::<Frame>()).ok_or(
            ValueTreeError::ResourceLimited("effect JSON closure stack admission"),
        )?,
        records.values.class,
    )?;
    let before = work.json_nodes_remaining;
    let mut stack = Vec::with_capacity(capacity);
    stack.push(frame(profile, root, records, work)?);
    loop {
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        let current = stack.last_mut().ok_or_else(invalid)?;
        let Some(entry) = current.cursor.next(records)? else {
            if current.members != current.length {
                return Err(invalid());
            }
            stack.pop();
            if stack.is_empty() {
                break;
            }
            continue;
        };
        work.cell()?;
        if let Some(slot) = current
            .keys
            .iter()
            .position(|key| key.as_bytes() == entry.key)
        {
            if current.ids[slot] != entry.value {
                return Err(invalid());
            }
            continue;
        }
        if !current.kind.container() {
            return Err(invalid());
        }
        let child = child(
            profile,
            &entry,
            current.kind == Kind::Array,
            current.length,
            records,
        )?;
        current.members = current.members.checked_add(1).ok_or_else(invalid)?;
        if current.members > current.length {
            return Err(invalid());
        }
        if stack.len() > work.max_depth {
            return Err(ValueTreeError::ResourceLimited("effect JSON closure depth"));
        }
        stack.push(frame(profile, &child, records, work)?);
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(JsonSubtreeQualificationV1 {
        root: root.clone(),
        profile,
        nodes: before - work.json_nodes_remaining,
        exact: true,
    }))
}
