//! Cold readmission consumes actual accepted History proofs, never CAS hints.
//! Obtaining those Recovery proofs still requires their real page/original
//! readers. No public constructor permits a caller to substitute a success flag.
use super::{
    provisional::{AcceptedCandidateCache, AcceptedCurrentOwner},
    *,
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_cognition_history::*,
    incremental_cognition_history_hash_outboard::CognitionHeadHashWorkV1,
    incremental_field_read::WorldFieldReadContextV1, incremental_runtime_history_source::*,
    incremental_world_current_complete::*,
    incremental_world_raw_complete::MailboxProvenanceScopeV1,
};
#[allow(clippy::too_many_arguments)]
pub(super) fn readmit_recovered_parent<M, R, E, C>(
    head: &VerifiedFormalCommitV1,
    roots: &RootSetV1,
    current: &ReservedValue<WorldCurrentStateQualificationV1>,
    runtime: &ReservedValue<AcceptedRuntimeHistorySourceV1>,
    cognition: &ReservedValue<CompleteCognitionHistorySourceV3>,
    results: &ReservedValue<
        oasis7_world_state::incremental_world_result_router_accepted::AcceptedWorldResultRoutingV1,
    >,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldCurrentStateWorkV1,
    hash_work: &mut CognitionHeadHashWorkV1,
) -> Result<AcceptedCandidateCache, FormalError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()
        .map_err(|e| invalid(format!("IWS cold context: {e}")))?;
    let memory = ctx.values.memory;
    if ctx.values.class != MemoryClass::Ordinary
        || current.allocation_class() != MemoryClass::Recovery
        || results.allocation_class() != MemoryClass::Recovery
        || runtime.allocation_class() != MemoryClass::Recovery
        || cognition.selected().occurrence().allocation_class() != MemoryClass::Recovery
    {
        return Err(invalid(
            "IWS explicit Recovery to Ordinary parent admission",
        ));
    }
    let _phase = memory
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(|_| invalid("IWS cold binding admission"))?;
    let roots_id = roots.digest().map_err(|_| invalid("IWS cold roots"))?;
    let id = head.verified_record_id();
    if !results.uses_budget(memory)
        || results.record_id() != id
        || results.roots_id() != roots_id
        || results.body_id() != runtime.occurrence().head_body_id()
        || results.cognition().occurrence().location()
            != cognition.selected().occurrence().location()
        || results.runtime_history().occurrence().location() != runtime.occurrence().location()
        || runtime.occurrence().head_record_id() != id
        || cognition.selected().occurrence().head_record_id() != id
        || runtime.manifest().roots != roots_id
        || cognition.selected().manifest().roots != roots_id
        || runtime.occurrence().head_body_id() != cognition.selected().occurrence().head_body_id()
    {
        return Err(invalid("IWS cold exact accepted provenance"));
    }
    let results=oasis7_world_state::incremental_world_result_router_accepted::readmit_accepted_world_result_routing_ordinary_v1(
        results,head,roots,memory,&mut ctx.cancelled,
    ).map_err(|e|invalid(format!("IWS cold Result admission: {e:?}")))?;
    current
        .validate_binding(roots)
        .map_err(|_| invalid("IWS cold Current root binding"))?;
    let scope = MailboxProvenanceScopeV1::Accepted { head, roots };
    let current = readmit_world_current_state_ordinary_v1(current, roots, &scope, ctx, work)
        .map_err(|e| invalid(format!("IWS cold Current admission: {e:?}")))?;
    let selected = readmit_cognition_history_view_ordinary_v1(
        cognition.selected(),
        head,
        roots,
        memory,
        &mut ctx.cancelled,
    )
    .map_err(|e| invalid(format!("IWS cold Cognition view admission: {e:?}")))?;
    let binding = bind_cognition_history_current_v1(
        &selected,
        roots,
        current.cognition().history_source(),
        ctx,
    )
    .map_err(|e| invalid(format!("IWS cold actual Current binding: {e}")))?;
    let cognition = readmit_complete_cognition_history_source_ordinary_v3(
        cognition,
        selected,
        binding,
        memory,
        hash_work,
        &mut ctx.cancelled,
    )
    .map_err(|e| invalid(format!("IWS cold complete Cognition admission: {e}")))?;
    let runtime = readmit_runtime_history_source_ordinary_v1(
        runtime,
        head,
        roots,
        memory,
        &mut ctx.cancelled,
    )
    .map_err(|e| invalid(format!("IWS cold Runtime admission: {e:?}")))?;
    let bytes = roots.state.iter().try_fold(
        std::mem::size_of::<RootSetV1>()
            .checked_add(512)
            .and_then(|n| n.checked_add(roots.world_id.len()))
            .ok_or_else(|| invalid("IWS cold root accounting"))?,
        |n, r| {
            n.checked_add(r.node_root.len())
                .and_then(|n| n.checked_add(128))
                .ok_or_else(|| invalid("IWS cold root accounting"))
        },
    )?;
    let roots = memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(|_| invalid("IWS cold roots admission"))?
        .retain(roots.clone());
    ctx.check()
        .map_err(|e| invalid(format!("IWS cold final context: {e}")))?;
    Ok(AcceptedCandidateCache {
        commit_record: id,
        current_owner: AcceptedCurrentOwner::Cold { current, results },
        // Logical Recovery/A alone never establishes local byte continuity.
        complete_data: None,
        local_closure: None,
        recovered_pages: None,
        roots,
        cognition,
        runtime,
    })
}
