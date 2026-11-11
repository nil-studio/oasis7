//! Logical typed-index provenance from the accepted v3 attestation only.
//! This is deliberately NOT the old full-read index qualification or a lease.
use super::*;
use crate::incremental_field_read::WorldFieldReadContextV1;
use oasis7_formal_protocol::{
    incremental_state::{MerkleNodeReader, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
#[path = "incremental_cognition_history_candidate_index_v3.rs"]
mod candidate_index_v3;
pub use candidate_index_v3::*;

pub struct AttestedCognitionHistoryIndexV3 {
    root: StateRootV1,
    field: CognitionHistoryFieldV1,
    records: u64,
    class: MemoryClass,
    head: [u8; 32],
    roots: [u8; 32],
}
impl AttestedCognitionHistoryIndexV3 {
    pub fn root(&self) -> &StateRootV1 {
        &self.root
    }
    pub fn field(&self) -> CognitionHistoryFieldV1 {
        self.field
    }
    pub fn record_count(&self) -> u64 {
        self.records
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub fn head_record_id(&self) -> [u8; 32] {
        self.head
    }
    pub fn roots_id(&self) -> [u8; 32] {
        self.roots
    }
}
pub(crate) fn validate_accepted_v3_current(
    view: &ReservedValue<AcceptedCognitionHistoryViewV1>,
    current: &ReservedValue<CognitionHistoryCurrentBindingV1>,
    memory: &MemoryBudget,
) -> Result<(), ValueTreeError> {
    let invalid = || ValueTreeError::Invalid("accepted v3 current source");
    if !view.uses_budget(memory)
        || view.reserved_bytes() < 16 << 10
        || !view.occurrence().uses_budget(memory)
        || view.occurrence().reserved_bytes()
            < std::mem::size_of::<
                oasis7_formal_protocol::incremental_material::VerifiedFactOccurrenceV1,
            >()
        || view.occurrence().allocation_class() != view.class()
        || view.occurrence().roots_id() != view.manifest().roots
        || view.occurrence().head_body_id() != view.occurrence().fact_body_id()
        || !current.uses_budget(memory)
        || current.reserved_bytes() < 1024
        || current.class() != view.class()
        || current.head_record_id() != view.occurrence().head_record_id()
        || current.roots_id() != view.manifest().roots
        || current.enabled() != view.manifest().enabled
    {
        return Err(invalid());
    }
    let source = view.source_v3().ok_or_else(invalid)?;
    if !source.uses_budget(memory)
        || source.reserved_bytes() < COGNITION_HISTORY_SOURCE_SUMMARY_ADMISSION_V3_BYTES
    {
        return Err(invalid());
    }
    source.validate_shape(view.manifest(), view.derived().ok_or_else(invalid)?)
}
pub fn recover_attested_cognition_history_index_v3(
    field: CognitionHistoryFieldV1,
    view: &ReservedValue<AcceptedCognitionHistoryViewV1>,
    current: &ReservedValue<CognitionHistoryCurrentBindingV1>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<AttestedCognitionHistoryIndexV3>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    validate_accepted_v3_current(view, current, memory)?;
    let declared = view.manifest().index_ref(field);
    if !declared.present {
        return Err(ValueTreeError::Invalid("absent attested index"));
    }
    let charge = memory.reserve(16 << 10, view.class())?;
    let result = AttestedCognitionHistoryIndexV3 {
        root: declared.root.clone(),
        field,
        records: declared.records,
        class: view.class(),
        head: view.occurrence().head_record_id(),
        roots: view.manifest().roots,
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(result))
}
pub fn read_attested_cognition_history_index_point_v3<M, R, E, C>(
    index: &ReservedValue<AttestedCognitionHistoryIndexV3>,
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
    if !index.uses_budget(context.values.memory)
        || index.reserved_bytes() < 16 << 10
        || index.class != context.values.class
        || ordinal >= index.records
    {
        return Err(ValueTreeError::Invalid("attested index point binding"));
    }
    super::index::read_cognition_history_index_point_parts_v1(
        &index.root,
        index.field,
        ordinal,
        context,
        work,
    )
}

/// Resource migration of the same logical v3 index. This copies only its fixed
/// descriptor after binding it to the actual Ordinary selected occurrence; it
/// neither traverses H nor upgrades the old full-read index qualification.
pub fn readmit_attested_cognition_history_index_ordinary_v3(
    source: &ReservedValue<AttestedCognitionHistoryIndexV3>,
    view: &ReservedValue<AcceptedCognitionHistoryViewV1>,
    current: &ReservedValue<CognitionHistoryCurrentBindingV1>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<AttestedCognitionHistoryIndexV3>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    validate_accepted_v3_current(view, current, memory)?;
    let declared = view.manifest().index_ref(source.field);
    if !source.uses_budget(memory)
        || source.reserved_bytes() < 16 << 10
        || source.class != MemoryClass::Recovery
        || view.class() != MemoryClass::Ordinary
        || !declared.present
        || source.root != declared.root
        || source.records != declared.records
        || source.head != view.occurrence().head_record_id()
        || source.roots != view.manifest().roots
    {
        return Err(ValueTreeError::Invalid("attested index resource source"));
    }
    let charge = memory.reserve(16 << 10, MemoryClass::Ordinary)?;
    let result = AttestedCognitionHistoryIndexV3 {
        root: source.root.clone(),
        field: source.field,
        records: source.records,
        class: MemoryClass::Ordinary,
        head: source.head,
        roots: source.roots,
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(result))
}
