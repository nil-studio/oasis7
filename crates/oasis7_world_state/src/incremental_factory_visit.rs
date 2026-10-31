//! Borrowed exhaustive factory traversal. Dynamic collections never aggregate.
use super::*;
use crate::state::{
    FactoryBuildJobState, FactoryBuildPowerObligationV1, FactoryConstructionPowerMode,
    FactoryProductionSnapshot, FactoryProductionState, FactoryProductionStatus, FactoryState,
};
use FactoryCollectionV1 as List;
use FactoryColumnV1 as Col;
use MemberPayloadRefV1 as P;
use oasis7_wasm_abi::{FactoryModuleSpec, MaterialStack};
use std::collections::BTreeMap;
fn leaf<'a, E: From<MemberRecordError>>(
    column: FactoryColumnV1,
    payload: P<'a>,
    emit: &mut impl FnMut(FactoryCellV1<'a>, P<'a>) -> Result<(), E>,
) -> Result<(), E> {
    emit(FactoryCellV1::Column(column), payload)
}
fn optional_text<'a, E: From<MemberRecordError>>(
    column: FactoryColumnV1,
    value: Option<&'a str>,
    emit: &mut impl FnMut(FactoryCellV1<'a>, P<'a>) -> Result<(), E>,
) -> Result<(), E> {
    leaf(column, value.map_or(P::Unit, P::Text), emit)
}
fn optional_u64<'a, E: From<MemberRecordError>>(
    column: FactoryColumnV1,
    value: Option<u64>,
    emit: &mut impl FnMut(FactoryCellV1<'a>, P<'a>) -> Result<(), E>,
) -> Result<(), E> {
    leaf(column, value.map_or(P::Unit, P::U64), emit)
}
fn optional_i64<'a, E: From<MemberRecordError>>(
    column: FactoryColumnV1,
    value: Option<i64>,
    emit: &mut impl FnMut(FactoryCellV1<'a>, P<'a>) -> Result<(), E>,
) -> Result<(), E> {
    leaf(column, value.map_or(P::Unit, P::I64), emit)
}
fn length(value: usize) -> Result<u64, MemberRecordError> {
    u64::try_from(value).map_err(|_| MemberRecordError::Invalid("factory collection length"))
}
fn strings<'a, E: From<MemberRecordError>>(
    collection: List,
    values: Option<&'a [String]>,
    emit: &mut impl FnMut(FactoryCellV1<'a>, P<'a>) -> Result<(), E>,
) -> Result<(), E> {
    emit(
        FactoryCellV1::Collection(collection),
        values.map_or(Ok(P::Unit), |v| length(v.len()).map(P::U64))?,
    )?;
    if let Some(values) = values {
        for (index, value) in values.iter().enumerate() {
            emit(
                FactoryCellV1::TextItem(collection, length(index)?),
                P::Text(value),
            )?;
        }
    }
    Ok(())
}
fn stacks<'a, E: From<MemberRecordError>>(
    collection: List,
    values: Option<&'a [MaterialStack]>,
    emit: &mut impl FnMut(FactoryCellV1<'a>, P<'a>) -> Result<(), E>,
) -> Result<(), E> {
    emit(
        FactoryCellV1::Collection(collection),
        values.map_or(Ok(P::Unit), |v| length(v.len()).map(P::U64))?,
    )?;
    if let Some(values) = values {
        for (index, value) in values.iter().enumerate() {
            let MaterialStack { kind, amount } = value;
            let index = length(index)?;
            emit(
                FactoryCellV1::StackItem(collection, index, FactoryStackColumnV1::Kind),
                P::Text(kind),
            )?;
            emit(
                FactoryCellV1::StackItem(collection, index, FactoryStackColumnV1::Amount),
                P::I64(*amount),
            )?;
        }
    }
    Ok(())
}
fn balances<'a, E: From<MemberRecordError>>(
    collection: List,
    values: Option<&'a BTreeMap<String, i64>>,
    emit: &mut impl FnMut(FactoryCellV1<'a>, P<'a>) -> Result<(), E>,
) -> Result<(), E> {
    emit(
        FactoryCellV1::Collection(collection),
        values.map_or(Ok(P::Unit), |v| length(v.len()).map(P::U64))?,
    )?;
    if let Some(values) = values {
        for (key, amount) in values {
            emit(FactoryCellV1::Balance(collection, key), P::I64(*amount))?;
        }
    }
    Ok(())
}
fn spec<'a, E: From<MemberRecordError>>(
    value: &'a FactoryModuleSpec,
    emit: &mut impl FnMut(FactoryCellV1<'a>, P<'a>) -> Result<(), E>,
) -> Result<(), E> {
    let FactoryModuleSpec {
        factory_id,
        display_name,
        tier,
        tags,
        build_cost,
        build_time_ticks,
        base_power_draw,
        recipe_slots,
        throughput_bps,
        maintenance_per_tick,
    } = value;
    leaf(Col::SpecFactoryId, P::Text(factory_id), emit)?;
    leaf(Col::SpecDisplayName, P::Text(display_name), emit)?;
    leaf(Col::SpecTier, P::U64(u64::from(*tier)), emit)?;
    strings(List::SpecTags, Some(tags), emit)?;
    stacks(List::SpecBuildCost, Some(build_cost), emit)?;
    leaf(
        Col::SpecBuildTimeTicks,
        P::U64(u64::from(*build_time_ticks)),
        emit,
    )?;
    leaf(Col::SpecBasePowerDraw, P::I64(*base_power_draw), emit)?;
    leaf(Col::SpecRecipeSlots, P::U64(u64::from(*recipe_slots)), emit)?;
    leaf(
        Col::SpecThroughputBps,
        P::U64(u64::from(*throughput_bps)),
        emit,
    )?;
    leaf(
        Col::SpecMaintenancePerTick,
        P::I64(*maintenance_per_tick),
        emit,
    )
}
fn snapshot<'a, E: From<MemberRecordError>>(
    value: Option<&'a FactoryProductionSnapshot>,
    emit: &mut impl FnMut(FactoryCellV1<'a>, P<'a>) -> Result<(), E>,
) -> Result<(), E> {
    let (
        recipe_id,
        consume,
        produce,
        byproducts,
        consume_ledger,
        power_required,
        output_ledger,
        bottleneck_tags,
        logistics_route_ids,
        logistics_path_ids,
    ) = if let Some(FactoryProductionSnapshot {
        recipe_id,
        consume,
        produce,
        byproducts,
        consume_ledger,
        power_required,
        output_ledger,
        bottleneck_tags,
        logistics_route_ids,
        logistics_path_ids,
    }) = value
    {
        (
            Some(recipe_id.as_str()),
            Some(consume.as_slice()),
            Some(produce.as_slice()),
            Some(byproducts.as_slice()),
            Some(consume_ledger),
            Some(*power_required),
            Some(output_ledger),
            Some(bottleneck_tags.as_slice()),
            Some(logistics_route_ids.as_slice()),
            Some(logistics_path_ids.as_slice()),
        )
    } else {
        (None, None, None, None, None, None, None, None, None, None)
    };
    leaf(
        Col::ProductionSnapshotPresent,
        P::Bool(value.is_some()),
        emit,
    )?;
    optional_text(Col::SnapshotRecipeId, recipe_id, emit)?;
    stacks(List::SnapshotConsume, consume, emit)?;
    stacks(List::SnapshotProduce, produce, emit)?;
    stacks(List::SnapshotByproducts, byproducts, emit)?;
    leaf(
        Col::SnapshotConsumeLedger,
        consume_ledger.map_or(P::Unit, P::Ledger),
        emit,
    )?;
    optional_i64(Col::SnapshotPowerRequired, power_required, emit)?;
    leaf(
        Col::SnapshotOutputLedger,
        output_ledger.map_or(P::Unit, P::Ledger),
        emit,
    )?;
    strings(List::SnapshotBottleneckTags, bottleneck_tags, emit)?;
    strings(List::SnapshotLogisticsRouteIds, logistics_route_ids, emit)?;
    strings(List::SnapshotLogisticsPathIds, logistics_path_ids, emit)
}
fn production<'a, E: From<MemberRecordError>>(
    value: &'a FactoryProductionState,
    emit: &mut impl FnMut(FactoryCellV1<'a>, P<'a>) -> Result<(), E>,
) -> Result<(), E> {
    let FactoryProductionState {
        status,
        active_jobs,
        current_job_id,
        current_recipe_id,
        last_started_at,
        last_completed_at,
        last_blocked_at,
        last_resumed_at,
        current_blocker_kind,
        current_blocker_detail,
        current_blocker_action_id,
        completed_jobs,
        last_completed_recipe_id,
        same_recipe_repeat_count,
        last_completed_canonical_snapshot,
    } = value;
    let status = match status {
        FactoryProductionStatus::Idle => 0,
        FactoryProductionStatus::Running => 1,
        FactoryProductionStatus::Blocked => 2,
        FactoryProductionStatus::Paused => 3,
    };
    leaf(Col::ProductionStatus, P::U64(status), emit)?;
    leaf(
        Col::ProductionActiveJobs,
        P::U64(u64::from(*active_jobs)),
        emit,
    )?;
    optional_u64(Col::ProductionCurrentJobId, *current_job_id, emit)?;
    optional_text(
        Col::ProductionCurrentRecipeId,
        current_recipe_id.as_deref(),
        emit,
    )?;
    optional_u64(Col::ProductionLastStartedAt, *last_started_at, emit)?;
    optional_u64(Col::ProductionLastCompletedAt, *last_completed_at, emit)?;
    optional_u64(Col::ProductionLastBlockedAt, *last_blocked_at, emit)?;
    optional_u64(Col::ProductionLastResumedAt, *last_resumed_at, emit)?;
    optional_text(
        Col::ProductionCurrentBlockerKind,
        current_blocker_kind.as_deref(),
        emit,
    )?;
    optional_text(
        Col::ProductionCurrentBlockerDetail,
        current_blocker_detail.as_deref(),
        emit,
    )?;
    optional_u64(
        Col::ProductionCurrentBlockerActionId,
        *current_blocker_action_id,
        emit,
    )?;
    leaf(Col::ProductionCompletedJobs, P::U64(*completed_jobs), emit)?;
    optional_text(
        Col::ProductionLastCompletedRecipeId,
        last_completed_recipe_id.as_deref(),
        emit,
    )?;
    leaf(
        Col::ProductionSameRecipeRepeatCount,
        P::U64(u64::from(*same_recipe_repeat_count)),
        emit,
    )?;
    snapshot(last_completed_canonical_snapshot.as_ref(), emit)
}
fn obligation<'a, E: From<MemberRecordError>>(
    value: Option<&'a FactoryBuildPowerObligationV1>,
    emit: &mut impl FnMut(FactoryCellV1<'a>, P<'a>) -> Result<(), E>,
) -> Result<(), E> {
    leaf(Col::ObligationPresent, P::Bool(value.is_some()), emit)?;
    let (
        payer_agent_id,
        profile_key,
        profile_revision,
        electricity_amount,
        mode,
        material_ledger,
        electricity_before,
        electricity_after,
        material_balances_before,
        material_balances_after,
    ) = if let Some(FactoryBuildPowerObligationV1 {
        payer_agent_id,
        profile_key,
        profile_revision,
        electricity_amount,
        mode,
        material_ledger,
        electricity_before,
        electricity_after,
        material_balances_before,
        material_balances_after,
    }) = value
    {
        let mode = match mode {
            FactoryConstructionPowerMode::StartOnlySink => 0,
        };
        (
            Some(payer_agent_id.as_str()),
            Some(profile_key.as_str()),
            Some(*profile_revision),
            Some(*electricity_amount),
            Some(mode),
            material_ledger.as_ref(),
            *electricity_before,
            *electricity_after,
            material_balances_before.as_ref(),
            material_balances_after.as_ref(),
        )
    } else {
        (None, None, None, None, None, None, None, None, None, None)
    };
    optional_text(Col::ObligationPayerAgentId, payer_agent_id, emit)?;
    optional_text(Col::ObligationProfileKey, profile_key, emit)?;
    optional_u64(Col::ObligationProfileRevision, profile_revision, emit)?;
    optional_i64(Col::ObligationElectricityAmount, electricity_amount, emit)?;
    optional_u64(Col::ObligationMode, mode, emit)?;
    leaf(
        Col::ObligationMaterialLedger,
        material_ledger.map_or(P::Unit, P::Ledger),
        emit,
    )?;
    optional_i64(Col::ObligationElectricityBefore, electricity_before, emit)?;
    optional_i64(Col::ObligationElectricityAfter, electricity_after, emit)?;
    balances(
        List::ObligationBalancesBefore,
        material_balances_before,
        emit,
    )?;
    balances(List::ObligationBalancesAfter, material_balances_after, emit)
}
pub(crate) fn visit<'a, E: From<MemberRecordError>>(
    value: FactoryRecordRefV1<'a>,
    emit: &mut impl FnMut(FactoryCellV1<'a>, P<'a>) -> Result<(), E>,
) -> Result<(), E> {
    emit(FactoryCellV1::Record, P::Unit)?;
    match value {
        FactoryRecordRefV1::Factory(value) => {
            let FactoryState {
                factory_id,
                site_id,
                builder_agent_id,
                spec: module_spec,
                input_ledger,
                output_ledger,
                durability_ppm,
                production: production_state,
                site_authority_revision,
                site_location_id,
                location_anchor_revision,
                construction_power_profile_key,
                construction_power_profile_revision,
                built_at,
            } = value;
            leaf(Col::FactoryFactoryId, P::Text(factory_id), emit)?;
            leaf(Col::FactorySiteId, P::Text(site_id), emit)?;
            leaf(Col::FactoryBuilderAgentId, P::Text(builder_agent_id), emit)?;
            spec(module_spec, emit)?;
            leaf(Col::FactoryInputLedger, P::Ledger(input_ledger), emit)?;
            leaf(Col::FactoryOutputLedger, P::Ledger(output_ledger), emit)?;
            leaf(Col::FactoryDurabilityPpm, P::I64(*durability_ppm), emit)?;
            production(production_state, emit)?;
            optional_u64(
                Col::FactorySiteAuthorityRevision,
                *site_authority_revision,
                emit,
            )?;
            optional_text(
                Col::FactorySiteLocationId,
                site_location_id.as_deref(),
                emit,
            )?;
            optional_u64(
                Col::FactoryLocationAnchorRevision,
                *location_anchor_revision,
                emit,
            )?;
            optional_text(
                Col::FactoryConstructionPowerProfileKey,
                construction_power_profile_key.as_deref(),
                emit,
            )?;
            optional_u64(
                Col::FactoryConstructionPowerProfileRevision,
                *construction_power_profile_revision,
                emit,
            )?;
            leaf(Col::FactoryBuiltAt, P::U64(*built_at), emit)
        }
        FactoryRecordRefV1::Build(value) => {
            let FactoryBuildJobState {
                job_id,
                builder_agent_id,
                site_id,
                spec: module_spec,
                consume_ledger,
                ready_at,
                contract_version,
                site_authority_revision,
                site_location_id,
                location_anchor_revision,
                construction_power_obligation,
            } = value;
            leaf(Col::BuildJobId, P::U64(*job_id), emit)?;
            leaf(Col::BuildBuilderAgentId, P::Text(builder_agent_id), emit)?;
            leaf(Col::BuildSiteId, P::Text(site_id), emit)?;
            spec(module_spec, emit)?;
            leaf(Col::BuildConsumeLedger, P::Ledger(consume_ledger), emit)?;
            leaf(Col::BuildReadyAt, P::U64(*ready_at), emit)?;
            leaf(
                Col::BuildContractVersion,
                P::U64(u64::from(*contract_version)),
                emit,
            )?;
            optional_u64(
                Col::BuildSiteAuthorityRevision,
                *site_authority_revision,
                emit,
            )?;
            optional_text(Col::BuildSiteLocationId, site_location_id.as_deref(), emit)?;
            optional_u64(
                Col::BuildLocationAnchorRevision,
                *location_anchor_revision,
                emit,
            )?;
            obligation(construction_power_obligation.as_ref(), emit)
        }
    }
}
