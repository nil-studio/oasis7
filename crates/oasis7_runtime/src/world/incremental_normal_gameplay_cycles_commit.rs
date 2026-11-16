//! Fixed gameplay event reducers. Raw events are private implementation data;
//! only the enclosing actual fallback/directive source flow calls these paths.
use super::selected;
use super::*;
use crate::world::incremental_normal_tail::NormalTailWorkV1;
use oasis7_formal_protocol::{
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    incremental_diplomacy::*, incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
};
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Current gameplay fixed reducer")
}
fn unsigned(value: Option<DiplomacyValueV1>) -> Result<Option<u64>, GameplayCycleErrorV1> {
    match value {
        None => Ok(None),
        Some(DiplomacyValueV1::Unsigned(n)) => Ok(Some(n)),
        _ => Err(bad().into()),
    }
}
fn signed(value: Option<DiplomacyValueV1>) -> Result<i64, GameplayCycleErrorV1> {
    match value {
        Some(DiplomacyValueV1::Signed(n)) => Ok(n),
        _ => Err(bad().into()),
    }
}
fn boolean(value: Option<DiplomacyValueV1>) -> Result<Option<bool>, GameplayCycleErrorV1> {
    match value {
        None => Ok(None),
        Some(DiplomacyValueV1::Bool(n)) => Ok(Some(n)),
        _ => Err(bad().into()),
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn expire_contract<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
>(
    event: &crate::DomainEvent,
    sources: &NormalGameplayCycleSourcesV1<'_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut NormalGameplayCycleWorkV1,
    tail: &mut NormalTailWorkV1<'_, '_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<(), GameplayCycleErrorV1> {
    let crate::DomainEvent::EconomicContractExpired {
        contract_id,
        creator_agent_id,
        counterparty_agent_id,
        creator_reputation_delta,
        counterparty_reputation_delta,
    } = event
    else {
        return Err(bad().into());
    };
    let field = DiplomacyFieldV1::EconomicContracts;
    let col = |c| DiplomacyScalarV1::Contract(c);
    let error_fee = tail.reserve(
        std::mem::size_of::<crate::WorldError>() + contract_id.len() + 256,
        MemoryClass::Ordinary,
    )?;
    let Some(mut proof) =
        selected::open_diplomacy(field, contract_id, sources, ctx, parent, work, tail, live)?
    else {
        return Err(GameplayCycleErrorV1::Business(error_fee.retain(
            crate::world_rules::invalid(format!("economic contract not found: {contract_id}")),
        )));
    };
    let fulfillment = selected::text(
        contract_id,
        field,
        col(ContractScalarV1::FulfillmentKind),
        &proof,
        state,
        ctx,
        work,
        tail,
    )?
    .ok_or_else(bad)?;
    tail.charge(fulfillment.len())?;
    let fulfillment = match fulfillment.as_str() {
        "atomic_exchange" => crate::EconomicContractFulfillmentKind::AtomicExchange,
        "service" => crate::EconomicContractFulfillmentKind::Service,
        _ => return Err(bad().into()),
    };
    if let Err(e)=oasis7_world_state::incremental_normal_gameplay_cycles_selected::reject_service_fulfillment_v1(fulfillment){return Err(GameplayCycleErrorV1::Business(error_fee.retain(e)))}
    let status = selected::text(
        contract_id,
        field,
        col(ContractScalarV1::Status),
        &proof,
        state,
        ctx,
        work,
        tail,
    )?
    .ok_or_else(bad)?;
    tail.charge(status.len() * 4)?;
    let status = selected::contract_status(&status)?;
    let projected=match oasis7_world_state::incremental_normal_gameplay_cycles_selected::selected_economic_expiry_projection_v1(contract_id,fulfillment,status,now){Ok(p)=>p,Err(e)=>return Err(GameplayCycleErrorV1::Business(error_fee.retain(e)))};
    let settled = unsigned(selected::scalar(
        contract_id,
        field,
        col(ContractScalarV1::SettledAt),
        &proof,
        state,
        ctx,
        tail,
    )?)?;
    let success = boolean(selected::scalar(
        contract_id,
        field,
        col(ContractScalarV1::SettlementSuccess),
        &proof,
        state,
        ctx,
        tail,
    )?)?;
    let transfer = signed(selected::scalar(
        contract_id,
        field,
        col(ContractScalarV1::TransferAmount),
        &proof,
        state,
        ctx,
        tail,
    )?)?;
    let tax = signed(selected::scalar(
        contract_id,
        field,
        col(ContractScalarV1::TaxAmount),
        &proof,
        state,
        ctx,
        tail,
    )?)?;
    let notes = selected::text(
        contract_id,
        field,
        col(ContractScalarV1::SettlementNotes),
        &proof,
        state,
        ctx,
        work,
        tail,
    )?;
    use DiplomacyScalarValueV1 as D;
    for (column, before, after) in [
        (
            ContractScalarV1::Status,
            Some(D::ContractStatus(status)),
            Some(D::ContractStatus(projected.status)),
        ),
        (
            ContractScalarV1::SettledAt,
            settled.map(D::U64),
            projected.settled_at.map(D::U64),
        ),
        (
            ContractScalarV1::SettlementSuccess,
            success.map(D::Bool),
            projected.settlement_success.map(D::Bool),
        ),
        (
            ContractScalarV1::TransferAmount,
            Some(D::I64(transfer)),
            Some(D::I64(projected.transfer_amount)),
        ),
        (
            ContractScalarV1::TaxAmount,
            Some(D::I64(tax)),
            Some(D::I64(projected.tax_amount)),
        ),
        (
            ContractScalarV1::SettlementNotes,
            notes.as_ref().map(|n| D::Text(n.as_str())),
            Some(D::Text(projected.settlement_notes)),
        ),
    ] {
        selected::change_diplomacy(
            contract_id,
            field,
            col(column),
            &mut proof,
            before,
            after,
            ctx,
            write,
            work,
            tail,
            live,
        )?;
    }
    // Preserve event IDs rather than equating them with the selected contract's
    // embedded participants. The original publisher uses these exact event fields.
    for (id, delta) in [
        (creator_agent_id, *creator_reputation_delta),
        (counterparty_agent_id, *counterparty_reputation_delta),
    ] {
        if delta != 0 {
            let (before, current) = selected::reputation(id, ctx, tail)?;
            selected::change_reputation(
                id,
                before,
                current.saturating_add(delta),
                ctx,
                write,
                tail,
                live,
            )?;
        }
    }
    selected::touch(
        creator_agent_id,
        false,
        now,
        sources,
        ctx,
        parent,
        write,
        work,
        tail,
        live,
    )?;
    selected::touch(
        counterparty_agent_id,
        false,
        now,
        sources,
        ctx,
        parent,
        write,
        work,
        tail,
        live,
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn timeout_crisis<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
>(
    event: &crate::DomainEvent,
    sources: &NormalGameplayCycleSourcesV1<'_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut NormalGameplayCycleWorkV1,
    tail: &mut NormalTailWorkV1<'_, '_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<(), GameplayCycleErrorV1> {
    let crate::DomainEvent::CrisisTimedOut {
        crisis_id,
        penalty_impact,
    } = event
    else {
        return Err(bad().into());
    };
    let field = DiplomacyFieldV1::Crises;
    let col = |c| DiplomacyScalarV1::Crisis(c);
    let error_fee = tail.reserve(
        std::mem::size_of::<crate::WorldError>() + crisis_id.len() + 128,
        MemoryClass::Ordinary,
    )?;
    let Some(mut proof) =
        selected::open_diplomacy(field, crisis_id, sources, ctx, parent, work, tail, live)?
    else {
        return Err(GameplayCycleErrorV1::Business(error_fee.retain(
            crate::world_rules::invalid(format!("crisis not found for timeout: {crisis_id}")),
        )));
    };
    let status = selected::text(
        crisis_id,
        field,
        col(CrisisScalarV1::Status),
        &proof,
        state,
        ctx,
        work,
        tail,
    )?
    .ok_or_else(bad)?;
    tail.charge(status.len() * 3)?;
    let status = selected::crisis_status(&status)?;
    let success = boolean(selected::scalar(
        crisis_id,
        field,
        col(CrisisScalarV1::Success),
        &proof,
        state,
        ctx,
        tail,
    )?)?;
    let impact = signed(selected::scalar(
        crisis_id,
        field,
        col(CrisisScalarV1::Impact),
        &proof,
        state,
        ctx,
        tail,
    )?)?;
    let resolved = unsigned(selected::scalar(
        crisis_id,
        field,
        col(CrisisScalarV1::ResolvedAt),
        &proof,
        state,
        ctx,
        tail,
    )?)?;
    let projected=oasis7_world_state::incremental_normal_gameplay_cycles_selected::selected_crisis_timeout_projection_v1(*penalty_impact,now);
    use DiplomacyScalarValueV1 as D;
    for (column, before, after) in [
        (
            CrisisScalarV1::Status,
            Some(D::CrisisStatus(status)),
            Some(D::CrisisStatus(projected.status)),
        ),
        (
            CrisisScalarV1::Success,
            success.map(D::Bool),
            projected.success.map(D::Bool),
        ),
        (
            CrisisScalarV1::Impact,
            Some(D::I64(impact)),
            Some(D::I64(projected.impact)),
        ),
        (
            CrisisScalarV1::ResolvedAt,
            resolved.map(D::U64),
            projected.resolved_at.map(D::U64),
        ),
    ] {
        selected::change_diplomacy(
            crisis_id,
            field,
            col(column),
            &mut proof,
            before,
            after,
            ctx,
            write,
            work,
            tail,
            live,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn finalize_governance<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
>(
    event: &crate::DomainEvent,
    sources: &NormalGameplayCycleSourcesV1<'_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut NormalGameplayCycleWorkV1,
    tail: &mut NormalTailWorkV1<'_, '_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<(), GameplayCycleErrorV1> {
    use oasis7_world_state::incremental_governance_records::*;
    let crate::DomainEvent::GovernanceProposalFinalized { proposal_key, .. } = event else {
        return Err(bad().into());
    };
    let error_fee = tail.reserve(
        std::mem::size_of::<crate::WorldError>() + proposal_key.len() + 128,
        MemoryClass::Ordinary,
    )?;
    let Some(mut proof) = selected::open_governance(
        GovernanceRecordKindV1::Proposal,
        proposal_key,
        sources,
        ctx,
        parent,
        work,
        tail,
        live,
    )?
    else {
        return Err(GameplayCycleErrorV1::Business(error_fee.retain(
            crate::world_rules::invalid(format!("governance proposal missing: {proposal_key}")),
        )));
    };
    let key = GovernanceRecordKeyV1 {
        kind: GovernanceRecordKindV1::Proposal,
        id: proposal_key,
    };
    let col = |column| GovernanceCellV1::Column(column);
    let status = selected::governance_scalar(
        key,
        col(GovernanceColumnV1::ProposalStatus),
        &proof,
        ctx,
        tail,
        live,
    )?;
    let finalized = selected::governance_scalar(
        key,
        col(GovernanceColumnV1::FinalizedAt),
        &proof,
        ctx,
        tail,
        live,
    )?;
    let winning = selected::governance_text(
        key,
        col(GovernanceColumnV1::WinningOption),
        &proof,
        ctx,
        work,
        tail,
        live,
    )?;
    let weight = selected::governance_scalar(
        key,
        col(GovernanceColumnV1::WinningWeight),
        &proof,
        ctx,
        tail,
        live,
    )?;
    let total = selected::governance_scalar(
        key,
        col(GovernanceColumnV1::TotalWeightAtFinalize),
        &proof,
        ctx,
        tail,
        live,
    )?;
    fn unsigned(
        value: GovernanceValueV1,
    ) -> Result<GovernanceValueRefV1<'static>, GameplayCycleErrorV1> {
        match value {
            GovernanceValueV1::Unsigned(n) => Ok(GovernanceValueRefV1::U64(n)),
            GovernanceValueV1::Unit => Ok(GovernanceValueRefV1::Unit),
            _ => Err(bad().into()),
        }
    }
    let projected=oasis7_world_state::incremental_normal_gameplay_cycles_selected::selected_governance_finalization_projection_v1(event,now);
    let status_after = match projected.status {
        crate::GovernanceProposalStatus::Passed => 1,
        crate::GovernanceProposalStatus::Rejected => 2,
        crate::GovernanceProposalStatus::Open => return Err(bad().into()),
    };
    use GovernanceValueRefV1 as D;
    for (column, before, after) in [
        (
            GovernanceColumnV1::ProposalStatus,
            unsigned(status)?,
            D::U64(status_after),
        ),
        (
            GovernanceColumnV1::FinalizedAt,
            unsigned(finalized)?,
            D::U64(now),
        ),
        (
            GovernanceColumnV1::WinningOption,
            winning.as_ref().map_or(D::Unit, |s| D::Text(s.as_str())),
            projected.winning_option.map_or(D::Unit, D::Text),
        ),
        (
            GovernanceColumnV1::WinningWeight,
            unsigned(weight)?,
            D::U64(projected.winning_weight),
        ),
        (
            GovernanceColumnV1::TotalWeightAtFinalize,
            unsigned(total)?,
            D::U64(projected.total_weight_at_finalize),
        ),
    ] {
        selected::change_governance(
            key,
            col(column),
            &mut proof,
            before,
            after,
            ctx,
            write,
            work,
            tail,
            live,
        )?;
    }
    if let Some(mut vote) = selected::open_governance(
        GovernanceRecordKindV1::Vote,
        proposal_key,
        sources,
        ctx,
        parent,
        work,
        tail,
        live,
    )? {
        let key = GovernanceRecordKeyV1 {
            kind: GovernanceRecordKindV1::Vote,
            id: proposal_key,
        };
        let cell = GovernanceCellV1::Column(GovernanceColumnV1::LastUpdatedAt);
        let before = selected::governance_scalar(key, cell, &vote, ctx, tail, live)?;
        selected::change_governance(
            key,
            cell,
            &mut vote,
            unsigned(before)?,
            D::U64(projected.vote_last_updated_at),
            ctx,
            write,
            work,
            tail,
            live,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn conclude_war<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
>(
    event: &crate::DomainEvent,
    sources: &NormalGameplayCycleSourcesV1<'_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut NormalGameplayCycleWorkV1,
    tail: &mut NormalTailWorkV1<'_, '_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<(), GameplayCycleErrorV1> {
    let crate::DomainEvent::WarConcluded {
        war_id,
        participant_outcomes,
        ..
    } = event
    else {
        return Err(bad().into());
    };
    let fee = tail.reserve(
        std::mem::size_of::<crate::WorldError>() + war_id.len() + 128,
        MemoryClass::Ordinary,
    )?;
    let field = DiplomacyFieldV1::Wars;
    let col = |v| DiplomacyScalarV1::War(v);
    let Some(mut proof) =
        selected::open_diplomacy(field, war_id, sources, ctx, parent, work, tail, live)?
    else {
        return Err(GameplayCycleErrorV1::Business(fee.retain(
            crate::world_rules::invalid(format!("war not found for conclusion: {war_id}")),
        )));
    };
    let aggressor = selected::text(
        war_id,
        field,
        col(WarScalarV1::AggressorAllianceId),
        &proof,
        state,
        ctx,
        work,
        tail,
    )?
    .ok_or_else(bad)?;
    let defender = selected::text(
        war_id,
        field,
        col(WarScalarV1::DefenderAllianceId),
        &proof,
        state,
        ctx,
        work,
        tail,
    )?
    .ok_or_else(bad)?;
    tail.charge(aggressor.len() + defender.len())?;
    let projected=oasis7_world_state::incremental_normal_gameplay_cycles_selected::selected_war_conclusion_projection_v1(&aggressor,&defender,event,now);
    let active = boolean(selected::scalar(
        war_id,
        field,
        col(WarScalarV1::Active),
        &proof,
        state,
        ctx,
        tail,
    )?)?;
    let aggressor_score = signed(selected::scalar(
        war_id,
        field,
        col(WarScalarV1::AggressorScore),
        &proof,
        state,
        ctx,
        tail,
    )?)?;
    let defender_score = signed(selected::scalar(
        war_id,
        field,
        col(WarScalarV1::DefenderScore),
        &proof,
        state,
        ctx,
        tail,
    )?)?;
    let concluded = unsigned(selected::scalar(
        war_id,
        field,
        col(WarScalarV1::ConcludedAt),
        &proof,
        state,
        ctx,
        tail,
    )?)?;
    let winner = selected::text(
        war_id,
        field,
        col(WarScalarV1::WinnerAllianceId),
        &proof,
        state,
        ctx,
        work,
        tail,
    )?;
    let loser = selected::text(
        war_id,
        field,
        col(WarScalarV1::LoserAllianceId),
        &proof,
        state,
        ctx,
        work,
        tail,
    )?;
    let summary = selected::text(
        war_id,
        field,
        col(WarScalarV1::SettlementSummary),
        &proof,
        state,
        ctx,
        work,
        tail,
    )?;
    use DiplomacyScalarValueV1 as D;
    for (column, before, after) in [
        (
            WarScalarV1::Active,
            active.map(D::Bool),
            Some(D::Bool(false)),
        ),
        (
            WarScalarV1::AggressorScore,
            Some(D::I64(aggressor_score)),
            Some(D::I64(projected.aggressor_score)),
        ),
        (
            WarScalarV1::DefenderScore,
            Some(D::I64(defender_score)),
            Some(D::I64(projected.defender_score)),
        ),
        (
            WarScalarV1::ConcludedAt,
            concluded.map(D::U64),
            projected.concluded_at.map(D::U64),
        ),
        (
            WarScalarV1::WinnerAllianceId,
            winner.as_ref().map(|s| D::Text(s.as_str())),
            Some(D::Text(projected.winner_alliance_id)),
        ),
        (
            WarScalarV1::LoserAllianceId,
            loser.as_ref().map(|s| D::Text(s.as_str())),
            Some(D::Text(projected.loser_alliance_id)),
        ),
        (
            WarScalarV1::SettlementSummary,
            summary.as_ref().map(|s| D::Text(s.as_str())),
            Some(D::Text(projected.settlement_summary)),
        ),
    ] {
        selected::change_diplomacy(
            war_id,
            field,
            col(column),
            &mut proof,
            before,
            after,
            ctx,
            write,
            work,
            tail,
            live,
        )?;
    }
    let mut length = selected::member_length(war_id, field, &proof, state, ctx, tail)?;
    while length > 0 {
        let ordinal = length - 1;
        let agent = selected::member_text(
            war_id,
            field,
            ordinal,
            DiplomacyMemberColumnV1::OutcomeAgentId,
            &proof,
            state,
            ctx,
            work,
            tail,
        )?
        .ok_or_else(bad)?;
        let electricity = signed(selected::member_scalar(
            war_id,
            field,
            ordinal,
            DiplomacyMemberColumnV1::OutcomeElectricityDelta,
            &proof,
            state,
            ctx,
            tail,
        )?)?;
        let data = signed(selected::member_scalar(
            war_id,
            field,
            ordinal,
            DiplomacyMemberColumnV1::OutcomeDataDelta,
            &proof,
            state,
            ctx,
            tail,
        )?)?;
        let reputation = signed(selected::member_scalar(
            war_id,
            field,
            ordinal,
            DiplomacyMemberColumnV1::OutcomeReputationDelta,
            &proof,
            state,
            ctx,
            tail,
        )?)?;
        tail.charge(agent.len())?;
        let memberfee = tail.reserve(
            agent.len() + std::mem::size_of::<crate::WarParticipantOutcome>(),
            MemoryClass::Ordinary,
        )?;
        let old = memberfee.retain(crate::WarParticipantOutcome {
            agent_id: agent.as_str().to_owned(),
            electricity_delta: electricity,
            data_delta: data,
            reputation_delta: reputation,
        });
        let guard = ctx.transaction.guard();
        proof = change_qualified_diplomacy_member_v1(
            DiplomacyMemberChangeV1 {
                field,
                id: war_id,
                qualification: &proof,
                expected_length: length,
                ordinal,
                expected: Some(DiplomacyMemberV1::Outcome(&old)),
                after: None,
                max_current_payload_bytes: work.max_text_bytes as u64,
                max_pending_node_bytes: work.max_pending_node_bytes,
            },
            ctx,
            write,
            &guard,
            live,
        )?
        .ok_or_else(bad)?;
        length -= 1;
    }
    for outcome in participant_outcomes {
        tail.charge(outcome.agent_id.len())?;
        let guard = ctx.transaction.guard();
        proof = change_qualified_diplomacy_member_v1(
            DiplomacyMemberChangeV1 {
                field,
                id: war_id,
                qualification: &proof,
                expected_length: length,
                ordinal: length,
                expected: None,
                after: Some(DiplomacyMemberV1::Outcome(outcome)),
                max_current_payload_bytes: work.max_text_bytes as u64,
                max_pending_node_bytes: work.max_pending_node_bytes,
            },
            ctx,
            write,
            &guard,
            live,
        )?
        .ok_or_else(bad)?;
        length = length.checked_add(1).ok_or_else(bad)?;
    }
    for outcome in participant_outcomes {
        selected::require_agent(&outcome.agent_id, sources, ctx, parent, work, tail, live)?;
        for (ordinal, kind, delta, context) in [
            (
                0,
                oasis7_agent_api::world_action::ResourceKind::Electricity,
                outcome.electricity_delta,
                "war electricity outcome",
            ),
            (
                1,
                oasis7_agent_api::world_action::ResourceKind::Data,
                outcome.data_delta,
                "war data outcome",
            ),
        ] {
            if delta == 0 {
                continue;
            }
            let before = selected::electricity_or_data(
                &outcome.agent_id,
                ordinal,
                sources,
                ctx,
                tail,
                live,
            )?;
            tail.charge(outcome.agent_id.len() + context.len())?;
            let fee = tail.reserve(
                outcome.agent_id.len() + context.len() + 512,
                MemoryClass::Ordinary,
            )?;
            let after=match oasis7_world_state::incremental_normal_gameplay_cycles_selected::apply_war_resource_delta_v1(kind,before.unwrap_or(0),delta,&outcome.agent_id,context){Ok(v)=>v,Err(e)=>return Err(GameplayCycleErrorV1::Business(fee.retain(e)))};
            selected::change_resource(
                &outcome.agent_id,
                ordinal,
                before,
                after,
                sources,
                ctx,
                write,
                tail,
                live,
            )?;
        }
        selected::touch(
            &outcome.agent_id,
            false,
            now,
            sources,
            ctx,
            parent,
            write,
            work,
            tail,
            live,
        )?;
        if outcome.reputation_delta != 0 {
            let (before, current) = selected::reputation(&outcome.agent_id, ctx, tail)?;
            selected::change_reputation(
                &outcome.agent_id,
                before,
                current.saturating_add(outcome.reputation_delta),
                ctx,
                write,
                tail,
                live,
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn grant_meta<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
>(
    event: &crate::DomainEvent,
    sources: &NormalGameplayCycleSourcesV1<'_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut NormalGameplayCycleWorkV1,
    tail: &mut NormalTailWorkV1<'_, '_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<(), GameplayCycleErrorV1> {
    use oasis7_world_state::incremental_world_native::{
        prepare_current_meta_progress_delta_v1, recover_current_meta_progress_v1,
        write_prepared_current_meta_progress_v1,
    };
    let crate::DomainEvent::MetaProgressGranted {
        operator_agent_id,
        target_agent_id,
        track,
        points,
        achievement_id,
    } = event
    else {
        return Err(bad().into());
    };
    // The original publisher has exactly these two presence gates, in order;
    // action-admission trim/amount/policy predicates do not apply to module output.
    selected::require_agent(operator_agent_id, sources, ctx, parent, work, tail, live)?;
    selected::require_agent(target_agent_id, sources, ctx, parent, work, tail, live)?;
    tail.charge(target_agent_id.len())?;
    {
        // MetaProgress's unique field declaration is Business (state_projection).
        let mut read = WorldFieldReadContextV1 {
            root: sources
                .roots
                .state_root(oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business),
            state: parent.state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: parent.values.source,
                memory: parent.values.memory,
                class: parent.values.class,
                io: &mut *parent.values.io,
            },
            key_work: &mut *parent.key_work,
            semantic_bytes: parent.semantic_bytes,
            record_bytes: parent.record_bytes,
            take_state_error: &mut parent.take_state_error,
            cancelled: &mut parent.cancelled,
        };
        let guard = ctx.transaction.guard();
        let before = recover_current_meta_progress_v1(
            target_agent_id,
            sources.native,
            sources.roots,
            ctx.transaction,
            &mut read,
            &mut work.native_scan,
            &mut work.native,
            &guard,
            live,
        )?;
        let after = prepare_current_meta_progress_delta_v1(
            &before,
            track,
            *points,
            achievement_id.as_deref(),
            now,
            sources.native,
            sources.roots,
            ctx.transaction,
            &mut read,
            &mut work.native_scan,
            &mut work.native,
            &mut work.meta,
            live,
        )?;
        // The sole writer demands a fresh strict Current owner; the private prepared
        // delta retains its exact origin and source fee through this genuine reopen.
        let guard = ctx.transaction.guard();
        let current = recover_current_meta_progress_v1(
            target_agent_id,
            sources.native,
            sources.roots,
            ctx.transaction,
            &mut read,
            &mut work.native_scan,
            &mut work.native,
            &guard,
            live,
        )?;
        write_prepared_current_meta_progress_v1(
            &after,
            &current,
            ctx.transaction,
            &mut read,
            write,
            &mut work.cells_remaining,
            &mut work.native_write,
            &mut work.meta,
            live,
        )?;
    }
    selected::touch(
        operator_agent_id,
        false,
        now,
        sources,
        ctx,
        parent,
        write,
        work,
        tail,
        live,
    )?;
    selected::touch(
        target_agent_id,
        false,
        now,
        sources,
        ctx,
        parent,
        write,
        work,
        tail,
        live,
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[path = "incremental_normal_gameplay_cycles_crisis_spawn.rs"]
mod crisis_spawn;
pub(super) use crisis_spawn::spawn_crisis;
