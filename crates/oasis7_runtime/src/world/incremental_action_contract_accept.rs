//! AcceptEconomicContract: prepay business, then Root's real Accepted, then commit.
//! Admission, Rule/Modify, common fees and PostAction remain with the sole driver.
use crate::world::{
    World,
    incremental_action_execution::{SelectedActionAuditV1, SelectedPendingActionV1},
    incremental_body::BodyMutationError,
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_module_invocation::RootRulePhaseOutcomeV1,
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
    },
    incremental_state::{MerkleNodeReader, StatePageWriter, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_agent_subtree::{
        AgentSubtreeStructureQualificationV1, CurrentAgentPresenceV1,
        read_current_agent_presence_v1,
    },
    incremental_diplomacy::DiplomacyValueV1 as RecordValueV1,
    incremental_diplomacy::{
        ContractScalarV1, DiplomacyFamilyQualificationV1, DiplomacyFieldV1,
        DiplomacyRecordQualificationV1, DiplomacyScalarV1, DiplomacyScalarsChangeV1,
        DiplomacySourceV1, change_qualified_diplomacy_scalars_v1,
        open_current_diplomacy_contract_record_v1, read_qualified_diplomacy_scalar_v1,
    },
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_event_decode::WorldEventDecodeWorkV1,
};
use std::cell::RefCell;
#[path = "incremental_action_contract_accept_event.rs"]
mod event;
#[path = "incremental_action_contract_accept_prepare.rs"]
mod prepare;
#[path = "incremental_action_contract_accept_read.rs"]
mod read;
#[path = "incremental_action_contract_accept_settle.rs"]
mod settle;
pub(in crate::world) struct ContractAcceptSourcesV1<'a> {
    pub agents: &'a ReservedValue<AgentSubtreeStructureQualificationV1>,
    pub diplomacy: &'a ReservedValue<DiplomacyFamilyQualificationV1>,
}
pub(in crate::world) struct ContractAcceptWorkV1 {
    pub steps_remaining: u64,
    pub bytes_remaining: u64,
    pub pending_entries_remaining: usize,
    pub contract_scan: StateScanBudget,
    pub cells_remaining: u64,
    pub max_text_bytes: u64,
    pub max_pending_node_bytes: usize,
}
impl ContractAcceptWorkV1 {
    fn spend(&mut self, n: usize) -> Result<(), BodyMutationError> {
        self.steps_remaining =
            self.steps_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "contract_accept business steps",
                ))?;
        self.bytes_remaining =
            self.bytes_remaining
                .checked_sub(u64::try_from(n).map_err(|_| {
                    ValueTreeError::ResourceLimited("contract_accept business bytes")
                })?)
                .ok_or(ValueTreeError::ResourceLimited(
                    "contract_accept business bytes",
                ))?;
        Ok(())
    }
}
fn overlay_error(error: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match error {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        WorldFieldOverlayErrorV1::Member(e) => BodyMutationError::Original(Box::new(e)),
    }
}
enum ContractAcceptIntentV1 {
    Rejected(crate::RejectReason),
    Resolve {
        current: ReservedValue<DiplomacyRecordQualificationV1>,
        before: ReservedValue<oasis7_world_state::gameplay_state::EconomicContractState>,
        after: ReservedValue<oasis7_world_state::gameplay_state::EconomicContractState>,
        body: crate::DomainEvent,
    },
}
/// Constructible only by actual Selected+same RuleOutcome Current preparation.
/// It has no future WorldEvent ID. Its immutable source and all growth fees
/// remain live across Accepted and the subsequent actual Current CAS.
pub(in crate::world) struct PreparedContractAcceptActionV1<'s, 'r> {
    selected: &'s SelectedPendingActionV1,
    outcome: &'r ReservedValue<RootRulePhaseOutcomeV1>,
    intent: ContractAcceptIntentV1,
    time: u64,
    identity: PreparedGuard,
    memory: MemoryBudget,
    event_fee: MemoryReservation,
    completion: ContractAcceptRejectionReservationV1,
}
/// Minted only after this consumer's actual Current business publication and
/// real event allocator read. The shared original sink accepts this private cap.
pub(in crate::world) struct PreparedContractAcceptEventV1 {
    event: ReservedValue<crate::WorldEvent>,
    action_id: u64,
    time: u64,
    identity: PreparedGuard,
}
impl PreparedContractAcceptEventV1 {
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        &self.event
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.identity)?;
        if !self.event.uses_budget(memory)
            || !tx.uses_budget(memory)
            || self.time != now
            || self.event.time != now
            || self.event.caused_by != Some(crate::CausedBy::Action(self.action_id))
            || !matches!(
                self.event.body,
                crate::WorldEventBody::Domain(crate::DomainEvent::EconomicContractAccepted { .. })
            )
        {
            return Err(BodyMutationError::Invalid(
                "contract_accept event receipt binding",
            ));
        }
        Ok(())
    }
}
pub(in crate::world) struct ContractAcceptRejectionReservationV1 {
    memory: MemoryBudget,
    fee: MemoryReservation,
}
enum CompletedContractAcceptResultV1 {
    Published {
        event: ReservedValue<crate::WorldEvent>,
        identity: PreparedGuard,
        _owner: ReservedValue<oasis7_world_state::gameplay_state::EconomicContractState>,
    },
    Rejected(SelectedActionAuditV1),
}
pub(in crate::world) struct CompletedContractAcceptV1 {
    accepted: Option<SelectedActionAuditV1>,
    identity: PreparedGuard,
    result: CompletedContractAcceptResultV1,
    _fee: MemoryReservation,
}
fn validate_accepted<'a, M: MerkleNodeReader>(
    disposition: &'a crate::world::incremental_module_invocation::SelectedAcceptedDispositionV1,
    selected: &SelectedPendingActionV1,
    tx: &ExecutionTransaction<'_, M>,
    memory: &MemoryBudget,
    now: u64,
) -> Result<Option<&'a SelectedActionAuditV1>, BodyMutationError> {
    disposition.validate(selected, tx, memory)?;
    let Some(audit) = disposition.audit() else {
        return Ok(None);
    };
    selected.validate(tx)?;
    audit.validate(tx)?;
    let event = audit.event();
    let envelope = selected.envelope();
    if !audit.uses_budget(memory)
        || !tx.uses_budget(memory)
        || event.time != now
        || event.caused_by != Some(crate::CausedBy::Action(envelope.id))
    {
        return Err(BodyMutationError::Invalid(
            "contract_accept Accepted receipt source",
        ));
    }
    match &event.body {
        crate::WorldEventBody::Domain(crate::DomainEvent::ActionAccepted {
            action_id,
            action_kind,
            actor_id,
            ..
        }) if *action_id == envelope.id
            && action_kind
                == crate::world::module_runtime_labels::action_kind_label(&envelope.action)
            && actor_id == envelope.action.actor_id().unwrap_or("system") =>
        {
            Ok(Some(audit))
        }
        _ => Err(BodyMutationError::Invalid(
            "contract_accept Accepted receipt kind/action",
        )),
    }
}
impl CompletedContractAcceptV1 {
    pub(in crate::world) fn reserve_rejection(
        memory: &MemoryBudget,
    ) -> Result<ContractAcceptRejectionReservationV1, BodyMutationError> {
        Ok(ContractAcceptRejectionReservationV1 {
            memory: memory.clone(),
            fee: memory
                .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?,
        })
    }
    pub(in crate::world) fn from_rejection<M: MerkleNodeReader>(
        accepted: crate::world::incremental_module_invocation::SelectedAcceptedDispositionV1,
        rejected: SelectedActionAuditV1,
        selected: &SelectedPendingActionV1,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        prepaid: ContractAcceptRejectionReservationV1,
    ) -> Result<Self, BodyMutationError> {
        let accepted_audit =
            validate_accepted(&accepted, selected, tx, memory, rejected.event().time)?;
        rejected.validate(tx)?;
        if !prepaid.memory.same_pool(memory)
            || !rejected.uses_budget(memory)
            || accepted_audit.is_some_and(|audit| rejected.event().id == audit.event().id)
            || rejected.event().caused_by != Some(crate::CausedBy::Action(selected.envelope().id))
            || !matches!(&rejected.event().body,crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected {action_id,..})if *action_id==selected.envelope().id)
        {
            return Err(BodyMutationError::Invalid(
                "contract_accept Rejected receipt binding",
            ));
        }
        Ok(Self {
            accepted: accepted.into_audit(),
            identity: tx.guard(),
            result: CompletedContractAcceptResultV1::Rejected(rejected),
            _fee: prepaid.fee,
        })
    }
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        match &self.result {
            CompletedContractAcceptResultV1::Published { event, .. } => event,
            CompletedContractAcceptResultV1::Rejected(a) => a.event(),
        }
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
    ) -> Result<(), ExecutionError> {
        tx.validate_transaction_identity(&self.identity)?;
        if let Some(accepted) = &self.accepted {
            accepted.validate(tx)?;
        }
        match &self.result {
            CompletedContractAcceptResultV1::Published { identity, .. } => {
                tx.validate_transaction_identity(identity)
            }
            CompletedContractAcceptResultV1::Rejected(a) => a.validate(tx),
        }
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_selected_contract_accept_v1<'s, 'r, M, P, V, W, E, C, PE, PC, D>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    selected: &'s SelectedPendingActionV1,
    outcome: &'r ReservedValue<RootRulePhaseOutcomeV1>,
    sources: &ContractAcceptSourcesV1<'_>,
    roots: &RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut ContractAcceptWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<PreparedContractAcceptActionV1<'s, 'r>, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    D: FnMut() -> bool,
{
    let result = prepare::prepare(
        step,
        selected,
        outcome,
        sources,
        roots,
        read,
        parent_read,
        work,
        guard,
        live,
    );
    if result.is_err() {
        let _: Result<(), ExecutionError> = step
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn commit_prepared_contract_accept_v1<M, P, V, W, E, C, PE, PC, ME, D, F>(
    prepared: PreparedContractAcceptActionV1<'_, '_>,
    accepted: crate::world::incremental_module_invocation::SelectedAcceptedDispositionV1,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    sources: &ContractAcceptSourcesV1<'_>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut ContractAcceptWorkV1,
    audit_work: &mut WorldEventDecodeWorkV1,
    max_event_bytes: usize,
    live: &ExecutionFence,
    mut persist: F,
) -> Result<CompletedContractAcceptV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    D: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let result = settle::commit(
        prepared,
        accepted,
        step,
        materials,
        sources,
        read,
        parent_read,
        work,
        audit_work,
        max_event_bytes,
        live,
        &mut persist,
    );
    if result.is_err() {
        let _: Result<(), ExecutionError> = step
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
