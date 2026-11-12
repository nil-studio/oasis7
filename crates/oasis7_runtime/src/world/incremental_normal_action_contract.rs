//! Open/SettleContract consumes actual Current and the sole fixed predicates.
//! This private child shares no World DTO or caller-supplied business permit.
use super::*;
use crate::world::event_processing::contract_fixed::{
    ContractFactsV1, ContractRecordV1, evaluate_contract_action_v1,
};
use oasis7_formal_protocol::{
    incremental_state::StateNamespaceV1, incremental_value::ValueReadContext,
};
use oasis7_world_state::{
    EconomicContractState, EconomicContractStatus, incremental_diplomacy::*,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_artifact_resources::RuntimeArtifactResourcesQualificationV1,
    incremental_world_rule_composite::WorldRuleCompositeReadWorkV1 as TypedRecordReadWorkV1,
};
#[path = "incremental_normal_action_contract_facts.rs"]
mod current;
pub(in crate::world) struct ContractSourcesV1<'a, 'source> {
    pub shared: &'a AllianceSourcesV1<'a, 'source>,
    pub artifacts: &'a ReservedValue<RuntimeArtifactResourcesQualificationV1>,
}
pub(in crate::world) struct ContractWorkV1 {
    pub shared: AllianceWorkV1,
    pub scalar: crate::world::incremental_action_contract_accept::ContractAcceptWorkV1,
    pub policy_scan: StateScanBudget,
    pub policy: TypedRecordReadWorkV1,
    pub binding: crate::world::incremental_industrial_eligibility::IndustrialEligibilityWorkV1,
}

#[path = "incremental_normal_action_contract_finance.rs"]
mod finance;

#[path = "incremental_normal_action_contract_commit.rs"]
mod commit;
#[path = "incremental_normal_action_contract_continuity.rs"]
mod settlement_continuity;
pub(in crate::world) use self::commit::commit_contract_action_v1;
struct EvaluatedContractV1 {
    body: crate::WorldEventBody,
    fees: Fees,
    before: Option<ReservedValue<EconomicContractState>>,
}
pub(in crate::world) struct PreparedContractActionV1<'s, 'r, 'source> {
    selected: &'s SelectedPendingActionV1,
    outcome: &'r ReservedValue<RootRulePhaseOutcomeV1>,
    sources: &'s ContractSourcesV1<'s, 'source>,
    body: crate::WorldEventBody,
    before: Option<ReservedValue<EconomicContractState>>,
    after: Option<ReservedValue<EconomicContractState>>,
    financial: Option<ReservedValue<finance::FinancialPlanV1>>,
    continuity: Option<settlement_continuity::ContinuityPlanV1>,
    fees: Fees,
    verification: Fees,
    identity: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    time: u64,
    event_fee: MemoryReservation,
    layout: ReservedValue<()>,
}
#[allow(clippy::too_many_arguments)]
fn evaluate<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    action_id: crate::ActionId,
    action: &crate::Action,
    sources: &ContractSourcesV1<'_, '_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut ContractWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
    fees: Fees,
) -> Result<EvaluatedContractV1, BodyMutationError> {
    let mut business = WorldFieldReadContextV1 {
        root: sources.shared.roots.state_root(StateNamespaceV1::Business),
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
    let inner = facts::CurrentAllianceFactsV1 {
        agents: sources.shared.agents,
        diplomacy: sources.shared.diplomacy,
        roots: sources.shared.roots,
        context,
        read: &mut business,
        common,
        index: &mut work.shared.index,
        pending: &mut work.shared.pending_entries_remaining,
        live,
        max_text: work.shared.max_text_bytes,
        fees,
    };
    let mut source = current::CurrentContractFactsV1 {
        source: inner,
        scalar: &mut work.scalar,
        policy_scan: &mut work.policy_scan,
        policy: &mut work.policy,
        contract_cells: &mut work.shared.scope_cells,
        binding: &mut work.binding,
        artifacts: sources.artifacts,
        now,
    };
    let body = evaluate_contract_action_v1(&mut source, action_id, action).map_err(evaluated)?;
    let before =
        if let crate::WorldEventBody::Domain(crate::DomainEvent::EconomicContractSettled {
            contract_id,
            ..
        }) = &body
        {
            match source.contract(contract_id).map_err(evaluated)? {
                Some(ContractRecordV1::Current(record)) => Some(record),
                _ => return Err(bad()),
            }
        } else {
            None
        };
    Ok(EvaluatedContractV1 {
        body,
        fees: source.source.fees,
        before,
    })
}
fn copy(text: &str, common: &mut FormalBoundedContextV1<'_>) -> Result<String, BodyMutationError> {
    charge(common, text.len())?;
    let text_copy = text.to_owned();
    if text_copy.capacity() > text.len() {
        return Err(limited());
    }
    Ok(text_copy)
}
fn after_record(
    body: &crate::WorldEventBody,
    before: Option<&EconomicContractState>,
    now: u64,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Option<ReservedValue<EconomicContractState>>, BodyMutationError> {
    let record = match body {
        crate::WorldEventBody::Domain(crate::DomainEvent::EconomicContractOpened {
            creator_agent_id,
            contract_id,
            counterparty_agent_id,
            fulfillment_kind,
            settlement_kind,
            settlement_amount,
            reputation_stake,
            expires_at,
            description,
        }) => {
            let payload = [
                creator_agent_id,
                contract_id,
                counterparty_agent_id,
                description,
            ]
            .iter()
            .try_fold(0usize, |n, s| n.checked_add(s.len()))
            .ok_or_else(limited)?;
            let fee = common
                .memory()
                .reserve(
                    size_of::<EconomicContractState>()
                        .checked_add(payload)
                        .ok_or_else(limited)?,
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            fee.retain(EconomicContractState {
                contract_id: copy(contract_id, common)?,
                creator_agent_id: copy(creator_agent_id, common)?,
                counterparty_agent_id: copy(counterparty_agent_id, common)?,
                fulfillment_kind: *fulfillment_kind,
                settlement_kind: *settlement_kind,
                settlement_amount: *settlement_amount,
                reputation_stake: *reputation_stake,
                expires_at: *expires_at,
                description: copy(description, common)?,
                status: EconomicContractStatus::Open,
                accepted_at: None,
                settled_at: None,
                settlement_success: None,
                transfer_amount: 0,
                tax_amount: 0,
                settlement_notes: None,
            })
        }
        crate::WorldEventBody::Domain(crate::DomainEvent::EconomicContractSettled {
            success,
            transfer_amount,
            tax_amount,
            notes,
            ..
        }) => {
            let before = before.ok_or_else(bad)?;
            let mut payload = notes.len();
            for text in [
                &before.contract_id,
                &before.creator_agent_id,
                &before.counterparty_agent_id,
                &before.description,
            ] {
                payload = payload.checked_add(text.len()).ok_or_else(limited)?;
            }
            if let Some(text) = &before.settlement_notes {
                payload = payload.checked_add(text.len()).ok_or_else(limited)?;
            }
            let fee = common
                .memory()
                .reserve(
                    size_of::<EconomicContractState>()
                        .checked_add(payload)
                        .ok_or_else(limited)?,
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            let after = EconomicContractState {
                contract_id: copy(&before.contract_id, common)?,
                creator_agent_id: copy(&before.creator_agent_id, common)?,
                counterparty_agent_id: copy(&before.counterparty_agent_id, common)?,
                fulfillment_kind: before.fulfillment_kind,
                settlement_kind: before.settlement_kind,
                settlement_amount: before.settlement_amount,
                reputation_stake: before.reputation_stake,
                expires_at: before.expires_at,
                description: copy(&before.description, common)?,
                status: EconomicContractStatus::Settled,
                accepted_at: before.accepted_at,
                settled_at: Some(now),
                settlement_success: Some(*success),
                transfer_amount: *transfer_amount,
                tax_amount: *tax_amount,
                settlement_notes: Some(copy(notes, common)?),
            };
            fee.retain(after)
        }
        crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected { .. }) => {
            return Ok(None);
        }
        _ => return Err(bad()),
    };
    Ok(Some(record))
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_contract_action_v1<
    's,
    'r,
    'source,
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    selected: &'s SelectedPendingActionV1,
    outcome: &'r ReservedValue<RootRulePhaseOutcomeV1>,
    sources: &'s ContractSourcesV1<'s, 'source>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut ContractWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<PreparedContractActionV1<'s, 'r, 'source>, BodyMutationError> {
    let result = (|| {
        charge(common, 0)?;
        selected.validate(context.transaction)?;
        if !outcome.uses_budget(common.memory())
            || !context.values.memory.same_pool(common.memory())
            || outcome.decision.verdict == crate::RuleVerdict::Deny
        {
            return Err(bad());
        }
        let action = outcome
            .override_action_for_selected_v1(selected, context.transaction)?
            .unwrap_or(&selected.envelope().action);
        if !matches!(
            action,
            crate::Action::OpenEconomicContract { .. }
                | crate::Action::SettleEconomicContract { .. }
        ) {
            return Err(bad());
        }
        let event_fee = common
            .memory()
            .reserve(
                size_of::<PreparedContractActionV1<'s, 'r, 'source>>()
                    + size_of::<crate::WorldEvent>()
                    + size_of::<PreparedAllianceEventV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let layout = common
            .memory()
            .reserve(
                size_of::<CompletedAllianceActionV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?
            .retain(());
        let result = evaluate(
            selected.envelope().id,
            action,
            sources,
            context,
            parent,
            work,
            common,
            now,
            live,
            std::array::from_fn(|_| None),
        )?;
        let after = after_record(&result.body, result.before.as_deref(), now, common)?;
        let financial = if let Some(before) = result.before.as_deref() {
            Some(finance::prepare_financial_v1(
                before,
                &result.body,
                sources,
                context,
                now,
                common,
                live,
            )?)
        } else {
            None
        };
        let continuity = if let Some(after) = after
            .as_deref()
            .filter(|record| record.status == EconomicContractStatus::Settled)
        {
            Some(settlement_continuity::prepare_continuity_v1(
                after,
                &result.body,
                sources,
                context,
                parent,
                work,
                common,
                now,
                live,
            )?)
        } else {
            None
        };
        let mut verification: Fees = std::array::from_fn(|_| None);
        for (i, fee) in result.fees.iter().enumerate() {
            if let Some(fee) = fee {
                verification[i] = Some(
                    common
                        .memory()
                        .reserve(fee.reserved_bytes(), MemoryClass::Ordinary)
                        .map_err(ValueTreeError::Memory)?
                        .retain(()),
                );
            }
        }
        Ok(PreparedContractActionV1 {
            selected,
            outcome,
            sources,
            body: result.body,
            before: result.before,
            after,
            financial,
            continuity,
            fees: result.fees,
            verification,
            identity: context.transaction.guard(),
            core: common.work().identity(),
            memory: common.memory().clone(),
            time: now,
            event_fee,
            layout,
        })
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = context
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
