//! Fixed plain/resolved product action state transition. The real selected
//! action admission precedes this producer; no receipt or fake RecipeJob exists.
use super::*;
use oasis7_wasm_abi::{MaterialStack, ProductValidationDecision};
use oasis7_world_state::incremental_world_native::{
    recover_current_latest_product_validation_v1, write_current_latest_product_validation_v1,
};
pub(in crate::world::incremental_normal_action_factory_schedule) fn result<M, P, R, V, E, C, PE, PC>(
    source: &mut current::CurrentFactoryScheduleFactsV1<
        '_,
        '_,
        '_,
        '_,
        '_,
        '_,
        '_,
        M,
        P,
        R,
        V,
        E,
        C,
        PE,
        PC,
    >,
    action_id: u64,
    requester: &str,
    module: &str,
    stack: &MaterialStack,
    decision: &ProductValidationDecision,
) -> Result<crate::WorldEventBody, current::EvaluationErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    use crate::world::event_processing::factory_schedule_fixed::FactoryBuildFactsV1;
    let mut bytes = requester
        .len()
        .checked_add(module.len())
        .and_then(|n| n.checked_add(stack.kind.len() + decision.product_id.len() + 4096))
        .ok_or_else(limited)?;
    for list in [&decision.notes, &decision.quality_levels] {
        bytes = bytes
            .checked_add(list.len() * size_of::<String>() * 3)
            .ok_or_else(limited)?;
        for value in list {
            source.charge(value.len())?;
            bytes = bytes.checked_add(value.len() * 24).ok_or_else(limited)?;
        }
    }
    source.fees.hold(bytes, source.common)?;
    let present = FactoryBuildFactsV1::agent_exists(source, requester)?;
    Ok(
        crate::world::World::product_validation_result_from_presence_v1(
            present, action_id, requester, module, stack, decision,
        ),
    )
}
pub(in crate::world::incremental_normal_action_factory_schedule) fn apply<M, P, V, W, D, E, C, PE, PC, CW>(
    body: &crate::WorldEventBody,
    sources: &FactoryScheduleSourcesV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut FactoryScheduleWorkV1,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    let crate::WorldEventBody::Domain(crate::DomainEvent::ProductValidated {
        requester_agent_id,
        stack,
        tradable,
        ..
    }) = body
    else {
        return Err(invalid());
    };
    let after = Some(oasis7_world_state::LastProductValidationState {
        product_id: stack.kind.clone(),
        tradable: *tradable,
    });
    let mut local = WorldFieldReadContextV1 {
        root: sources
            .roots
            .state_root(oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business),
        state: read.state,
        values: ValueReadContext {
            source: read.values.source,
            memory: read.values.memory,
            class: read.values.class,
            io: &mut *read.values.io,
        },
        key_work: &mut *read.key_work,
        semantic_bytes: read.semantic_bytes,
        record_bytes: read.record_bytes,
        take_state_error: &mut read.take_state_error,
        cancelled: &mut read.cancelled,
    };
    let guard = step.transaction.guard();
    let before = recover_current_latest_product_validation_v1(
        sources.lifecycle.native,
        sources.roots,
        step.transaction,
        &mut local,
        &mut work.lifecycle.native_scan,
        &mut work.lifecycle.native_read,
        &guard,
        live,
    )
    .map_err(overlay)?;
    write_current_latest_product_validation_v1(
        &before,
        &after,
        step.transaction,
        &mut local,
        write,
        &mut work.lifecycle.records_remaining,
        &mut work.lifecycle.native_write,
        live,
    )
    .map_err(overlay)?;
    drop(local);
    crate::world::incremental_normal_industry_due::touch_current_product_requester_v1(
        requester_agent_id,
        false,
        sources,
        step,
        read,
        parent,
        work,
        live,
    )
}
