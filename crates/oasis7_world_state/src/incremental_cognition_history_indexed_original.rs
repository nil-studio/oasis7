//! Join an authenticated index point, a real accepted occurrence and the
//! streamed original's local schema. This is one record, not a complete
//! History/lifecycle/envelope/selected durable World admission.
use super::*;
use crate::incremental_cognition_history_cbor::CognitionHistoryRecordSchemaV1;
use oasis7_formal_protocol::{
    incremental_material::VerifiedFactOccurrenceV1, incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, ReservedValue};

pub struct IndexedCognitionOriginalV1 {
    point: ReservedValue<CognitionHistoryIndexPointV1>,
    schema: ReservedValue<CognitionHistoryRecordSchemaV1>,
    head_record: [u8; 32],
    roots: [u8; 32],
    head_body: [u8; 32],
}
impl IndexedCognitionOriginalV1 {
    pub fn ordinal(&self) -> u64 {
        self.point.ordinal
    }
    pub fn row(&self) -> CognitionHistoryIndexRowV1 {
        self.point.row
    }
    pub(crate) fn schema_carrier(&self) -> &ReservedValue<CognitionHistoryRecordSchemaV1> {
        &self.schema
    }
    pub fn schema(&self) -> &CognitionHistoryRecordSchemaV1 {
        &self.schema
    }
    pub fn head_record_id(&self) -> &[u8; 32] {
        &self.head_record
    }
    pub fn roots_id(&self) -> &[u8; 32] {
        &self.roots
    }
    pub fn head_body_id(&self) -> &[u8; 32] {
        &self.head_body
    }
}

/// Consume actual retained point/schema carriers; a copied public row or a
/// callback returning Ok cannot mint this join. MissingIdentity and response
/// WrapperOnly remain visible and do not become business semantic validity.
pub fn bind_indexed_cognition_original_v1(
    point: ReservedValue<CognitionHistoryIndexPointV1>,
    schema: ReservedValue<CognitionHistoryRecordSchemaV1>,
    occurrence: &ReservedValue<VerifiedFactOccurrenceV1>,
    selected: &ReservedValue<AcceptedCognitionHistoryViewV1>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<IndexedCognitionOriginalV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let semantic = schema.semantic();
    let selected_fact = selected.occurrence();
    let class = selected.class();
    let index = selected.manifest().index_ref(point.field);
    if !point.uses_budget(memory)
        || point.reserved_bytes() < 16 << 10
        || !schema.uses_budget(memory)
        || schema.reserved_bytes() < 1024
        || !occurrence.uses_budget(memory)
        || occurrence.reserved_bytes() < std::mem::size_of::<VerifiedFactOccurrenceV1>()
        || !selected.uses_budget(memory)
        || selected.reserved_bytes() < 16 << 10
        || !selected_fact.uses_budget(memory)
        || selected_fact.reserved_bytes() < std::mem::size_of::<VerifiedFactOccurrenceV1>()
        || point.class != class
        || semantic.class() != class
        || occurrence.allocation_class() != class
        || selected_fact.allocation_class() != class
        || !index.present
        || point.root != index.root
        || point.ordinal >= index.records
        || semantic.field() != point.field
        || occurrence.kind() != point.field.fact_kind()
        || occurrence.location() != point.row.location
        || occurrence.original_bytes() != point.row.encoded_bytes
        || semantic.id() != &point.row.location.original
        || semantic.bytes() != point.row.encoded_bytes
        || occurrence.head_record_id() != selected_fact.head_record_id()
        || occurrence.roots_id() != selected_fact.roots_id()
        || occurrence.head_body_id() != selected_fact.head_body_id()
    {
        return Err(ValueTreeError::Invalid(
            "Cognition indexed original binding",
        ));
    }
    let charge = memory.reserve(2048, class)?;
    let result = IndexedCognitionOriginalV1 {
        head_record: occurrence.head_record_id(),
        roots: occurrence.roots_id(),
        head_body: occurrence.head_body_id(),
        point,
        schema,
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(result))
}
