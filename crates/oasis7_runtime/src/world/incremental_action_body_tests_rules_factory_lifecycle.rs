//! Genuine pending FIFO consumer for the lifecycle Root dispatcher.
use super::*;
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn evaluate_with_factory_lifecycle<'a>(
    fixture: &genesis::Fixture<'a>,
    reader: &Reader<'a>,
    transaction: &mut ExecutionTransaction<'_, Reader<'a>>,
    pending: &mut oasis7_proto::memory_budget::ReservedValue<
        oasis7_world_state::incremental_pending_action_overlay::PendingActionCurrentV1,
    >,
    pending_work:&mut oasis7_world_state::incremental_pending_action_family::PendingActionFamilyWorkV1,
    directory: &oasis7_proto::memory_budget::ReservedValue<AcceptedInvocationDirectoryV1>,
    counter: &WorldEventCounterV1,
    originals: &mut Originals<'a>,
    memory: &'a MemoryBudget,
    now: u64,
    live: &ExecutionFence,
    body: NonmovementExecutionFixtureV1<'_>,
) -> (
    oasis7_proto::memory_budget::ReservedValue<RootRulePhaseOutcomeV1>,
    crate::world::incremental_action_factory_lifecycle::CompletedFactoryLifecycleV1,
) {
    let (outcome, _, _, _, _, _, _, _, completed, _, _, _) = evaluate(
        fixture,
        reader,
        transaction,
        FixtureInputV1::Pending(pending, pending_work),
        directory,
        counter,
        originals,
        memory,
        now,
        live,
        None,
        Some(body),
    );
    (
        outcome.expect("actual Rule outcome"),
        completed.expect("Root factory lifecycle receipt"),
    )
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn try_evaluate_with_factory_lifecycle<'a>(
    fixture: &genesis::Fixture<'a>,
    reader: &Reader<'a>,
    transaction: &mut ExecutionTransaction<'_, Reader<'a>>,
    pending: &mut oasis7_proto::memory_budget::ReservedValue<
        oasis7_world_state::incremental_pending_action_overlay::PendingActionCurrentV1,
    >,
    pending_work:&mut oasis7_world_state::incremental_pending_action_family::PendingActionFamilyWorkV1,
    directory: &oasis7_proto::memory_budget::ReservedValue<AcceptedInvocationDirectoryV1>,
    counter: &WorldEventCounterV1,
    originals: &mut Originals<'a>,
    memory: &'a MemoryBudget,
    now: u64,
    live: &ExecutionFence,
    cancelled: Option<&AtomicBool>,
    body: NonmovementExecutionFixtureV1<'_>,
) -> Result<
    (
        oasis7_proto::memory_budget::ReservedValue<RootRulePhaseOutcomeV1>,
        crate::world::incremental_action_factory_lifecycle::CompletedFactoryLifecycleV1,
    ),
    NonmovementFixtureErrorV1,
> {
    let mut failure = None;
    let (outcome, _, _, _, _, _, _, _, completed, _, _, _) = evaluate(
        fixture,
        reader,
        transaction,
        FixtureInputV1::PendingFallible(pending, pending_work, &mut failure, cancelled),
        directory,
        counter,
        originals,
        memory,
        now,
        live,
        None,
        Some(body),
    );
    if let Some(error) = failure {
        return Err(error);
    }
    Ok((
        outcome.expect("actual Rule outcome"),
        completed.expect("Root factory lifecycle receipt"),
    ))
}
