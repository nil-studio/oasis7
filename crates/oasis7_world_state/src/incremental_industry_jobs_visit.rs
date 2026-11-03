use super::*;
use crate::{CommittedRecipeOrigin, MaterialTransitPriority};
use IndustryCollectionV1 as List;
use IndustryColumnV1 as Col;
use MemberPayloadRefV1 as P;
use oasis7_agent_api::world_action::IndustrialRefV1;
use oasis7_proto::gameplay_submission_origin::GameplaySubmissionOrigin;
use oasis7_wasm_abi::MaterialStack;
use std::collections::BTreeMap;
fn leaf<'a, E: From<MemberRecordError>>(
    column: Col,
    payload: P<'a>,
    emit: &mut impl FnMut(IndustryCellV1<'a>, P<'a>) -> Result<(), E>,
) -> Result<(), E> {
    emit(IndustryCellV1::Column(column), payload)
}
fn number(n: usize) -> Result<u64, MemberRecordError> {
    u64::try_from(n).map_err(|_| MemberRecordError::Invalid("industry collection length"))
}
fn strings<'a, E: From<MemberRecordError>>(
    list: List,
    values: &'a [String],
    emit: &mut impl FnMut(IndustryCellV1<'a>, P<'a>) -> Result<(), E>,
) -> Result<(), E> {
    emit(
        IndustryCellV1::Collection(list),
        P::U64(number(values.len())?),
    )?;
    for (index, value) in values.iter().enumerate() {
        emit(
            IndustryCellV1::TextItem(list, number(index)?),
            P::Text(value),
        )?;
    }
    Ok(())
}
fn stacks<'a, E: From<MemberRecordError>>(
    list: List,
    values: &'a [MaterialStack],
    emit: &mut impl FnMut(IndustryCellV1<'a>, P<'a>) -> Result<(), E>,
) -> Result<(), E> {
    emit(
        IndustryCellV1::Collection(list),
        P::U64(number(values.len())?),
    )?;
    for (index, value) in values.iter().enumerate() {
        let MaterialStack { kind, amount } = value;
        let index = number(index)?;
        emit(
            IndustryCellV1::StackItem(list, index, IndustryStackColumnV1::Kind),
            P::Text(kind),
        )?;
        emit(
            IndustryCellV1::StackItem(list, index, IndustryStackColumnV1::Amount),
            P::I64(*amount),
        )?;
    }
    Ok(())
}
fn balances<'a, E: From<MemberRecordError>>(
    list: List,
    values: &'a BTreeMap<String, i64>,
    emit: &mut impl FnMut(IndustryCellV1<'a>, P<'a>) -> Result<(), E>,
) -> Result<(), E> {
    emit(
        IndustryCellV1::Collection(list),
        P::U64(number(values.len())?),
    )?;
    for (name, amount) in values {
        emit(IndustryCellV1::Balance(list, name), P::I64(*amount))?;
    }
    Ok(())
}
pub(super) fn recipe<'a, E: From<MemberRecordError>>(
    value: &'a RecipeJobState,
    emit: &mut impl FnMut(IndustryCellV1<'a>, P<'a>) -> Result<(), E>,
) -> Result<(), E> {
    let RecipeJobState {
        industrial_root_ref,
        emergency_power_world_binding,
        committed_recipe_origin,
        job_id,
        requester_agent_id,
        factory_id,
        recipe_id,
        accepted_batches,
        consume,
        produce,
        byproducts,
        power_required,
        power_owner_agent_id,
        duration_ticks,
        consume_ledger,
        output_ledger,
        bottleneck_tags,
        logistics_route_ids,
        logistics_path_ids,
        ready_at,
    } = value;
    industrial(industrial_root_ref.as_ref(), emit)?;
    leaf(
        Col::RecordEmergencyPowerWorldBinding,
        emergency_power_world_binding
            .as_deref()
            .map_or(P::Unit, P::Text),
        emit,
    )?;
    origin(committed_recipe_origin.as_ref(), emit)?;
    leaf(Col::RecordJobId, P::U64(*job_id), emit)?;
    leaf(
        Col::RecordRequesterAgentId,
        P::Text(requester_agent_id),
        emit,
    )?;
    leaf(Col::RecordFactoryId, P::Text(factory_id), emit)?;
    leaf(Col::RecordRecipeId, P::Text(recipe_id), emit)?;
    leaf(
        Col::RecordAcceptedBatches,
        P::U64(u64::from(*accepted_batches)),
        emit,
    )?;
    stacks(List::Consume, consume, emit)?;
    stacks(List::Produce, produce, emit)?;
    stacks(List::Byproducts, byproducts, emit)?;
    leaf(Col::RecordPowerRequired, P::I64(*power_required), emit)?;
    leaf(
        Col::RecordPowerOwnerAgentId,
        power_owner_agent_id.as_deref().map_or(P::Unit, P::Text),
        emit,
    )?;
    leaf(
        Col::RecordDurationTicks,
        P::U64(u64::from(*duration_ticks)),
        emit,
    )?;
    leaf(Col::RecordConsumeLedger, P::Ledger(consume_ledger), emit)?;
    leaf(Col::RecordOutputLedger, P::Ledger(output_ledger), emit)?;
    strings(List::BottleneckTags, bottleneck_tags, emit)?;
    strings(List::LogisticsRouteIds, logistics_route_ids, emit)?;
    strings(List::LogisticsPathIds, logistics_path_ids, emit)?;
    leaf(Col::RecordReadyAt, P::U64(*ready_at), emit)?;
    Ok(())
}
pub(super) fn transit<'a, E: From<MemberRecordError>>(
    value: &'a MaterialTransitJobState,
    emit: &mut impl FnMut(IndustryCellV1<'a>, P<'a>) -> Result<(), E>,
) -> Result<(), E> {
    let MaterialTransitJobState {
        job_id,
        requester_agent_id,
        from_ledger,
        to_ledger,
        kind,
        amount,
        distance_km,
        loss_bps,
        priority,
        route_id,
        path_id,
        route_ids,
        tariff_electricity_total,
        reroute_count,
        ready_at,
    } = value;
    leaf(Col::RecordJobId, P::U64(*job_id), emit)?;
    leaf(
        Col::RecordRequesterAgentId,
        P::Text(requester_agent_id),
        emit,
    )?;
    leaf(Col::RecordFromLedger, P::Ledger(from_ledger), emit)?;
    leaf(Col::RecordToLedger, P::Ledger(to_ledger), emit)?;
    leaf(Col::RecordKind, P::Text(kind), emit)?;
    leaf(Col::RecordAmount, P::I64(*amount), emit)?;
    leaf(Col::RecordDistanceKm, P::I64(*distance_km), emit)?;
    leaf(Col::RecordLossBps, P::I64(*loss_bps), emit)?;
    leaf(
        Col::RecordPriority,
        P::U64(match priority {
            MaterialTransitPriority::Standard => 0,
            MaterialTransitPriority::Urgent => 1,
        }),
        emit,
    )?;
    leaf(
        Col::RecordRouteId,
        route_id.as_deref().map_or(P::Unit, P::Text),
        emit,
    )?;
    leaf(
        Col::RecordPathId,
        path_id.as_deref().map_or(P::Unit, P::Text),
        emit,
    )?;
    strings(List::RouteIds, route_ids, emit)?;
    leaf(
        Col::RecordTariffElectricityTotal,
        P::I64(*tariff_electricity_total),
        emit,
    )?;
    leaf(
        Col::RecordRerouteCount,
        P::U64(u64::from(*reroute_count)),
        emit,
    )?;
    leaf(Col::RecordReadyAt, P::U64(*ready_at), emit)?;
    Ok(())
}
pub(super) fn completion<'a, E: From<MemberRecordError>>(
    value: &'a RecipeCompletionReceiptV1,
    emit: &mut impl FnMut(IndustryCellV1<'a>, P<'a>) -> Result<(), E>,
) -> Result<(), E> {
    let RecipeCompletionReceiptV1 {
        committed_recipe_origin,
        job_id,
        requester_agent_id,
        factory_id,
        recipe_id,
        accepted_batches,
        produce,
        byproducts,
        output_ledger,
        bottleneck_tags,
        logistics_route_ids,
        logistics_path_ids,
    } = value;
    origin(committed_recipe_origin.as_ref(), emit)?;
    leaf(Col::RecordJobId, P::U64(*job_id), emit)?;
    leaf(
        Col::RecordRequesterAgentId,
        P::Text(requester_agent_id),
        emit,
    )?;
    leaf(Col::RecordFactoryId, P::Text(factory_id), emit)?;
    leaf(Col::RecordRecipeId, P::Text(recipe_id), emit)?;
    leaf(
        Col::RecordAcceptedBatches,
        P::U64(u64::from(*accepted_batches)),
        emit,
    )?;
    stacks(List::Produce, produce, emit)?;
    stacks(List::Byproducts, byproducts, emit)?;
    leaf(Col::RecordOutputLedger, P::Ledger(output_ledger), emit)?;
    strings(List::BottleneckTags, bottleneck_tags, emit)?;
    strings(List::LogisticsRouteIds, logistics_route_ids, emit)?;
    strings(List::LogisticsPathIds, logistics_path_ids, emit)?;
    Ok(())
}
pub(super) fn failure<'a, E: From<MemberRecordError>>(
    value: &'a FactoryProductionFailureDispositionV1,
    emit: &mut impl FnMut(IndustryCellV1<'a>, P<'a>) -> Result<(), E>,
) -> Result<(), E> {
    let FactoryProductionFailureDispositionV1 {
        action_id,
        requester_agent_id,
        factory_id,
        recipe_id,
        blocker_kind,
        blocker_detail,
        disposition_kind,
        consumed_inputs,
        lost_inputs,
        consumed_power,
        lost_power,
        next_action,
        next_recheck,
    } = value;
    leaf(Col::RecordActionId, P::U64(*action_id), emit)?;
    leaf(
        Col::RecordRequesterAgentId,
        P::Text(requester_agent_id),
        emit,
    )?;
    leaf(Col::RecordFactoryId, P::Text(factory_id), emit)?;
    leaf(Col::RecordRecipeId, P::Text(recipe_id), emit)?;
    leaf(Col::RecordBlockerKind, P::Text(blocker_kind), emit)?;
    leaf(Col::RecordBlockerDetail, P::Text(blocker_detail), emit)?;
    leaf(Col::RecordDispositionKind, P::Text(disposition_kind), emit)?;
    stacks(List::ConsumedInputs, consumed_inputs, emit)?;
    stacks(List::LostInputs, lost_inputs, emit)?;
    leaf(Col::RecordConsumedPower, P::I64(*consumed_power), emit)?;
    leaf(Col::RecordLostPower, P::I64(*lost_power), emit)?;
    leaf(Col::RecordNextAction, P::Text(next_action), emit)?;
    leaf(
        Col::RecordNextRecheck,
        next_recheck.map_or(P::Unit, P::U64),
        emit,
    )?;
    Ok(())
}
pub(super) fn settlement<'a, E: From<MemberRecordError>>(
    value: &'a LogisticsSettlementReceiptV1,
    emit: &mut impl FnMut(IndustryCellV1<'a>, P<'a>) -> Result<(), E>,
) -> Result<(), E> {
    let LogisticsSettlementReceiptV1 {
        job_id,
        path_id,
        route_ids,
        tariff_electricity_total,
        owner_payouts,
        governance_tax_electricity,
    } = value;
    leaf(Col::RecordJobId, P::U64(*job_id), emit)?;
    leaf(
        Col::RecordPathId,
        path_id.as_deref().map_or(P::Unit, P::Text),
        emit,
    )?;
    strings(List::RouteIds, route_ids, emit)?;
    leaf(
        Col::RecordTariffElectricityTotal,
        P::I64(*tariff_electricity_total),
        emit,
    )?;
    balances(List::OwnerPayouts, owner_payouts, emit)?;
    leaf(
        Col::RecordGovernanceTaxElectricity,
        P::I64(*governance_tax_electricity),
        emit,
    )?;
    Ok(())
}
fn industrial<'a, E: From<MemberRecordError>>(
    value: Option<&'a IndustrialRefV1>,
    emit: &mut impl FnMut(IndustryCellV1<'a>, P<'a>) -> Result<(), E>,
) -> Result<(), E> {
    leaf(Col::RecordIndustrialRootRef, P::Bool(value.is_some()), emit)?;
    match value {
        Some(value) => {
            let IndustrialRefV1 {
                world_binding,
                domain,
                schema,
                id,
                revision,
                digest,
            } = value;
            leaf(
                Col::IndustrialRootWorldBinding,
                P::Text(world_binding),
                emit,
            )?;
            leaf(Col::IndustrialRootDomain, P::Text(domain), emit)?;
            leaf(Col::IndustrialRootSchema, P::Text(schema), emit)?;
            leaf(Col::IndustrialRootId, P::Text(id), emit)?;
            leaf(Col::IndustrialRootRevision, P::U64(*revision), emit)?;
            leaf(Col::IndustrialRootDigest, P::Text(digest), emit)?;
        }
        None => {
            leaf(Col::IndustrialRootWorldBinding, P::Unit, emit)?;
            leaf(Col::IndustrialRootDomain, P::Unit, emit)?;
            leaf(Col::IndustrialRootSchema, P::Unit, emit)?;
            leaf(Col::IndustrialRootId, P::Unit, emit)?;
            leaf(Col::IndustrialRootRevision, P::Unit, emit)?;
            leaf(Col::IndustrialRootDigest, P::Unit, emit)?;
        }
    }
    Ok(())
}
fn origin<'a, E: From<MemberRecordError>>(
    value: Option<&'a CommittedRecipeOrigin>,
    emit: &mut impl FnMut(IndustryCellV1<'a>, P<'a>) -> Result<(), E>,
) -> Result<(), E> {
    leaf(
        Col::RecordCommittedRecipeOrigin,
        P::Bool(value.is_some()),
        emit,
    )?;
    match value {
        Some(value) => {
            let CommittedRecipeOrigin {
                consensus_action_id,
                consensus_submitter_player_id,
                action_payload_hash,
                committed_height,
                action_root,
                submission,
            } = value;
            leaf(
                Col::OriginConsensusActionId,
                P::U64(*consensus_action_id),
                emit,
            )?;
            leaf(
                Col::OriginConsensusSubmitterPlayerId,
                P::Text(consensus_submitter_player_id),
                emit,
            )?;
            leaf(
                Col::OriginActionPayloadHash,
                P::Text(action_payload_hash),
                emit,
            )?;
            leaf(Col::OriginCommittedHeight, P::U64(*committed_height), emit)?;
            leaf(Col::OriginActionRoot, P::Text(action_root), emit)?;
            submission_identity(Some(submission), emit)?;
        }
        None => {
            leaf(Col::OriginConsensusActionId, P::Unit, emit)?;
            leaf(Col::OriginConsensusSubmitterPlayerId, P::Unit, emit)?;
            leaf(Col::OriginActionPayloadHash, P::Unit, emit)?;
            leaf(Col::OriginCommittedHeight, P::Unit, emit)?;
            leaf(Col::OriginActionRoot, P::Unit, emit)?;
            submission_identity(None, emit)?;
        }
    }
    Ok(())
}
fn submission_identity<'a, E: From<MemberRecordError>>(
    value: Option<&'a GameplaySubmissionOrigin>,
    emit: &mut impl FnMut(IndustryCellV1<'a>, P<'a>) -> Result<(), E>,
) -> Result<(), E> {
    match value {
        Some(value) => {
            let GameplaySubmissionOrigin {
                verified_player_id,
                public_key,
                auth_nonce,
                hosted_registration_nonce,
                requester_agent_id,
                factory_id,
                recipe_id,
            } = value;
            leaf(
                Col::SubmissionVerifiedPlayerId,
                P::Text(verified_player_id),
                emit,
            )?;
            leaf(Col::SubmissionPublicKey, P::Text(public_key), emit)?;
            leaf(Col::SubmissionAuthNonce, P::U64(*auth_nonce), emit)?;
            leaf(
                Col::SubmissionHostedRegistrationNonce,
                hosted_registration_nonce
                    .as_deref()
                    .map_or(P::Unit, P::Text),
                emit,
            )?;
            leaf(
                Col::SubmissionRequesterAgentId,
                P::Text(requester_agent_id),
                emit,
            )?;
            leaf(Col::SubmissionFactoryId, P::Text(factory_id), emit)?;
            leaf(Col::SubmissionRecipeId, P::Text(recipe_id), emit)?;
        }
        None => {
            leaf(Col::SubmissionVerifiedPlayerId, P::Unit, emit)?;
            leaf(Col::SubmissionPublicKey, P::Unit, emit)?;
            leaf(Col::SubmissionAuthNonce, P::Unit, emit)?;
            leaf(Col::SubmissionHostedRegistrationNonce, P::Unit, emit)?;
            leaf(Col::SubmissionRequesterAgentId, P::Unit, emit)?;
            leaf(Col::SubmissionFactoryId, P::Unit, emit)?;
            leaf(Col::SubmissionRecipeId, P::Unit, emit)?;
        }
    }
    Ok(())
}
