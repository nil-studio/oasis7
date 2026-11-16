//! The concrete pre-Rule failure lane. A real initial admission and one real
//! pre-VM/worker failure source are consumed; no RuleOutcome is fabricated.
use super::ordinary_settlement::OrdinaryDriverErrorV1;
use super::*;
use crate::world::{
    incremental_action_admission::UnboundActionAdmissionV1,
    incremental_action_execution::{
        AcceptedActorReadContextV1, SelectedActionAuditV1, SelectedPendingActionV1,
    },
    incremental_normal_action_factory_schedule::{
        FactoryScheduleSourcesV1, FactoryScheduleWorkV1, factory_module::*,
    },
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionError, ExecutionTransaction, PreparedGuard},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryReservation};
use oasis7_world_state::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_event_decode::WorldEventDecodeWorkV1,
};
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("factory module rejection source")
}
fn pay(common: &mut FormalBoundedContextV1<'_>, bytes: usize) -> Result<(), BodyMutationError> {
    common
        .charge(1, u64::try_from(bytes).map_err(|_| bad())?)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))
}
pub(in crate::world) struct FactoryModuleInitialAdmissionV1<'s> {
    permission: ReservedValue<UnboundActionAdmissionV1<'s>>,
    selected: &'s SelectedPendingActionV1,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    now: u64,
    fee: ReservedValue<()>,
}
pub(in crate::world) fn prepare_factory_module_initial_admission_v1<'s, M: MerkleNodeReader>(
    permission: ReservedValue<UnboundActionAdmissionV1<'s>>,
    selected: &'s SelectedPendingActionV1,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<FactoryModuleInitialAdmissionV1<'s>, BodyMutationError> {
    permission.validate(selected, tx, common.memory(), live)?;
    if !permission.uses_budget(common.memory()) {
        return Err(bad());
    }
    pay(
        common,
        std::mem::size_of::<FactoryModuleInitialAdmissionV1>(),
    )?;
    let fee = common
        .memory()
        .reserve(
            std::mem::size_of::<FactoryModuleInitialAdmissionV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?
        .retain(());
    Ok(FactoryModuleInitialAdmissionV1 {
        permission,
        selected,
        guard: tx.guard(),
        core: common.work().identity(),
        now,
        fee,
    })
}
impl FactoryModuleInitialAdmissionV1<'_> {
    fn validate<M: MerkleNodeReader>(
        &self,
        selected: &SelectedPendingActionV1,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.permission
            .validate_evidence_identity(selected, tx, common.memory(), live)?;
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        pay(common, 128)?;
        if !std::ptr::eq(self.selected, selected)
            || self.core != common.work().identity()
            || self.now != now
            || !self.permission.uses_budget(common.memory())
            || !self.fee.uses_budget(common.memory())
        {
            return Err(bad());
        }
        Ok(())
    }
}
/// All payloads are owned after publication, so no queue-selection borrow can
/// escape the FIFO. Actual source payloads are dropped only after their facts
/// and correlated audits have succeeded.
pub(in crate::world) struct CompletedFactoryModuleRejectionV1 {
    accepted: Option<SelectedActionAuditV1>,
    rejected: SelectedActionAuditV1,
    module_failure: Option<ReservedValue<crate::WorldEvent>>,
    followup: Option<PreparedFactoryModuleFollowupV1>,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    now: u64,
    action_id: u64,
    fee: ReservedValue<()>,
}
impl CompletedFactoryModuleRejectionV1 {
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        self.rejected.event()
    }
    pub(in crate::world) fn validate_transaction<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
    ) -> Result<(), ExecutionError> {
        tx.validate_transaction_identity(&self.guard)?;
        self.rejected.validate(tx)?;
        if let Some(a) = &self.accepted {
            a.validate(tx)?
        }
        Ok(())
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
        identity: WorkIdentityV1,
    ) -> Result<(), BodyMutationError> {
        self.validate_transaction(tx)?;
        if self.now != now
            || self.core != identity
            || !self.fee.uses_budget(memory)
            || !tx.uses_budget(memory)
            || self.rejected.event().caused_by != Some(crate::CausedBy::Action(self.action_id))
            || !matches!(&self.rejected.event().body,crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected{action_id,..})if *action_id==self.action_id)
        {
            return Err(bad());
        }
        if let Some(f) = &self.module_failure {
            if !f.uses_budget(memory)
                || f.time != now
                || f.caused_by.is_some()
                || !matches!(&f.body, crate::WorldEventBody::ModuleCallFailed(_))
            {
                return Err(bad());
            }
        }
        if let Some(f) = &self.followup {
            f.validate_identity(tx, memory, now, identity)?;
        }
        Ok(())
    }
}
pub(in crate::world) struct PreparedFactoryModuleFollowupV1 {
    event: ReservedValue<crate::WorldEvent>,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    now: u64,
    action_id: u64,
    _payload: MemoryReservation,
}
impl PreparedFactoryModuleFollowupV1 {
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        &self.event
    }
    fn validate_identity<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
        core: WorkIdentityV1,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        if self.core != core
            || self.now != now
            || !self.event.uses_budget(memory)
            || !self._payload.uses_budget(memory)
            || self.event.caused_by != Some(crate::CausedBy::Action(self.action_id))
            || !matches!(&self.event.body,crate::WorldEventBody::Domain(crate::DomainEvent::FactoryProductionBlocked{action_id,..})if *action_id==self.action_id)
        {
            return Err(bad());
        }
        Ok(())
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
        core: WorkIdentityV1,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.validate_identity(tx, memory, now, core)?;
        tx.validate_guard(&self.guard, live)?;
        Ok(())
    }
}

fn copy_reason(
    reason: &crate::RejectReason,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(crate::RejectReason, MemoryReservation), BodyMutationError> {
    struct Count<'a, 'b> {
        common: &'a mut FormalBoundedContextV1<'b>,
        n: usize,
        first: Option<BodyMutationError>,
    }
    impl std::io::Write for Count<'_, '_> {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            if self.first.is_some() {
                return Err(std::io::ErrorKind::Other.into());
            }
            if let Err(e) = pay(self.common, b.len()) {
                self.first = Some(e);
                return Err(std::io::ErrorKind::Other.into());
            }
            self.n = self
                .n
                .checked_add(b.len())
                .ok_or(std::io::ErrorKind::InvalidData)?;
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count {
        common,
        n: 0,
        first: None,
    };
    let encoded = serde_json::to_writer(&mut count, reason);
    if let Some(e) = count.first.take() {
        return Err(e);
    }
    encoded.map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let bytes = count
        .n
        .checked_mul(2048)
        .and_then(|n| n.checked_add(std::mem::size_of::<crate::RejectReason>()))
        .ok_or_else(bad)?;
    drop(count);
    pay(common, bytes)?;
    let fee = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    Ok((reason.clone(), fee))
}
fn overlay(
    error: oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1,
) -> BodyMutationError {
    match error {
        oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Execution(e) => {
            e.into()
        }
        oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(e) => {
            e.into()
        }
        oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Member(e) => {
            BodyMutationError::Original(Box::new(e))
        }
    }
}
fn source_selected<'a, 's>(
    source: &'a FactoryModuleBeforeRulesOutcomeV1<'s>,
) -> Result<&'a SelectedPendingActionV1, BodyMutationError> {
    match source {
        FactoryModuleBeforeRulesOutcomeV1::PreRejected(v) => Ok(v.selected()),
        FactoryModuleBeforeRulesOutcomeV1::Denied(v) => Ok(v.selected()),
        FactoryModuleBeforeRulesOutcomeV1::Invalid(v) => Ok(v.selected()),
        FactoryModuleBeforeRulesOutcomeV1::Failed(v) => Ok(v.selected()),
        FactoryModuleBeforeRulesOutcomeV1::Resolved(_) => Err(bad()),
    }
}
fn validate_source<M: MerkleNodeReader>(
    source: &FactoryModuleBeforeRulesOutcomeV1<'_>,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError> {
    match source {
        FactoryModuleBeforeRulesOutcomeV1::PreRejected(v) => v.validate(tx, common, now, live),
        FactoryModuleBeforeRulesOutcomeV1::Denied(v) => v.validate(tx, common, now, live),
        FactoryModuleBeforeRulesOutcomeV1::Invalid(v) => v.validate(tx, common, now, live),
        FactoryModuleBeforeRulesOutcomeV1::Failed(v) => v.validate(tx, common, now, live),
        FactoryModuleBeforeRulesOutcomeV1::Resolved(_) => Err(bad()),
    }
}
fn failure_payload<'a>(
    source: &'a FactoryModuleBeforeRulesOutcomeV1<'_>,
) -> Result<
    Option<(
        &'a str,
        &'a str,
        oasis7_wasm_abi::ModuleCallErrorCode,
        &'a str,
    )>,
    BodyMutationError,
> {
    match source {
        FactoryModuleBeforeRulesOutcomeV1::Invalid(v) => {
            let f = v.failure();
            Ok(Some((&f.module_id, &f.trace_id, f.code.clone(), &f.detail)))
        }
        FactoryModuleBeforeRulesOutcomeV1::Failed(v) => match v.execution().vm_failure() {
            Some(crate::WorldError::ModuleCallFailed {
                module_id,
                trace_id,
                code,
                detail,
            }) => Ok(Some((module_id, trace_id, code.clone(), detail))),
            _ => Err(bad()),
        },
        FactoryModuleBeforeRulesOutcomeV1::PreRejected(_)
        | FactoryModuleBeforeRulesOutcomeV1::Denied(_) => Ok(None),
        FactoryModuleBeforeRulesOutcomeV1::Resolved(_) => Err(bad()),
    }
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader
        + ValueNodeWriter
        + oasis7_formal_protocol::incremental_state::StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    fn capture_factory_module_followup_v1<P, PE, PC>(
        &mut self,
        selected: &SelectedPendingActionV1,
        sources: &FactoryScheduleSourcesV1<'_>,
        parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        work: &mut FactoryScheduleWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<
        Option<ReservedValue<crate::world::step::FactoryProductionFollowupContext>>,
        RootModuleExecutionErrorV1<O::Error>,
    >
    where
        P: MerkleNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        let factory = match &selected.envelope().action {
            crate::Action::ScheduleRecipeWithModule { factory_id, .. } => factory_id,
            _ => return Ok(None),
        };
        let before=crate::world::incremental_normal_action_factory_schedule::with_factory_schedule_read_io_v1(sources.roots,self.ctx,parent,work,common,|ctx,_,w|{
   let guard=ctx.transaction.guard();Ok(oasis7_world_state::incremental_factory_schedule_factory::recover_current_schedule_complete_factory_v1(factory,sources.lifecycle.factories,ctx,&mut w.factory,&guard,&live_owner())?)
  }).map_err(|e|match e{crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1::Body(e)=>RootModuleExecutionErrorV1::Body(e),crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1::World(e)=>{match common.memory().reserve(std::mem::size_of::<crate::WorldError>()+factory.len().saturating_mul(24)+4096,MemoryClass::Ordinary){Ok(fee)=>RootModuleExecutionErrorV1::Business(fee.retain(e)),Err(error)=>RootModuleExecutionErrorV1::Value(ValueTreeError::Memory(error))}}})?;
        let Some(before) = before else {
            return Ok(None);
        };
        let bytes = std::mem::size_of::<crate::world::step::FactoryProductionFollowupContext>()
            .checked_add(
                before
                    .production
                    .current_blocker_kind
                    .as_ref()
                    .map_or(0, String::len),
            )
            .and_then(|n| {
                n.checked_add(
                    before
                        .production
                        .current_blocker_detail
                        .as_ref()
                        .map_or(0, String::len),
                )
            })
            .ok_or_else(bad)?;
        pay(common, bytes).map_err(RootModuleExecutionErrorV1::Body)?;
        let fee = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(Some(fee.retain(
            crate::world::step::FactoryProductionFollowupContext {
                active_jobs: before.production.active_jobs,
                current_status: Some(before.production.status),
                blocked_at: before.production.last_blocked_at,
                blocker_kind: before.production.current_blocker_kind.clone(),
                blocker_detail: before.production.current_blocker_detail.clone(),
            },
        )))
    }
    fn persist_factory_module_failure_v1(
        &mut self,
        source: &FactoryModuleBeforeRulesOutcomeV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<Option<ReservedValue<crate::WorldEvent>>, RootModuleExecutionErrorV1<O::Error>>
    {
        // The old Build resolver ran the VM in a cloned staged World and dropped
        // its ModuleCallFailed journal on every failed resolution. Recipe/Product
        // ran directly in the action World and retain exactly one failure event.
        if matches!(
            &source_selected(source)
                .map_err(RootModuleExecutionErrorV1::Body)?
                .envelope()
                .action,
            crate::Action::BuildFactoryWithModule { .. }
        ) {
            return Ok(None);
        }
        let Some((module_id, trace_id, code, detail)) =
            failure_payload(source).map_err(RootModuleExecutionErrorV1::Body)?
        else {
            return Ok(None);
        };
        let bytes = module_id
            .len()
            .checked_add(trace_id.len())
            .and_then(|n| {
                n.checked_add(
                    detail.len()
                        + std::mem::size_of::<crate::WorldEvent>()
                        + std::mem::size_of::<oasis7_wasm_abi::ModuleCallFailure>(),
                )
            })
            .ok_or_else(bad)?;
        pay(common, bytes).map_err(RootModuleExecutionErrorV1::Body)?;
        let fee = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let id=crate::world::incremental_normal_tail::event_preview::preview_current_normal_event_id_v1(self.ctx,common).map_err(RootModuleExecutionErrorV1::Body)?;
        let event = fee.retain(crate::WorldEvent {
            id,
            time: self.now,
            caused_by: None,
            body: crate::WorldEventBody::ModuleCallFailed(oasis7_wasm_abi::ModuleCallFailure {
                module_id: module_id.to_owned(),
                trace_id: trace_id.to_owned(),
                code,
                detail: detail.to_owned(),
            }),
        });
        with_current_root_module_meter_v1(self, common, live_owner, |root, _| {
            crate::world::incremental_module_execution_event::persist_module_output_event_v1(
                root.now,
                &event,
                root.ctx.transaction,
                root.events,
            )
            .map(|_| ())
            .map_err(RootModuleExecutionErrorV1::Body)
        })?;
        Ok(Some(event))
    }
    fn factory_module_reason_v1(
        &self,
        source: &FactoryModuleBeforeRulesOutcomeV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(crate::RejectReason, MemoryReservation), BodyMutationError> {
        match source {
            FactoryModuleBeforeRulesOutcomeV1::PreRejected(v) => copy_reason(v.reason(), common),
            FactoryModuleBeforeRulesOutcomeV1::Denied(v) => copy_reason(v.reason(), common),
            FactoryModuleBeforeRulesOutcomeV1::Failed(v) => copy_reason(v.reason(), common),
            FactoryModuleBeforeRulesOutcomeV1::Invalid(v) => {
                let f = v.failure();
                let bytes = f
                    .module_id
                    .len()
                    .checked_add(f.trace_id.len())
                    .and_then(|n| {
                        n.checked_add(f.detail.len() + std::mem::size_of::<crate::WorldError>())
                    })
                    .ok_or_else(bad)?;
                pay(common, bytes)?;
                let _error_fee = common
                    .memory()
                    .reserve(bytes, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?;
                let error = crate::WorldError::ModuleCallFailed {
                    module_id: f.module_id.clone(),
                    trace_id: f.trace_id.clone(),
                    code: f.code.clone(),
                    detail: f.detail.clone(),
                };
                let reason = format_failure_rejection_v1(&error, common)?;
                copy_reason(&reason, common)
            }
            FactoryModuleBeforeRulesOutcomeV1::Resolved(_) => Err(bad()),
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn execute_factory_module_rejection_v1<'selected, P, ER, CR, PE, PC, ME>(
        &mut self,
        source: FactoryModuleBeforeRulesOutcomeV1<'selected>,
        initial: FactoryModuleInitialAdmissionV1<'selected>,
        sources: &FactoryScheduleSourcesV1<'_>,
        work: &mut FactoryScheduleWorkV1,
        materials:&mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<'_,M,ME>,
        read: &mut WorldFieldReadContextV1<'_, P, R, ER, CR>,
        parent_read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        common: &mut FormalBoundedContextV1<'_>,
        mut accepted_entries: Option<&mut usize>,
        audit_work: &mut WorldEventDecodeWorkV1,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        tail_work: &mut crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<CompletedFactoryModuleRejectionV1, OrdinaryDriverErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        ER: FnMut() -> Option<ValueTreeError>,
        CR: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
        ME: FnMut() -> Option<ValueTreeError>,
    {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let selected = source_selected(&source).map_err(RootModuleExecutionErrorV1::Body)?;
            let live = live_owner();
            initial
                .validate(selected, self.ctx.transaction, common, self.now, &live)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            validate_source(&source, self.ctx.transaction, common, self.now, &live)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            pay(
                common,
                std::mem::size_of::<CompletedFactoryModuleRejectionV1>(),
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            let fee = common
                .memory()
                .reserve(
                    std::mem::size_of::<CompletedFactoryModuleRejectionV1>(),
                    MemoryClass::Ordinary,
                )
                .map_err(|e| {
                    RootModuleExecutionErrorV1::Body(BodyMutationError::Value(
                        ValueTreeError::Memory(e),
                    ))
                })?
                .retain(());
            let module_failure =
                self.persist_factory_module_failure_v1(&source, common, live_owner)?;
            let captured = self.capture_factory_module_followup_v1(
                selected,
                sources,
                parent_read,
                work,
                common,
                live_owner,
            )?;
            let (reason, _reason_copy_fee) = self
                .factory_module_reason_v1(&source, common)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let guard = self.ctx.transaction.guard();
            let live = live_owner();
            let mut step = IwsStepContext::borrow_counted_at(
                self.ctx.transaction,
                self.now,
                &guard,
                &live,
                self.events.values,
                &mut self.events.mutation,
                &mut self.events.original_work,
                self.events.world_events,
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            // Temporarily lend the original-family entry lane to the one shared mapper.
            // It is restored on every return/unwind with its actual consumed remainder;
            // no source quota is recreated, refilled or silently replaced by target work.
            let parked_pending = work.pending_entries_remaining;
            if let Some(entries) = accepted_entries.as_deref() {
                work.pending_entries_remaining = *entries;
            }
            let audits = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::world::incremental_step::factory_schedule_write_io::paid_with_audit(
                    &mut step,
                    read,
                    parent_read,
                    self.members,
                    materials,
                    work,
                    audit_work,
                    common,
                    &live,
                    |step, read, parent, _write, materials, work, audit, _tail| {
                        let mut actor_read = AcceptedActorReadContextV1 {
                            roots: sources.roots,
                            agents: sources.lifecycle.agents,
                            current: read,
                            parent,
                            pending_entries: &mut work.pending_entries_remaining,
                            live: &live,
                        };
                        let accepted = step.append_selected_accepted(
                            materials,
                            &live,
                            selected,
                            &mut actor_read,
                            self.events.max_event_bytes,
                            audit,
                            &mut self.events.persist_original,
                        )?;
                        let rejected = step.append_selected_rejection_owned(
                            materials,
                            &live,
                            selected,
                            reason,
                            self.events.max_event_bytes,
                            audit,
                            &mut self.events.persist_original,
                        )?;
                        Ok((accepted, rejected))
                    },
                )
            }));
            if let Some(entries) = accepted_entries.as_deref_mut() {
                *entries = work.pending_entries_remaining;
                work.pending_entries_remaining = parked_pending;
            }
            let (accepted, rejected) = match audits {
                Ok(v) => v.map_err(RootModuleExecutionErrorV1::Body)?,
                Err(p) => std::panic::resume_unwind(p),
            };
            drop(step);
            if crate::World::should_emit_action_accepted(&selected.envelope().action)
                != accepted.is_some()
            {
                return Err(RootModuleExecutionErrorV1::Body(bad()).into());
            }
            let persisted=crate::world::incremental_module_execution_event::PersistedWorldEventV1::from_action_audit(&rejected);
            with_current_root_route_meter_v1(
                self,
                common,
                live_owner,
                enumeration,
                tail_work,
                |root, live, enumeration, tail| {
                    root.route_post_action_v1(
                        selected,
                        Some(&persisted),
                        directory,
                        enumeration,
                        live,
                    )?;
                    root.route_latest_world_event_v1(directory, enumeration, tail, live)?;
                    Ok(())
                },
            )?;
            let followup = self.complete_factory_module_followup_v1(
                selected,
                &rejected,
                captured.as_deref(),
                sources,
                work,
                materials,
                read,
                parent_read,
                common,
                live_owner,
            )?;
            if let Some(prepared) = &followup {
                self.publish_factory_module_followup_v1(
                    prepared,
                    materials,
                    common,
                    directory,
                    enumeration,
                    tail_work,
                    live_owner,
                )?;
            }
            let completed = CompletedFactoryModuleRejectionV1 {
                accepted,
                rejected,
                module_failure,
                followup,
                guard: self.ctx.transaction.guard(),
                core: common.work().identity(),
                now: self.now,
                action_id: selected.envelope().id,
                fee,
            };
            completed
                .validate(
                    self.ctx.transaction,
                    common.memory(),
                    self.now,
                    common.work().identity(),
                )
                .map_err(RootModuleExecutionErrorV1::Body)?;
            // Actual selected/failure/initial source fees live through all publication;
            // the independent retained audit/event payloads now own every returned byte.
            drop(source);
            drop(initial);
            Ok(completed)
        }));
        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                Err(error)
            }
            Err(panic) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                std::panic::resume_unwind(panic)
            }
        }
    }
    fn complete_factory_module_followup_v1<P, ER, CR, PE, PC, ME>(
        &mut self,
        selected: &SelectedPendingActionV1,
        rejection: &SelectedActionAuditV1,
        captured: Option<&crate::world::step::FactoryProductionFollowupContext>,
        sources: &FactoryScheduleSourcesV1<'_>,
        work: &mut FactoryScheduleWorkV1,
        materials:&mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<'_,M,ME>,
        read: &mut WorldFieldReadContextV1<'_, P, R, ER, CR>,
        parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<Option<PreparedFactoryModuleFollowupV1>, RootModuleExecutionErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        ER: FnMut() -> Option<ValueTreeError>,
        CR: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
        ME: FnMut() -> Option<ValueTreeError>,
    {
        rejection.validate(self.ctx.transaction)?;
        selected.validate(self.ctx.transaction)?;
        let crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected { action_id, reason }) =
            &rejection.event().body
        else {
            return Err(bad().into());
        };
        if *action_id != selected.envelope().id
            || rejection.event().caused_by != Some(crate::CausedBy::Action(*action_id))
        {
            return Err(bad().into());
        }
        let Some(captured) = captured else {
            return Ok(None);
        };
        let (requester, factory, recipe) = match &selected.envelope().action {
            crate::Action::ScheduleRecipeWithModule {
                requester_agent_id,
                factory_id,
                recipe_id,
                ..
            } => (requester_agent_id, factory_id, recipe_id),
            _ => return Ok(None),
        };
        let notes = match reason {
            crate::RejectReason::RuleDenied { notes } => notes
                .iter()
                .try_fold(0usize, |n, s| n.checked_add(s.len() + 3).ok_or_else(bad))?,
            _ => 0,
        };
        let bytes = requester
            .len()
            .checked_add(factory.len())
            .and_then(|n| n.checked_add(recipe.len()))
            .and_then(|n| n.checked_add(notes))
            .and_then(|n| n.checked_mul(24))
            .and_then(|n| {
                n.checked_add(8192 + std::mem::size_of::<PreparedFactoryModuleFollowupV1>())
            })
            .ok_or_else(bad)?;
        pay(common, bytes).map_err(RootModuleExecutionErrorV1::Body)?;
        let payload = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let Some(body) = crate::World::derive_factory_production_followup_from_context_v1(
            selected.envelope(),
            &rejection.event().body,
            Some(captured),
        ) else {
            return Ok(None);
        };
        let crate::DomainEvent::FactoryProductionBlocked {
            requester_agent_id,
            factory_id,
            blocker_kind,
            blocker_detail,
            ..
        } = &body
        else {
            return Err(bad().into());
        };
        if oasis7_world_state::state::industry_transition::recipe_blocker_is_terminal_v1(
            blocker_kind,
        ) {
            return Err(bad().into());
        }
        let guard = self.ctx.transaction.guard();
        let live = live_owner();
        let mut step = IwsStepContext::borrow_counted_at(
            self.ctx.transaction,
            self.now,
            &guard,
            &live,
            self.events.values,
            &mut self.events.mutation,
            &mut self.events.original_work,
            self.events.world_events,
        )
        .map_err(RootModuleExecutionErrorV1::Body)?;
        let prepared=crate::world::incremental_step::factory_schedule_write_io::paid_with_tail(&mut step,read,parent,self.members,materials,work,common,&live,
   |step,read,parent,write,materials,work,tail|{
    let now=step.time();
    use oasis7_world_state::{incremental_factory_records::*,incremental_field_overlay::WorldFieldOverlayContextV1};
    let mut ctx=WorldFieldOverlayContextV1{transaction:&mut *step.transaction,values:oasis7_formal_protocol::incremental_value::ValueReadContext{source:read.values.source,memory:read.values.memory,class:read.values.class,io:&mut *read.values.io},key_work:&mut *read.key_work,semantic_bytes:read.semantic_bytes,record_bytes:read.record_bytes,take_state_error:&mut read.take_state_error,cancelled:&mut read.cancelled};
    let guard=ctx.transaction.guard();
    let before=oasis7_world_state::incremental_factory_schedule_factory::recover_current_schedule_complete_factory_v1(factory_id,sources.lifecycle.factories,&mut ctx,&mut work.factory,&guard,&live).map_err(overlay)?;
    let after=if let Some(before)=before{
     let layout=crate::world::incremental_normal_industry_due::factory_copy_layout(&before,work)?.checked_add(blocker_kind.len()).and_then(|n|n.checked_add(blocker_detail.len())).ok_or_else(bad)?;
     tail.charge(layout)?;let fee=ctx.values.memory.reserve(layout,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
     let mut after=(*before).clone();
     oasis7_world_state::state::industry_transition::block_recipe_factory_production_v1(&mut after,None,blocker_kind,blocker_detail,now);
     let after=fee.retain(after);let guard=ctx.transaction.guard();
     replace_factory_record_overlay_v1(FactoryRecordChangeV1{key:FactoryRecordKeyV1::Factory(factory_id),expected:Some(FactoryRecordRefV1::Factory(&before)),after:Some(FactoryRecordRefV1::Factory(&after)),max_current_payload_bytes:work.lifecycle.max_scalar_bytes as u64},&mut ctx,write,&mut work.lifecycle.factory,&guard,&live).map_err(overlay)?;
     Some(after)
    }else{None};drop(ctx);
    crate::world::incremental_normal_industry_due::refresh_current_blocked_factory_stage_v1(factory_id,after.as_deref(),sources,step,read,write,work,&live)?;
    let exists={
     let mut ctx=WorldFieldOverlayContextV1{transaction:&mut *step.transaction,values:oasis7_formal_protocol::incremental_value::ValueReadContext{source:read.values.source,memory:read.values.memory,class:read.values.class,io:&mut *read.values.io},key_work:&mut *read.key_work,semantic_bytes:read.semantic_bytes,record_bytes:read.record_bytes,take_state_error:&mut read.take_state_error,cancelled:&mut read.cancelled};
     let guard=ctx.transaction.guard();
     oasis7_world_state::incremental_agent_subtree::read_current_agent_presence_v1(requester_agent_id,sources.lifecycle.agents,sources.roots,&mut ctx,parent,&mut work.lifecycle.pending_entries_remaining,&guard,&live).map_err(overlay)?==oasis7_world_state::incremental_agent_subtree::CurrentAgentPresenceV1::Existing
    };
    if exists{
     crate::world::incremental_normal_action_factory_schedule::set_blocked_recipe_activity_v1(requester_agent_id,selected.envelope().id,factory_id,blocker_kind,blocker_detail,sources.lifecycle.agents,step,read,work,&live)?;
     crate::world::incremental_normal_industry_due::touch_current_product_requester_v1(requester_agent_id,false,sources,step,read,parent,work,&live)?;
    }
    let guard=step.transaction.guard();materials.contribute(step.transaction,step.values,step.mutation,&guard,&live)?;
    let allocator=step.read_allocator(crate::world::incremental_allocators::RuntimeAllocatorV1::Event)?;
    let(id,_,_)=crate::World::preview_next_event_id(allocator.next,allocator.era);
    let fee=step.mutation.memory.reserve(std::mem::size_of::<crate::WorldEvent>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
    Ok(PreparedFactoryModuleFollowupV1{event:fee.retain(crate::WorldEvent{id,time:step.time(),caused_by:Some(crate::CausedBy::Action(selected.envelope().id)),body:crate::WorldEventBody::Domain(body.clone())}),guard:step.transaction.guard(),core:tail.identity(),now:step.time(),action_id:selected.envelope().id,_payload:payload})
   }).map_err(RootModuleExecutionErrorV1::Body)?;
        Ok(Some(prepared))
    }
    pub(in crate::world) fn publish_factory_module_followup_v1<
        ME: FnMut() -> Option<ValueTreeError>,
    >(
        &mut self,
        prepared: &PreparedFactoryModuleFollowupV1,
        materials:&mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<'_,M,ME>,
        common: &mut FormalBoundedContextV1<'_>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        tail_work: &mut crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<usize, RootModuleExecutionErrorV1<O::Error>> {
        let persisted=crate::world::incremental_module_execution_event::contribute_factory_module_followup_event_v1(self.now,prepared,common,&live_owner(),self.ctx.transaction,self.events,materials).map_err(RootModuleExecutionErrorV1::Body)?;
        with_current_root_route_meter_v1(
            self,
            common,
            live_owner,
            enumeration,
            tail_work,
            |root, live, enumeration, _tail| {
                root.route_persisted_event_v1(&persisted, None, directory, enumeration, live)
            },
        )
    }
}
