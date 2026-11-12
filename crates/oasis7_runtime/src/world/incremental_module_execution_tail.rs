//! Route the real last event after intervening module output contributions.
use super::*;
use crate::world::incremental_world_event_tail::{
    CurrentWorldEventTailWorkV1, recover_latest_counted_world_event_v1,
};
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
    /// Publish the actual completed transit and immediately run its Event
    /// subscriptions through this transaction's real invocation directory.
    /// The private reducer owner is required; callers cannot supply an event
    /// body or claim that an unrelated event has been persisted.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn publish_normal_material_transit_v1<ME>(
        &mut self,
        prepared: &crate::world::incremental_normal_tail::transits::PreparedNormalMaterialTransitV1,
        materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<'_, M, ME>,
        common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<usize, RootModuleExecutionErrorV1<O::Error>>
    where
        ME: FnMut() -> Option<ValueTreeError>,
    {
        let result = (|| {
            let live = live_owner();
            let persisted = crate::world::incremental_module_execution_event::contribute_normal_material_transit_event_v1(
                self.now,
                prepared,
                common,
                &live,
                self.ctx.transaction,
                self.events,
                materials,
            ).map_err(RootModuleExecutionErrorV1::Body)?;
            self.route_persisted_event_v1(&persisted, None, directory, enumeration, live_owner)
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }

    /// Publish the actual completed factory build and immediately run its Event
    /// subscriptions through this transaction's real invocation directory.
    /// The private reducer owner is required; callers cannot supply an event
    /// body or claim that an unrelated event has been persisted.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn publish_normal_factory_built_v1<ME>(
        &mut self,
        prepared: &crate::world::incremental_normal_tail::factory_built::PreparedNormalFactoryBuiltV1,
        materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<'_, M, ME>,
        common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<usize, RootModuleExecutionErrorV1<O::Error>>
    where
        ME: FnMut() -> Option<ValueTreeError>,
    {
        let result = (|| {
            let live = live_owner();
            let persisted = crate::world::incremental_module_execution_event::contribute_normal_factory_built_event_v1(
                self.now,
                prepared,
                common,
                &live,
                self.ctx.transaction,
                self.events,
                materials,
            ).map_err(RootModuleExecutionErrorV1::Body)?;
            self.route_persisted_event_v1(&persisted, None, directory, enumeration, live_owner)
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }

    /// Publish the actual completed recipe and immediately run its Event
    /// subscriptions through this transaction's real invocation directory.
    /// The private reducer owner is required; callers cannot supply an event
    /// body or claim that an unrelated event has been persisted.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn publish_normal_recipe_completed_v1<ME>(
        &mut self,
        prepared: &crate::world::incremental_normal_industry_due::PreparedNormalRecipeCompletedV1,
        materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<'_, M, ME>,
        common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<usize, RootModuleExecutionErrorV1<O::Error>>
    where
        ME: FnMut() -> Option<ValueTreeError>,
    {
        let result = (|| {
            let live = live_owner();
            let persisted = crate::world::incremental_module_execution_event::contribute_normal_recipe_completed_event_v1(
                self.now,
                prepared,
                common,
                &live,
                self.ctx.transaction,
                self.events,
                materials,
            ).map_err(RootModuleExecutionErrorV1::Body)?;
            self.route_persisted_event_v1(&persisted, None, directory, enumeration, live_owner)
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn publish_normal_product_attempt_v1<ME>(
        &mut self,
        source: &crate::world::incremental_normal_industry_due::PreparedNormalProductAttemptV1,
        materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<'_, M, ME>,
        common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<usize, RootModuleExecutionErrorV1<O::Error>>
    where
        ME: FnMut() -> Option<ValueTreeError>,
    {
        let result = (|| {
            let live = live_owner();
            let persisted = crate::world::incremental_module_execution_event::contribute_due_product_attempt_event_v1(
                self.now, source, common, &live, self.ctx.transaction, self.events, materials,
            ).map_err(RootModuleExecutionErrorV1::Body)?;
            let era = matches!(
                &source.event().body,
                crate::WorldEventBody::Domain(
                    crate::DomainEvent::ProductValidationAttemptStarted { .. }
                        | crate::DomainEvent::ProductValidationRecorded { .. }
                        | crate::DomainEvent::ProductValidated { .. }
                )
            )
            .then(|| source.event_era());
            self.route_persisted_event_v1(&persisted, era, directory, enumeration, live_owner)
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn publish_normal_product_recorded_v1<ME>(
        &mut self,
        source: &crate::world::incremental_normal_industry_due::PreparedNormalProductRecordedV1,
        materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<'_, M, ME>,
        common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<usize, RootModuleExecutionErrorV1<O::Error>>
    where
        ME: FnMut() -> Option<ValueTreeError>,
    {
        let result = (|| {
            let live = live_owner();
            let persisted = crate::world::incremental_module_execution_event::contribute_due_product_recorded_event_v1(
                self.now, source, common, &live, self.ctx.transaction, self.events, materials,
            ).map_err(RootModuleExecutionErrorV1::Body)?;
            let era = matches!(
                &source.event().body,
                crate::WorldEventBody::Domain(
                    crate::DomainEvent::ProductValidationAttemptStarted { .. }
                        | crate::DomainEvent::ProductValidationRecorded { .. }
                        | crate::DomainEvent::ProductValidated { .. }
                )
            )
            .then(|| source.event_era());
            self.route_persisted_event_v1(&persisted, era, directory, enumeration, live_owner)
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn publish_normal_product_result_v1<ME>(
        &mut self,
        source: &crate::world::incremental_normal_industry_due::PreparedNormalProductResultV1,
        materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<'_, M, ME>,
        common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<usize, RootModuleExecutionErrorV1<O::Error>>
    where
        ME: FnMut() -> Option<ValueTreeError>,
    {
        let result = (|| {
            let live = live_owner();
            let persisted = crate::world::incremental_module_execution_event::contribute_due_product_result_event_v1(
                self.now, source, common, &live, self.ctx.transaction, self.events, materials,
            ).map_err(RootModuleExecutionErrorV1::Body)?;
            let era = matches!(
                &source.event().body,
                crate::WorldEventBody::Domain(
                    crate::DomainEvent::ProductValidationAttemptStarted { .. }
                        | crate::DomainEvent::ProductValidationRecorded { .. }
                        | crate::DomainEvent::ProductValidated { .. }
                )
            )
            .then(|| source.event_era());
            self.route_persisted_event_v1(&persisted, era, directory, enumeration, live_owner)
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn publish_normal_recipe_failure_v1<ME>(
        &mut self,
        source: &crate::world::incremental_normal_industry_due::PreparedNormalRecipeFailureV1,
        materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<'_, M, ME>,
        common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<usize, RootModuleExecutionErrorV1<O::Error>>
    where
        ME: FnMut() -> Option<ValueTreeError>,
    {
        let result = (|| {
            let live = live_owner();
            let persisted = crate::world::incremental_module_execution_event::contribute_due_recipe_failure_event_v1(
                self.now, source, common, &live, self.ctx.transaction, self.events, materials,
            ).map_err(RootModuleExecutionErrorV1::Body)?;
            self.route_persisted_event_v1(&persisted, None, directory, enumeration, live_owner)
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn publish_normal_product_module_failure_v1<ME>(
        &mut self,
        source: &crate::world::incremental_normal_industry_due::PreparedNormalProductModuleFailureV1,
        materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<'_, M, ME>,
        common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<usize, RootModuleExecutionErrorV1<O::Error>>
    where
        ME: FnMut() -> Option<ValueTreeError>,
    {
        let result = (|| {
            let live = live_owner();
            let persisted = crate::world::incremental_module_execution_event::contribute_due_product_module_failure_event_v1(
                self.now, source, common, &live, self.ctx.transaction, self.events, materials,
            ).map_err(RootModuleExecutionErrorV1::Body)?;
            self.route_persisted_event_v1(&persisted, None, directory, enumeration, live_owner)
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn publish_normal_product_cursor_v1<ME>(
        &mut self,
        source: &crate::world::incremental_normal_industry_due::PreparedNormalProductCursorV1,
        materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<'_, M, ME>,
        common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<usize, RootModuleExecutionErrorV1<O::Error>>
    where
        ME: FnMut() -> Option<ValueTreeError>,
    {
        let result = (|| {
            let live = live_owner();
            let persisted = crate::world::incremental_module_execution_event::contribute_due_product_cursor_event_v1(
                self.now, source, common, &live, self.ctx.transaction, self.events, materials,
            ).map_err(RootModuleExecutionErrorV1::Body)?;
            let _ = (persisted, directory, enumeration);
            // Cursor advancement is a durable routing marker, not another
            // subscription delivery. The original path appends it only.
            Ok(0)
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }

    /// World::step routes journal.last() after PostAction, which may differ
    /// from the original business result. Recover only the counter's actual
    /// typed producer tail; no committed History or World scan is involved.
    pub(in crate::world) fn route_latest_world_event_v1(
        &mut self,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        work: &mut CurrentWorldEventTailWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<usize, RootModuleExecutionErrorV1<O::Error>> {
        let result = (|| {
            let owner = recover_latest_counted_world_event_v1(
                self.events.world_events,
                self.ctx.transaction,
                self.originals,
                self.ctx.values.memory,
                self.now,
                work,
                &mut self.ctx.cancelled,
                live_owner,
            )
            .map_err(RootModuleExecutionErrorV1::EventTail)?;
            let Some(owner) = owner else {
                return Ok(0);
            };
            let event = crate::world::incremental_module_execution_event::PersistedWorldEventV1::from_counted_tail(&owner);
            self.route_persisted_event_v1(&event, None, directory, enumeration, live_owner)
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
