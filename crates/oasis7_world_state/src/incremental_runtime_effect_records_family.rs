//! Complete pending/inflight structural closure, never dispatch authority.
use super::*;
use crate::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_key_read::{OriginalKeyInfo, OriginalKeyKind, OriginalSemanticKeyProbe},
    incremental_record_value_read::{
        RecordReadLabelsV1, RecordReadProfileV1, RecordValueV1, read_record_value_v1,
    },
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::{
        MerkleNodeReader, StateEntryV1, StateScanBudget, StateScanContext, StateScanError,
    },
};
use oasis7_proto::memory_budget::ReservedValue;
use std::{
    cell::RefCell,
    io::{Cursor, Write},
};
pub(super) fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("runtime effect family closure")
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeEffectQueueFrontierV1 {
    pub head: Option<u64>,
    pub tail: Option<u64>,
    pub count: u64,
    pub next_ordinal: u64,
}
#[derive(Debug)]
pub struct RuntimeEffectFamilyQualificationV1 {
    roots: [u8; 32],
    frontier: RuntimeEffectQueueFrontierV1,
    counts: [u64; 2],
}
impl RuntimeEffectFamilyQualificationV1 {
    pub fn validate_binding(&self, roots: &[u8; 32]) -> Result<(), ValueTreeError> {
        if &self.roots == roots {
            Ok(())
        } else {
            Err(invalid())
        }
    }
    pub fn pending_frontier(&self) -> RuntimeEffectQueueFrontierV1 {
        self.frontier
    }
    pub fn record_counts(&self) -> [u64; 2] {
        self.counts
    }
}
pub(super) fn mint(
    roots: [u8; 32],
    frontier: RuntimeEffectQueueFrontierV1,
    counts: [u64; 2],
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    class: MemoryClass,
) -> Result<ReservedValue<RuntimeEffectFamilyQualificationV1>, ValueTreeError> {
    Ok(memory
        .reserve(512, class)?
        .retain(RuntimeEffectFamilyQualificationV1 {
            roots,
            frontier,
            counts,
        }))
}
const READ_LABELS: RecordReadLabelsV1 = RecordReadLabelsV1 {
    utf8: "effect record UTF8",
    ledger_utf8: "effect unused ledger",
    ledger_frame: "effect unused ledger frame",
    boolean: "effect bool",
    fixed: "effect fixed",
    profile: "effect member profile",
    integer: "effect integer",
};
fn spend(work: &mut RuntimeEffectJsonWorkV1) -> Result<(), ValueTreeError> {
    work.cells_remaining = work
        .cells_remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("effect owner cells"))?;
    Ok(())
}
pub(super) fn read<R: ValueNodeReader, C: FnMut() -> bool>(
    id: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    codec: MemberPayloadCodecV1,
    nullable: bool,
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
            nullable,
            integer_max: u64::MAX,
            labels: READ_LABELS,
        },
    )
}
pub(super) fn reference<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: RuntimeEffectKindV1,
    id: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    json: bool,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    semantic: &mut impl Write,
) -> Result<StateRootV1, ValueTreeError> {
    let _scratch = records.values.memory.reserve(8192, records.values.class)?;
    let mut bytes = [0; 165];
    records
        .values
        .verify_closure(id, records.record_bytes, &mut *records.cancelled)?;
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
        semantic,
        &mut Cursor::new(bytes.as_mut_slice()),
        &mut *records.cancelled,
    )?;
    let binding = WorldFieldKeyV1::from_bytes(key.0.as_bytes()).map_err(ValueTreeError::Invalid)?;
    let root = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(root
        .validate_binding(
            &if json {
                kind.json_schema()
            } else {
                kind.schema()
            },
            &binding.semantic_key.ok_or_else(invalid)?,
            StateNamespaceV1::Effects,
        )
        .map_err(|_| invalid())?
        .clone())
}
pub(super) fn body<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    kind: RuntimeEffectKindV1,
    root: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeEffectJsonWorkV1,
) -> Result<(), ValueTreeError> {
    if root.namespace != StateNamespaceV1::Effects || root.leaf_count != 10 {
        return Err(invalid());
    }
    let json = {
        let mut records = ctx.records();
        let mut cursor = json_read::cursor::Cursor::new(root, &mut records)?;
        let meta = WorldFieldKeyV1::new(
            Space::Body.descriptor(),
            WorldFieldRecordKindV1::Metadata,
            None,
        )
        .map_err(ValueTreeError::Invalid)?;
        let mut keys = Vec::with_capacity(10);
        keys.push((meta, 0));
        keys.push(records.key(Space::Body.descriptor(), WorldFieldRecordKindV1::Entry, &())?);
        for name in [
            "birth_era",
            "intent_id",
            "kind",
            "cap_ref",
            "origin.kind",
            "origin.name",
            "origin.module_id",
            "params",
        ] {
            keys.push(records.key(
                Space::Body.descriptor(),
                WorldFieldRecordKindV1::Entry,
                &((), name),
            )?);
        }
        let mut ids = Vec::with_capacity(10);
        for (key, _) in &keys {
            ids.push(tree::lookup(root, key, &mut records, LABELS)?.ok_or_else(invalid)?);
        }
        records
            .values
            .verify_closure(&ids[0], records.record_bytes, &mut *records.cancelled)?;
        records.presence(&ids[0], &keys[0].0, 0)?;
        read(
            &ids[1],
            &keys[1],
            MemberPayloadCodecV1::Unit,
            false,
            0,
            &mut records,
            &mut std::io::sink(),
        )?;
        read(
            &ids[2],
            &keys[2],
            MemberPayloadCodecV1::U64,
            true,
            8,
            &mut records,
            &mut std::io::sink(),
        )?;
        for i in 3..6 {
            read(
                &ids[i],
                &keys[i],
                MemberPayloadCodecV1::Text,
                false,
                work.max_payload_bytes,
                &mut records,
                &mut std::io::sink(),
            )?;
        }
        let mut text = [0; 8];
        let observed = read(
            &ids[6],
            &keys[6],
            MemberPayloadCodecV1::Text,
            false,
            8,
            &mut records,
            &mut Cursor::new(text.as_mut_slice()),
        )?;
        let RecordValueV1::Text { utf8_bytes } = observed else {
            return Err(invalid());
        };
        let (name, module) = match text.get(..utf8_bytes as usize).ok_or_else(invalid)? {
            b"Reducer" | b"Plan" => (true, false),
            b"Module" => (false, true),
            b"System" => (false, false),
            _ => return Err(invalid()),
        };
        for (i, active) in [(7, name), (8, module)] {
            read(
                &ids[i],
                &keys[i],
                if active {
                    MemberPayloadCodecV1::Text
                } else {
                    MemberPayloadCodecV1::Unit
                },
                false,
                if active { work.max_payload_bytes } else { 0 },
                &mut records,
                &mut std::io::sink(),
            )?;
        }
        let json = reference(
            kind,
            &ids[9],
            &keys[9],
            true,
            &mut records,
            &mut std::io::sink(),
        )?;
        while let Some(entry) = cursor.next(&mut records)? {
            spend(work)?;
            let slot = keys
                .iter()
                .position(|(key, _)| key.as_bytes() == entry.key)
                .ok_or_else(invalid)?;
            if ids[slot] != entry.value {
                return Err(invalid());
            }
        }
        json
    };
    inspect_runtime_effect_json_body_v1(&json, ctx, work)?;
    Ok(())
}
pub(super) fn optional<R: ValueNodeReader, C: FnMut() -> bool>(
    id: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<Option<u64>, ValueTreeError> {
    match read(
        id,
        key,
        MemberPayloadCodecV1::U64,
        true,
        8,
        records,
        &mut std::io::sink(),
    )? {
        RecordValueV1::Unit => Ok(None),
        RecordValueV1::Unsigned(v) => Ok(Some(v)),
        _ => Err(invalid()),
    }
}
pub(super) fn pending_item<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    root: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeEffectJsonWorkV1,
) -> Result<(StateRootV1, Option<u64>, Option<u64>), ValueTreeError> {
    if root.namespace != StateNamespaceV1::Effects || root.leaf_count != 5 {
        return Err(invalid());
    }
    let mut records = ctx.records();
    let _scratch = records.values.memory.reserve(8192, records.values.class)?;
    let descriptor = Space::PendingItem.descriptor();
    let keys = [
        (
            WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None)
                .map_err(ValueTreeError::Invalid)?,
            0,
        ),
        records.key(descriptor, WorldFieldRecordKindV1::Entry, &())?,
        records.key(descriptor, WorldFieldRecordKindV1::Entry, &((), "body"))?,
        records.key(descriptor, WorldFieldRecordKindV1::Entry, &((), "prev"))?,
        records.key(descriptor, WorldFieldRecordKindV1::Entry, &((), "next"))?,
    ];
    let mut ids = [[0; 32]; 5];
    for (i, (key, _)) in keys.iter().enumerate() {
        ids[i] = tree::lookup(root, key, &mut records, LABELS)?.ok_or_else(invalid)?;
    }
    records
        .values
        .verify_closure(&ids[0], records.record_bytes, &mut *records.cancelled)?;
    records.presence(&ids[0], &keys[0].0, 0)?;
    read(
        &ids[1],
        &keys[1],
        MemberPayloadCodecV1::Unit,
        false,
        0,
        &mut records,
        &mut std::io::sink(),
    )?;
    let body = reference(
        RuntimeEffectKindV1::Inflight,
        &ids[2],
        &keys[2],
        false,
        &mut records,
        &mut std::io::sink(),
    )?;
    let prev = optional(&ids[3], &keys[3], &mut records)?;
    let next = optional(&ids[4], &keys[4], &mut records)?;
    let mut cursor = json_read::cursor::Cursor::new(root, &mut records)?;
    while let Some(entry) = cursor.next(&mut records)? {
        spend(work)?;
        let slot = keys
            .iter()
            .position(|(key, _)| key.as_bytes() == entry.key)
            .ok_or_else(invalid)?;
        if ids[slot] != entry.value {
            return Err(invalid());
        }
    }
    Ok((body, prev, next))
}
fn pending_neighbor<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ordinal: u64,
    column: &str,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<u64>, ValueTreeError> {
    let root = ctx.root;
    let mut records = ctx.records();
    let outer = records.key(
        RuntimeEffectKindV1::Pending.descriptor(),
        WorldFieldRecordKindV1::Entry,
        &ordinal,
    )?;
    let id = tree::lookup(root, &outer.0, &mut records, LABELS)?.ok_or_else(invalid)?;
    let child = reference(
        RuntimeEffectKindV1::Pending,
        &id,
        &outer,
        false,
        &mut records,
        &mut std::io::sink(),
    )?;
    if child.leaf_count != 5 {
        return Err(invalid());
    }
    let field = records.key(
        Space::PendingItem.descriptor(),
        WorldFieldRecordKindV1::Entry,
        &((), column),
    )?;
    let id = tree::lookup(&child, &field.0, &mut records, LABELS)?.ok_or_else(invalid)?;
    optional(&id, &field, &mut records)
}
#[derive(Default)]
struct Stats {
    meta: [bool; 2],
    format: [bool; 2],
    head: Option<Option<u64>>,
    tail: Option<Option<u64>>,
    count: Option<u64>,
    next_ordinal: Option<u64>,
    first: Option<u64>,
    last: Option<u64>,
    counts: [u64; 2],
    min: Option<u64>,
    max: Option<u64>,
}
fn outer<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    entry: &StateEntryV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeEffectJsonWorkV1,
    stats: &mut Stats,
) -> Result<(), ValueTreeError> {
    spend(work)?;
    if !entry.key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(());
    }
    let binding = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    if binding.owner != FieldOwner::Runtime {
        return Ok(());
    }
    let Some(kind) = RuntimeEffectKindV1::ALL
        .into_iter()
        .find(|kind| kind.name() == binding.name)
    else {
        return Ok(());
    };
    let slot = usize::from(kind == RuntimeEffectKindV1::Inflight);
    let key = WorldFieldKeyV1::new(kind.descriptor(), binding.kind, binding.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != entry.key || entry.namespace != StateNamespaceV1::Effects {
        return Err(invalid());
    }
    let (root, ordinal) = {
        let mut records = ctx.records();
        if binding.kind == WorldFieldRecordKindV1::Metadata {
            records.values.verify_closure(
                &entry.value,
                records.record_bytes,
                &mut *records.cancelled,
            )?;
            records.presence(&entry.value, &key, 0)?;
            stats.meta[slot] = true;
            return Ok(());
        }
        if binding.kind != WorldFieldRecordKindV1::Entry {
            return Err(invalid());
        }
        let format = records.key(
            kind.descriptor(),
            WorldFieldRecordKindV1::Entry,
            &(FORMAT, ()),
        )?;
        if key == format.0 {
            read(
                &entry.value,
                &format,
                MemberPayloadCodecV1::Unit,
                false,
                0,
                &mut records,
                &mut std::io::sink(),
            )?;
            stats.format[slot] = true;
            return Ok(());
        }
        if kind == RuntimeEffectKindV1::Pending {
            for name in ["head", "tail", "count", "next_ordinal"] {
                let field = records.key(
                    kind.descriptor(),
                    WorldFieldRecordKindV1::Entry,
                    &((FORMAT, ()), name),
                )?;
                if key == field.0 {
                    let optional = matches!(name, "head" | "tail");
                    let value = read(
                        &entry.value,
                        &field,
                        MemberPayloadCodecV1::U64,
                        optional,
                        8,
                        &mut records,
                        &mut std::io::sink(),
                    )?;
                    let value = match value {
                        RecordValueV1::Unsigned(v) => Some(v),
                        RecordValueV1::Unit if optional => None,
                        _ => return Err(invalid()),
                    };
                    match name {
                        "head" => stats.head = Some(value),
                        "tail" => stats.tail = Some(value),
                        "count" => stats.count = value,
                        _ => stats.next_ordinal = value,
                    }
                    return Ok(());
                }
            }
        }
        let header = read_member_record_header_v1(
            &entry.value,
            &key,
            records.values,
            MemberRecordLimitsV1 {
                semantic_bytes: records.semantic_bytes,
                payload_bytes: 165,
                record_bytes: records.record_bytes,
            },
            &mut *records.cancelled,
        )?;
        let mut sink = std::io::sink();
        let mut probe = OriginalSemanticKeyProbe::new(
            if kind == RuntimeEffectKindV1::Pending {
                OriginalKeyKind::Unsigned
            } else {
                OriginalKeyKind::Text
            },
            &mut sink,
        );
        let result = reference(
            kind,
            &entry.value,
            &(key, header.semantic_bytes),
            false,
            &mut records,
            &mut probe,
        );
        if let Some(error) = probe.error() {
            return Err(ValueTreeError::Invalid(error));
        }
        let root = result?;
        let ordinal = match probe.finish()? {
            OriginalKeyInfo::Unsigned(v) => Some(v),
            OriginalKeyInfo::Text { .. } => None,
        };
        (root, ordinal)
    };
    if let Some(ordinal) = ordinal {
        let (body_root, prev, next) = pending_item(&root, ctx, work)?;
        if prev.is_some_and(|v| v >= ordinal) || next.is_some_and(|v| v <= ordinal) {
            return Err(invalid());
        }
        for (neighbor, reciprocal) in [(prev, "next"), (next, "prev")] {
            if let Some(neighbor) = neighbor {
                if pending_neighbor(neighbor, reciprocal, ctx)? != Some(ordinal) {
                    return Err(invalid());
                }
            } else if reciprocal == "next" {
                if stats.first.replace(ordinal).is_some() {
                    return Err(invalid());
                }
            } else if stats.last.replace(ordinal).is_some() {
                return Err(invalid());
            }
        }
        body(RuntimeEffectKindV1::Inflight, &body_root, ctx, work)?;
    } else {
        body(kind, &root, ctx, work)?;
    }
    stats.counts[slot] = stats.counts[slot].checked_add(1).ok_or_else(invalid)?;
    if let Some(ordinal) = ordinal {
        stats.min = Some(stats.min.map_or(ordinal, |v| v.min(ordinal)));
        stats.max = Some(stats.max.map_or(ordinal, |v| v.max(ordinal)));
    }
    Ok(())
}
/// Full owner structural qualification over the authenticated six-root set.
/// No accepted-head, capability grant, outbox or dispatch permission is minted.
pub fn qualify_runtime_effect_records_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut RuntimeEffectJsonWorkV1,
) -> Result<ReservedValue<RuntimeEffectFamilyQualificationV1>, ValueTreeError> {
    roots.validate().map_err(|_| invalid())?;
    ctx.check()?;
    if !roots.state.iter().any(|root| root == ctx.root) {
        return Err(invalid());
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let _scratch = ctx.values.memory.reserve(16 << 10, ctx.values.class)?;
    let cancel = RefCell::new(&mut ctx.cancelled);
    let mut stats = Stats::default();
    for root in &roots.state {
        let mut local = WorldFieldReadContextV1 {
            root,
            state: ctx.state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: || (cancel.borrow_mut())(),
        };
        let result = StateScanContext {
            source: ctx.state,
            memory: local.values.memory,
            class: local.values.class,
            io: &mut *scan,
        }
        .visit_entries(
            root,
            |entry| outer(entry, &mut local, work, &mut stats),
            || (cancel.borrow_mut())(),
        );
        let original = (local.take_state_error)();
        if (cancel.borrow_mut())() {
            return Err(ValueTreeError::Cancelled);
        }
        if let Some(error) = original {
            return Err(error);
        }
        match result {
            Ok(_) => (),
            Err(StateScanError::Visitor(e)) => return Err(e),
            Err(StateScanError::Memory(e)) => return Err(e.into()),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(StateScanError::ResourceLimited) => {
                return Err(ValueTreeError::ResourceLimited("effect outer scan"));
            }
            Err(_) => return Err(invalid()),
        }
    }
    let head = stats.head.ok_or_else(invalid)?;
    let tail = stats.tail.ok_or_else(invalid)?;
    let count = stats.count.ok_or_else(invalid)?;
    let next_ordinal = stats.next_ordinal.ok_or_else(invalid)?;
    if stats.meta != [true; 2]
        || stats.format != [true; 2]
        || count != stats.counts[0]
        || head != stats.first
        || tail != stats.last
        || (count == 0) != head.is_none()
        || head.is_none() != tail.is_none()
        || stats.max.is_some_and(|v| v >= next_ordinal)
    {
        return Err(invalid());
    }
    let digest = roots.digest().map_err(|_| invalid())?;
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(RuntimeEffectFamilyQualificationV1 {
        roots: digest,
        frontier: RuntimeEffectQueueFrontierV1 {
            head,
            tail,
            count,
            next_ordinal,
        },
        counts: stats.counts,
    }))
}
