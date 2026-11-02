//! Full fixed-root qualification for the Runtime Artifacts owner cohort.
use super::{RuntimeArtifactFieldV1 as Field, RuntimeArtifactRecordV1 as Record, key};
use crate::{
    incremental_field_member::{
        MemberPayloadCodecV1, MemberRecordLimitsV1, read_member_record_key_v1,
        read_member_record_v1,
    },
    incremental_field_presence::{PresenceRecordLimitsV1, read_field_presence_v1},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_projection::{FieldOwner, FieldShape},
    incremental_record_value_read::{
        RecordReadLabelsV1, RecordReadProfileV1, RecordValueV1, read_record_value_v1,
    },
    incremental_semantic_key::SemanticKeyBudget,
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{
        MerkleNodeReader, StateEntryV1, StateNamespaceV1, StateRootV1, StateScanBudget,
        StateScanContext, StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::{cell::RefCell, io, io::Write};

#[path = "incremental_runtime_artifact_resources_family_schema.rs"]
mod schema;

const MAX_DEPTH: usize = 5;
const WORLD_FIELD_DOMAIN: &[u8] = b"oasis7.iws.world-field-key.v1\0";
const RECORD_LABELS: RecordReadLabelsV1 = RecordReadLabelsV1 {
    utf8: "runtime artifact family UTF-8",
    ledger_utf8: "runtime artifact family ledger UTF-8",
    ledger_frame: "runtime artifact family ledger frame",
    boolean: "runtime artifact family boolean",
    fixed: "runtime artifact family fixed payload",
    profile: "runtime artifact family member profile",
    integer: "runtime artifact family integer range",
};

fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("runtime artifact family closure")
}

/// Caller-owned state scan budgets are disjoint by nesting depth and shared
/// across all siblings at that depth. Value IO, cells and key work remain one
/// continuous caller-owned budget for the complete family.
pub struct RuntimeArtifactResourcesReadContextV1<'a, M, R, E, C> {
    pub state: &'a M,
    pub values: ValueReadContext<'a, R>,
    pub key_work: &'a mut SemanticKeyBudget,
    pub semantic_bytes: u64,
    pub record_bytes: u64,
    pub max_payload_bytes: u64,
    pub scans: &'a mut [StateScanBudget; MAX_DEPTH],
    pub take_state_error: E,
    pub cancelled: C,
}

/// A raw bootstrap/recovery qualification of the three Runtime Artifacts
/// descriptors. It conveys no dispatch, signing, authority, or World-ready
/// capability.
#[derive(Debug)]
pub struct RuntimeArtifactResourcesQualificationV1 {
    roots: [u8; 32],
    counts: [u64; 3],
    class: MemoryClass,
}
impl RuntimeArtifactResourcesQualificationV1 {
    pub fn validate_binding(
        &self,
        roots: &[u8; 32],
        class: MemoryClass,
    ) -> Result<(), ValueTreeError> {
        if &self.roots == roots && self.class == class {
            Ok(())
        } else {
            Err(invalid())
        }
    }
    pub fn record_counts(&self) -> [u64; 3] {
        self.counts
    }
}

#[derive(Clone, Copy)]
enum Decoded {
    Unsigned(u64),
    Bool(bool),
    Other,
}

type ReadCellResult = (
    schema::Cell,
    key::ParsedKey,
    Decoded,
    Option<(Record, StateRootV1)>,
);

#[derive(Debug, Default)]
struct DynamicProgress {
    expected: Option<u64>,
    actual: u64,
    maximum: Option<u64>,
}
#[derive(Debug)]
struct RecordProgress {
    metadata: bool,
    fixed_mask: u128,
    fixed_count: u8,
    group_mask: u8,
    dynamic: [DynamicProgress; 3],
    option_present: [Option<bool>; 3],
    option_value_seen: [bool; 3],
}
impl RecordProgress {
    fn new() -> Self {
        Self {
            metadata: false,
            fixed_mask: 0,
            fixed_count: 0,
            group_mask: 0,
            dynamic: std::array::from_fn(|_| DynamicProgress::default()),
            option_present: [None; 3],
            option_value_seen: [false; 3],
        }
    }
    fn fixed(
        &mut self,
        slot: u8,
        value: Decoded,
        count_group: Option<u8>,
        option: Option<(u8, schema::OptionPart)>,
    ) -> Result<(), ValueTreeError> {
        let bit = 1u128.checked_shl(u32::from(slot)).ok_or_else(invalid)?;
        if self.fixed_mask & bit != 0 {
            return Err(invalid());
        }
        self.fixed_mask |= bit;
        self.fixed_count = self.fixed_count.checked_add(1).ok_or_else(invalid)?;
        if let Some(group) = count_group {
            let Decoded::Unsigned(value) = value else {
                return Err(invalid());
            };
            let target = self
                .dynamic
                .get_mut(usize::from(group))
                .ok_or_else(invalid)?;
            if target.expected.replace(value).is_some() {
                return Err(invalid());
            }
        }
        if let Some((id, part)) = option {
            let index = usize::from(id);
            match part {
                schema::OptionPart::Present => {
                    let Decoded::Bool(present) = value else {
                        return Err(invalid());
                    };
                    if self.option_present.get(index).is_none()
                        || self.option_present[index].replace(present).is_some()
                    {
                        return Err(invalid());
                    }
                }
                schema::OptionPart::Value => {
                    if self.option_value_seen.get(index).is_none() || self.option_value_seen[index]
                    {
                        return Err(invalid());
                    }
                    self.option_value_seen[index] = true;
                }
            }
        }
        Ok(())
    }
    fn group(&mut self, id: u8) -> Result<(), ValueTreeError> {
        let bit = 1u8.checked_shl(u32::from(id)).ok_or_else(invalid)?;
        if self.group_mask & bit != 0 {
            return Err(invalid());
        }
        self.group_mask |= bit;
        Ok(())
    }
    fn dynamic(&mut self, group: u8, ordinal: Option<u64>) -> Result<(), ValueTreeError> {
        let item = self
            .dynamic
            .get_mut(usize::from(group))
            .ok_or_else(invalid)?;
        item.actual = item.actual.checked_add(1).ok_or_else(invalid)?;
        if let Some(ordinal) = ordinal {
            item.maximum = Some(item.maximum.map_or(ordinal, |old| old.max(ordinal)));
        }
        Ok(())
    }
    fn finish(&self, record: Record) -> Result<(), ValueTreeError> {
        let layout = schema::layout(record);
        let expected_fixed_mask = (1u128 << layout.fixed_fields) - 1;
        if !self.metadata
            || self.fixed_count != layout.fixed_fields
            || self.fixed_mask != expected_fixed_mask
            || self.group_mask != layout.groups
        {
            return Err(invalid());
        }
        for group in 0..layout.dynamic_groups {
            let entry = &self.dynamic[usize::from(group)];
            let expected = entry.expected.ok_or_else(invalid)?;
            let ordinal = layout.ordinal_groups & (1u8 << group) != 0;
            if entry.actual != expected
                || (entry.actual == 0 && entry.maximum.is_some())
                || (ordinal && entry.actual != 0 && entry.maximum != Some(entry.actual - 1))
                || (!ordinal && entry.maximum.is_some())
            {
                return Err(invalid());
            }
        }
        for index in 0..self.option_present.len() {
            if let Some(present) = self.option_present[index] {
                if present != self.option_value_seen[index] {
                    return Err(invalid());
                }
            } else if self.option_value_seen[index] {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

/// Qualify all required Artifact records and recursively validate every typed
/// child. This is an explicit bounded full scan for bootstrap/recovery, not an
/// ordinary transaction path.
pub fn qualify_runtime_artifact_resources_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    context: &mut RuntimeArtifactResourcesReadContextV1<'_, M, R, E, C>,
    work: &mut super::genesis::RuntimeArtifactResourcesWorkV1,
) -> Result<ReservedValue<RuntimeArtifactResourcesQualificationV1>, ValueTreeError> {
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    roots.validate().map_err(|_| invalid())?;
    let memory = context.values.memory;
    let class = context.values.class;
    if context.state.allocation_class() != Some(class)
        || !context
            .state
            .allocation_budget()
            .is_some_and(|pool| pool.same_pool(memory))
    {
        return Err(ValueTreeError::Invalid(
            "runtime artifact family pool/class",
        ));
    }
    let digest = roots.digest().map_err(|_| invalid())?;
    let mut metadata = [false; 3];
    let mut entries = [false; 3];
    let mut counts = [0u64; 3];
    let RuntimeArtifactResourcesReadContextV1 {
        state,
        values,
        key_work,
        semantic_bytes,
        record_bytes,
        max_payload_bytes,
        scans,
        take_state_error,
        cancelled,
    } = context;
    let cancelled = RefCell::new(cancelled);
    let (field_scan, nested_scans) = scans.split_first_mut().ok_or_else(invalid)?;
    let state = *state;
    let mut verifier = FamilyVerifier {
        state,
        values,
        key_work,
        semantic_bytes: *semantic_bytes,
        record_bytes: *record_bytes,
        max_payload_bytes: *max_payload_bytes,
        take_state_error,
        cancelled: &cancelled,
        work,
    };
    let source = verifier.state;
    let cancel_ref = verifier.cancelled;
    for root in &roots.state {
        let field_scan_result = StateScanContext {
            source,
            memory,
            class,
            io: field_scan,
        }
        .visit_entries(
            root,
            |entry| {
                verifier.spend_cell()?;
                let Some(binding) = field_binding(entry)? else {
                    return Ok(());
                };
                let Some(index) = field_index(binding.name) else {
                    return Ok(());
                };
                let field = Field::ALL[index];
                // A recognized descriptor name belongs to one exact Runtime
                // namespace. Looking only at Artifacts would let a generic
                // same-name field in another RootSet root sit beside the
                // canonical field and escape this family's closure check.
                if root.namespace != StateNamespaceV1::Artifacts
                    || binding.owner != FieldOwner::Runtime
                    || binding.namespace != StateNamespaceV1::Artifacts
                    || binding.shape != FieldShape::Value
                    || entry.namespace != root.namespace
                {
                    return Err(invalid());
                }
                let key =
                    WorldFieldKeyV1::new(field.descriptor(), binding.kind, binding.semantic_key)
                        .map_err(ValueTreeError::Invalid)?;
                if key.as_bytes() != entry.key {
                    return Err(invalid());
                }
                match binding.kind {
                    WorldFieldRecordKindV1::Metadata => {
                        if metadata[index] {
                            return Err(invalid());
                        }
                        verifier.read_presence(&entry.value, &key, 0)?;
                        metadata[index] = true;
                    }
                    WorldFieldRecordKindV1::Entry => {
                        if entries[index] {
                            return Err(invalid());
                        }
                        let (expected, semantic) =
                            verifier.key(field.descriptor(), WorldFieldRecordKindV1::Entry, &())?;
                        if expected != key {
                            return Err(invalid());
                        }
                        if semantic > verifier.semantic_bytes {
                            return Err(ValueTreeError::ResourceLimited(
                                "runtime artifact semantic key",
                            ));
                        }
                        let root = verifier.read_outer_root(field, &entry.value, &key)?;
                        match root {
                            None if field == Field::LatestChainResourceDelta => counts[index] = 0,
                            Some(root) => {
                                verifier.verify_record(field_record(field), &root, nested_scans)?;
                                counts[index] = 1;
                            }
                            None => return Err(invalid()),
                        }
                        entries[index] = true;
                    }
                    _ => return Err(invalid()),
                }
                Ok(())
            },
            || (cancel_ref.borrow_mut())(),
        );
        if let Some(error) = (verifier.take_state_error)() {
            return Err(error);
        }
        map_scan_result(field_scan_result)?;
    }
    if verifier.cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if metadata != [true; 3] || entries != [true; 3] {
        return Err(invalid());
    }
    let charge = memory.reserve(
        std::mem::size_of::<RuntimeArtifactResourcesQualificationV1>(),
        class,
    )?;
    if verifier.cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(RuntimeArtifactResourcesQualificationV1 {
        roots: digest,
        counts,
        class,
    }))
}

struct FamilyVerifier<'s, 'v, 'a, M, R, E, C> {
    state: &'s M,
    values: &'v mut ValueReadContext<'a, R>,
    key_work: &'v mut SemanticKeyBudget,
    semantic_bytes: u64,
    record_bytes: u64,
    max_payload_bytes: u64,
    take_state_error: &'v mut E,
    cancelled: &'v RefCell<C>,
    work: &'v mut super::genesis::RuntimeArtifactResourcesWorkV1,
}

impl<M, R, E, C> FamilyVerifier<'_, '_, '_, M, R, E, C>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    fn cancelled(&self) -> bool {
        (self.cancelled.borrow_mut())()
    }

    fn spend_cell(&mut self) -> Result<(), ValueTreeError> {
        let cancel = self.cancelled;
        self.work.spend(&mut || (cancel.borrow_mut())())
    }

    fn key<K: crate::incremental_semantic_key::SemanticKeyV1 + ?Sized>(
        &mut self,
        field: crate::incremental_projection::FieldDescriptor,
        kind: WorldFieldRecordKindV1,
        semantic_key: &K,
    ) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
        let cancel = self.cancelled;
        let mut cancelled = || (cancel.borrow_mut())();
        let mut reader = InventoryValueReader {
            values: self.values,
            key_work: self.key_work,
            semantic_bytes: self.semantic_bytes,
            record_bytes: self.record_bytes,
            cancelled: &mut cancelled,
        };
        reader.key(field, kind, semantic_key)
    }

    fn read_presence(
        &mut self,
        value: &[u8; 32],
        key: &WorldFieldKeyV1,
        semantic_bytes: u64,
    ) -> Result<(), ValueTreeError> {
        let cancel = self.cancelled;
        let mut cancelled = || (cancel.borrow_mut())();
        let mut reader = InventoryValueReader {
            values: self.values,
            key_work: self.key_work,
            semantic_bytes: self.semantic_bytes,
            record_bytes: self.record_bytes,
            cancelled: &mut cancelled,
        };
        reader.presence(value, key, semantic_bytes)
    }

    fn read_outer_root(
        &mut self,
        field: Field,
        value_root: &[u8; 32],
        key: &WorldFieldKeyV1,
    ) -> Result<Option<StateRootV1>, ValueTreeError> {
        let mut payload = PayloadCapture::default();
        let mut decoder = key::KeyDecoder::new();
        let cancel = self.cancelled;
        let mut cancelled = || (cancel.borrow_mut())();
        let info = read_member_record_v1(
            value_root,
            key,
            self.values,
            MemberRecordLimitsV1 {
                semantic_bytes: self.semantic_bytes,
                payload_bytes: self.max_payload_bytes,
                record_bytes: self.record_bytes,
            },
            &mut decoder,
            &mut payload,
            &mut cancelled,
        )?;
        let parsed = decoder.finish().map_err(ValueTreeError::Invalid)?;
        if parsed.value != key::KeyValue::Unit {
            return Err(invalid());
        }
        let semantic = WorldFieldKeyV1::from_bytes(key.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or_else(invalid)?;
        let expected = self.key(field.descriptor(), WorldFieldRecordKindV1::Entry, &())?;
        if expected.0 != *key || expected.1 != info.semantic_bytes {
            return Err(invalid());
        }
        if field == Field::LatestChainResourceDelta && info.codec == MemberPayloadCodecV1::Unit {
            if info.payload_bytes != 0 || payload.len != 0 {
                return Err(invalid());
            }
            if self.cancelled() {
                return Err(ValueTreeError::Cancelled);
            }
            return Ok(None);
        }
        if info.codec != MemberPayloadCodecV1::RecordRoot
            || info.payload_bytes != RECORD_ROOT_BYTES as u64
            || payload.len != RECORD_ROOT_BYTES
            || payload.overflow
        {
            return Err(invalid());
        }
        let reference =
            RecordRootRefV1::from_canonical_bytes(&payload.bytes).map_err(|_| invalid())?;
        let child = reference
            .validate_binding(&field.schema(), &semantic, StateNamespaceV1::Artifacts)
            .map_err(|_| invalid())?
            .clone();
        Ok(Some(child))
    }

    fn verify_record(
        &mut self,
        record: Record,
        root: &StateRootV1,
        scans: &mut [StateScanBudget],
    ) -> Result<(), ValueTreeError> {
        if root.namespace != StateNamespaceV1::Artifacts || scans.is_empty() {
            return Err(invalid());
        }
        let (scan, child_scans) = scans.split_first_mut().ok_or_else(invalid)?;
        let source = self.state;
        let memory = self.values.memory;
        let class = self.values.class;
        let cancel_ref = self.cancelled;
        let mut progress = RecordProgress::new();
        let result = StateScanContext {
            source,
            memory,
            class,
            io: scan,
        }
        .visit_entries(
            root,
            |entry| {
                self.spend_cell()?;
                let binding = WorldFieldKeyV1::from_bytes(&entry.key).map_err(|_| invalid())?;
                if binding.owner != FieldOwner::Runtime
                    || binding.namespace != StateNamespaceV1::Artifacts
                    || binding.shape != FieldShape::NestedMap
                    || binding.name != record.name()
                    || entry.namespace != StateNamespaceV1::Artifacts
                {
                    return Err(invalid());
                }
                let field_key =
                    WorldFieldKeyV1::new(record.descriptor(), binding.kind, binding.semantic_key)
                        .map_err(ValueTreeError::Invalid)?;
                if field_key.as_bytes() != entry.key {
                    return Err(invalid());
                }
                match binding.kind {
                    WorldFieldRecordKindV1::Metadata => {
                        if progress.metadata {
                            return Err(invalid());
                        }
                        self.read_presence(&entry.value, &field_key, 0)?;
                        progress.metadata = true;
                    }
                    WorldFieldRecordKindV1::Group => {
                        if binding.semantic_key.is_none() {
                            return Err(invalid());
                        }
                        let mut decoder = key::KeyDecoder::new();
                        let cancel = self.cancelled;
                        let mut cancelled = || (cancel.borrow_mut())();
                        read_field_presence_v1(
                            &entry.value,
                            &field_key,
                            self.values,
                            PresenceRecordLimitsV1 {
                                semantic_bytes: self.semantic_bytes,
                                record_bytes: self.record_bytes,
                            },
                            &mut decoder,
                            &mut cancelled,
                        )?;
                        let parsed = decoder.finish().map_err(ValueTreeError::Invalid)?;
                        let Some(schema::Cell::Group(id)) =
                            schema::classify(record, WorldFieldRecordKindV1::Group as u8, parsed)
                        else {
                            return Err(invalid());
                        };
                        progress.group(id)?;
                    }
                    WorldFieldRecordKindV1::Entry => {
                        let (cell, parsed, decoded, child) = self.read_cell(
                            record,
                            &entry.value,
                            &field_key,
                            binding.semantic_key.ok_or_else(invalid)?,
                        )?;
                        match cell {
                            schema::Cell::Fixed {
                                slot,
                                count_group,
                                option,
                                ..
                            } => {
                                progress.fixed(slot, decoded, count_group, option)?;
                            }
                            schema::Cell::Dynamic { group, ordinal, .. } => {
                                let index = if ordinal {
                                    let key::KeyValue::Unsigned(value) = parsed.value else {
                                        return Err(invalid());
                                    };
                                    Some(value)
                                } else {
                                    None
                                };
                                progress.dynamic(group, index)?;
                            }
                            schema::Cell::Group(_) => return Err(invalid()),
                        }
                        if let Some((child_kind, child_root)) = child {
                            self.verify_record(child_kind, &child_root, child_scans)?;
                        }
                    }
                    _ => return Err(invalid()),
                }
                Ok(())
            },
            || (cancel_ref.borrow_mut())(),
        );
        let state_error = (self.take_state_error)();
        if let Some(error) = state_error {
            return Err(error);
        }
        map_scan_result(result)?;
        progress.finish(record)
    }

    fn read_cell(
        &mut self,
        record: Record,
        value_root: &[u8; 32],
        field_key: &WorldFieldKeyV1,
        owner_key: [u8; 32],
    ) -> Result<ReadCellResult, ValueTreeError> {
        let mut decoder = key::KeyDecoder::new();
        let cancel = self.cancelled;
        let mut cancelled = || (cancel.borrow_mut())();
        let info = read_member_record_key_v1(
            value_root,
            field_key,
            self.values,
            MemberRecordLimitsV1 {
                semantic_bytes: self.semantic_bytes,
                payload_bytes: self.max_payload_bytes,
                record_bytes: self.record_bytes,
            },
            &mut decoder,
            &mut cancelled,
        )?;
        let parsed = decoder.finish().map_err(ValueTreeError::Invalid)?;
        let cell = schema::classify(record, WorldFieldRecordKindV1::Entry as u8, parsed)
            .ok_or_else(invalid)?;
        let payload = match cell {
            schema::Cell::Fixed { payload, .. } | schema::Cell::Dynamic { payload, .. } => payload,
            schema::Cell::Group(_) => return Err(invalid()),
        };
        match payload {
            schema::Payload::Root(child) => {
                let mut capture = PayloadCapture::default();
                let cancel = self.cancelled;
                let mut cancelled = || (cancel.borrow_mut())();
                let payload_info = read_member_record_v1(
                    value_root,
                    field_key,
                    self.values,
                    MemberRecordLimitsV1 {
                        semantic_bytes: self.semantic_bytes,
                        payload_bytes: self.max_payload_bytes,
                        record_bytes: self.record_bytes,
                    },
                    &mut io::sink(),
                    &mut capture,
                    &mut cancelled,
                )?;
                if payload_info != info {
                    return Err(invalid());
                }
                if info.codec != MemberPayloadCodecV1::RecordRoot
                    || info.payload_bytes != RECORD_ROOT_BYTES as u64
                    || capture.len != RECORD_ROOT_BYTES
                    || capture.overflow
                {
                    return Err(invalid());
                }
                let child_record = match child {
                    schema::Child::Exact(child) => child,
                    schema::Child::DeltaEntry => {
                        let records = [
                            Record::DeltaRuntimeResource,
                            Record::DeltaMaterialLedger,
                            Record::DeltaChunkResource,
                        ];
                        let reference = RecordRootRefV1::from_canonical_bytes(&capture.bytes)
                            .map_err(|_| invalid())?;
                        records
                            .into_iter()
                            .find(|candidate| reference.schema == candidate.schema())
                            .ok_or_else(invalid)?
                    }
                };
                let reference =
                    RecordRootRefV1::from_canonical_bytes(&capture.bytes).map_err(|_| invalid())?;
                let child_root = reference
                    .validate_binding(
                        &child_record.schema(),
                        &owner_key,
                        StateNamespaceV1::Artifacts,
                    )
                    .map_err(|_| invalid())?
                    .clone();
                Ok((
                    cell,
                    parsed,
                    Decoded::Other,
                    Some((child_record, child_root)),
                ))
            }
            scalar => {
                let codec = scalar.codec().ok_or_else(invalid)?;
                if info.codec != codec {
                    return Err(invalid());
                }
                let (integer_max, labels) = match scalar {
                    schema::Payload::U64 { maximum } => (maximum.unwrap_or(u64::MAX), None),
                    schema::Payload::Text { labels } => (u64::MAX, labels),
                    _ => (u64::MAX, None),
                };
                let mut small_text = SmallText::default();
                if labels.is_some() && info.payload_bytes > small_text.bytes.len() as u64 {
                    return Err(invalid());
                }
                let cancel = self.cancelled;
                let mut cancelled = || (cancel.borrow_mut())();
                let mut reader = InventoryValueReader {
                    values: self.values,
                    key_work: self.key_work,
                    semantic_bytes: self.semantic_bytes,
                    record_bytes: self.record_bytes,
                    cancelled: &mut cancelled,
                };
                let mut output = ProfileOutput {
                    small: labels.map(|_| &mut small_text),
                };
                let decoded = read_record_value_v1(
                    value_root,
                    &(field_key.clone(), info.semantic_bytes),
                    &mut reader,
                    self.max_payload_bytes,
                    &mut output,
                    RecordReadProfileV1 {
                        codec,
                        nullable: false,
                        integer_max,
                        labels: RECORD_LABELS,
                    },
                )?;
                if let Some(allowed) = labels {
                    let text = std::str::from_utf8(&small_text.bytes[..small_text.len])
                        .map_err(|_| invalid())?;
                    if small_text.overflow || !allowed.contains(&text) {
                        return Err(invalid());
                    }
                }
                Ok((cell, parsed, decoded_value(decoded)?, None))
            }
        }
    }
}

struct PayloadCapture {
    bytes: [u8; RECORD_ROOT_BYTES],
    len: usize,
    overflow: bool,
}
impl Default for PayloadCapture {
    fn default() -> Self {
        Self {
            bytes: [0; RECORD_ROOT_BYTES],
            len: 0,
            overflow: false,
        }
    }
}
impl Write for PayloadCapture {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let remaining = self.bytes.len().saturating_sub(self.len);
        let take = remaining.min(bytes.len());
        self.bytes[self.len..self.len + take].copy_from_slice(&bytes[..take]);
        self.len += take;
        self.overflow |= take != bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Default)]
struct SmallText {
    bytes: [u8; 32],
    len: usize,
    overflow: bool,
}
impl Write for SmallText {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let take = self.bytes.len().saturating_sub(self.len).min(bytes.len());
        self.bytes[self.len..self.len + take].copy_from_slice(&bytes[..take]);
        self.len += take;
        self.overflow |= take != bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct ProfileOutput<'a> {
    small: Option<&'a mut SmallText>,
}
impl Write for ProfileOutput<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Some(small) = self.small.as_deref_mut() {
            small.write(bytes)
        } else {
            Ok(bytes.len())
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        if let Some(small) = self.small.as_deref_mut() {
            small.flush()
        } else {
            Ok(())
        }
    }
}

fn decoded_value(value: RecordValueV1) -> Result<Decoded, ValueTreeError> {
    Ok(match value {
        RecordValueV1::Unsigned(value) => Decoded::Unsigned(value),
        RecordValueV1::Bool(value) => Decoded::Bool(value),
        RecordValueV1::Unit
        | RecordValueV1::Signed(_)
        | RecordValueV1::Text { .. }
        | RecordValueV1::Ledger(_) => Decoded::Other,
    })
}

fn field_binding(
    entry: &StateEntryV1,
) -> Result<Option<crate::incremental_world_field_key::WorldFieldKeyRefV1<'_>>, ValueTreeError> {
    if !entry.key.starts_with(WORLD_FIELD_DOMAIN) {
        return Ok(None);
    }
    WorldFieldKeyV1::from_bytes(&entry.key)
        .map(Some)
        .map_err(ValueTreeError::Invalid)
}

fn field_index(name: &str) -> Option<usize> {
    Field::ALL.iter().position(|field| field.name() == name)
}

fn field_record(field: Field) -> Record {
    match field {
        Field::ChainResourceManifest => Record::ChainResourceManifest,
        Field::LatestChainResourceDelta => Record::ChainResourceDelta,
        Field::BuiltinReleaseManifest => Record::BuiltinReleaseManifest,
    }
}

fn map_scan_result(
    result: Result<u64, StateScanError<ValueTreeError>>,
) -> Result<(), ValueTreeError> {
    match result {
        Ok(_) => Ok(()),
        Err(StateScanError::Visitor(error)) => Err(error),
        Err(StateScanError::Memory(error)) => Err(ValueTreeError::Memory(error)),
        Err(StateScanError::Cancelled) => Err(ValueTreeError::Cancelled),
        Err(StateScanError::ResourceLimited) => Err(ValueTreeError::ResourceLimited(
            "runtime artifact state scan",
        )),
        Err(StateScanError::Invalid(_) | StateScanError::Tree(_)) => Err(invalid()),
    }
}
