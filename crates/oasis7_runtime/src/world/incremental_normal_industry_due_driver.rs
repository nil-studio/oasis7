//! Sole complete fixed industrial due phase. Builds finish and route before
//! recipe ordering; every recipe rebinds actual Current before terminal checks.
//! Successful return is not an ExecutedTick or publication qualification.
use super::*;
use crate::world::{
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_factory_directory_orders::{
        AcceptedFactoryOrderDirectoryV1, FactoryOrderDirectoryWorkV1,
    },
    incremental_factory_directory_records::{
        AcceptedFactoryRecipeDirectoryV1, FactoryRecipeDirectoryWorkV1,
    },
    incremental_industrial_eligibility::IndustrialEligibilityWorkV1,
    incremental_module_invocation::{
        AcceptedInvocationDirectoryV1, CurrentInvocationEnumerationWorkV1,
        RootModuleExecutionContextV1, RootModuleExecutionErrorV1,
    },
    incremental_normal_action_factory_schedule::{FactoryScheduleSourcesV1, FactoryScheduleWorkV1},
    incremental_normal_tail::NormalTailWorkV1,
    incremental_step::IwsStepContext,
    incremental_world_event_counter::WorldJournalRetentionV1,
};
use delivery::{
    CurrentProductValidationDeliveryIndexV1, DueProductReplayOutcomeV1,
    ProductValidationDeliveryWorkV1,
};
use oasis7_formal_protocol::{
    incremental_material::OriginalObjectStreamReader,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_rules_family::WorldRulesQualificationV1,
};
use std::mem::size_of;
/// Only borrowed genuine holders; no setter, caller root, permission flag or
/// independent work meter can construct a completed result through this view.
pub(in crate::world) struct NormalIndustryDueSourcesV1<'a> {
    pub(in crate::world) factory: &'a FactoryScheduleSourcesV1<'a>,
    pub(in crate::world) rules: &'a ReservedValue<WorldRulesQualificationV1>,
    pub(in crate::world) records: &'a ReservedValue<AcceptedFactoryRecipeDirectoryV1>,
    pub(in crate::world) orders: &'a ReservedValue<AcceptedFactoryOrderDirectoryV1>,
    pub(in crate::world) retention: &'a ReservedValue<WorldJournalRetentionV1>,
}
pub(in crate::world) struct NormalIndustryDueWorkV1<'a> {
    pub(in crate::world) factory: &'a mut FactoryScheduleWorkV1,
    pub(in crate::world) terminal: &'a mut IndustrialEligibilityWorkV1,
    pub(in crate::world) records: &'a mut FactoryRecipeDirectoryWorkV1,
    pub(in crate::world) orders: &'a mut FactoryOrderDirectoryWorkV1,
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn execute_normal_industry_due_v1<P, CE, CC, PE, PC, ME>(
        &mut self,
        sources: &NormalIndustryDueSourcesV1<'_>,
        work: &mut NormalIndustryDueWorkV1<'_>,
        delivery: &mut CurrentProductValidationDeliveryIndexV1<'_>,
        delivery_work: &mut ProductValidationDeliveryWorkV1,
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
        macro_rules! body {
            ($e:expr) => {
                $e.map_err(RootModuleExecutionErrorV1::Body)?
            };
        }
        macro_rules! step {
            () => {{
                let guard = self.ctx.transaction.guard();
                let live = live_owner();
                IwsStepContext::borrow_counted_at(
                    self.ctx.transaction,
                    self.now,
                    &guard,
                    &live,
                    self.events.values,
                    &mut self.events.mutation,
                    &mut self.events.original_work,
                    self.events.world_events,
                )
                .map_err(RootModuleExecutionErrorV1::Body)?
            }};
        }
        let result = (|| {
            body!(
                body!(normal.common_mut())
                    .charge(
                        1,
                        u64::try_from(
                            size_of::<NormalIndustryDueSourcesV1<'_>>()
                                + size_of::<NormalIndustryDueWorkV1<'_>>()
                        )
                        .map_err(|_| RootModuleExecutionErrorV1::Body(
                            BodyMutationError::Invalid("industrial source header")
                        ))?
                    )
                    .map_err(|e| BodyMutationError::Original(Box::new(e)))
            );
            body!(
                self.events
                    .world_events
                    .bind_journal_retention_v1(self.ctx.transaction, sources.retention)
            );
            let mut deferred = body!(super::deferred::DueEventQueueV1::new(
                self.ctx.transaction,
                body!(normal.common_mut()),
                self.now,
                &live_owner()
            ));
            let _ = self.execute_due_builds_v1(
                &mut deferred,
                materials,
                current,
                parent,
                sources.factory,
                work.factory,
                normal,
                directory,
                enumeration,
                live_owner,
            )?;
            let initial = {
                let live = live_owner();
                let mut step = step!();
                body!(directories::open_current_due_records_v1(
                    sources.records,
                    work.records,
                    &mut step,
                    current,
                    parent,
                    self.members,
                    materials,
                    sources.factory,
                    work.factory,
                    body!(normal.common_mut()),
                    &live
                ))
            };
            let queue = {
                let live = live_owner();
                let mut step = step!();
                body!(recipe_select::select_due_recipes_v1(
                    &initial,
                    work.records,
                    &mut step,
                    current,
                    parent,
                    self.members,
                    materials,
                    sources.factory,
                    work.factory,
                    body!(normal.common_mut()),
                    &live
                ))
            };
            drop(initial);
            for selected in queue {
                let pending = selected.pending;
                let records = {
                    let live = live_owner();
                    let mut step = step!();
                    body!(directories::open_current_due_records_v1(
                        sources.records,
                        work.records,
                        &mut step,
                        current,
                        parent,
                        self.members,
                        materials,
                        sources.factory,
                        work.factory,
                        body!(normal.common_mut()),
                        &live
                    ))
                };
                let decision = {
                    let live = live_owner();
                    let mut ctx = WorldFieldOverlayContextV1 {
                        transaction: &mut *self.ctx.transaction,
                        values: ValueReadContext {
                            source: current.values.source,
                            memory: current.values.memory,
                            class: current.values.class,
                            io: &mut *current.values.io,
                        },
                        key_work: &mut *current.key_work,
                        semantic_bytes: current.semantic_bytes,
                        record_bytes: current.record_bytes,
                        take_state_error: &mut current.take_state_error,
                        cancelled: &mut current.cancelled,
                    };
                    body!(owner_sources::with_current_due_owner_sources_v1(
                        sources.orders,
                        sources.rules,
                        sources.factory,
                        &mut ctx,
                        parent,
                        work.terminal,
                        work.orders,
                        body!(normal.common_mut()),
                        &live,
                        |owners, ctx, parent, terminal_work, common| {
                            terminal::check_current_terminal_v1(
                                &pending,
                                sources.rules,
                                &owners,
                                &records,
                                sources.factory,
                                ctx,
                                parent,
                                work.factory,
                                terminal_work,
                                common,
                                self.now,
                                &live,
                            )
                        }
                    ))
                };
                let phase = match decision {
                    terminal::IndustrialTerminalDecisionV1::Ready(phase) => phase,
                    terminal::IndustrialTerminalDecisionV1::Blocked(error) => {
                        let common = body!(normal.common_mut());
                        body!(
                            common
                                .charge(1, size_of::<PreparedDueRecipeFailureV1<'_>>() as u64)
                                .map_err(|e| BodyMutationError::Original(Box::new(e)))
                        );
                        let fee = common
                            .memory()
                            .reserve(
                                size_of::<PreparedDueRecipeFailureV1<'_>>(),
                                MemoryClass::Ordinary,
                            )
                            .map_err(ValueTreeError::Memory)?;
                        let failure = PreparedDueRecipeFailureV1 {
                            pending: &pending,
                            cause: DueRecipeFailureCauseV1::IndustrialTerminal { error },
                            guard: self.ctx.transaction.guard(),
                            identity: common.work().identity(),
                            memory: common.memory().clone(),
                            now: self.now,
                            _phase: fee,
                        };
                        let cap = {
                            let live = live_owner();
                            let mut step = step!();
                            body!(caps::failure_commit::commit_current_due_recipe_failure_v1(
                                &failure,
                                &mut step,
                                materials,
                                sources.factory,
                                current,
                                parent,
                                self.members,
                                work.factory,
                                body!(normal.common_mut()),
                                &live
                            ))
                        };
                        if let Some(cap) = cap {
                            let event = self.persist_due_recipe_failure_v1(
                                cap,
                                materials,
                                body!(normal.common_mut()),
                                live_owner,
                            )?;
                            body!(deferred.push(
                                event,
                                self.ctx.transaction,
                                body!(normal.common_mut()),
                                self.now,
                                &live_owner()
                            ));
                        }
                        continue;
                    }
                };
                let outputs = pending
                    .produce
                    .len()
                    .checked_add(pending.byproducts.len())
                    .ok_or_else(|| {
                        RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                            "industrial output count",
                        ))
                    })?;
                let mut rejected = false;
                for index in 0..outputs {
                    match self.execute_due_product_output_v1(
                        &pending,
                        index,
                        &mut deferred,
                        materials,
                        current,
                        parent,
                        sources.factory,
                        work.factory,
                        normal,
                        directory,
                        enumeration,
                        live_owner,
                    )? {
                        product_driver::DueProductOutputV1::NoModule => {}
                        product_driver::DueProductOutputV1::Replay(history) => {
                            match self.resume_due_product_receipt_v1(
                                &pending,
                                index,
                                &history,
                                delivery,
                                delivery_work,
                                materials,
                                current,
                                parent,
                                sources.factory,
                                work.factory,
                                normal,
                                directory,
                                enumeration,
                                live_owner,
                            )? {
                                DueProductReplayOutcomeV1::Accepted => {}
                                DueProductReplayOutcomeV1::Rejected => {
                                    let common = body!(normal.common_mut());
                                    body!(
                                        common
                                            .charge(
                                                1,
                                                size_of::<PreparedDueRecipeFailureV1<'_>>() as u64
                                            )
                                            .map_err(|e| BodyMutationError::Original(Box::new(e)))
                                    );
                                    let fee = common
                                        .memory()
                                        .reserve(
                                            size_of::<PreparedDueRecipeFailureV1<'_>>(),
                                            MemoryClass::Ordinary,
                                        )
                                        .map_err(ValueTreeError::Memory)?;
                                    let source = PreparedDueRecipeFailureV1 {
                                        pending: &pending,
                                        cause:
                                            DueRecipeFailureCauseV1::PersistedProductValidation {
                                                history: &history,
                                            },
                                        guard: self.ctx.transaction.guard(),
                                        identity: common.work().identity(),
                                        memory: common.memory().clone(),
                                        now: self.now,
                                        _phase: fee,
                                    };
                                    let cap = {
                                        let live = live_owner();
                                        let mut step = step!();
                                        body!(caps::failure_commit::commit_current_due_recipe_failure_v1(&source,&mut step,materials,sources.factory,current,parent,self.members,work.factory,body!(normal.common_mut()),&live))
                                    };
                                    if let Some(cap) = cap {
                                        let event = self.persist_due_recipe_failure_v1(
                                            cap,
                                            materials,
                                            body!(normal.common_mut()),
                                            live_owner,
                                        )?;
                                        body!(deferred.push(
                                            event,
                                            self.ctx.transaction,
                                            body!(normal.common_mut()),
                                            self.now,
                                            &live_owner()
                                        ));
                                    }
                                    rejected = true;
                                    break;
                                }
                            }
                        }
                        product_driver::DueProductOutputV1::Written { receipt, result } => {
                            if matches!(
                                &result.event().body,
                                crate::WorldEventBody::Domain(
                                    crate::DomainEvent::ActionRejected { .. }
                                )
                            ) {
                                let common = body!(normal.common_mut());
                                body!(
                                    common
                                        .charge(
                                            1,
                                            size_of::<PreparedDueRecipeFailureV1<'_>>() as u64
                                        )
                                        .map_err(|e| BodyMutationError::Original(Box::new(e)))
                                );
                                let fee = common
                                    .memory()
                                    .reserve(
                                        size_of::<PreparedDueRecipeFailureV1<'_>>(),
                                        MemoryClass::Ordinary,
                                    )
                                    .map_err(ValueTreeError::Memory)?;
                                let source = PreparedDueRecipeFailureV1 {
                                    pending: &pending,
                                    cause: DueRecipeFailureCauseV1::ProductValidation {
                                        receipt: &receipt,
                                        result: &result,
                                    },
                                    guard: self.ctx.transaction.guard(),
                                    identity: common.work().identity(),
                                    memory: common.memory().clone(),
                                    now: self.now,
                                    _phase: fee,
                                };
                                let cap = {
                                    let live = live_owner();
                                    let mut step = step!();
                                    body!(
                                        caps::failure_commit::commit_current_due_recipe_failure_v1(
                                            &source,
                                            &mut step,
                                            materials,
                                            sources.factory,
                                            current,
                                            parent,
                                            self.members,
                                            work.factory,
                                            body!(normal.common_mut()),
                                            &live
                                        )
                                    )
                                };
                                if let Some(cap) = cap {
                                    let event = self.persist_due_recipe_failure_v1(
                                        cap,
                                        materials,
                                        body!(normal.common_mut()),
                                        live_owner,
                                    )?;
                                    body!(deferred.push(
                                        event,
                                        self.ctx.transaction,
                                        body!(normal.common_mut()),
                                        self.now,
                                        &live_owner()
                                    ));
                                }
                                rejected = true;
                                break;
                            }
                        }
                    }
                }
                if rejected {
                    continue;
                }
                // Consume the original paid selected job only after actual terminal and
                // every required product receipt/replay succeeded. The one-slot holder's
                // allocation was admitted by the terminal phase before its construction.
                let common = body!(normal.common_mut());
                let mut fees = Vec::with_capacity(1);
                if fees.capacity() != 1 {
                    return Err(RootModuleExecutionErrorV1::Body(
                        BodyMutationError::Invalid("industrial terminal fee slot"),
                    ));
                }
                fees.push(phase);
                let terminal = ValidatedDueRecipeTerminalV1 {
                    pending,
                    guard: self.ctx.transaction.guard(),
                    identity: common.work().identity(),
                    memory: common.memory().clone(),
                    now: self.now,
                    _terminal_fees: fees,
                };
                let cap = {
                    let live = live_owner();
                    let mut step = step!();
                    body!(recipe_commit::complete_current_due_recipe_v1(
                        terminal,
                        &mut step,
                        materials,
                        sources.factory,
                        current,
                        parent,
                        self.members,
                        work.factory,
                        body!(normal.common_mut()),
                        &live
                    ))
                };
                let event = self.persist_due_recipe_completed_v1(
                    cap,
                    materials,
                    body!(normal.common_mut()),
                    live_owner,
                )?;
                body!(deferred.push(
                    event,
                    self.ctx.transaction,
                    body!(normal.common_mut()),
                    self.now,
                    &live_owner()
                ));
            }
            self.flush_due_events_v1(
                &mut deferred,
                body!(normal.common_mut()),
                directory,
                enumeration,
                live_owner,
            )?;
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
