//! Recover Runtime source from a real accepted Result occurrence and pages.
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFactKindV1, RootSetV1},
    incremental_material::*,
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_history_index::RuntimeHistoryIndexWorkV1,
    incremental_runtime_history_manifest::{
        RUNTIME_HISTORY_MANIFEST_BYTES_V1, RuntimeHistoryManifestV1,
    },
    incremental_runtime_history_source::{
        AcceptedRuntimeHistorySourceV1, recover_runtime_history_source_v1,
    },
};
#[allow(clippy::too_many_arguments)]
pub(super) fn recover_runtime<O, P, M, R, E, C, F>(
    head: &VerifiedFormalCommitV1,
    roots: &RootSetV1,
    hint: FactLocationV1,
    facts: &mut FactReadContextV1<'_, O, P, F>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeHistoryIndexWorkV1,
) -> Result<ReservedValue<AcceptedRuntimeHistorySourceV1>, FormalError>
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
    read.check()
        .map_err(|e| invalid(format!("IWS cold Runtime context: {e}")))?;
    if facts.class != MemoryClass::Recovery
        || read.values.class != MemoryClass::Recovery
        || !facts.memory.same_pool(read.values.memory)
    {
        return Err(invalid("IWS cold Runtime source phase/pool"));
    }
    let selected = verify_fact_occurrence_v1(
        &FactRequestV1 {
            kind: ExecutionFactKindV1::Result,
            head,
            roots,
            location: hint,
        },
        facts,
    )
    .map_err(|e| invalid(format!("IWS cold Runtime actual Result: {e:?}")))?;
    let raw = facts
        .originals
        .read_original(
            &hint.original,
            RUNTIME_HISTORY_MANIFEST_BYTES_V1,
            facts.memory,
        )
        .map_err(|e| invalid(format!("IWS cold Runtime original: {e:?}")))?;
    if !raw.uses_budget(facts.memory)
        || raw.len() != RUNTIME_HISTORY_MANIFEST_BYTES_V1
        || blake3::hash(&raw).as_bytes() != &hint.original
    {
        return Err(invalid("IWS cold Runtime manifest bytes"));
    }
    let manifest = RuntimeHistoryManifestV1::from_canonical_bytes(&raw)
        .map_err(|e| invalid(format!("IWS cold Runtime manifest framing: {e}")))?;
    recover_runtime_history_source_v1(head, roots, &manifest, selected, read, work)
        .map_err(|e| invalid(format!("IWS cold Runtime source: {e:?}")))
}
