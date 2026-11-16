//! Fixed original gameplay stage: full Tick preselection/VM route, then either
//! original fallback substages or the captured gameplay module emit cut. Domain
//! events are persisted as they apply, but their module routing is deferred
//! until the entire stage has completed, matching the original step owner.
use super::*;
use super::{commit, decode, failure, io, plan, select, war};
use crate::world::{
    incremental_body_event::materials::CorePolicyMaterialContextV1,
    incremental_module_invocation::*, incremental_normal_industry_due::deferred::DueEventQueueV1,
    incremental_normal_tail::NormalTailWorkV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_material::OriginalObjectStreamReader,
    incremental_schedule::SchedulePageWriter,
    incremental_state::{StatePageWriter, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_history_source::AcceptedRuntimeHistorySourceV1,
};
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("original Current gameplay cycle stage")
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter + SchedulePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    /// The original invalid-output branch appends its failure before returning
    /// the original WorldError. It never enters the successful domain route queue.
    fn persist_gameplay_directive_failure_v1<ME>(
        &mut self,
        source: &PreparedGameplayDirectiveFailureV1,
        materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
        common: &mut FormalBoundedContextV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<(), RootModuleExecutionErrorV1<O::Error>>
    where
        ME: FnMut() -> Option<ValueTreeError>,
    {
        let live = live_owner();
        let persisted=crate::world::incremental_module_execution_event::contribute_normal_gameplay_directive_failure_v1(self.now,source,common,&live,self.ctx.transaction,self.events,materials).map_err(RootModuleExecutionErrorV1::Body)?;
        drop(persisted);
        self.ctx
            .transaction
            .validate_guard(&self.ctx.transaction.guard(), &live_owner())?;
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn commit_gameplay_domain_v1<P, PE, PC, ME>(
        &mut self,
        owned: plan::OwnedGameplayDomainV1,
        sources: &NormalGameplayCycleSourcesV1<'_>,
        work: &mut NormalGameplayCycleWorkV1,
        parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
        queue: &mut DueEventQueueV1,
        common: &mut FormalBoundedContextV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<(), RootModuleExecutionErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
        ME: FnMut() -> Option<ValueTreeError>,
    {
        let now = self.now;
        let state = self.state;
        let live = live_owner();
        io::run(
            self.ctx,
            parent,
            self.members,
            materials,
            work,
            common,
            |ctx, parent, write, _materials, work, tail| match &owned.body {
                crate::DomainEvent::EconomicContractExpired { .. } => commit::expire_contract(
                    &owned.body,
                    sources,
                    state,
                    ctx,
                    parent,
                    write,
                    work,
                    tail,
                    now,
                    &live,
                ),
                crate::DomainEvent::GovernanceProposalFinalized { .. } => {
                    commit::finalize_governance(
                        &owned.body,
                        sources,
                        state,
                        ctx,
                        parent,
                        write,
                        work,
                        tail,
                        now,
                        &live,
                    )
                }
                crate::DomainEvent::CrisisSpawned { .. } => commit::spawn_crisis(
                    &owned.body,
                    sources,
                    state,
                    ctx,
                    parent,
                    write,
                    work,
                    tail,
                    now,
                    &live,
                ),
                crate::DomainEvent::CrisisTimedOut { .. } => commit::timeout_crisis(
                    &owned.body,
                    sources,
                    state,
                    ctx,
                    parent,
                    write,
                    work,
                    tail,
                    now,
                    &live,
                ),
                crate::DomainEvent::WarConcluded { .. } => commit::conclude_war(
                    &owned.body,
                    sources,
                    state,
                    ctx,
                    parent,
                    write,
                    work,
                    tail,
                    now,
                    &live,
                ),
                crate::DomainEvent::MetaProgressGranted { .. } => commit::grant_meta(
                    &owned.body,
                    sources,
                    state,
                    ctx,
                    parent,
                    write,
                    work,
                    tail,
                    now,
                    &live,
                ),
                _ => Err(bad().into()),
            },
        )
        .map_err(GameplayCycleErrorV1::into_root)?;
        let id=crate::world::incremental_normal_tail::event_preview::preview_current_normal_event_id_v1(self.ctx,common).map_err(RootModuleExecutionErrorV1::Body)?;
        let guard = self.ctx.transaction.guard();
        self.ctx.transaction.validate_guard(&guard, &live_owner())?;
        let prepared = PreparedGameplayCycleEventV1 {
            event: crate::WorldEvent {
                id,
                time: now,
                caused_by: None,
                body: crate::WorldEventBody::Domain(owned.body),
            },
            guard,
            identity: common.work().identity(),
            memory: common.memory().clone(),
            now,
            _event_fee: owned.fee,
            _directive_fee: owned.backing,
        };
        let event = self.persist_due_gameplay_cycle_v1(prepared, materials, common, live_owner)?;
        queue
            .push(event, self.ctx.transaction, common, now, &live_owner())
            .map_err(RootModuleExecutionErrorV1::Body)?;
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn execute_normal_gameplay_cycles_v1<P, CE, CC, PE, PC, ME>(
        &mut self,
        history: &ReservedValue<AcceptedRuntimeHistorySourceV1>,
        sources: &NormalGameplayCycleSourcesV1<'_>,
        work: &mut NormalGameplayCycleWorkV1,
        materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
        current: &mut WorldFieldReadContextV1<'_, P, R, CE, CC>,
        parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        normal: &mut NormalTailWorkV1<'_, '_>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<(), RootModuleExecutionErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        CE: FnMut() -> Option<ValueTreeError>,
        CC: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
        ME: FnMut() -> Option<ValueTreeError>,
    {
        macro_rules! common {
            () => {
                normal
                    .common_mut()
                    .map_err(RootModuleExecutionErrorV1::Body)?
            };
        }
        let result = (|| {
            let now = self.now;
            let state = self.state;
            // Includes ALL active invocations before any/role decision and ALL due
            // manifests before the first VM. Nothing has appended a journal event yet.
            let route = self.prepare_gameplay_tick_route_v1(
                state,
                work,
                common!(),
                directory,
                enumeration,
                live_owner,
            )?;
            let has_gameplay_tick = route.has_gameplay_tick();
            let start_id =
                self.gameplay_journal_start_id_v1(history, sources, current, work, common!())?;
            self.execute_gameplay_tick_route_v1(route, common!(), live_owner)?;
            let mut queue =
                DueEventQueueV1::new(self.ctx.transaction, common!(), now, &live_owner())
                    .map_err(RootModuleExecutionErrorV1::Body)?;
            if has_gameplay_tick {
                let emits = self.collect_gameplay_emits_v1(
                    history,
                    sources,
                    current,
                    work,
                    start_id,
                    common!(),
                    live_owner,
                )?;
                for original in emits {
                    let crate::WorldEventBody::ModuleEmitted(emit) = &original.event().body else {
                        return Err(bad().into());
                    };
                    let decoded = io::run(
                        self.ctx,
                        parent,
                        self.members,
                        materials,
                        work,
                        common!(),
                        |_, _, _, _, work, tail| {
                            Ok(decode::decode_gameplay_directives_v1(
                                &emit.payload,
                                work.max_directive_depth,
                                tail,
                            ))
                        },
                    )
                    .map_err(GameplayCycleErrorV1::into_root)?;
                    let directives = match decoded {
                        Ok(directives) => directives,
                        Err(decode::GameplayDirectiveDecodeErrorV1::Body(e)) => {
                            return Err(RootModuleExecutionErrorV1::Body(e));
                        }
                        Err(decode::GameplayDirectiveDecodeErrorV1::Json { error, heap }) => {
                            let data = io::run(
                                self.ctx,
                                parent,
                                self.members,
                                materials,
                                work,
                                common!(),
                                |_, _, _, _, _, tail| {
                                    failure::prepare_directive_failure_v1(
                                        original, error, heap, tail,
                                    )
                                },
                            )
                            .map_err(GameplayCycleErrorV1::into_root)?;
                            let id=crate::world::incremental_normal_tail::event_preview::preview_current_normal_event_id_v1(self.ctx,common!()).map_err(RootModuleExecutionErrorV1::Body)?;
                            let identity = common!().work().identity();
                            let memory = common!().memory().clone();
                            let guard = self.ctx.transaction.guard();
                            self.ctx.transaction.validate_guard(&guard, &live_owner())?;
                            let (cap, business) = data.finish(id, now, guard, identity, memory);
                            self.persist_gameplay_directive_failure_v1(
                                &cap,
                                materials,
                                common!(),
                                live_owner,
                            )?;
                            return Err(RootModuleExecutionErrorV1::Business(business));
                        }
                    };
                    for directive in directives {
                        let live = live_owner();
                        let owned = io::run(
                            self.ctx,
                            parent,
                            self.members,
                            materials,
                            work,
                            common!(),
                            |ctx, parent, _, _, work, tail| {
                                plan::prepare_directive_v1(
                                    directive, sources, state, ctx, parent, work, tail, now, &live,
                                )
                            },
                        )
                        .map_err(GameplayCycleErrorV1::into_root)?;
                        if let Some(owned) = owned {
                            self.commit_gameplay_domain_v1(
                                owned,
                                sources,
                                work,
                                parent,
                                materials,
                                &mut queue,
                                common!(),
                                live_owner,
                            )?;
                        }
                    }
                }
            } else {
                for stage in [
                    select::FallbackStageV1::Contract,
                    select::FallbackStageV1::Governance,
                ] {
                    let live = live_owner();
                    let events = io::run(
                        self.ctx,
                        parent,
                        self.members,
                        materials,
                        work,
                        common!(),
                        |ctx, parent, _, _, work, tail| {
                            plan::build_stage_plan_v1(
                                stage, sources, state, ctx, parent, work, tail, now, &live,
                            )
                        },
                    )
                    .map_err(GameplayCycleErrorV1::into_root)?;
                    for event in events {
                        self.commit_gameplay_domain_v1(
                            event,
                            sources,
                            work,
                            parent,
                            materials,
                            &mut queue,
                            common!(),
                            live_owner,
                        )?;
                    }
                }
                let live = live_owner();
                let spawn =
                    io::run(
                        self.ctx,
                        parent,
                        self.members,
                        materials,
                        work,
                        common!(),
                        |ctx, parent, _, _, work, tail| {
                            let active = select::has_active_crisis_v1(
                                sources, ctx, parent, work, tail, now, &live,
                            )?;
                            // At most 20 decimal time digits plus the fixed original literals.
                            let fee = tail.reserve(
                                std::mem::size_of::<crate::WorldEvent>() + 64,
                                MemoryClass::Ordinary,
                            )?;
                            tail.charge(64)?;
                            Ok(kernel::prepare_crisis_auto_spawn_event_v1(active, now).map(
                                |body| plan::OwnedGameplayDomainV1 {
                                    body,
                                    fee,
                                    backing: None,
                                },
                            ))
                        },
                    )
                    .map_err(GameplayCycleErrorV1::into_root)?;
                if let Some(spawn) = spawn {
                    self.commit_gameplay_domain_v1(
                        spawn,
                        sources,
                        work,
                        parent,
                        materials,
                        &mut queue,
                        common!(),
                        live_owner,
                    )?;
                }
                let live = live_owner();
                let events = io::run(
                    self.ctx,
                    parent,
                    self.members,
                    materials,
                    work,
                    common!(),
                    |ctx, parent, _, _, work, tail| {
                        plan::build_stage_plan_v1(
                            select::FallbackStageV1::CrisisTimeout,
                            sources,
                            state,
                            ctx,
                            parent,
                            work,
                            tail,
                            now,
                            &live,
                        )
                    },
                )
                .map_err(GameplayCycleErrorV1::into_root)?;
                for event in events {
                    self.commit_gameplay_domain_v1(
                        event,
                        sources,
                        work,
                        parent,
                        materials,
                        &mut queue,
                        common!(),
                        live_owner,
                    )?;
                }
                loop {
                    let live = live_owner();
                    let event = io::run(
                        self.ctx,
                        parent,
                        self.members,
                        materials,
                        work,
                        common!(),
                        |ctx, parent, _, _, work, tail| {
                            war::prepare_next_war_v1(
                                sources, state, ctx, parent, work, tail, now, &live,
                            )
                        },
                    )
                    .map_err(GameplayCycleErrorV1::into_root)?;
                    let Some(event) = event else { break };
                    self.commit_gameplay_domain_v1(
                        event,
                        sources,
                        work,
                        parent,
                        materials,
                        &mut queue,
                        common!(),
                        live_owner,
                    )?;
                }
            }
            self.flush_due_events_v1(&mut queue, common!(), directory, enumeration, live_owner)?;
            Ok(())
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }
}
