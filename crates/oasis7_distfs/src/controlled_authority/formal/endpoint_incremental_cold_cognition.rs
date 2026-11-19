//! Actual accepted view, Current binding, index points and cache frontiers.
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFactKindV1, RootSetV1},
    incremental_material::*,
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_cognition_history::*,
    incremental_cognition_history_hash_outboard::CognitionHeadHashWorkV1,
    incremental_cognition_lifecycle_qualification_v2::recover_attested_cognition_lifecycle_v3,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_current_complete::WorldCurrentStateQualificationV1,
};
#[allow(clippy::too_many_arguments)]
pub(super) fn recover_cognition<O, P, M, R, E, C, F>(
    head: &VerifiedFormalCommitV1,
    roots: &RootSetV1,
    hint: FactLocationV1,
    current_state: &ReservedValue<WorldCurrentStateQualificationV1>,
    facts: &mut FactReadContextV1<'_, O, P, F>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    index_work: &mut CognitionHistoryIndexWorkV1,
    hash_work: &mut CognitionHeadHashWorkV1,
) -> Result<ReservedValue<CompleteCognitionHistorySourceV3>, FormalError>
where
    O: OriginalObjectStreamReader,
    O::Error: std::fmt::Debug,
    P: MerkleNodeReader,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    F: FnMut() -> bool,
{
    macro_rules! value {
        ($e:expr) => {
            $e.map_err(|e| invalid(format!("IWS actual cold Cognition: {e}")))?
        };
    }
    value!(read.check());
    if facts.class != MemoryClass::Recovery
        || read.values.class != MemoryClass::Recovery
        || !facts.memory.same_pool(read.values.memory)
        || current_state.allocation_class() != MemoryClass::Recovery
    {
        return Err(invalid("IWS cold Cognition phase/pool"));
    }
    current_state
        .validate_binding(roots)
        .map_err(|_| invalid("IWS cold Cognition Current roots"))?;
    let memory = read.values.memory;
    let selected = recover_selected_cognition_history_view_v1(
        &FactRequestV1 {
            kind: ExecutionFactKindV1::Result,
            head,
            roots,
            location: hint,
        },
        facts,
    )
    .map_err(|e| invalid(format!("IWS cold actual selected Cognition: {e:?}")))?;
    let current = value!(bind_cognition_history_current_v1(
        &selected,
        roots,
        current_state.cognition().history_source(),
        read
    ));
    let mut indexes = std::array::from_fn(|_| None);
    for (slot, field) in CognitionHistoryFieldV1::ALL.into_iter().enumerate() {
        if selected.manifest().index_ref(field).present {
            indexes[slot] = Some(value!(recover_attested_cognition_history_index_v3(
                field,
                &selected,
                &current,
                memory,
                &mut read.cancelled
            )));
        }
    }
    let mut heads = std::array::from_fn(|_| None);
    let (lifecycle, disabled) = if selected.manifest().enabled {
        let refs = selected
            .derived()
            .ok_or_else(|| invalid("IWS cold v3 derived references missing"))?;
        let mut caches = std::array::from_fn(|_| None);
        for (slot, id) in refs.caches.into_iter().enumerate() {
            if let Some(id) = id {
                caches[slot] = Some(value!(recover_cognition_head_cache_v1(
                    id,
                    &mut read.values,
                    hash_work,
                    &mut read.cancelled
                )));
            }
        }
        let caches = value!(bind_persisted_cognition_head_caches_v2(
            caches,
            &selected,
            &current,
            memory,
            hash_work,
            &mut read.cancelled
        ));
        for (slot, field) in [
            CognitionHistoryFieldV1::JournalEvents,
            CognitionHistoryFieldV1::EconomyJournal,
            CognitionHistoryFieldV1::EconomyProvisionJournal,
        ]
        .into_iter()
        .enumerate()
        {
            let declared = selected.manifest().index_ref(field);
            if !declared.present {
                continue;
            }
            let index = indexes[slot]
                .as_ref()
                .ok_or_else(|| invalid("IWS cold required index missing"))?;
            let last = if declared.records == 0 {
                None
            } else {
                Some(value!(read_attested_cognition_history_index_point_v3(
                    index,
                    declared.records - 1,
                    read,
                    index_work
                )))
            };
            heads[slot] = Some(value!(recover_attested_cognition_head_source_v3(
                field,
                &selected,
                &current,
                &caches,
                last.as_ref(),
                memory,
                hash_work,
                &mut read.cancelled
            )));
        }
        (
            Some(value!(recover_attested_cognition_lifecycle_v3(
                &selected,
                &current,
                memory,
                &mut read.cancelled
            ))),
            None,
        )
    } else {
        (
            None,
            Some(value!(recover_disabled_cognition_history_source_v3(
                &selected,
                &current,
                memory,
                &mut read.cancelled
            ))),
        )
    };
    let result = value!(recover_complete_cognition_history_source_v3(
        RecoveredCognitionHistoryComponentsV3 {
            selected,
            current,
            indexes,
            heads,
            lifecycle,
            disabled
        },
        memory,
        hash_work,
        &mut read.cancelled
    ));
    value!(read.check());
    Ok(result)
}
