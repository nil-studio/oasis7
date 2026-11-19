//! Complete selected Release Apply orchestration. Every body is derived from
//! actual Current review evidence and the original install/governance path.
use super::event::{CompletedModuleLifecycleActionV1, ModuleLifecycleCommittedStateV1};
use super::*;
use crate::world::{
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_module_invocation::{AcceptedInvocationDirectoryV1, SelectedAcceptedDispositionV1},
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext},
};
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
};
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn execute_release_apply_after_accepted_v1<
    M,
    P,
    V,
    W,
    D,
    E,
    C,
    PE,
    PC,
    CW,
    ME,
    F,
>(
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    accepted: SelectedAcceptedDispositionV1,
    sources: &ModuleLifecycleSourcesV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    module: &mut ModuleLifecycleWorkV1<'_>,
    factory: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
    max_event_bytes: usize,
    persist: &mut F,
    live: &ExecutionFence,
) -> Result<CompletedModuleLifecycleActionV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        accepted.validate(selected, step.transaction, common.memory())?;
        let now = step.time();
        let decision = {
            let mut ctx = WorldFieldOverlayContextV1 {
                transaction: &mut *step.transaction,
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
            super::release_facts::prepare_current_release_apply_v1(
                selected, outcome, &accepted, sources, &mut ctx, parent, factory, module, common,
                directory, now, live,
            )?
        };
        let source = match decision {
            super::release_facts::CurrentReleaseApplyDecisionV1::Ready(source) => source,
            super::release_facts::CurrentReleaseApplyDecisionV1::Rejected(rejection) => {
                let event = super::event::publish_current_release_rejection_v1(
                    rejection,
                    accepted.into_audit(),
                    selected,
                    outcome,
                    step,
                    read,
                    materials,
                    common,
                    max_event_bytes,
                    persist,
                    live,
                )?;
                return Ok(CompletedModuleLifecycleActionV1::Applied {
                    first: event,
                    second: None,
                });
            }
        };
        let result = super::install_driver::execute_current_release_install_v1(
            source,
            selected,
            outcome,
            accepted,
            sources,
            step,
            read,
            parent,
            write,
            materials,
            module,
            factory,
            common,
            directory,
            max_event_bytes,
            persist,
            live,
        )?;
        let (plan, accepted, first, last) = match result {
            super::install_driver::ExecutedCurrentSelectedInstallV1::Ready {
                plan,
                accepted,
                first,
                last,
            } => (plan, accepted, first, last),
            super::install_driver::ExecutedCurrentSelectedInstallV1::Rejected(failure) => {
                let mut failure = failure.into_business_v1()?;
                let first = failure.first.take();
                let event = super::event::publish_current_module_install_failure_v1(
                    failure,
                    selected,
                    outcome,
                    step,
                    read,
                    materials,
                    common,
                    max_event_bytes,
                    persist,
                    live,
                )?;
                return Ok(match first {
                    Some(first) => CompletedModuleLifecycleActionV1::Applied {
                        first,
                        second: Some(event),
                    },
                    None => CompletedModuleLifecycleActionV1::Applied {
                        first: event,
                        second: None,
                    },
                });
            }
        };
        let installed = super::instance_tail_commit::commit_current_module_install_plan_v1(
            plan,
            accepted,
            selected,
            outcome,
            sources,
            step,
            read,
            parent,
            write,
            materials,
            module,
            factory,
            common,
            max_event_bytes,
            persist,
            live,
        )?;
        drop(last);
        let ModuleLifecycleCommittedStateV1::Install(state) = &installed.state else {
            return Err(invalid());
        };
        let completion = super::release_completion::prepare_current_release_completion_v1(
            state,
            step.transaction,
            common,
            now,
            live,
        )?;
        super::release_completion::commit_current_release_profiles_v1(
            &completion,
            sources,
            step,
            read,
            parent,
            write,
            materials,
            module,
            factory,
            common,
            max_event_bytes,
            persist,
            live,
        )?;
        let final_plan = {
            let mut ctx = WorldFieldOverlayContextV1 {
                transaction: &mut *step.transaction,
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
            super::release_plan::prepare_current_applied_release_v1(
                &completion,
                selected,
                outcome,
                sources,
                &mut ctx,
                parent,
                factory,
                module,
                common,
                directory,
                now,
                live,
            )?
        };
        let final_event = super::release_commit::commit_current_module_release_v1(
            final_plan,
            None,
            selected,
            outcome,
            sources,
            step,
            read,
            parent,
            write,
            materials,
            module,
            factory,
            common,
            max_event_bytes,
            persist,
            live,
        )?;
        drop(completion);
        // The first install/Governance owner retains the actual Accepted audit;
        // intermediate originals stay in the transaction and are never routed twice.
        Ok(match first {
            Some(first) => CompletedModuleLifecycleActionV1::Applied {
                first,
                second: Some(final_event),
            },
            None => CompletedModuleLifecycleActionV1::Applied {
                first: installed,
                second: Some(final_event),
            },
        })
    }));
    let result = match attempt {
        Ok(result) => result,
        Err(payload) => {
            poison(step.transaction);
            std::panic::resume_unwind(payload)
        }
    };
    if result.is_err() {
        poison(step.transaction)
    }
    result
}
fn poison<M: MerkleNodeReader>(tx: &mut ExecutionTransaction<'_, M>) {
    let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> = tx
        .contribute(|_| {
            Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
        });
}
