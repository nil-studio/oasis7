//! Provisional typed lifecycle tree preparation. Only an accepted receiver
//! or complete bootstrap may turn its root into an owner qualification.
use crate::{
    incremental_cognition_history::CognitionHistoryFieldV1,
    incremental_cognition_history_cbor::{
        CognitionHistoryRecordSchemaV1, CognitionOriginalTextProjectionV1,
    },
    incremental_cognition_history_lifecycle::*,
    incremental_cognition_lifecycle_row_v2::*,
    incremental_field_member::*,
    incremental_field_presence::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape},
    incremental_record_tree as tree,
};
use oasis7_formal_protocol::{
    incremental_material::FactLocationV1,
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
const LABELS: tree::RecordTreeLabelsV1 = tree::RecordTreeLabelsV1 {
    read: "lifecycle index read",
    pending: "lifecycle index pending",
    tree: "lifecycle index tree",
    digest: "lifecycle index digest",
    read_allowance: "lifecycle read allowance",
    admission: "lifecycle source admission",
    bound: "lifecycle source bound",
    producer_pool: "lifecycle producer pool",
    page: "lifecycle page",
    write_allowance: "lifecycle write allowance",
};
pub fn lifecycle_lookup_field_v2() -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::Cognition,
        name: "history.lifecycle.lookup.v2",
        destination: FieldDestination::State(StateNamespaceV1::Artifacts),
        shape: FieldShape::ByteMap,
    }
}
pub fn lifecycle_tuple_schema_v2() -> [u8; 32] {
    *blake3::hash(b"oasis7.cognition.lifecycle-tuple.v2").as_bytes()
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Cognition lifecycle index")
}
pub struct CognitionLifecycleWorkV2 {
    pub key_bytes_remaining: u64,
    pub cells_remaining: u64,
    pub max_pending_node_bytes: usize,
}
impl CognitionLifecycleWorkV2 {
    fn cell(&mut self) -> Result<(), ValueTreeError> {
        self.cells_remaining = self
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("lifecycle cells"))?;
        Ok(())
    }
}
pub struct CognitionLifecycleAppendV2<'a> {
    pub schema: &'a ReservedValue<CognitionHistoryRecordSchemaV1>,
    pub keys: [&'a str; 5],
    pub ordinal: u64,
    pub location: FactLocationV1,
}
pub struct PreparedCognitionLifecycleIndexV2 {
    pub root: StateRootV1,
    pub keys: u64,
}
fn projection(
    keys: [&str; 5],
    work: &mut CognitionLifecycleWorkV2,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<[CognitionOriginalTextProjectionV1; 5], ValueTreeError> {
    let mut out = [CognitionOriginalTextProjectionV1::Missing; 5];
    for (i, key) in keys.into_iter().enumerate() {
        let mut hash = blake3::Hasher::new();
        for bytes in key.as_bytes().chunks(16 << 10) {
            if cancelled() {
                return Err(ValueTreeError::Cancelled);
            }
            work.key_bytes_remaining = work
                .key_bytes_remaining
                .checked_sub(bytes.len() as u64)
                .ok_or(ValueTreeError::ResourceLimited("lifecycle key bytes"))?;
            hash.update(bytes);
        }
        out[i] = CognitionOriginalTextProjectionV1::Text {
            bytes: key.len() as u64,
            utf8_digest: *hash.finalize().as_bytes(),
        };
    }
    Ok(out)
}
pub fn cognition_lifecycle_lookup_id_v2(
    keys: &[CognitionOriginalTextProjectionV1; 5],
) -> Result<[u8; 32], ValueTreeError> {
    let mut h = blake3::Hasher::new();
    h.update(b"oasis7.cognition.lifecycle-lookup.v2\0");
    for key in keys {
        let CognitionOriginalTextProjectionV1::Text { bytes, utf8_digest } = key else {
            return Err(invalid());
        };
        if *bytes == 0 {
            return Err(invalid());
        }
        h.update(&bytes.to_be_bytes());
        h.update(utf8_digest);
    }
    Ok(*h.finalize().as_bytes())
}
/// Prepare one actual original's scalar transition. The caller must bind its
/// ordinal/location to the real FactSet and compare the result at the receiver.
pub fn prepare_cognition_lifecycle_append_v2<M, R, E, C, W, D>(
    parent: &StateRootV1,
    keys: u64,
    input: CognitionLifecycleAppendV2<'_>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, D>,
    work: &mut CognitionLifecycleWorkV2,
) -> Result<PreparedCognitionLifecycleIndexV2, MemberRecordError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    D: FnMut() -> bool,
{
    read.check()?;
    let memory = read.values.memory;
    let _scratch = memory
        .reserve(32 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    if read.values.class != MemoryClass::Ordinary
        || !write.memory.same_pool(memory)
        || !input.schema.uses_budget(memory)
        || input.schema.semantic().class() != MemoryClass::Ordinary
        || input.schema.semantic().field() != CognitionHistoryFieldV1::JournalEvents
        || input.schema.semantic().id() != &input.location.original
        || parent.namespace != StateNamespaceV1::Artifacts
    {
        return Err(invalid().into());
    }
    let lifecycle = input.schema.journal_lifecycle().ok_or_else(invalid)?;
    let kind = input.schema.event_kind().ok_or_else(invalid)?;
    if !lifecycle.is_complete() || !is_cognition_lifecycle_kind_v1(kind) {
        if (read.cancelled)() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        return Ok(PreparedCognitionLifecycleIndexV2 {
            root: parent.clone(),
            keys,
        });
    }
    if projection(input.keys, work, &mut read.cancelled)? != *lifecycle.keys() {
        return Err(invalid().into());
    }
    let lookup = hex::encode(cognition_lifecycle_lookup_id_v2(lifecycle.keys())?);
    let policy = tree::RecordTreeWritePolicyV1 {
        max_pending_node_bytes: work.max_pending_node_bytes,
        labels: LABELS,
    };
    let field = lifecycle_lookup_field_v2();
    work.cell()?;
    let (pointer_key, pointer_semantic, old_id, count) = {
        let mut records = read.records();
        let (key, semantic) = records.key(field, WorldFieldRecordKindV1::Entry, &lookup)?;
        if keys == 0 {
            if *parent != StateRootV1::empty(StateNamespaceV1::Artifacts).map_err(|_| invalid())? {
                return Err(invalid().into());
            }
            (key, semantic, None, None)
        } else {
            if parent.leaf_count != keys.checked_add(2).ok_or_else(invalid)? {
                return Err(invalid().into());
            }
            let count_key = records.key(field, WorldFieldRecordKindV1::Entry, &())?;
            let id =
                tree::lookup(parent, &count_key.0, &mut records, LABELS)?.ok_or_else(invalid)?;
            if records.unsigned(&id, &count_key.0, count_key.1)? != keys {
                return Err(invalid().into());
            }
            let old = tree::lookup(parent, &key, &mut records, LABELS)?;
            (key, semantic, old, Some((count_key.0, id)))
        }
    };
    let tuple = (
        input.keys[0],
        (
            input.keys[1],
            (input.keys[2], (input.keys[3], input.keys[4])),
        ),
    );
    let mut before = None;
    if let Some(id) = old_id {
        work.cell()?;
        let child = {
            let mut records = read.records();
            let bytes = records.fixed::<RECORD_ROOT_BYTES>(
                &id,
                &pointer_key,
                pointer_semantic,
                MemberPayloadCodecV1::RecordRoot,
            )?;
            RecordRootRefV1::from_canonical_bytes(&bytes)
                .map_err(|_| invalid())?
                .validate_binding(
                    &lifecycle_tuple_schema_v2(),
                    &WorldFieldKeyV1::from_bytes(pointer_key.as_bytes())
                        .map_err(ValueTreeError::Invalid)?
                        .semantic_key
                        .ok_or_else(invalid)?,
                    StateNamespaceV1::Artifacts,
                )
                .map_err(|_| invalid())?
                .clone()
        };
        if child.leaf_count != 2 {
            return Err(invalid().into());
        }
        let key = {
            let mut records = read.records();
            records
                .key(
                    lifecycle_tuple_field_v2(),
                    WorldFieldRecordKindV1::Bytes,
                    &tuple,
                )?
                .0
        };
        let id = {
            let mut records = read.records();
            tree::lookup(&child, &key, &mut records, LABELS)?.ok_or_else(invalid)?
        };
        let value = read_cognition_lifecycle_row_v2(
            &id,
            &key,
            &mut read.values,
            MemberRecordLimitsV1 {
                semantic_bytes: read.semantic_bytes,
                payload_bytes: LIFECYCLE_ROW_BYTES_V2 as u64,
                record_bytes: read.record_bytes,
            },
            &mut read.cancelled,
        )?;
        if value.keys() != lifecycle.keys() {
            return Err(invalid().into());
        }
        before = Some(value.row());
    }
    let update = advance_cognition_lifecycle_v1(kind, before.map(|row| row.state), || {
        let attempt = lifecycle.transport_attempt();
        if attempt == 0 {
            return Err(CognitionLifecycleErrorV1::TransportInvalid);
        }
        Ok((before.map(|row| row.attempt).unwrap_or(0), attempt))
    })
    .map_err(|error| ValueTreeError::Invalid(error.message()))?
    .ok_or_else(invalid)?;
    let row = CognitionLifecycleRowV2 {
        state: update.state,
        attempt: update
            .transport_attempt
            .unwrap_or(before.map(|row| row.attempt).unwrap_or(0)),
        journal_ordinal: input.ordinal,
        original: input.location,
    };
    let mut child = StateRootV1::empty(StateNamespaceV1::Artifacts).map_err(|_| invalid())?;
    let metadata = write_field_metadata_v1(
        lifecycle_tuple_field_v2(),
        &mut PresenceRecordWriteContext {
            sink: &mut *write.sink,
            memory: write.memory,
            io: &mut *write.io,
            key_work: &mut *write.key_work,
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: write.limits.semantic_bytes,
                record_bytes: write.limits.record_bytes,
            },
            cancelled: &mut write.cancelled,
        },
    )?;
    child = tree::append(
        child,
        &metadata.field_key,
        metadata.value_root,
        write,
        policy,
    )?;
    let record = write_member_record_v1(
        lifecycle_tuple_field_v2(),
        WorldFieldRecordKindV1::Bytes,
        &tuple,
        MemberPayloadRefV1::Bytes(&row.canonical_bytes()),
        write,
    )?;
    child = tree::append(child, &record.field_key, record.value_root, write, policy)?;
    let reference = RecordRootRefV1::new(
        lifecycle_tuple_schema_v2(),
        WorldFieldKeyV1::from_bytes(pointer_key.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or_else(invalid)?,
        child,
    )
    .map_err(|_| invalid())?;
    let pointer = write_member_record_v1(
        field,
        WorldFieldRecordKindV1::Entry,
        &lookup,
        MemberPayloadRefV1::RecordRoot(&reference),
        write,
    )?;
    let next = keys
        .checked_add(u64::from(old_id.is_none()))
        .ok_or_else(invalid)?;
    let mut root = parent.clone();
    if keys == 0 {
        let meta = write_field_metadata_v1(
            field,
            &mut PresenceRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: PresenceRecordLimitsV1 {
                    semantic_bytes: write.limits.semantic_bytes,
                    record_bytes: write.limits.record_bytes,
                },
                cancelled: &mut write.cancelled,
            },
        )?;
        root = tree::append(root, &meta.field_key, meta.value_root, write, policy)?;
    }
    if old_id.is_none() {
        let record = write_member_record_v1(
            field,
            WorldFieldRecordKindV1::Entry,
            &(),
            MemberPayloadRefV1::U64(next),
            write,
        )?;
        root = tree::edit(
            root,
            &record.field_key,
            (count.map(|(_, id)| id), Some(record.value_root)),
            &mut read.values,
            write,
            &mut read.cancelled,
            policy,
        )?;
    }
    root = tree::edit(
        root,
        &pointer_key,
        (old_id, Some(pointer.value_root)),
        &mut read.values,
        write,
        &mut read.cancelled,
        policy,
    )?;
    if root.leaf_count != next.checked_add(2).ok_or_else(invalid)? {
        return Err(invalid().into());
    }
    if (read.cancelled)() || (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(PreparedCognitionLifecycleIndexV2 { root, keys: next })
}

/// Point-lookup an internal projection locator, then audit the complete
/// two-leaf tuple child and stream its original five strings. This does not
/// prove that the whole family or any lifecycle transition is valid.
pub fn read_cognition_lifecycle_lookup_v2<M, R, E, C>(
    root: &StateRootV1,
    projection: &[CognitionOriginalTextProjectionV1; 5],
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut oasis7_formal_protocol::incremental_state::StateScanBudget,
    work: &mut CognitionLifecycleWorkV2,
) -> Result<
    Option<
        ReservedValue<
            crate::incremental_cognition_lifecycle_row_v2::CheckedCognitionLifecycleRowV2,
        >,
    >,
    ValueTreeError,
>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    use oasis7_formal_protocol::incremental_state::{StateScanContext, StateScanError};
    read.check()?;
    let _scratch = read.values.memory.reserve(16 << 10, read.values.class)?;
    work.cell()?;
    let locator = hex::encode(cognition_lifecycle_lookup_id_v2(projection)?);
    let (key, semantic) = {
        let mut records = read.records();
        records.key(
            lifecycle_lookup_field_v2(),
            WorldFieldRecordKindV1::Entry,
            &locator,
        )?
    };
    let id = {
        let mut records = read.records();
        tree::lookup(root, &key, &mut records, LABELS)?
    };
    let Some(id) = id else {
        read.check()?;
        return Ok(None);
    };
    let bytes = {
        let mut records = read.records();
        records.fixed::<RECORD_ROOT_BYTES>(&id, &key, semantic, MemberPayloadCodecV1::RecordRoot)?
    };
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
    let owner = WorldFieldKeyV1::from_bytes(key.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or_else(invalid)?;
    let child = reference
        .validate_binding(
            &lifecycle_tuple_schema_v2(),
            &owner,
            StateNamespaceV1::Artifacts,
        )
        .map_err(|_| invalid())?;
    if child.leaf_count != 2 {
        return Err(invalid());
    }
    let mut entries = Vec::with_capacity(2);
    let scanned = StateScanContext {
        source: read.state,
        memory: read.values.memory,
        class: read.values.class,
        io: scan,
    }
    .visit_entries(
        child,
        |entry| {
            if entries.len() == 2
                || entry.key.len() > crate::incremental_world_field_key::WORLD_FIELD_KEY_MAX_BYTES
            {
                return Err(invalid());
            }
            entries.push(entry.clone());
            Ok(())
        },
        &mut read.cancelled,
    );
    scanned.map_err(|error| match error {
        StateScanError::Visitor(error) => error,
        StateScanError::Memory(error) => ValueTreeError::Memory(error),
        StateScanError::Cancelled => ValueTreeError::Cancelled,
        StateScanError::ResourceLimited => ValueTreeError::ResourceLimited("lifecycle child scan"),
        StateScanError::Invalid(_) => invalid(),
        StateScanError::Tree(_) => (read.take_state_error)().unwrap_or_else(invalid),
    })?;
    let metadata = WorldFieldKeyV1::new(
        lifecycle_tuple_field_v2(),
        WorldFieldRecordKindV1::Metadata,
        None,
    )
    .map_err(ValueTreeError::Invalid)?;
    let mut row = None;
    let mut found_metadata = false;
    for entry in entries {
        let parsed = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
        let field = lifecycle_tuple_field_v2();
        if parsed.owner != field.owner
            || parsed.name != field.name
            || parsed.namespace != StateNamespaceV1::Artifacts
            || parsed.shape != FieldShape::ByteMap
        {
            return Err(invalid());
        }
        if entry.key == metadata.as_bytes() {
            let mut records = read.records();
            records.presence(&entry.value, &metadata, 0)?;
            found_metadata = true;
        } else {
            if parsed.kind != WorldFieldRecordKindV1::Bytes || row.is_some() {
                return Err(invalid());
            }
            let key = WorldFieldKeyV1::new(field, parsed.kind, parsed.semantic_key)
                .map_err(ValueTreeError::Invalid)?;
            let value = read_cognition_lifecycle_row_v2(
                &entry.value,
                &key,
                &mut read.values,
                MemberRecordLimitsV1 {
                    semantic_bytes: read.semantic_bytes,
                    payload_bytes: LIFECYCLE_ROW_BYTES_V2 as u64,
                    record_bytes: read.record_bytes,
                },
                &mut read.cancelled,
            )?;
            if value.keys() != projection {
                return Err(invalid());
            }
            row = Some(value);
        }
    }
    if !found_metadata {
        return Err(invalid());
    }
    let row = row.ok_or_else(invalid)?;
    read.check()?;
    Ok(Some(row))
}
