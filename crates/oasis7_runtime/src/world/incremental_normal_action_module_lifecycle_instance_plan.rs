//! Private install tail source: genuine selected prelude, actual primitive29
//! sequence, and either unchanged Current Manifest or actual governed Apply.
use super::*;
use super::{
    governance_finality::ValidatedCurrentModuleApplyV1,
    instance_current::ModuleInstanceFactsFeesV1,
    instance_execute::CurrentSelectedInstallDecisionV1,
};

pub(super) enum InstanceTailOperationV1 {
    Install,
    ReleaseInstall {
        source: Box<super::release_facts::ValidatedCurrentReleaseApplyV1>,
    },
    Upgrade {
        instance:
            ReservedValue<crate::world::incremental_module_read::PreparedModuleInstanceFields>,
        snapshot: ReservedValue<crate::ModuleInstanceState>,
    },
    Rollback {
        instance:
            ReservedValue<crate::world::incremental_module_read::PreparedModuleInstanceFields>,
        snapshot: ReservedValue<crate::ModuleInstanceState>,
        target_source: ReservedValue<
            oasis7_world_state::incremental_runtime_artifact_config::SelectedModuleManifestV1,
        >,
        target_manifest: crate::ModuleManifest,
        from_wasm_hash: String,
        from_release_request_id: Option<u64>,
    },
}
pub(super) struct CurrentInstallTailSeedV1 {
    operation: InstanceTailOperationV1,
    facts: Option<ModuleInstanceFactsFeesV1>,
    fee_kind: oasis7_agent_api::world_action::ResourceKind,
    fee_amount: i64,
    selected: usize,
    outcome: usize,
    action_id: u64,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    _fee: MemoryReservation,
}
impl CurrentInstallTailSeedV1 {
    pub(super) fn action_id(&self) -> u64 {
        self.action_id
    }
    pub(super) fn operation(&self) -> &InstanceTailOperationV1 {
        &self.operation
    }
    pub(super) fn fee_kind(&self) -> oasis7_agent_api::world_action::ResourceKind {
        self.fee_kind
    }
    pub(super) fn fee_amount(&self) -> i64 {
        self.fee_amount
    }
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        selected: &SelectedPendingActionV1,
        outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
        tx: &ExecutionTransaction<'_, M>,
        common: &FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        selected.validate(tx)?;
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if let InstanceTailOperationV1::ReleaseInstall { source } = &self.operation {
            source.validate_provenance(
                selected,
                outcome,
                tx,
                common.memory(),
                common.work().identity(),
                source.now(),
                live,
            )?;
        }
        if let InstanceTailOperationV1::Upgrade { instance, snapshot }
        | InstanceTailOperationV1::Rollback {
            instance, snapshot, ..
        } = &self.operation
            && (!instance.uses_budget(common.memory()) || !snapshot.uses_budget(common.memory()))
        {
            return Err(invalid());
        }
        if let InstanceTailOperationV1::Rollback { target_source, .. } = &self.operation
            && !target_source.uses_budget(common.memory())
        {
            return Err(invalid());
        }
        if self.selected != selected as *const _ as usize
            || self.outcome != outcome as *const _ as usize
            || self.action_id != selected.envelope().id
            || self.core != common.work().identity()
            || !self._fee.uses_budget(common.memory())
            || !outcome.uses_budget(common.memory())
        {
            return Err(invalid());
        }
        Ok(())
    }
}
fn install_fields<'a>(
    action: &'a crate::Action,
    operation: &'a InstanceTailOperationV1,
) -> Result<
    (
        &'a str,
        &'a crate::ModuleManifest,
        bool,
        Option<&'a oasis7_agent_api::world_action::ModuleInstallTarget>,
    ),
    BodyMutationError,
> {
    if let InstanceTailOperationV1::ReleaseInstall { source } = operation {
        if !matches!(
            action,
            crate::Action::ModuleReleaseApply { .. }
                | crate::Action::ModuleReleaseApplyWithFinality { .. }
        ) {
            return Err(invalid());
        }
        let request = source.request();
        return Ok((
            &request.requester_agent_id,
            &request.manifest,
            request.activate,
            Some(&request.install_target),
        ));
    }
    match action {
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
        } => Ok((installer_agent_id, manifest, *activate, None)),
        crate::Action::InstallModuleToTargetFromArtifact {
            installer_agent_id,
            manifest,
            activate,
            install_target,
        }
        | crate::Action::InstallModuleToTargetFromArtifactWithFinality {
            installer_agent_id,
            manifest,
            activate,
            install_target,
            ..
        } => Ok((
            installer_agent_id,
            manifest,
            *activate,
            Some(install_target),
        )),
        crate::Action::UpgradeModuleFromArtifact {
            upgrader_agent_id,
            manifest,
            activate,
            ..
        }
        | crate::Action::UpgradeModuleFromArtifactWithFinality {
            upgrader_agent_id,
            manifest,
            activate,
            ..
        } => match operation {
            InstanceTailOperationV1::Upgrade { snapshot, .. } => Ok((
                upgrader_agent_id,
                manifest,
                *activate,
                Some(&snapshot.install_target),
            )),
            _ => Err(invalid()),
        },
        crate::Action::RollbackModuleInstance {
            operator_agent_id, ..
        }
        | crate::Action::RollbackModuleInstanceWithFinality {
            operator_agent_id, ..
        } => match operation {
            InstanceTailOperationV1::Rollback {
                snapshot,
                target_manifest,
                ..
            } => Ok((
                operator_agent_id,
                target_manifest,
                snapshot.active,
                Some(&snapshot.install_target),
            )),
            _ => Err(invalid()),
        },
        _ => Err(invalid()),
    }
}
fn seed<M: MerkleNodeReader>(
    operation: InstanceTailOperationV1,
    facts: ModuleInstanceFactsFeesV1,
    fee_kind: oasis7_agent_api::world_action::ResourceKind,
    fee_amount: i64,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<CurrentInstallTailSeedV1, BodyMutationError> {
    selected.validate(tx)?;
    tx.validate_guard(&tx.guard(), live)?;
    let action = outcome
        .override_action_for_selected_v1(selected, tx)?
        .unwrap_or(&selected.envelope().action);
    let (_, manifest, _, _) = install_fields(action, &operation)?;
    if fee_kind != oasis7_agent_api::world_action::ResourceKind::Electricity
        || fee_amount != crate::World::module_install_fee_amount(manifest)
    {
        return Err(invalid());
    }
    pay(common, std::mem::size_of::<CurrentInstallTailSeedV1>())?;
    let fee = common
        .memory()
        .reserve(
            std::mem::size_of::<CurrentInstallTailSeedV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    Ok(CurrentInstallTailSeedV1 {
        operation,
        facts: Some(facts),
        fee_kind,
        fee_amount,
        selected: selected as *const _ as usize,
        outcome: outcome as *const _ as usize,
        action_id: selected.envelope().id,
        guard: tx.guard(),
        core: common.work().identity(),
        _fee: fee,
    })
}
pub(super) fn take_governed_install_seed_v1<M: MerkleNodeReader>(
    decision: CurrentSelectedInstallDecisionV1,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<
    (
        super::governance_plan::PreparedModuleManifestUpdateV1,
        CurrentInstallTailSeedV1,
    ),
    BodyMutationError,
> {
    let CurrentSelectedInstallDecisionV1::Governed {
        update,
        facts,
        fee_kind,
        fee_amount,
    } = decision
    else {
        return Err(invalid());
    };
    Ok((
        update,
        seed(
            InstanceTailOperationV1::Install,
            facts,
            fee_kind,
            fee_amount,
            selected,
            outcome,
            tx,
            common,
            live,
        )?,
    ))
}
pub(super) fn take_governed_upgrade_seed_v1<M: MerkleNodeReader>(
    decision: super::upgrade_execute::CurrentSelectedUpgradeDecisionV1,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<
    (
        super::governance_plan::PreparedModuleManifestUpdateV1,
        CurrentInstallTailSeedV1,
    ),
    BodyMutationError,
> {
    let super::upgrade_execute::CurrentSelectedUpgradeDecisionV1::Governed {
        update,
        instance,
        snapshot,
        facts,
        fee_kind,
        fee_amount,
    } = decision
    else {
        return Err(invalid());
    };
    Ok((
        update,
        seed(
            InstanceTailOperationV1::Upgrade { instance, snapshot },
            facts,
            fee_kind,
            fee_amount,
            selected,
            outcome,
            tx,
            common,
            live,
        )?,
    ))
}
pub(super) fn take_governed_rollback_seed_v1<M: MerkleNodeReader>(
    decision: super::rollback_execute::CurrentSelectedRollbackDecisionV1,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<
    (
        super::governance_plan::PreparedModuleManifestUpdateV1,
        CurrentInstallTailSeedV1,
    ),
    BodyMutationError,
> {
    let super::rollback_execute::CurrentSelectedRollbackDecisionV1::Governed {
        update,
        instance,
        snapshot,
        target_source,
        target_manifest,
        from_wasm_hash,
        from_release_request_id,
        facts,
        fee_kind,
        fee_amount,
    } = decision
    else {
        return Err(invalid());
    };
    Ok((
        update,
        seed(
            InstanceTailOperationV1::Rollback {
                instance,
                snapshot,
                target_source,
                target_manifest,
                from_wasm_hash,
                from_release_request_id,
            },
            facts,
            fee_kind,
            fee_amount,
            selected,
            outcome,
            tx,
            common,
            live,
        )?,
    ))
}
pub(super) fn take_unchanged_install_seed_v1<M: MerkleNodeReader>(
    decision: CurrentSelectedInstallDecisionV1,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<(CurrentInstallTailSeedV1, CurrentInstallGovernanceSourceV1), BodyMutationError> {
    let CurrentSelectedInstallDecisionV1::NoGovernance {
        manifest,
        hash,
        facts,
        fee_kind,
        fee_amount,
    } = decision
    else {
        return Err(invalid());
    };
    manifest.validate(tx, common, live)?;
    let seed = seed(
        InstanceTailOperationV1::Install,
        facts,
        fee_kind,
        fee_amount,
        selected,
        outcome,
        tx,
        common,
        live,
    )?;
    Ok((
        seed,
        CurrentInstallGovernanceSourceV1::Unchanged { manifest, hash },
    ))
}

pub(super) enum CurrentInstallGovernanceSourceV1 {
    Unchanged {
        manifest: super::governance_manifest::CurrentModuleWorldManifestV1,
        hash: ReservedValue<String>,
    },
    Apply(ValidatedCurrentModuleApplyV1),
}
/// The writer consumes governance separately, retaining this entire tail while
/// applying its exact expected rows. No optional authority gap is exposed.
pub(super) struct CurrentModuleInstallTailV1 {
    pub(super) event: ReservedValue<crate::DomainEvent>,
    pub(super) rows: super::instance_tail_rows::PreparedCurrentModuleInstallRowsV1,
    pub(super) seed: CurrentInstallTailSeedV1,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    now: u64,
    _fee: MemoryReservation,
}
pub(super) struct SelectedCurrentModuleInstallPlanV1 {
    pub(super) governance: CurrentInstallGovernanceSourceV1,
    pub(super) tail: CurrentModuleInstallTailV1,
}
impl CurrentModuleInstallTailV1 {
    pub(super) fn validate_evidence_v1<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        core: WorkIdentityV1,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if let InstanceTailOperationV1::ReleaseInstall { source } = &self.seed.operation {
            source.validate_evidence(tx, memory, core, now, live)?;
        }
        if self.core != core
            || self.now != now
            || !self._fee.uses_budget(memory)
            || !self.event.uses_budget(memory)
            || !self.seed._fee.uses_budget(memory)
        {
            return Err(invalid());
        }
        self.rows.validate_provenance(tx, memory, core, now, live)
    }
}
impl SelectedCurrentModuleInstallPlanV1 {
    pub(super) fn validate_current_v1<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        tail: &crate::world::incremental_normal_tail::NormalTailWorkV1<'_, '_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_guard(&self.tail.guard, live)?;
        self.tail
            .validate_evidence_v1(tx, tail.memory(), tail.identity(), now, live)?;
        match &self.governance {
            CurrentInstallGovernanceSourceV1::Unchanged { manifest, hash } => {
                manifest.validate_evidence_v1(tx, tail.memory(), live)?;
                if !hash.uses_budget(tail.memory()) {
                    return Err(invalid());
                }
            }
            CurrentInstallGovernanceSourceV1::Apply(source) => {
                source.validate_current_v1(tx, tail, now, live)?
            }
        }
        Ok(())
    }
}
struct InstallCopyCauseV1 {
    error: serde_json::Error,
    _fee: MemoryReservation,
}
impl std::fmt::Debug for InstallCopyCauseV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.error, f)
    }
}
impl std::fmt::Display for InstallCopyCauseV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.error, f)
    }
}
impl std::error::Error for InstallCopyCauseV1 {}

/// Run only after the actual rows draft, then finality. This phase performs no
/// transaction reads/writes: the finality strict guard stays the actual cut.
#[allow(clippy::too_many_arguments)]
pub(super) fn bind_current_install_tail_v1<M: MerkleNodeReader>(
    seed: CurrentInstallTailSeedV1,
    rows: super::instance_tail_rows::PreparedCurrentModuleInstallRowsV1,
    governance: CurrentInstallGovernanceSourceV1,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<SelectedCurrentModuleInstallPlanV1, BodyMutationError> {
    seed.validate(selected, outcome, tx, common, live)?;
    rows.validate_selection(selected, outcome)?;
    rows.validate_provenance(tx, common.memory(), common.work().identity(), now, live)?;
    let action = outcome
        .override_action_for_selected_v1(selected, tx)?
        .unwrap_or(&selected.envelope().action);
    let (actor, manifest, activate, target) = install_fields(action, seed.operation())?;
    let (proposal_id, manifest_hash) = match &governance {
        CurrentInstallGovernanceSourceV1::Unchanged { manifest, hash } => {
            manifest.validate_evidence_v1(tx, common.memory(), live)?;
            (0, hash.as_str())
        }
        CurrentInstallGovernanceSourceV1::Apply(source) => {
            {
                let tail = crate::world::incremental_normal_tail::NormalTailWorkV1::new(common);
                source.validate_current_v1(tx, &tail, now, live)?;
            }
            (
                source.prefix.proposal.original_id(),
                source.prefix.applied_hash.as_str(),
            )
        }
    };
    let target = target.unwrap_or(&oasis7_agent_api::world_action::ModuleInstallTarget::SelfAgent);
    let measured = match super::governance_json::module_json_value_paid_v1(
        &(
            actor,
            manifest,
            target,
            manifest_hash,
            rows.event_instance_id(),
            match seed.operation() {
                InstanceTailOperationV1::Install
                | InstanceTailOperationV1::ReleaseInstall { .. } => None,
                InstanceTailOperationV1::Upgrade { snapshot, .. }
                | InstanceTailOperationV1::Rollback { snapshot, .. } => Some(&**snapshot),
            },
            match seed.operation() {
                InstanceTailOperationV1::Rollback { from_wasm_hash, .. } => {
                    Some(from_wasm_hash.as_str())
                }
                _ => None,
            },
        ),
        common,
    ) {
        Ok(v) => v,
        Err(super::governance_json::ModuleGovernanceJsonErrorV1::Body(e)) => return Err(e),
        Err(super::governance_json::ModuleGovernanceJsonErrorV1::Json { error, _fee }) => {
            return Err(BodyMutationError::Original(Box::new(InstallCopyCauseV1 {
                error,
                _fee,
            })));
        }
    };
    let bytes = measured
        .fee
        .bytes()
        .checked_mul(2)
        .and_then(|n| n.checked_add(std::mem::size_of::<crate::DomainEvent>()))
        .ok_or_else(invalid)?;
    pay(common, bytes)?;
    let fee = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let event = fee.retain(match (seed.operation(), action) {
        (
            InstanceTailOperationV1::Install,
            crate::Action::InstallModuleFromArtifact { .. }
            | crate::Action::InstallModuleFromArtifactWithFinality { .. }
            | crate::Action::InstallModuleToTargetFromArtifact { .. }
            | crate::Action::InstallModuleToTargetFromArtifactWithFinality { .. },
        ) => crate::DomainEvent::ModuleInstalled {
            installer_agent_id: actor.to_owned(),
            instance_id: rows.event_instance_id().to_owned(),
            module_id: manifest.module_id.clone(),
            module_version: manifest.version.clone(),
            wasm_hash: manifest.wasm_hash.clone(),
            install_target: target.clone(),
            active: activate,
            proposal_id,
            manifest_hash: manifest_hash.to_owned(),
            fee_kind: seed.fee_kind,
            fee_amount: seed.fee_amount,
        },
        (
            InstanceTailOperationV1::ReleaseInstall { .. },
            crate::Action::ModuleReleaseApply { .. }
            | crate::Action::ModuleReleaseApplyWithFinality { .. },
        ) => crate::DomainEvent::ModuleInstalled {
            installer_agent_id: actor.to_owned(),
            instance_id: rows.event_instance_id().to_owned(),
            module_id: manifest.module_id.clone(),
            module_version: manifest.version.clone(),
            wasm_hash: manifest.wasm_hash.clone(),
            install_target: target.clone(),
            active: activate,
            proposal_id,
            manifest_hash: manifest_hash.to_owned(),
            fee_kind: seed.fee_kind,
            fee_amount: seed.fee_amount,
        },
        (
            InstanceTailOperationV1::Upgrade { snapshot, .. },
            crate::Action::UpgradeModuleFromArtifact {
                from_module_version,
                ..
            }
            | crate::Action::UpgradeModuleFromArtifactWithFinality {
                from_module_version,
                ..
            },
        ) => crate::DomainEvent::ModuleUpgraded {
            upgrader_agent_id: actor.to_owned(),
            instance_id: rows.event_instance_id().to_owned(),
            module_id: snapshot.module_id.clone(),
            from_module_version: from_module_version.clone(),
            to_module_version: manifest.version.clone(),
            wasm_hash: manifest.wasm_hash.clone(),
            install_target: target.clone(),
            active: activate,
            proposal_id,
            manifest_hash: manifest_hash.to_owned(),
            fee_kind: seed.fee_kind,
            fee_amount: seed.fee_amount,
        },
        (
            InstanceTailOperationV1::Rollback {
                snapshot,
                target_manifest,
                from_wasm_hash,
                from_release_request_id,
                ..
            },
            crate::Action::RollbackModuleInstance {
                target_module_version,
                ..
            }
            | crate::Action::RollbackModuleInstanceWithFinality {
                target_module_version,
                ..
            },
        ) => crate::DomainEvent::ModuleRollbackApplied {
            operator_agent_id: actor.to_owned(),
            instance_id: rows.event_instance_id().to_owned(),
            module_id: snapshot.module_id.clone(),
            from_module_version: snapshot.module_version.clone(),
            to_module_version: target_module_version.trim().to_owned(),
            wasm_hash: target_manifest.wasm_hash.clone(),
            from_wasm_hash: Some(from_wasm_hash.clone()),
            from_release_request_id: *from_release_request_id,
            install_target: target.clone(),
            active: snapshot.active,
            proposal_id,
            manifest_hash: manifest_hash.to_owned(),
            fee_kind: seed.fee_kind,
            fee_amount: seed.fee_amount,
        },
        _ => return Err(invalid()),
    });
    drop(measured);
    pay(common, std::mem::size_of::<CurrentModuleInstallTailV1>())?;
    let layout = common
        .memory()
        .reserve(
            std::mem::size_of::<CurrentModuleInstallTailV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    Ok(SelectedCurrentModuleInstallPlanV1 {
        governance,
        tail: CurrentModuleInstallTailV1 {
            event,
            rows,
            seed,
            guard: tx.guard(),
            core: common.work().identity(),
            now,
            _fee: layout,
        },
    })
}

pub(super) fn take_release_install_seed_v1<M: MerkleNodeReader>(
    source: super::release_facts::ValidatedCurrentReleaseApplyV1,
    install_facts: ModuleInstanceFactsFeesV1,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<CurrentInstallTailSeedV1, BodyMutationError> {
    source.validate_provenance(
        selected,
        outcome,
        tx,
        common.memory(),
        common.work().identity(),
        source.now(),
        live,
    )?;
    let amount = crate::World::module_install_fee_amount(&source.request().manifest);
    pay(
        common,
        std::mem::size_of::<super::release_facts::ValidatedCurrentReleaseApplyV1>()
            .checked_add(std::mem::size_of::<CurrentInstallTailSeedV1>())
            .ok_or_else(invalid)?,
    )?;
    let box_fee = common
        .memory()
        .reserve(
            std::mem::size_of::<super::release_facts::ValidatedCurrentReleaseApplyV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut layout = common
        .memory()
        .reserve(
            std::mem::size_of::<CurrentInstallTailSeedV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut box_fee = box_fee;
    layout
        .merge(&mut box_fee)
        .map_err(BodyMutationError::Invalid)?;
    Ok(CurrentInstallTailSeedV1 {
        operation: InstanceTailOperationV1::ReleaseInstall {
            source: Box::new(source),
        },
        facts: Some(install_facts),
        fee_kind: oasis7_agent_api::world_action::ResourceKind::Electricity,
        fee_amount: amount,
        selected: selected as *const _ as usize,
        outcome: outcome as *const _ as usize,
        action_id: selected.envelope().id,
        guard: tx.guard(),
        core: common.work().identity(),
        _fee: layout,
    })
}
