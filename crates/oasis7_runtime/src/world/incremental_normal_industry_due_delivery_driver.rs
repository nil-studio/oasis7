//! Exact receipt retry: replay immutable originals and advance the original
//! cursor. This path never invokes the Product validator again.
use super::super::validation_state::{self, CurrentProductHistoryV1, RoutedProductSourceV1};
use super::*;
use crate::world::{
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_module_invocation::{
        AcceptedInvocationDirectoryV1, CurrentInvocationEnumerationWorkV1,
        RootModuleExecutionContextV1, RootModuleExecutionErrorV1,
    },
    incremental_normal_action_factory_schedule::{FactoryScheduleSourcesV1, FactoryScheduleWorkV1},
    incremental_normal_tail::NormalTailWorkV1,
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeWriter, ValueReadContext},
};
#[derive(Debug, PartialEq, Eq)]
pub(in crate::world) enum DueProductReplayOutcomeV1 {
    Accepted,
    Rejected,
}
/// Minted only below, after the exact synchronous subscription route returned
/// Ok (including zero subscribers). Only that completion can advance cursor.
pub(in crate::world) struct RoutedProductReplayV1 {
    source: PreparedProductValidationReplayV1,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    now: u64,
    fee: ReservedValue<()>,
    cursor_claimed: std::cell::Cell<bool>,
}
fn body_error(error: DeliveryDataErrorV1) -> BodyMutationError {
    match error {
        DeliveryDataErrorV1::Body(e) => e,
        DeliveryDataErrorV1::Value(e) => e.into(),
        DeliveryDataErrorV1::Formal(e) => BodyMutationError::Original(Box::new(e)),
    }
}
impl RoutedProductReplayV1 {
    pub(in crate::world::incremental_normal_industry_due) fn validate_evidence_identity<
        M: MerkleNodeReader,
    >(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
        identity: WorkIdentityV1,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        self.source
            .validate_evidence_identity(tx, memory, identity)
            .map_err(body_error)?;
        if self.identity != identity
            || self.now != now
            || !self.fee.uses_budget(memory)
            || !World::is_product_validation_event(self.source.event())
        {
            return Err(BodyMutationError::Invalid(
                "Product replay route completion",
            ));
        }
        if self.cursor_claimed.replace(true) {
            return Err(BodyMutationError::Invalid(
                "Product replay cursor already consumed",
            ));
        }
        Ok(())
    }
    pub(in crate::world::incremental_normal_industry_due) fn event_id(&self) -> u64 {
        self.source.event().id
    }
    pub(in crate::world::incremental_normal_industry_due) fn event_id_era(&self) -> u64 {
        self.source.event_id_era()
    }
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
    pub(in crate::world::incremental_normal_industry_due) fn resume_due_product_receipt_v1<
        P,
        CE,
        CC,
        PE,
        PC,
        ME,
    >(
        &mut self,
        pending: &ReservedValue<crate::RecipeJobState>,
        output_index: usize,
        history: &ReservedValue<CurrentProductHistoryV1>,
        delivery: &mut CurrentProductValidationDeliveryIndexV1<'_>,
        delivery_work: &mut ProductValidationDeliveryWorkV1,
        materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
        current: &mut WorldFieldReadContextV1<'_, P, R, CE, CC>,
        parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        sources: &FactoryScheduleSourcesV1<'_>,
        work: &mut FactoryScheduleWorkV1,
        normal: &mut NormalTailWorkV1<'_, '_>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<DueProductReplayOutcomeV1, RootModuleExecutionErrorV1<O::Error>>
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
            let live = live_owner();
            let evidence_identity = common!().work().identity();
            history
                .validate_evidence_identity(
                    self.ctx.transaction,
                    common!().memory(),
                    self.now,
                    evidence_identity,
                    pending,
                    &live,
                )
                .map_err(RootModuleExecutionErrorV1::Body)?;
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
                                    "replay output index",
                                ))
                            })?,
                    )
            }
            .ok_or_else(|| {
                RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid("replay output absent"))
            })?;
            let receipt = history.receipt().ok_or_else(|| {
                RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                    "replay actual receipt absent",
                ))
            })?;
            let selected_index = u32::try_from(output_index).map_err(|_| {
                RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                    "replay output index width",
                ))
            })?;
            if receipt.validation_index != Some(selected_index)
                || receipt.stack != *stack
                || receipt.requester_agent_id != pending.requester_agent_id
            {
                return Err(RootModuleExecutionErrorV1::Body(
                    BodyMutationError::Invalid("replay selected output source binding"),
                ));
            }
            if !World::product_validation_decision_matches_stack(&receipt.decision, stack) {
                return Ok(DueProductReplayOutcomeV1::Rejected);
            }
            let plan = delivery
                .plan_receipt(
                    receipt,
                    self.ctx,
                    &mut self.events.mutation,
                    self.originals,
                    delivery_work,
                    common!(),
                    &live,
                )
                .map_err(RootModuleExecutionErrorV1::ProductDelivery)?;
            let has_delivery = plan.has_delivery();
            for item in plan.into_replay_items() {
                if item.was_routed_at_plan() {
                    continue;
                }
                let live = live_owner();
                let prepared = delivery
                    .prepare_exact_replay(
                        item,
                        self.ctx,
                        self.originals,
                        delivery_work,
                        common!(),
                        &live,
                    )
                    .map_err(RootModuleExecutionErrorV1::ProductDelivery)?;
                let Some(prepared) = prepared else {
                    continue;
                };
                let persisted=crate::world::incremental_module_execution_event::PersistedWorldEventV1::from_exact_product_replay(&prepared,self.ctx.transaction,common!(),&live)
                    .map_err(|e|RootModuleExecutionErrorV1::Body(body_error(e)))?;
                self.route_persisted_event_v1(
                    &persisted,
                    Some(prepared.event_id_era()),
                    directory,
                    enumeration,
                    live_owner,
                )?;
                drop(persisted);
                let fee = common!()
                    .memory()
                    .reserve(
                        size_of::<RoutedProductReplayV1>() + 128,
                        MemoryClass::Ordinary,
                    )
                    .map_err(ValueTreeError::Memory)?;
                let routed = RoutedProductReplayV1 {
                    source: prepared,
                    guard: self.ctx.transaction.guard(),
                    identity: common!().work().identity(),
                    now: self.now,
                    fee: fee.retain(()),
                    cursor_claimed: std::cell::Cell::new(false),
                };
                let cursor = {
                    let live = live_owner();
                    let mut step = step!();
                    body!(validation_state::commit_product_cursor_v1(
                        RoutedProductSourceV1::Replay(&routed),
                        &mut step,
                        current,
                        parent,
                        self.members,
                        materials,
                        work,
                        common!(),
                        &live
                    ))
                };
                self.publish_normal_product_cursor_v1(
                    &cursor,
                    materials,
                    common!(),
                    directory,
                    enumeration,
                    live_owner,
                )?;
            }
            if !has_delivery {
                let result = {
                    let live = live_owner();
                    let mut step = step!();
                    body!(validation_state::commit_product_receipt_delivery_v1(
                        pending,
                        history,
                        sources,
                        &mut step,
                        current,
                        parent,
                        self.members,
                        materials,
                        work,
                        common!(),
                        &live
                    ))
                };
                self.publish_normal_product_result_v1(
                    &result,
                    materials,
                    common!(),
                    directory,
                    enumeration,
                    live_owner,
                )?;
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
                        common!(),
                        &live
                    ))
                };
                self.publish_normal_product_cursor_v1(
                    &cursor,
                    materials,
                    common!(),
                    directory,
                    enumeration,
                    live_owner,
                )?;
            }
            Ok(DueProductReplayOutcomeV1::Accepted)
        })();
        if result.is_err() {
            let _: Result<(), ExecutionError> = self
                .ctx
                .transaction
                .contribute(|_| Err(ExecutionError::Aborted));
        }
        result
    }
}
