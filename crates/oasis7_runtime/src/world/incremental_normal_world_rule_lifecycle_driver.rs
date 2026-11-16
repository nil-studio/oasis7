//! Fixed lifecycle batch consumer. The initial selector completes before any
//! command is applied; every command then reopens Current, and routing waits
//! until the entire batch has been persisted.
use super::delivery::IndustrialDeliveryErrorV1;
use super::driver_sources::{
    NormalWorldRuleLifecycleSourcesV1, NormalWorldRuleLifecycleWorkV1,
    with_current_lifecycle_sources_v1,
};
use super::*;
use crate::world::{
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_module_invocation::{
        AcceptedInvocationDirectoryV1, CurrentInvocationEnumerationWorkV1,
        RootModuleExecutionContextV1, RootModuleExecutionErrorV1,
    },
    incremental_normal_industry_due::deferred::DueEventQueueV1,
    incremental_normal_sources::BoundNormalTickParentV1,
    incremental_normal_tail::NormalTailWorkV1,
    incremental_normal_world_rule_observation_v2::ObservationJsonOriginalSinkV2,
};
use oasis7_formal_protocol::{
    incremental_material::OriginalObjectStreamReader,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
};
#[derive(Debug)]
struct LifecycleStageError(IndustrialDeliveryErrorV1);
impl std::fmt::Display for LifecycleStageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "world-rule lifecycle: {:?}", self.0)
    }
}
impl std::error::Error for LifecycleStageError {}
fn stage_error<OE>(error: IndustrialDeliveryErrorV1) -> RootModuleExecutionErrorV1<OE> {
    match error {
        IndustrialDeliveryErrorV1::Body(error) => RootModuleExecutionErrorV1::Body(error),
        other => RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(
            LifecycleStageError(other),
        ))),
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
    pub(in crate::world) fn execute_normal_world_rule_lifecycle_v1<P, CE, CC, PE, PC, ME, S>(
        &mut self,
        state: &M,
        sources: &NormalWorldRuleLifecycleSourcesV1<'_>,
        work: &mut NormalWorldRuleLifecycleWorkV1<'_>,
        normal_parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        extra: &mut emergency_close::sources::EmergencyServiceSourceWorkV1<'_>,
        sink: &mut S,
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
        S: ObservationJsonOriginalSinkV2,
        S::Error: std::fmt::Debug + Send + Sync + 'static,
    {
        let result = (|| {
            normal
                .common_mut()
                .map_err(RootModuleExecutionErrorV1::Body)?
                .charge(
                    1,
                    (std::mem::size_of_val(sources) + std::mem::size_of_val(work)) as u64,
                )
                .map_err(|e| {
                    RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
                })?;
            self.events
                .world_events
                .bind_journal_retention_v1(self.ctx.transaction, sources.retention)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            macro_rules! overlay {
                () => {
                    WorldFieldOverlayContextV1 {
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
                    }
                };
            }
            let seeds = {
                let live = live_owner();
                let mut ctx = overlay!();
                with_current_lifecycle_sources_v1(
                    sources,
                    &mut ctx,
                    parent,
                    work.delivery,
                    work.orders,
                    normal
                        .common_mut()
                        .map_err(RootModuleExecutionErrorV1::Body)?,
                    &live,
                    |delivery, ctx, parent, work_delivery, common| {
                        super::driver_select::capture_initial_lifecycle_seeds_v1(
                            sources,
                            delivery,
                            state,
                            ctx,
                            parent,
                            self.members,
                            materials,
                            work_delivery,
                            work.world,
                            extra,
                            common,
                            self.now,
                            &live,
                        )
                    },
                )
                .map_err(stage_error)?
            };
            let selected = seeds
                .into_work(
                    normal
                        .common_mut()
                        .map_err(RootModuleExecutionErrorV1::Body)?,
                )
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let mut queue = DueEventQueueV1::new(
                self.ctx.transaction,
                normal
                    .common_mut()
                    .map_err(RootModuleExecutionErrorV1::Body)?,
                self.now,
                &live_owner(),
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            for selected in selected {
                let publication = {
                    let live = live_owner();
                    let mut ctx = overlay!();
                    with_current_lifecycle_sources_v1(
                        sources,
                        &mut ctx,
                        parent,
                        work.delivery,
                        work.orders,
                        normal
                            .common_mut()
                            .map_err(RootModuleExecutionErrorV1::Body)?,
                        &live,
                        |delivery, ctx, parent, work_delivery, common| {
                            let header = header::prepare_current_lifecycle_header_v1(
                                &selected,
                                &header::LifecycleHeaderSourcesV1 {
                                    rules: sources.rules,
                                    delivery,
                                    governance: sources.governance,
                                },
                                state,
                                ctx,
                                parent,
                                self.members,
                                materials,
                                work_delivery,
                                common,
                                self.now,
                                &live,
                            )?;
                            let command = match header {
                                header::LifecycleHeaderPreparationV1::Rejected(_) => {
                                    return Ok(None);
                                }
                                header::LifecycleHeaderPreparationV1::Retained(retained) => {
                                    return Ok(Some(
                                        publication::prepare_retained_lifecycle_publication_v1(
                                            retained, ctx, common, self.now, &live,
                                        )?,
                                    ));
                                }
                                header::LifecycleHeaderPreparationV1::Fresh(header) => {
                                    super::driver_execute::execute_current_lifecycle_command_v1(
                                        header,
                                        sources,
                                        delivery,
                                        normal_parent,
                                        state,
                                        ctx,
                                        parent,
                                        self.members,
                                        materials,
                                        work_delivery,
                                        work.world,
                                        extra,
                                        common,
                                        self.now,
                                        &live,
                                        sink,
                                    )?
                                }
                            };
                            match command {
                                written::LifecycleCommandPreparationV1::Rejected(_) => Ok(None),
                                written::LifecycleCommandPreparationV1::Retained(retained) => Ok(
                                    Some(publication::prepare_retained_lifecycle_publication_v1(
                                        retained, ctx, common, self.now, &live,
                                    )?),
                                ),
                                written::LifecycleCommandPreparationV1::Fresh(written) => {
                                    Ok(Some(publication::prepare_fresh_lifecycle_publication_v1(
                                        written,
                                        sources.rules,
                                        delivery,
                                        state,
                                        ctx,
                                        parent,
                                        self.members,
                                        materials,
                                        work_delivery,
                                        common,
                                        self.now,
                                        &live,
                                    )?))
                                }
                            }
                        },
                    )
                    .map_err(stage_error)?
                };
                if let Some(publication) = publication {
                    let retained = self.persist_world_rule_lifecycle_v1(
                        publication,
                        materials,
                        normal
                            .common_mut()
                            .map_err(RootModuleExecutionErrorV1::Body)?,
                        live_owner,
                    )?;
                    queue
                        .push(
                            retained,
                            self.ctx.transaction,
                            normal
                                .common_mut()
                                .map_err(RootModuleExecutionErrorV1::Body)?,
                            self.now,
                            &live_owner(),
                        )
                        .map_err(RootModuleExecutionErrorV1::Body)?;
                }
            }
            self.flush_due_events_v1(
                &mut queue,
                normal
                    .common_mut()
                    .map_err(RootModuleExecutionErrorV1::Body)?,
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
