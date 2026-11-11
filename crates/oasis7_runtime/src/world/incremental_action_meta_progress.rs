//! GrantMetaProgress: prepay business, then Root's real Accepted, then commit.
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
    incremental_field_member::{MemberRecordLimitsV1, MemberRecordWriteContext},
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_event_decode::WorldEventDecodeWorkV1,
    incremental_world_native::{
        MetaProgressMutationWorkV1, PreparedCurrentMetaProgressV1,
        WorldNativeFieldsQualificationV1, WorldNativeSelectedReadWorkV1,
        prepare_current_meta_progress_delta_v1, recover_current_meta_progress_v1,
        write_prepared_current_meta_progress_v1,
    },
    incremental_world_state_genesis::TypedRecordWriteWorkV1,
};
use std::cell::RefCell;
#[path = "incremental_action_meta_progress_event.rs"]
mod event;
#[path = "incremental_action_meta_progress_prepare.rs"]
mod prepare;
#[path = "incremental_action_meta_progress_read.rs"]
mod read;
#[path = "incremental_action_meta_progress_settle.rs"]
mod settle;
pub(in crate::world) struct MetaProgressSourcesV1<'a> {
    pub agents: &'a ReservedValue<AgentSubtreeStructureQualificationV1>,
    pub native: &'a ReservedValue<WorldNativeFieldsQualificationV1>,
}
pub(in crate::world) struct MetaProgressWorkV1 {
    pub steps_remaining: u64,
    pub bytes_remaining: u64,
    pub pending_entries_remaining: usize,
    pub native_scan: StateScanBudget,
    pub native_read: WorldNativeSelectedReadWorkV1,
    pub native_write: TypedRecordWriteWorkV1,
    pub records_remaining: u64,
    pub mutation: MetaProgressMutationWorkV1,
}
impl MetaProgressWorkV1 {
    fn spend(&mut self, n: usize) -> Result<(), BodyMutationError> {
        self.steps_remaining = self
            .steps_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("meta business steps"))?;
        self.bytes_remaining = self
            .bytes_remaining
            .checked_sub(
                u64::try_from(n)
                    .map_err(|_| ValueTreeError::ResourceLimited("meta business bytes"))?,
            )
            .ok_or(ValueTreeError::ResourceLimited("meta business bytes"))?;
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
enum MetaIntentV1 {
    Rejected(crate::RejectReason),
    Grant {
        delta: ReservedValue<PreparedCurrentMetaProgressV1>,
        body: crate::DomainEvent,
    },
}
/// Constructible only by actual Selected+same RuleOutcome Current preparation.
/// It has no future WorldEvent ID. Its immutable source and all growth fees
/// remain live across Accepted and the subsequent actual Current CAS.
pub(in crate::world) struct PreparedMetaProgressActionV1<'s, 'r> {
    selected: &'s SelectedPendingActionV1,
    outcome: &'r ReservedValue<RootRulePhaseOutcomeV1>,
    intent: MetaIntentV1,
    time: u64,
    identity: PreparedGuard,
    memory: MemoryBudget,
    event_fee: MemoryReservation,
    completion: MetaProgressRejectionReservationV1,
}
/// Minted only after this consumer's actual Current business publication and
/// real event allocator read. The shared original sink accepts this private cap.
pub(in crate::world) struct PreparedMetaProgressEventV1 {
    event: ReservedValue<crate::WorldEvent>,
    action_id: u64,
    time: u64,
    identity: PreparedGuard,
}
impl PreparedMetaProgressEventV1 {
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
                crate::WorldEventBody::Domain(crate::DomainEvent::MetaProgressGranted { .. })
            )
        {
            return Err(BodyMutationError::Invalid("meta event receipt binding"));
        }
        Ok(())
    }
}
pub(in crate::world) struct MetaProgressRejectionReservationV1 {
    memory: MemoryBudget,
    fee: MemoryReservation,
}
enum CompletedMetaResultV1 {
    Published {
        event: ReservedValue<crate::WorldEvent>,
        identity: PreparedGuard,
        _delta: ReservedValue<PreparedCurrentMetaProgressV1>,
    },
    Rejected(SelectedActionAuditV1),
}
pub(in crate::world) struct CompletedMetaProgressV1 {
    accepted: Option<SelectedActionAuditV1>,
    identity: PreparedGuard,
    result: CompletedMetaResultV1,
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
        return Err(BodyMutationError::Invalid("meta Accepted receipt source"));
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
            "meta Accepted receipt kind/action",
        )),
    }
}
impl CompletedMetaProgressV1 {
    pub(in crate::world) fn reserve_rejection(
        memory: &MemoryBudget,
    ) -> Result<MetaProgressRejectionReservationV1, BodyMutationError> {
        Ok(MetaProgressRejectionReservationV1 {
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
        prepaid: MetaProgressRejectionReservationV1,
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
            return Err(BodyMutationError::Invalid("meta Rejected receipt binding"));
        }
        Ok(Self {
            accepted: accepted.into_audit(),
            identity: tx.guard(),
            result: CompletedMetaResultV1::Rejected(rejected),
            _fee: prepaid.fee,
        })
    }
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        match &self.result {
            CompletedMetaResultV1::Published { event, .. } => event,
            CompletedMetaResultV1::Rejected(a) => a.event(),
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
            CompletedMetaResultV1::Published { identity, .. } => {
                tx.validate_transaction_identity(identity)
            }
            CompletedMetaResultV1::Rejected(a) => a.validate(tx),
        }
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_selected_meta_progress_v1<'s, 'r, M, P, V, W, E, C, PE, PC, D>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    selected: &'s SelectedPendingActionV1,
    outcome: &'r ReservedValue<RootRulePhaseOutcomeV1>,
    sources: &MetaProgressSourcesV1<'_>,
    roots: &RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut MetaProgressWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<PreparedMetaProgressActionV1<'s, 'r>, BodyMutationError>
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
pub(in crate::world) fn commit_prepared_meta_progress_v1<M, P, V, W, E, C, PE, PC, ME, D, F>(
    prepared: PreparedMetaProgressActionV1<'_, '_>,
    accepted: crate::world::incremental_module_invocation::SelectedAcceptedDispositionV1,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    sources: &MetaProgressSourcesV1<'_>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut MetaProgressWorkV1,
    audit_work: &mut WorldEventDecodeWorkV1,
    max_event_bytes: usize,
    live: &ExecutionFence,
    mut persist: F,
) -> Result<CompletedMetaProgressV1, BodyMutationError>
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
