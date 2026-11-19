//! Selected compile/deploy: original preflight, real bytes, exact Current writes.
use super::deployment_rules::{
    DeploymentBytesV1, ModuleDeploymentDecisionV1, evaluate_module_deployment_v1,
};
use super::event::CompletedModuleLifecycleActionV1;
use super::*;
use crate::world::{
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_module_artifact_original::{
        ArtifactOriginalWriteErrorV1, persist_artifact_original_data_v1,
    },
    incremental_module_invocation::{
        AcceptedInvocationDirectoryV1, RootModuleExecutionErrorV1, SelectedAcceptedDispositionV1,
    },
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    incremental_material::OriginalObjectStreamReader,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext},
};
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_event_decode::WorldEventDecodeWorkV1,
};

pub(in crate::world) fn is_deployment_action_v1(action: &crate::Action) -> bool {
    matches!(
        action,
        crate::Action::CompileModuleArtifactFromSource { .. }
            | crate::Action::DeployModuleArtifact { .. }
    )
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn execute_deployment_after_accepted_v1<
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
    O,
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
    originals: &mut O,
    directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
    max_event_bytes: usize,
    audit: &mut WorldEventDecodeWorkV1,
    persist: &mut F,
    live: &ExecutionFence,
) -> Result<CompletedModuleLifecycleActionV1, RootModuleExecutionErrorV1<O::Error>>
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
    O: OriginalObjectStreamReader,
{
    let attempt = (|| {
        accepted.validate(selected, step.transaction, step.mutation.memory)?;
        let effective = outcome
            .override_action_for_selected_v1(selected, step.transaction)?
            .unwrap_or(&selected.envelope().action);
        if !is_deployment_action_v1(effective)
            || !outcome.uses_budget(common.memory())
            || !step.mutation.memory.same_pool(common.memory())
        {
            return Err(invalid().into());
        }
        let (decision, fees) = {
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
            let mut facts = super::facts::CurrentMarketFactsV1 {
                sources,
                ctx: &mut ctx,
                parent,
                factory,
                module,
                common,
                live,
                directory,
                fees: std::array::from_fn(|_| None),
            };
            let result = evaluate_module_deployment_v1(&mut facts, effective);
            let fees = facts.fees;
            let decision=match result {
    Ok(value)=>value,
    Err(crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1::Body(error))=>return Err(error.into()),
    Err(crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1::World(error))=>return Err(super::execute::evaluation_error_v1(error,fees).into()),
   };
            (decision, fees)
        };
        match decision {
            ModuleDeploymentDecisionV1::Rejected(reason) => {
                let layout = common
                    .memory()
                    .reserve(
                        std::mem::size_of::<CompletedModuleLifecycleActionV1>(),
                        MemoryClass::Ordinary,
                    )
                    .map_err(ValueTreeError::Memory)?;
                let event = step.append_selected_rejection_owned(
                    materials,
                    live,
                    selected,
                    reason,
                    max_event_bytes,
                    audit,
                    persist,
                )?;
                Ok(CompletedModuleLifecycleActionV1::Rejected {
                    accepted: accepted.into_audit(),
                    event,
                    core: common.work().identity(),
                    memory: common.memory().clone(),
                    _layout: layout,
                    _prepare_fees: fees,
                })
            }
            ModuleDeploymentDecisionV1::Ready {
                publisher,
                hash,
                bytes,
                fee,
            } => {
                let data = match &bytes {
                    DeploymentBytesV1::Borrowed(bytes) => *bytes,
                    DeploymentBytesV1::Compiled(bytes) => bytes.as_slice(),
                };
                // Preserve the original registration's second SHA pass before any new
                // original is installed. Both passes poll the same continuing work.
                use sha2::{Digest, Sha256};
                let mut digest = Sha256::new();
                for chunk in data.chunks(8192) {
                    common
                        .charge(1 + chunk.len() as u64, chunk.len() as u64)
                        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
                    digest.update(chunk);
                }
                common
                    .charge(128, 32)
                    .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
                let computed: [u8; 32] = digest.finalize().into();
                let mut expected = [0u8; 32];
                hex::decode_to_slice(&hash, &mut expected).map_err(|_| invalid())?;
                if expected != computed {
                    return Err(invalid().into());
                }
                let original = persist_artifact_original_data_v1(originals, data, common).map_err(
                    |error| match error {
                        ArtifactOriginalWriteErrorV1::Resource(error) => {
                            RootModuleExecutionErrorV1::Body(error)
                        }
                        ArtifactOriginalWriteErrorV1::Storage(error) => {
                            RootModuleExecutionErrorV1::ArtifactOriginalStorage(error)
                        }
                        ArtifactOriginalWriteErrorV1::Unsupported => {
                            RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                                "artifact original writer unavailable",
                            ))
                        }
                    },
                )?;
                // Only this private fixed callback can discharge the original writer's
                // physical prerequisite. It validates the exact bytes already persisted
                // and read back above; it is not a caller-supplied no-op acknowledgement.
                crate::world::incremental_step::factory_schedule_write_io::paid_with_module(
                    step,
                    read,
                    parent,
                    write,
                    materials,
                    factory,
                    module,
                    common,
                    live,
                    |step, read, parent, write, _materials, _factory, module, _tail| {
                        use oasis7_world_state::incremental_runtime_artifact_content::*;
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
                        let registered = match read_runtime_artifact_content_overlay_v1(
                            RuntimeArtifactContentFieldV1::Registered,
                            &hash,
                            &mut ctx,
                        )
                        .map_err(overlay_error)?
                        {
                            RuntimeArtifactContentValueV1::Registered(value) => value,
                            _ => return Err(invalid()),
                        };
                        let before = match read_runtime_artifact_content_overlay_v1(
                            RuntimeArtifactContentFieldV1::Original,
                            &hash,
                            &mut ctx,
                        )
                        .map_err(overlay_error)?
                        {
                            RuntimeArtifactContentValueV1::Original(value) => value,
                            _ => return Err(invalid()),
                        };
                        let mut verify = |id, actual: &[u8]| {
                            if id != original
                                || actual.len() != data.len()
                                || actual.as_ptr() != data.as_ptr()
                            {
                                return Err(ValueTreeError::Invalid(
                                    "artifact physical source binding",
                                ));
                            }
                            Ok(())
                        };
                        let guard = ctx.transaction.guard();
                        change_runtime_artifact_content_overlay_v1(
                            RuntimeArtifactContentChangeV1::Registered {
                                key: &hash,
                                expected: registered,
                                after: true,
                            },
                            &mut ctx,
                            write,
                            module.content,
                            &mut verify,
                            &guard,
                            live,
                        )
                        .map_err(overlay_error)?;
                        let guard = ctx.transaction.guard();
                        change_runtime_artifact_content_overlay_v1(
                            RuntimeArtifactContentChangeV1::Original {
                                key: &hash,
                                expected: before,
                                after: Some(data),
                            },
                            &mut ctx,
                            write,
                            module.content,
                            &mut verify,
                            &guard,
                            live,
                        )
                        .map_err(overlay_error)?;
                        Ok(())
                    },
                )?;
                let domain = crate::DomainEvent::ModuleArtifactDeployed {
                    publisher_agent_id: publisher.to_owned(),
                    wasm_hash: hash,
                    bytes_len: data.len() as u64,
                    fee_kind: oasis7_agent_api::world_action::ResourceKind::Electricity,
                    fee_amount: fee,
                };
                let first = super::execute::persist_market_event_v1(
                    selected.envelope().id,
                    domain,
                    fees,
                    accepted.into_audit(),
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
                Ok(CompletedModuleLifecycleActionV1::Applied {
                    first,
                    second: None,
                })
            }
        }
    })();
    if attempt.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            step.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    attempt
}
