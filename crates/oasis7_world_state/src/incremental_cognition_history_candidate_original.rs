//! Candidate provenance is separate from accepted History authority.
use super::*;
use crate::incremental_cognition_history_cbor::CognitionHistoryRecordSchemaV1;
use oasis7_formal_protocol::{
    incremental_material::CheckedCandidateFactOccurrenceV1, incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

pub struct CheckedCognitionIndexedOriginalV3 {
    point: ReservedValue<CognitionHistoryIndexPointV1>,
    schema: ReservedValue<CognitionHistoryRecordSchemaV1>,
    record: [u8; 32],
    roots: [u8; 32],
    body: [u8; 32],
}
impl CheckedCognitionIndexedOriginalV3 {
    pub fn ordinal(&self) -> u64 {
        self.point.ordinal()
    }
    pub fn row(&self) -> CognitionHistoryIndexRowV1 {
        self.point.row()
    }
    pub fn schema(&self) -> &CognitionHistoryRecordSchemaV1 {
        &self.schema
    }
    pub(crate) fn schema_carrier(&self) -> &ReservedValue<CognitionHistoryRecordSchemaV1> {
        &self.schema
    }
    pub fn candidate_record_id(&self) -> [u8; 32] {
        self.record
    }
    pub fn roots_id(&self) -> [u8; 32] {
        self.roots
    }
    pub fn body_id(&self) -> [u8; 32] {
        self.body
    }
}
pub fn bind_checked_cognition_indexed_original_v3(
    point: ReservedValue<CognitionHistoryIndexPointV1>,
    schema: ReservedValue<CognitionHistoryRecordSchemaV1>,
    occurrence: &ReservedValue<CheckedCandidateFactOccurrenceV1>,
    selected: &ReservedValue<CheckedCognitionHistoryViewV1>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CheckedCognitionIndexedOriginalV3>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let selected_fact = selected.occurrence();
    let semantic = schema.semantic();
    let index = selected.manifest().index_ref(point.field);
    if !point.uses_budget(memory)
        || point.reserved_bytes() < 16 << 10
        || point.class != MemoryClass::Ordinary
        || !schema.uses_budget(memory)
        || schema.reserved_bytes() < 1024
        || semantic.class() != MemoryClass::Ordinary
        || !occurrence.uses_budget(memory)
        || occurrence.reserved_bytes() < std::mem::size_of::<CheckedCandidateFactOccurrenceV1>()
        || !selected.uses_budget(memory)
        || selected.reserved_bytes() < 16 << 10
        || !selected_fact.uses_budget(memory)
        || selected_fact.reserved_bytes() < std::mem::size_of::<CheckedCandidateFactOccurrenceV1>()
        || selected_fact.kind() != ExecutionFactKindV1::Result
        || occurrence.kind() != point.field.fact_kind()
        || occurrence.candidate_record_id() != selected_fact.candidate_record_id()
        || occurrence.roots_id() != selected_fact.roots_id()
        || occurrence.roots_id() != selected.manifest().roots
        || occurrence.body_id() != selected_fact.body_id()
        || occurrence.location() != point.row.location
        || occurrence.original_bytes() != point.row.encoded_bytes
        || semantic.field() != point.field
        || semantic.id() != &point.row.location.original
        || semantic.bytes() != point.row.encoded_bytes
        || !index.present
        || point.root != index.root
        || point.ordinal >= index.records
    {
        return Err(ValueTreeError::Invalid(
            "checked Cognition original binding",
        ));
    }
    let charge = memory.reserve(2048, MemoryClass::Ordinary)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(CheckedCognitionIndexedOriginalV3 {
        record: occurrence.candidate_record_id(),
        roots: occurrence.roots_id(),
        body: occurrence.body_id(),
        point,
        schema,
    }))
}
