//! Actual reader-backed dual History conjunction for one endpoint candidate.
use oasis7_formal_protocol::{
    incremental_material::OriginalObjectStreamReader,
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::{
    incremental_cognition_history::*, incremental_field_member::MemberRecordWriteContext,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_history_index::RuntimeHistoryIndexQualificationV1,
    incremental_runtime_history_source::*,
};
pub(super) struct CheckedSources {
    pub(super) runtime: ReservedValue<CheckedRuntimeHistorySourceV1>,
    pub(super) indexes: [ReservedValue<RuntimeHistoryIndexQualificationV1>; 4],
    pub(super) cognition: ReservedValue<CheckedCognitionHistorySourceV3>,
}
#[allow(clippy::too_many_arguments)]
pub(super) fn read_sources<'a, M, R, E, C, W, D, O, H>(
    runtime_request: &RuntimeHistoryCandidateReadRequestV1<'_>,
    cognition_request: &'a CognitionHistorySelectionRequestV1<'a>,
    selected: ReservedValue<CheckedCognitionHistoryViewV1>,
    current: CognitionHistoryCurrentSourceV1<'_>,
    parent: Option<&ReservedValue<CompleteCognitionHistorySourceV3>>,
    read: &mut WorldFieldReadContextV1<'a, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, D>,
    objects: &mut O,
    history_originals: &mut H,
    runtime_work: &mut RuntimeHistoryCandidateReadWorkV1,
    cognition_work: &mut CognitionHistoryCandidateReaderWorkV3<'_>,
) -> Result<CheckedSources, super::FormalError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    D: FnMut() -> bool,
    O: OriginalObjectStreamReader,
    H: CognitionHistoryOriginalReadV3,
    O::Error: std::fmt::Debug,
{
    if runtime_request.core.candidate_record_id() != cognition_request.core.candidate_record_id() {
        return Err(super::invalid("IWS dual History exact candidate"));
    }
    let runtime = read_candidate_runtime_history_source_v1(
        runtime_request,
        objects,
        read,
        write,
        runtime_work,
    )
    .map_err(|e| super::invalid(format!("IWS Runtime History readers: {e:?}")))?;
    let cognition = check_candidate_complete_cognition_history_from_readers_v3(
        cognition_request,
        selected,
        current,
        parent,
        read,
        write,
        history_originals,
        cognition_work,
    )
    .map_err(|e| super::invalid(format!("IWS Cognition History readers: {e}")))?;
    Ok(CheckedSources {
        runtime: runtime.source,
        indexes: runtime.indexes,
        cognition,
    })
}
