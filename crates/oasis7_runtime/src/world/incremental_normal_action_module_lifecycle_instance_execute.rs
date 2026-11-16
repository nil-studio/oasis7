//! Selected/Rule-bound install consumer. The original install prelude feeds
//! the real governance sequence; no arbitrary module plan is accepted.
use super::*;
use super::{
    instance_current::{CurrentModuleInstanceFactsV1, ModuleInstanceFactsFeesV1},
    instance_rules::{ModuleInstallPreludeV1, prepare_module_install_prelude_v1},
};
use crate::world::{
    incremental_module_invocation::{AcceptedInvocationDirectoryV1, SelectedAcceptedDispositionV1},
    incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1,
};
use oasis7_formal_protocol::incremental_value::ValueNodeReader;
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
};

pub(super) enum CurrentSelectedInstallDecisionV1 {
    Rejected {
        reason: crate::RejectReason,
        facts: ModuleInstanceFactsFeesV1,
        _fee: MemoryReservation,
    },
    NoGovernance {
        manifest: super::governance_manifest::CurrentModuleWorldManifestV1,
        hash: ReservedValue<String>,
        facts: ModuleInstanceFactsFeesV1,
        fee_kind: oasis7_agent_api::world_action::ResourceKind,
        fee_amount: i64,
    },
    Governed {
        update: super::governance_plan::PreparedModuleManifestUpdateV1,
        facts: ModuleInstanceFactsFeesV1,
        fee_kind: oasis7_agent_api::world_action::ResourceKind,
        fee_amount: i64,
    },
}
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_current_selected_install_v1<M, P, R, E, C, PE, PC>(
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    accepted: &SelectedAcceptedDispositionV1,
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
    live: &ExecutionFence,
) -> Result<CurrentSelectedInstallDecisionV1, FactoryScheduleReadErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    accepted.validate(selected, ctx.transaction, common.memory())?;
    let action = outcome
        .override_action_for_selected_v1(selected, ctx.transaction)?
        .unwrap_or(&selected.envelope().action);
    let (actor, manifest, activate) = match action {
        crate::Action::InstallModuleFromArtifact {
            installer_agent_id,
            manifest,
            activate,
        }
        | crate::Action::InstallModuleFromArtifactWithFinality {
            installer_agent_id,
            manifest,
            activate,
            ..
        }
        | crate::Action::InstallModuleToTargetFromArtifact {
            installer_agent_id,
            manifest,
            activate,
            ..
        }
        | crate::Action::InstallModuleToTargetFromArtifactWithFinality {
            installer_agent_id,
            manifest,
            activate,
            ..
        } => (installer_agent_id.as_str(), manifest, *activate),
        _ => return Err(invalid().into()),
    };
    prepare_current_install_fields_v1(
        actor, manifest, activate, None, sources, ctx, parent, factory, module, common, directory,
        live,
    )
}

#[allow(clippy::too_many_arguments)]
fn prepare_current_install_fields_v1<M, P, R, E, C, PE, PC>(
    actor: &str,
    manifest: &crate::ModuleManifest,
    activate: bool,
    release_profiles: Option<&crate::ModuleProfileChanges>,
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
    live: &ExecutionFence,
) -> Result<CurrentSelectedInstallDecisionV1, FactoryScheduleReadErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    let mut source = CurrentModuleInstanceFactsV1::new(
        sources, ctx, parent, factory, module, common, live, directory,
    )?;
    let result = prepare_module_install_prelude_v1(&mut source, actor, manifest, activate);
    let facts = source.into_parts();
    let prelude = result?;
    let (changes, fee_kind, fee_amount) = match prelude {
        ModuleInstallPreludeV1::Rejected(reason) => {
            pay(common, 4096)?;
            let fee = common
                .memory()
                .reserve(4096, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            return Ok(CurrentSelectedInstallDecisionV1::Rejected {
                reason,
                facts,
                _fee: fee,
            });
        }
        ModuleInstallPreludeV1::Plan {
            changes,
            fee_kind,
            fee_amount,
        } => (changes, fee_kind, fee_amount),
    };
    let before =
        crate::world::incremental_normal_action_factory_schedule::with_module_lifecycle_read_io_v1(
            sources.families.roots,
            ctx,
            parent,
            factory,
            common,
            module,
            |ctx, parent, factory, module| {
                Ok(
                    super::governance_manifest::read_current_module_world_manifest_v1(
                        sources, ctx, parent, factory, module, live,
                    )?,
                )
            },
        )?;
    if changes.is_empty() && release_profiles.is_none_or(|profiles| profiles.is_empty()) {
        let (manifest, fee) = before.owned_v1(common)?;
        let hash = crate::world::incremental_normal_action_diplomacy::fixed_json_digest_v1(
            &manifest, common,
        )?;
        drop(manifest);
        drop(fee);
        return Ok(CurrentSelectedInstallDecisionV1::NoGovernance {
            manifest: before,
            hash,
            facts,
            fee_kind,
            fee_amount,
        });
    }
    match super::governance_plan::prepare_module_manifest_update_v1(
        before,
        &changes,
        ctx.transaction,
        common,
        live,
    )? {
        super::governance_plan::ModuleManifestUpdateDecisionV1::Ready(mut update) => {
            if let Some(profiles) = release_profiles {
                let commitment =
                    match super::release_shadow_current::release_profile_commitment_paid_v1(
                        manifest, activate, profiles, common,
                    ) {
                        Ok(value) => value,
                        Err(super::release_shadow_current::Failure::Body(error)) => {
                            return Err(error.into());
                        }
                        Err(super::release_shadow_current::Failure::Business(reason)) => {
                            pay(common, reason.len().checked_add(8192).ok_or_else(invalid)?)?;
                            let fee = common
                                .memory()
                                .reserve(
                                    reason.len().checked_add(8192).ok_or_else(invalid)?,
                                    MemoryClass::Ordinary,
                                )
                                .map_err(ValueTreeError::Memory)?;
                            return Ok(CurrentSelectedInstallDecisionV1::Rejected {
                                reason: crate::RejectReason::RuleDenied {
                                    notes: vec![format!(
                                        "module release profile commitment rejected: {reason}"
                                    )],
                                },
                                facts,
                                _fee: fee,
                            });
                        }
                    };
                if let Some(commitment) = commitment {
                    let heap=oasis7_world_state::incremental_collection_layout::json_object_output_heap_v1(1)?.checked_mul(usize::BITS as usize).and_then(|n|n.checked_add(8192)).ok_or_else(invalid)?;
                    pay(common, heap)?;
                    let mut fee = common
                        .memory()
                        .reserve(heap, MemoryClass::Ordinary)
                        .map_err(ValueTreeError::Memory)?;
                    let serde_json::Value::Object(content) = &mut update.manifest.content else {
                        return Err(invalid().into());
                    };
                    content.insert(
                        "module_profile_commitment".to_owned(),
                        serde_json::Value::String(commitment.to_string()),
                    );
                    update
                        .changes_fee
                        .merge(&mut fee)
                        .map_err(BodyMutationError::Invalid)?;
                }
            }
            Ok(CurrentSelectedInstallDecisionV1::Governed {
                update,
                facts,
                fee_kind,
                fee_amount,
            })
        }
        super::governance_plan::ModuleManifestUpdateDecisionV1::Rejected { reason, _fee } => {
            Ok(CurrentSelectedInstallDecisionV1::Rejected {
                reason,
                facts,
                _fee,
            })
        }
    }
}

/// The real release request supplies the install input. Selected/Rule identity
/// and the one original Accepted disposition remain those of the release.
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_current_release_install_v1<M, P, R, E, C, PE, PC>(
    release: &super::release_facts::ValidatedCurrentReleaseApplyV1,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    accepted: &SelectedAcceptedDispositionV1,
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
    live: &ExecutionFence,
) -> Result<CurrentSelectedInstallDecisionV1, FactoryScheduleReadErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    accepted.validate(selected, ctx.transaction, common.memory())?;
    release.validate_provenance(
        selected,
        outcome,
        ctx.transaction,
        common.memory(),
        common.work().identity(),
        release.now(),
        live,
    )?;
    let action = outcome
        .override_action_for_selected_v1(selected, ctx.transaction)?
        .unwrap_or(&selected.envelope().action);
    if !matches!(
        action,
        crate::Action::ModuleReleaseApply { .. }
            | crate::Action::ModuleReleaseApplyWithFinality { .. }
    ) {
        return Err(invalid().into());
    }
    let request = release.request();
    prepare_current_install_fields_v1(
        &request.requester_agent_id,
        &request.manifest,
        request.activate,
        Some(&request.profile_changes),
        sources,
        ctx,
        parent,
        factory,
        module,
        common,
        directory,
        live,
    )
}
