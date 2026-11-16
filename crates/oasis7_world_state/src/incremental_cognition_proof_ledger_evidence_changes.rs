//! Notes are issued only after the actual side-ledger edit qualifies. They are
//! data selectors; the real Current owner is still required for every read.
use super::*;
#[derive(Clone, Copy, Debug)]
pub struct ProofLedgerEvidenceChangeV2 {
    field: CognitionProofLedgerFieldV1,
    first: u64,
    end: Option<u64>,
}
impl ProofLedgerEvidenceChangeV2 {
    pub fn field(&self) -> CognitionProofLedgerFieldV1 {
        self.field
    }
    /// None means an actual whole-array replacement, not missing metadata.
    pub fn changed_range(&self) -> Option<std::ops::Range<u64>> {
        self.end.map(|end| self.first..end)
    }
    pub(super) fn plan(
        field: CognitionProofLedgerFieldV1,
        path: &[JsonPathSegmentV1<'_>],
        edit: &JsonSubtreeEditV1<'_>,
    ) -> Self {
        let (first, end) = if let Some(JsonPathSegmentV1::Index(index)) = path.first() {
            (*index, index.checked_add(1))
        } else if path.is_empty() {
            match edit {
                JsonSubtreeEditV1::Append {
                    expected_len,
                    values,
                } => (*expected_len, expected_len.checked_add(values.len() as u64)),
                JsonSubtreeEditV1::Truncate { expected_len, len } => (*len, Some(*expected_len)),
                _ => (0, None),
            }
        } else {
            (0, None)
        };
        Self { field, first, end }
    }
}
impl CurrentCognitionProofLedgerV1 {
    pub fn evidence_change_count_v2<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<usize, ValueTreeError> {
        self.validate_context(tx, common, live)?;
        Ok(self.evidence_changes.len())
    }
    pub fn evidence_change_v2<M: MerkleNodeReader>(
        &self,
        index: usize,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<ProofLedgerEvidenceChangeV2, ValueTreeError> {
        self.validate_context(tx, common, live)?;
        common
            .charge(1, std::mem::size_of::<ProofLedgerEvidenceChangeV2>() as u64)
            .map_err(common_error)?;
        self.evidence_changes
            .get(index)
            .copied()
            .ok_or_else(invalid)
    }
    pub fn evidence_array_len_v2<M: MerkleNodeReader>(
        &self,
        field: CognitionProofLedgerFieldV1,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<u64, ValueTreeError> {
        self.validate_context(tx, common, live)?;
        if field.index() > 1 {
            return Err(invalid());
        }
        // begin accepts the original closed array Q; every edit above rechecks that
        // same container kind. The original JSON grammar is 5 headers + n children.
        self.fields[field.index()]
            .as_ref()
            .ok_or_else(invalid)?
            .root()
            .leaf_count
            .checked_sub(5)
            .ok_or_else(invalid)
    }
}

/// Preserve the semantic-key cancellation/resource/memory cause across the
/// record wrapper. Wrapping it as IO would hide the bounded reader's first cause.
fn point_error(error: MemberRecordError) -> ValueTreeError {
    error.into_value_error()
}
#[derive(Clone, Copy)]
enum LedgerPointV3<'a> {
    Row(u64),
    ServiceMember(&'a str),
}
impl CurrentCognitionProofLedgerV1 {
    /// Reopen one complete row; actual array absence remains distinct from an
    /// unread source. The original V1/V2 call and its grammar are unchanged.
    #[expect(
        clippy::too_many_arguments,
        reason = "Bounded inputs are independently validated"
    )]
    pub fn read_evidence_row_v2<M, P, R, E, C>(
        &self,
        field: CognitionProofLedgerFieldV1,
        ordinal: u64,
        tx: &ExecutionTransaction<'_, M>,
        read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
        json: &mut JsonReadWorkV1,
        path: &mut crate::incremental_json_records::JsonScalarPathReadWorkV1,
        max_heap: usize,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Option<ReservedValue<serde_json::Value>>, ValueTreeError>
    where
        M: MerkleNodeReader,
        P: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    {
        self.read_fixed_point_v3(
            field,
            LedgerPointV3::Row(ordinal),
            tx,
            read,
            json,
            path,
            max_heap,
            common,
            live,
        )
    }
    /// Fixed service fields only. Preserve the original legal Value domain:
    /// absent/non-object maps to no member, never a fabricated empty object.
    /// Legacy manifests lacking these fields remain an explicit missing source.
    #[expect(
        clippy::too_many_arguments,
        reason = "Bounded inputs are independently validated"
    )]
    pub fn read_service_object_member_v3<M, P, R, E, C>(
        &self,
        field: CognitionProofLedgerFieldV1,
        key: &str,
        tx: &ExecutionTransaction<'_, M>,
        read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
        json: &mut JsonReadWorkV1,
        path: &mut crate::incremental_json_records::JsonScalarPathReadWorkV1,
        max_heap: usize,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Option<ReservedValue<serde_json::Value>>, ValueTreeError>
    where
        M: MerkleNodeReader,
        P: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    {
        self.read_fixed_point_v3(
            field,
            LedgerPointV3::ServiceMember(key),
            tx,
            read,
            json,
            path,
            max_heap,
            common,
            live,
        )
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Bounded inputs are independently validated"
    )]
    fn read_fixed_point_v3<M, P, R, E, C>(
        &self,
        field: CognitionProofLedgerFieldV1,
        point: LedgerPointV3<'_>,
        tx: &ExecutionTransaction<'_, M>,
        read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
        json: &mut JsonReadWorkV1,
        path: &mut crate::incremental_json_records::JsonScalarPathReadWorkV1,
        max_heap: usize,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Option<ReservedValue<serde_json::Value>>, ValueTreeError>
    where
        M: MerkleNodeReader,
        P: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    {
        self.validate_context(tx, common, live)?;
        let allowed = match point {
            LedgerPointV3::Row(_) => field.index() < 2,
            LedgerPointV3::ServiceMember(_) => matches!(
                field,
                CognitionProofLedgerFieldV1::ServiceCausalProposals
                    | CognitionProofLedgerFieldV1::ActionDigests
            ),
        };
        if !allowed
            || read.values.class != MemoryClass::Ordinary
            || !read.values.memory.same_pool(common.memory())
        {
            return Err(invalid());
        }
        if field.index() >= self.wire_version.field_count() {
            return Err(ValueTreeError::Invalid(
                "legacy ProofLedger cognition service field source missing",
            ));
        }
        let Some(parent) = self.fields[field.index()].as_ref() else {
            return match point {
                LedgerPointV3::Row(_) => Err(invalid()),
                LedgerPointV3::ServiceMember(_) => Ok(None),
            };
        };
        if let LedgerPointV3::Row(ordinal) = point {
            let count = parent
                .root()
                .leaf_count
                .checked_sub(5)
                .ok_or_else(invalid)?;
            if ordinal >= count {
                return Ok(None);
            }
        }
        let p = profile(field);
        let jp = JsonRecordProfileV1 {
            field: p.field,
            schema: p.schema,
        };
        let (_, meter, stop) = common.parts();
        if stop() {
            return Err(ValueTreeError::Cancelled);
        }
        let lease = meter.begin_child().map_err(common_error)?;
        let original = [
            read.values.io.nodes_remaining,
            json.cells_remaining,
            json.json_nodes_remaining,
            path.cells_remaining,
            path.json_nodes_remaining,
            read.values.io.read_bytes_remaining,
            read.key_work.bytes_remaining,
        ];
        let step = lease.remaining_steps() / 5;
        let byte = lease.remaining_read_bytes() / 2;
        read.values.io.nodes_remaining = read.values.io.nodes_remaining.min(step);
        json.cells_remaining = json.cells_remaining.min(step);
        json.json_nodes_remaining = json.json_nodes_remaining.min(step);
        path.cells_remaining = path.cells_remaining.min(step);
        path.json_nodes_remaining = path.json_nodes_remaining.min(step);
        read.values.io.read_bytes_remaining = read.values.io.read_bytes_remaining.min(byte);
        read.key_work.bytes_remaining = read.key_work.bytes_remaining.min(byte);
        let cap = [
            read.values.io.nodes_remaining,
            json.cells_remaining,
            json.json_nodes_remaining,
            path.cells_remaining,
            path.json_nodes_remaining,
            read.values.io.read_bytes_remaining,
            read.key_work.bytes_remaining,
        ];
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || -> Result<Option<ReservedValue<serde_json::Value>>, ValueTreeError> {
                let mut cancelled = || stop() || (read.cancelled)();
                let mut selected = WorldFieldReadContextV1 {
                    root: parent.root(),
                    state: read.state,
                    values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                        source: read.values.source,
                        memory: read.values.memory,
                        class: read.values.class,
                        io: &mut *read.values.io,
                    },
                    key_work: &mut *read.key_work,
                    semantic_bytes: read.semantic_bytes,
                    record_bytes: read.record_bytes,
                    take_state_error: &mut read.take_state_error,
                    cancelled: &mut cancelled,
                };
                let q = match point {
                    LedgerPointV3::Row(ordinal) => {
                        Some(crate::incremental_json_records::qualify_json_path_v1(
                            jp,
                            parent,
                            &[JsonPathSegmentV1::Index(ordinal)],
                            &mut selected,
                            path,
                        ))
                    }
                    LedgerPointV3::ServiceMember(key) => {
                        let value=crate::incremental_json_records::qualify_optional_json_object_member_records_v1(jp,parent,key,&mut selected.records(),json).map_err(point_error)?;
                        value.map(Ok)
                    }
                };
                let Some(q) = q else { return Ok(None) };
                let q = q.map_err(point_error)?;
                let (value, fee) = crate::incremental_json_records::materialize_records(
                    jp,
                    &q,
                    &mut selected.records(),
                    json,
                    max_heap,
                )?;
                Ok(Some(fee.retain(value)))
            },
        ));
        let result = match attempt {
            Ok(result) => result,
            Err(panic) => {
                read.values.io.nodes_remaining = 0;
                json.cells_remaining = 0;
                json.json_nodes_remaining = 0;
                path.cells_remaining = 0;
                path.json_nodes_remaining = 0;
                read.values.io.read_bytes_remaining = 0;
                read.key_work.bytes_remaining = 0;
                drop(lease);
                std::panic::resume_unwind(panic)
            }
        };
        let mut steps = 0u64;
        let mut bytes = 0u64;
        let mut broken = false;
        for (i, lane) in [
            &mut read.values.io.nodes_remaining,
            &mut json.cells_remaining,
            &mut json.json_nodes_remaining,
            &mut path.cells_remaining,
            &mut path.json_nodes_remaining,
            &mut read.values.io.read_bytes_remaining,
            &mut read.key_work.bytes_remaining,
        ]
        .into_iter()
        .enumerate()
        {
            broken |= *lane > cap[i];
            let used = cap[i].saturating_sub(*lane);
            *lane = original[i] - used;
            let total = if i < 5 { &mut steps } else { &mut bytes };
            if let Some(next) = total.checked_add(used) {
                *total = next
            } else {
                broken = true
            }
        }
        let paid = lease.consume(steps, bytes).map_err(common_error);
        lease.finish();
        match result {
            Err(error) => Err(error),
            Ok(value) => {
                paid?;
                if broken {
                    return Err(invalid());
                }
                if stop() {
                    return Err(ValueTreeError::Cancelled);
                }
                Ok(value)
            }
        }
    }
}
impl CurrentCognitionProofLedgerV1 {
    /// Borrowed root DATA for the fixed paired cache-freeze comparison only.
    pub fn evidence_field_root_v2<M: MerkleNodeReader>(
        &self,
        field: CognitionProofLedgerFieldV1,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<&StateRootV1, ValueTreeError> {
        self.validate_context(tx, common, live)?;
        if field.index() > 1 {
            return Err(invalid());
        }
        Ok(self.fields[field.index()]
            .as_ref()
            .ok_or_else(invalid)?
            .root())
    }
}
