//! Independent response-only index successor replay. This does not certify
//! response business semantics, historical lifecycle closure or HistoryReady.
use super::*;
use crate::incremental_field_member::{MemberRecordError, MemberRecordWriteContext};
use crate::incremental_field_read::WorldFieldReadContextV1;
use oasis7_formal_protocol::{
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};

pub fn verify_cognition_response_index_successor_v1<M, R, E, C, W, D>(
    parent: &ReservedValue<AcceptedCognitionHistoryViewV1>,
    after: &ReservedValue<AcceptedCognitionHistoryViewV1>,
    parent_index: &ReservedValue<CognitionHistoryIndexQualificationV1>,
    original: &ReservedValue<IndexedCognitionOriginalV1>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, D>,
    work: &mut CognitionHistoryIndexWorkV1,
) -> Result<ReservedValue<CognitionHistoryIndexQualificationV1>, MemberRecordError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    D: FnMut() -> bool,
{
    read.check()?;
    let memory = read.values.memory;
    let before = parent.manifest();
    let next = after.manifest();
    let field = CognitionHistoryFieldV1::Responses;
    if !parent.uses_budget(memory)
        || !after.uses_budget(memory)
        || !original.uses_budget(memory)
        || parent.class() != MemoryClass::Ordinary
        || after.class() != MemoryClass::Ordinary
        || parent.reserved_bytes() < 16 << 10
        || after.reserved_bytes() < 16 << 10
        || original.reserved_bytes() < 2048
        || !before.enabled
        || !next.enabled
        || before.heads != next.heads
        || parent.derived() != after.derived()
        || before.revision.checked_add(1) != Some(next.revision)
        || next.previous_view != Some(parent.occurrence().location())
        || parent.occurrence().location().height.checked_add(1)
            != Some(after.occurrence().location().height)
        || original.head_record_id() != &after.occurrence().head_record_id()
        || original.head_body_id() != &after.occurrence().head_body_id()
        || original.roots_id() != &next.roots
        || original.schema().semantic().class() != MemoryClass::Ordinary
        || original.schema().semantic().field() != field
        || original.ordinal() != before.index_ref(field).records
        || original.row().location.height != after.occurrence().location().height
        || CognitionHistoryFieldV1::ALL
            .iter()
            .any(|f| *f != field && before.index_ref(*f) != next.index_ref(*f))
    {
        return Err(MemberRecordError::Invalid(
            "Cognition response successor binding",
        ));
    }
    parent_index.validate_binding(field, before.index_ref(field))?;
    let replay =
        prepare_cognition_history_index_append_v1(parent_index, original.row(), read, write, work)?;
    replay.validate_binding(field, next.index_ref(field))?;
    read.check()?;
    if (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(replay)
}
