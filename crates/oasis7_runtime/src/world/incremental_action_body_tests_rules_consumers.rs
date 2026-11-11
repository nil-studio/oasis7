//! Typed fixture consumers of the real Root selected/FIFO paths.
use super::*;
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn evaluate_with_body<'a>(
    fixture: &genesis::Fixture<'a>,
    reader: &Reader<'a>,
    transaction: &mut ExecutionTransaction<'_, Reader<'a>>,
    selected: &crate::world::incremental_action_execution::SelectedPendingActionV1,
    directory: &oasis7_proto::memory_budget::ReservedValue<AcceptedInvocationDirectoryV1>,
    counter: &WorldEventCounterV1,
    originals: &mut Originals<'a>,
    memory: &'a MemoryBudget,
    now: u64,
    live: &ExecutionFence,
    body: NonmovementExecutionFixtureV1<'_>,
) -> (
    oasis7_proto::memory_budget::ReservedValue<RootRulePhaseOutcomeV1>,
    crate::world::incremental_action_body::CompletedBodyActionV1,
) {
    let (outcome, _, completed, _, _, _, _, _, _, _, _, _) = evaluate(
        fixture,
        reader,
        transaction,
        FixtureInputV1::Selected(selected),
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
        completed.expect("Root Body execution result"),
    )
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn evaluate_with_collect_data<'a>(
    fixture: &genesis::Fixture<'a>,
    reader: &Reader<'a>,
    transaction: &mut ExecutionTransaction<'_, Reader<'a>>,
    selected: &crate::world::incremental_action_execution::SelectedPendingActionV1,
    directory: &oasis7_proto::memory_budget::ReservedValue<AcceptedInvocationDirectoryV1>,
    counter: &WorldEventCounterV1,
    originals: &mut Originals<'a>,
    memory: &'a MemoryBudget,
    now: u64,
    live: &ExecutionFence,
    body: NonmovementExecutionFixtureV1<'_>,
) -> (
    oasis7_proto::memory_budget::ReservedValue<RootRulePhaseOutcomeV1>,
    crate::world::incremental_action_data::CompletedCollectDataV1,
) {
    let (outcome, _, _, completed, _, _, _, _, _, _, _, _) = evaluate(
        fixture,
        reader,
        transaction,
        FixtureInputV1::Selected(selected),
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
        completed.expect("Root CollectData execution result"),
    )
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn evaluate_with_transfer_resource<'a>(
    fixture: &genesis::Fixture<'a>,
    reader: &Reader<'a>,
    transaction: &mut ExecutionTransaction<'_, Reader<'a>>,
    pending: &mut oasis7_proto::memory_budget::ReservedValue<
        oasis7_world_state::incremental_pending_action_overlay::PendingActionCurrentV1,
    >,
    pending_work: &mut oasis7_world_state::incremental_pending_action_family::PendingActionFamilyWorkV1,
    directory: &oasis7_proto::memory_budget::ReservedValue<AcceptedInvocationDirectoryV1>,
    counter: &WorldEventCounterV1,
    originals: &mut Originals<'a>,
    memory: &'a MemoryBudget,
    now: u64,
    live: &ExecutionFence,
    body: NonmovementExecutionFixtureV1<'_>,
) -> (
    oasis7_proto::memory_budget::ReservedValue<RootRulePhaseOutcomeV1>,
    crate::world::incremental_action_data::transfer_resource::CompletedTransferResourceV1,
) {
    let (outcome, _, _, _, completed, _, _, _, _, _, _, _) = evaluate(
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
        completed.expect("Root TransferResource execution result"),
    )
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn evaluate_with_data_access<'a>(
    fixture: &genesis::Fixture<'a>,
    reader: &Reader<'a>,
    transaction: &mut ExecutionTransaction<'_, Reader<'a>>,
    pending: &mut oasis7_proto::memory_budget::ReservedValue<
        oasis7_world_state::incremental_pending_action_overlay::PendingActionCurrentV1,
    >,
    pending_work: &mut oasis7_world_state::incremental_pending_action_family::PendingActionFamilyWorkV1,
    directory: &oasis7_proto::memory_budget::ReservedValue<AcceptedInvocationDirectoryV1>,
    counter: &WorldEventCounterV1,
    originals: &mut Originals<'a>,
    memory: &'a MemoryBudget,
    now: u64,
    live: &ExecutionFence,
    body: NonmovementExecutionFixtureV1<'_>,
) -> (
    oasis7_proto::memory_budget::ReservedValue<RootRulePhaseOutcomeV1>,
    crate::world::incremental_action_data::data_access::CompletedDataAccessV1,
) {
    let (outcome, _, _, _, _, completed, _, _, _, _, _, _) = evaluate(
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
        completed.expect("Root DataAccess execution result"),
    )
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn evaluate_with_material_transfer<'a>(
    fixture: &genesis::Fixture<'a>,
    reader: &Reader<'a>,
    transaction: &mut ExecutionTransaction<'_, Reader<'a>>,
    selected: &crate::world::incremental_action_execution::SelectedPendingActionV1,
    directory: &oasis7_proto::memory_budget::ReservedValue<AcceptedInvocationDirectoryV1>,
    counter: &WorldEventCounterV1,
    originals: &mut Originals<'a>,
    memory: &'a MemoryBudget,
    now: u64,
    live: &ExecutionFence,
    body: NonmovementExecutionFixtureV1<'_>,
) -> (
    oasis7_proto::memory_budget::ReservedValue<RootRulePhaseOutcomeV1>,
    crate::world::incremental_material_normalization::transfer::CompletedMaterialTransferV1,
) {
    let (outcome, _, _, _, _, _, completed, _, _, _, _, _) = evaluate(
        fixture,
        reader,
        transaction,
        FixtureInputV1::Selected(selected),
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
        completed.expect("Root material transfer receipt"),
    )
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn evaluate_with_route_availability<'a>(
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
    crate::world::incremental_action_route_availability::CompletedRouteAvailabilityV1,
) {
    let (outcome, _, _, _, _, _, _, completed, _, _, _, _) = evaluate(
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
        completed.expect("Root route availability receipt"),
    )
}
