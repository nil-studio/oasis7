//! Fixed per-output actual attempt/worker/record/result sequence. Existing
//! receipts are handed to the exact counted-event replay lane; they are never
//! treated as delivered and never cause another validator invocation here.
use super::*;
use crate::world::{
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_module_invocation::{
        AcceptedInvocationDirectoryV1, CurrentInvocationEnumerationWorkV1,
        RootModuleExecutionContextV1, RootModuleExecutionErrorV1,
    },
    incremental_normal_action_factory_schedule::{
        FactoryScheduleSourcesV1, FactoryScheduleWorkV1, read_due_product_module_v1,
    },
    incremental_normal_tail::NormalTailWorkV1,
    incremental_step::IwsStepContext,
};
use caps::validation_state::{self, CurrentProductHistoryV1, RoutedProductSourceV1};
use oasis7_formal_protocol::{
    incremental_material::OriginalObjectStreamReader,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::incremental_field_read::WorldFieldReadContextV1;
pub(super) enum DueProductOutputV1 {
    NoModule,
    Replay(ReservedValue<CurrentProductHistoryV1>),
    Written {
        receipt: ReservedValue<crate::ProductValidationReceiptV1>,
        result: PreparedNormalProductResultV1,
    },
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
    pub(super) fn execute_due_product_output_v1<P, CE, CC, PE, PC, ME>(
        &mut self,
        pending: &ReservedValue<crate::RecipeJobState>,
        output_index: usize,
        deferred: &mut super::deferred::DueEventQueueV1,
        materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
        current: &mut WorldFieldReadContextV1<'_, P, R, CE, CC>,
        parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        sources: &FactoryScheduleSourcesV1<'_>,
        work: &mut FactoryScheduleWorkV1,
        normal: &mut NormalTailWorkV1<'_, '_>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<DueProductOutputV1, RootModuleExecutionErrorV1<O::Error>>
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
            let stack = if output_index < pending.produce.len() {
                pending.produce.get(output_index)
            } else {
                pending
                    .byproducts
                    .get(
                        output_index
                            .checked_sub(pending.produce.len())
                            .ok_or_else(|| {
                                RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                                    "due output index",
                                ))
                            })?,
                    )
            }
            .ok_or_else(|| {
                RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid("due output absent"))
            })?;
            let history = {
                let live = live_owner();
                let mut step = step!();
                body!(validation_state::read_product_history_v1(
                    pending,
                    output_index,
                    sources,
                    &mut step,
                    current,
                    parent,
                    self.members,
                    materials,
                    work,
                    body!(normal.common_mut()),
                    &live
                ))
            };
            if history.receipt().is_some() {
                return Ok(DueProductOutputV1::Replay(history));
            }
            let module = if let Some(attempt) = history.attempt() {
                Some(body!(product_values::copy_text_v1(
                    &attempt.module_id,
                    body!(normal.common_mut())
                )))
            } else {
                let live = live_owner();
                let mut ctx =
                    oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1 {
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
                body!(read_due_product_module_v1(
                    &stack.kind,
                    sources,
                    &mut ctx,
                    parent,
                    work,
                    body!(normal.common_mut()),
                    self.now,
                    &live
                ))
            };
            let Some(module) = module else {
                return Ok(DueProductOutputV1::NoModule);
            };
            let decision = if history.attempt().is_some() {
                body!(product_values::failed_decision_v1(
                    &module,
                    stack,
                    "attempt had no committed decision; module call was not retried",
                    body!(normal.common_mut())
                ))
            } else {
                let attempt = body!(product_values::attempt_v1(
                    pending,
                    &module,
                    stack,
                    output_index,
                    body!(normal.common_mut())
                ));
                let prepared = {
                    let live = live_owner();
                    let mut step = step!();
                    body!(validation_state::commit_product_attempt_v1(
                        &attempt,
                        sources,
                        &mut step,
                        current,
                        parent,
                        self.members,
                        materials,
                        work,
                        body!(normal.common_mut()),
                        &live
                    ))
                };
                let persisted = self.persist_due_product_attempt_v1(
                    prepared,
                    materials,
                    body!(normal.common_mut()),
                    live_owner,
                )?;
                // Original record_attempt appends the Attempt first, then
                // drains earlier economy events, then routes this Attempt.
                self.flush_due_events_v1(
                    deferred,
                    body!(normal.common_mut()),
                    directory,
                    enumeration,
                    live_owner,
                )?;
                let _ = self.route_deferred_due_event_v1(
                    &persisted,
                    body!(normal.common_mut()),
                    directory,
                    enumeration,
                    live_owner,
                )?;
                let prepared = persisted.product_attempt().ok_or_else(|| {
                    RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                        "actual deferred Attempt source",
                    ))
                })?;
                let cursor = {
                    let live = live_owner();
                    let mut step = step!();
                    body!(validation_state::commit_product_cursor_v1(
                        RoutedProductSourceV1::Attempt(prepared),
                        &mut step,
                        current,
                        parent,
                        self.members,
                        materials,
                        work,
                        body!(normal.common_mut()),
                        &live
                    ))
                };
                self.publish_normal_product_cursor_v1(
                    &cursor,
                    materials,
                    body!(normal.common_mut()),
                    directory,
                    enumeration,
                    live_owner,
                )?;
                let call = body!(product_call::prepare_product_call_v1(
                    pending,
                    &module,
                    output_index,
                    self.ctx.transaction,
                    body!(normal.common_mut()),
                    self.now,
                    &live_owner()
                ));
                let execution = self.execute_due_product_request_v1(
                    &call,
                    body!(normal.common_mut()),
                    live_owner,
                )?;
                if let Some(vm) = body!(execution.vm_failure_source_v1(
                    &call,
                    self.ctx.transaction,
                    body!(normal.common_mut()),
                    self.now,
                    &live_owner()
                )) {
                    let cap = {
                        let live = live_owner();
                        let mut step = step!();
                        body!(caps::module_failure_state::commit_product_vm_failure_v1(
                            &vm,
                            &mut step,
                            current,
                            parent,
                            self.members,
                            materials,
                            work,
                            body!(normal.common_mut()),
                            &live
                        ))
                    };
                    self.publish_normal_product_module_failure_v1(
                        &cap,
                        materials,
                        body!(normal.common_mut()),
                        directory,
                        enumeration,
                        live_owner,
                    )?;
                }
                if let Some(error) = execution.business_failure() {
                    let reason = body!(product_values::debug_text_v1(
                        error,
                        body!(normal.common_mut())
                    ));
                    body!(product_values::failed_decision_v1(
                        &module,
                        stack,
                        &reason,
                        body!(normal.common_mut())
                    ))
                } else {
                    let output = execution.output().ok_or_else(|| {
                        RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                            "due actual worker outcome",
                        ))
                    })?;
                    match body!(invalid_output::decode_product_output_v1(
                        &call,
                        output,
                        self.ctx.transaction,
                        body!(normal.common_mut()),
                        self.now,
                        &live_owner()
                    )) {
                        invalid_output::DecodedDueProductOutputV1::Decision(value) => {
                            let decision = value.as_ref().map_err(|_| {
                                RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                                    "due actual decoded decision",
                                ))
                            })?;
                            body!(product_values::copied_decision_v1(
                                decision,
                                body!(normal.common_mut())
                            ))
                        }
                        invalid_output::DecodedDueProductOutputV1::Invalid(source) => {
                            let cap = {
                                let live = live_owner();
                                let mut step = step!();
                                body!(
                                    caps::module_failure_state::commit_product_module_failure_v1(
                                        &source,
                                        &mut step,
                                        current,
                                        parent,
                                        self.members,
                                        materials,
                                        work,
                                        body!(normal.common_mut()),
                                        &live
                                    )
                                )
                            };
                            self.publish_normal_product_module_failure_v1(
                                &cap,
                                materials,
                                body!(normal.common_mut()),
                                directory,
                                enumeration,
                                live_owner,
                            )?;
                            let failure = &*source.failure;
                            let bytes = failure
                                .module_id
                                .len()
                                .checked_add(failure.trace_id.len())
                                .and_then(|n| n.checked_add(failure.detail.len()))
                                .and_then(|n| {
                                    n.checked_add(std::mem::size_of::<crate::WorldError>())
                                })
                                .ok_or_else(|| {
                                    RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                                        "due failure reason layout",
                                    ))
                                })?;
                            body!(
                                body!(normal.common_mut())
                                    .charge(
                                        1,
                                        u64::try_from(bytes).map_err(|_| {
                                            RootModuleExecutionErrorV1::Body(
                                                BodyMutationError::Invalid(
                                                    "due failure reason layout",
                                                ),
                                            )
                                        })?
                                    )
                                    .map_err(|e| BodyMutationError::Original(Box::new(e)))
                            );
                            let fee = body!(normal.common_mut())
                                .memory()
                                .reserve(bytes, MemoryClass::Ordinary)
                                .map_err(ValueTreeError::Memory)?;
                            let error = fee.retain(crate::WorldError::ModuleCallFailed {
                                module_id: failure.module_id.clone(),
                                trace_id: failure.trace_id.clone(),
                                code: failure.code.clone(),
                                detail: failure.detail.clone(),
                            });
                            let reason = body!(product_values::debug_text_v1(
                                &*error,
                                body!(normal.common_mut())
                            ));
                            body!(product_values::failed_decision_v1(
                                &module,
                                stack,
                                &reason,
                                body!(normal.common_mut())
                            ))
                        }
                    }
                }
            };
            let receipt = body!(product_values::receipt_v1(
                pending,
                &module,
                stack,
                output_index,
                &decision,
                body!(normal.common_mut())
            ));
            let recorded = {
                let live = live_owner();
                let mut step = step!();
                body!(validation_state::commit_product_recorded_v1(
                    pending,
                    &receipt,
                    sources,
                    &mut step,
                    current,
                    parent,
                    self.members,
                    materials,
                    work,
                    body!(normal.common_mut()),
                    &live
                ))
            };
            self.publish_normal_product_recorded_v1(
                &recorded,
                materials,
                body!(normal.common_mut()),
                directory,
                enumeration,
                live_owner,
            )?;
            let cursor = {
                let live = live_owner();
                let mut step = step!();
                body!(validation_state::commit_product_cursor_v1(
                    RoutedProductSourceV1::Recorded(&recorded),
                    &mut step,
                    current,
                    parent,
                    self.members,
                    materials,
                    work,
                    body!(normal.common_mut()),
                    &live
                ))
            };
            self.publish_normal_product_cursor_v1(
                &cursor,
                materials,
                body!(normal.common_mut()),
                directory,
                enumeration,
                live_owner,
            )?;
            let result = {
                let live = live_owner();
                let mut step = step!();
                body!(validation_state::commit_product_result_v1(
                    pending,
                    &module,
                    stack,
                    &decision.decision,
                    sources,
                    &mut step,
                    current,
                    parent,
                    self.members,
                    materials,
                    work,
                    body!(normal.common_mut()),
                    &live
                ))
            };
            self.publish_normal_product_result_v1(
                &result,
                materials,
                body!(normal.common_mut()),
                directory,
                enumeration,
                live_owner,
            )?;
            if matches!(
                &result.event().body,
                crate::WorldEventBody::Domain(crate::DomainEvent::ProductValidated { .. })
            ) {
                let cursor = {
                    let live = live_owner();
                    let mut step = step!();
                    body!(validation_state::commit_product_cursor_v1(
                        RoutedProductSourceV1::Result(&result),
                        &mut step,
                        current,
                        parent,
                        self.members,
                        materials,
                        work,
                        body!(normal.common_mut()),
                        &live
                    ))
                };
                self.publish_normal_product_cursor_v1(
                    &cursor,
                    materials,
                    body!(normal.common_mut()),
                    directory,
                    enumeration,
                    live_owner,
                )?;
            }
            Ok(DueProductOutputV1::Written { receipt, result })
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
