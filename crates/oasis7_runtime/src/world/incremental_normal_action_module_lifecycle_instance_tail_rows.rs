//! Real Current install-tail preflight. All business failures occur before the
//! governance Apply is written. The later writer uses these exact before rows.
use super::current::{CurrentModuleMarketAgentV1, read_market_agent_v1};
use super::instance_plan::CurrentInstallTailSeedV1;
use super::*;
use crate::world::{
    incremental_module_read::{ModuleReadContext, PreparedModuleInstanceFields},
    incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1,
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::incremental_value::{
    ValueNodeReader, ValueNodeWriter, ValueReadContext,
};
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_installed_module_targets::{
        InstalledModuleTargetReadV1, read_installed_module_target_overlay_v1,
    },
};

pub(super) struct PreparedCurrentModuleInstallRowsV1 {
    pub(super) agent: CurrentModuleMarketAgentV1,
    pub(super) after_stock: oasis7_agent_api::world_resource::ResourceStock,
    pub(super) world_before: Option<i64>,
    pub(super) world_after: i64,
    pub(super) fee_kind: oasis7_agent_api::world_action::ResourceKind,
    pub(super) fee_amount: i64,
    pub(super) target_before: Option<oasis7_agent_api::world_action::ModuleInstallTarget>,
    pub(super) before_instance: Option<ReservedValue<PreparedModuleInstanceFields>>,
    instance: oasis7_world_state::ModuleInstanceState,
    event_instance_id: String,
    stored_instance_key: String,
    pub(super) wake: Option<Option<u64>>,
    pub(super) rollback_freeze: Option<PreparedRollbackFreezeV1>,
    next_before: u64,
    next_after: u64,
    selected: usize,
    outcome: usize,
    action_id: u64,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    now: u64,
    _fee: MemoryReservation,
}
pub(super) struct PreparedRollbackFreezeV1 {
    pub(super) before: ReservedValue<
        oasis7_world_state::incremental_world_native::CurrentFactoryNativeRecordV1<
            oasis7_world_state::state::ModuleAdmissionFreeze,
        >,
    >,
    pub(super) key: String,
}
impl PreparedCurrentModuleInstallRowsV1 {
    pub(super) fn validate_selection(
        &self,
        selected: &SelectedPendingActionV1,
        outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    ) -> Result<(), BodyMutationError> {
        if self.selected != selected as *const _ as usize
            || self.outcome != outcome as *const _ as usize
            || self.action_id != selected.envelope().id
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub(super) fn stored_instance_key(&self) -> &str {
        &self.stored_instance_key
    }
    pub(super) fn event_instance_id(&self) -> &str {
        &self.event_instance_id
    }
    pub(super) fn instance(&self) -> &oasis7_world_state::ModuleInstanceState {
        &self.instance
    }
    pub(super) fn retained_heap(&self) -> usize {
        self._fee.bytes()
    }
    pub(super) fn next_before(&self) -> u64 {
        self.next_before
    }
    pub(super) fn next_after(&self) -> u64 {
        self.next_after
    }
    pub(super) fn validate_provenance<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        core: WorkIdentityV1,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.core != core
            || self.now != now
            || !self._fee.uses_budget(memory)
            || !self.agent._fee.uses_budget(memory)
        {
            return Err(invalid());
        }
        Ok(())
    }
}
pub(super) enum CurrentModuleInstallRowsDecisionV1 {
    Ready(PreparedCurrentModuleInstallRowsV1),
    Rejected {
        error: ReservedValue<crate::WorldError>,
    },
}

#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_current_module_install_rows_v1<M, P, R, W, D, E, C, PE, PC>(
    seed: &CurrentInstallTailSeedV1,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    sources: &ModuleLifecycleSourcesV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, R, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    module: &mut ModuleLifecycleWorkV1<'_>,
    factory: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<CurrentModuleInstallRowsDecisionV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    W: ValueNodeWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    use super::instance_plan::InstanceTailOperationV1;
    use oasis7_agent_api::world_action::ModuleInstallTarget as T;
    seed.validate(selected, outcome, step.transaction, common, live)?;
    let action = outcome
        .override_action_for_selected_v1(selected, step.transaction)?
        .unwrap_or(&selected.envelope().action);
    let (actor, manifest, active, target, replacement) = match (seed.operation(), action) {
        (
            InstanceTailOperationV1::ReleaseInstall { source },
            crate::Action::ModuleReleaseApply { .. }
            | crate::Action::ModuleReleaseApplyWithFinality { .. },
        ) => {
            let request = source.request();
            (
                request.requester_agent_id.as_str(),
                &request.manifest,
                request.activate,
                &request.install_target,
                None,
            )
        }
        (
            InstanceTailOperationV1::Install,
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
            },
        ) => (
            installer_agent_id.as_str(),
            manifest,
            *activate,
            &T::SelfAgent,
            None,
        ),
        (
            InstanceTailOperationV1::Install,
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
            },
        ) => (
            installer_agent_id.as_str(),
            manifest,
            *activate,
            install_target,
            None,
        ),
        (
            InstanceTailOperationV1::Upgrade { snapshot, .. },
            crate::Action::UpgradeModuleFromArtifact {
                upgrader_agent_id,
                from_module_version,
                manifest,
                activate,
                ..
            }
            | crate::Action::UpgradeModuleFromArtifactWithFinality {
                upgrader_agent_id,
                from_module_version,
                manifest,
                activate,
                ..
            },
        ) => (
            upgrader_agent_id.as_str(),
            manifest,
            *activate,
            &snapshot.install_target,
            Some((snapshot.instance_id.as_str(), from_module_version.as_str())),
        ),
        (
            InstanceTailOperationV1::Rollback {
                snapshot,
                target_manifest,
                ..
            },
            crate::Action::RollbackModuleInstance {
                operator_agent_id, ..
            }
            | crate::Action::RollbackModuleInstanceWithFinality {
                operator_agent_id, ..
            },
        ) => (
            operator_agent_id.as_str(),
            target_manifest,
            snapshot.active,
            &snapshot.install_target,
            Some((
                snapshot.instance_id.as_str(),
                snapshot.module_version.as_str(),
            )),
        ),
        _ => return Err(invalid()),
    };
    let rollback = matches!(seed.operation(), InstanceTailOperationV1::Rollback { .. });
    let from_hash_len = match seed.operation() {
        InstanceTailOperationV1::Rollback { from_wasm_hash, .. } => from_wasm_hash.len(),
        _ => 0,
    };
    let target_len = match target {
        T::SelfAgent => 0,
        T::LocationInfrastructure { location_id } => location_id.len(),
    };
    let text = actor
        .len()
        .checked_add(manifest.module_id.len())
        .and_then(|n| n.checked_add(manifest.version.len()))
        .and_then(|n| n.checked_add(manifest.wasm_hash.len()))
        .and_then(|n| n.checked_add(target_len))
        .and_then(|n| n.checked_add(from_hash_len))
        .and_then(|n| {
            n.checked_add(replacement.map_or(0, |(id, from)| id.len().saturating_add(from.len())))
        })
        .ok_or_else(invalid)?;
    let heap = text
        .checked_mul(8)
        .and_then(|n| n.checked_add(module.max_selected_heap))
        .and_then(|n| n.checked_add(16384))
        .ok_or_else(invalid)?;
    pay(common, heap)?;
    let mut fee = common
        .memory()
        .reserve(heap, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    if seed.fee_amount() < 0 {
        return Ok(CurrentModuleInstallRowsDecisionV1::Rejected {
            error: fee.retain(crate::WorldError::ResourceBalanceInvalid {
                reason: format!("module action fee must be >= 0, got {}", seed.fee_amount()),
            }),
        });
    }
    let now = step.time();
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
    let result =
        crate::world::incremental_normal_action_factory_schedule::with_module_lifecycle_read_io_v1(
            sources.families.roots,
            &mut ctx,
            parent,
            factory,
            common,
            module,
            |ctx, parent, factory, module| {
                let Some(agent) = read_market_agent_v1(actor, sources, ctx, parent, factory, live)?
                else {
                    return Ok(Err(crate::WorldError::AgentNotFound {
                        agent_id: actor.to_owned(),
                    }));
                };
                let guard = ctx.transaction.guard();
                let world_before=oasis7_world_state::incremental_inventory_resource_overlay::read_current_world_resource_v1(seed.fee_kind(),ctx,&guard,live).map_err(overlay_error)?;
                let mut after_stock = agent.stock.clone();
                let mut last = agent.body.last_active;
                let world_after = match oasis7_world_state::settle_module_action_fee_from_records_v1(
                    &mut after_stock,
                    &mut last,
                    world_before.unwrap_or(0),
                    actor,
                    seed.fee_kind(),
                    seed.fee_amount(),
                    now,
                ) {
                    Ok(v) => v,
                    Err(error) => return Ok(Err(error)),
                };
                let next_before=oasis7_world_state::incremental_world_u64::read_world_u64_overlay_v1(oasis7_world_state::incremental_world_u64::WorldU64FieldV1::NextModuleInstanceId,ctx).map_err(overlay_error)?;
                use std::fmt::Write as _;
                let event_instance_id = if let Some((id, _)) = replacement {
                    id.to_owned()
                } else {
                    let mut id = String::with_capacity(
                        manifest
                            .module_id
                            .len()
                            .checked_add(21)
                            .ok_or_else(invalid)?,
                    );
                    write!(&mut id, "{}#{}", manifest.module_id, next_before.max(1))
                        .map_err(|_| invalid())?;
                    id
                };
                // Upgrade follows the original second lookup at the *inner* Domain ID. It
                // must not reuse the action's outer lookup or invent an equality constraint.
                let stored_instance_key = if replacement.is_some() {
                    event_instance_id.clone()
                } else {
                    event_instance_id.trim().to_owned()
                };
                let before_instance = {
                    let mut read = ModuleReadContext {
                        qualification: sources.families.instances,
                        values: ctx.values.source,
                        memory: ctx.values.memory,
                        io: &mut *ctx.values.io,
                        max_value_bytes: module.max_selected_heap as u64,
                        cancelled: &mut ctx.cancelled,
                    };
                    read.read_instance(&stored_instance_key, ctx.transaction)?
                };
                let mut location = Vec::with_capacity(module.max_selected_heap);
                let target_before = match read_installed_module_target_overlay_v1(
                    &manifest.module_id,
                    ctx,
                    module.max_selected_heap as u64,
                    &mut location,
                )
                .map_err(overlay_error)?
                {
                    None => None,
                    Some(InstalledModuleTargetReadV1::SelfAgent) => Some(T::SelfAgent),
                    Some(InstalledModuleTargetReadV1::LocationInfrastructure { utf8_bytes }) => {
                        if utf8_bytes != location.len() as u64 {
                            return Err(invalid().into());
                        }
                        Some(T::LocationInfrastructure {
                            location_id: String::from_utf8(location).map_err(|_| invalid())?,
                        })
                    }
                };
                Ok(Ok((
                    agent,
                    after_stock,
                    world_before,
                    world_after,
                    next_before,
                    event_instance_id,
                    stored_instance_key,
                    before_instance,
                    target_before,
                )))
            },
        );
    let rows = match result {
        Ok(v) => v,
        Err(FactoryScheduleReadErrorV1::Body(error)) => return Err(error),
        Err(FactoryScheduleReadErrorV1::World(error)) => Err(error),
    };
    let (
        agent,
        after_stock,
        world_before,
        world_after,
        next_before,
        event_instance_id,
        stored_instance_key,
        before_instance,
        target_before,
    ) = match rows {
        Ok(v) => v,
        Err(error) => {
            return Ok(CurrentModuleInstallRowsDecisionV1::Rejected {
                error: fee.retain(error),
            });
        }
    };
    let (instance, next_after) = if let Some((_, from)) = replacement {
        let before = before_instance
            .as_ref()
            .map(|row| row.snapshot_paid_v1(common))
            .transpose()?;
        let copy_heap = before
            .as_ref()
            .map_or(0, |row| row.reserved_bytes())
            .checked_add(text.checked_mul(4).ok_or_else(invalid)?)
            .and_then(|n| n.checked_add(8192))
            .ok_or_else(invalid)?;
        pay(common, copy_heap)?;
        let mut copied = common
            .memory()
            .reserve(copy_heap, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        fee.merge(&mut copied).map_err(BodyMutationError::Invalid)?;
        let after=oasis7_world_state::state::module_instance_transition::prepare_replaced_module_record_v1(before.as_deref(),&event_instance_id,&manifest.module_id,from,&manifest.version,&manifest.wasm_hash,target,active,actor,if rollback{oasis7_world_state::state::module_instance_transition::ModuleInstanceReplacementKindV1::Rollback}else{oasis7_world_state::state::module_instance_transition::ModuleInstanceReplacementKindV1::Upgrade});
        match after {
            Ok(instance) => (instance, next_before),
            Err(error) => {
                return Ok(CurrentModuleInstallRowsDecisionV1::Rejected {
                    error: fee.retain(error),
                });
            }
        }
    } else {
        let(instance,_,next)=oasis7_world_state::state::module_instance_transition::prepare_installed_module_record_v1(&event_instance_id,&manifest.module_id,&manifest.version,&manifest.wasm_hash,actor,target,active,now,next_before);
        (instance, next)
    };
    let rollback_freeze = if let InstanceTailOperationV1::Rollback {
        snapshot,
        from_wasm_hash,
        ..
    } = seed.operation()
    {
        use oasis7_world_state::incremental_module_instance_fields::{
            ModuleInstanceFieldV1 as F, ModuleInstanceScalarRefV1 as S,
        };
        let previous = before_instance.as_ref().ok_or_else(invalid)?;
        let S::Text(actual_hash) = previous.field(F::WasmHash) else {
            return Err(invalid());
        };
        pay(common, actual_hash.len().min(from_wasm_hash.len()))?;
        // The original empty-hash fallback is the exact FROM registry record
        // already read by this private rollback prelude. No intermediate stage
        // routes actions or writes that registry row; Apply only activates TO.
        if from_wasm_hash.is_empty() || (!actual_hash.is_empty() && actual_hash != from_wasm_hash) {
            return Ok(CurrentModuleInstallRowsDecisionV1::Rejected {
                error: fee.retain(crate::WorldError::ResourceBalanceInvalid {
                    reason: "rollback FROM artifact identity mismatch".to_owned(),
                }),
            });
        }
        let key = oasis7_world_state::state::ModuleAdmissionFreeze::key(
            &snapshot.module_id,
            &snapshot.module_version,
            from_wasm_hash,
        );
        common
            .charge(24, 0)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let descriptor =
            oasis7_world_state::incremental_world_native::world_native_field_descriptors_v1()?
                .into_iter()
                .find(|field| field.name == "module_admission_freezes")
                .ok_or_else(invalid)?;
        let oasis7_world_state::incremental_field_member::FieldDestination::State(namespace) =
            descriptor.destination
        else {
            return Err(invalid());
        };
        let before = crate::world::incremental_normal_action_factory_schedule::with_module_lifecycle_read_io_v1(sources.families.roots, &mut ctx, parent, factory, common, module, |ctx, parent, factory, _| {
            let guard = ctx.transaction.guard();
            let mut read = WorldFieldReadContextV1 { root: sources.families.roots.state_root(namespace), state: parent.state, values: ValueReadContext { source: ctx.values.source, memory: ctx.values.memory, class: ctx.values.class, io: &mut *ctx.values.io }, key_work: &mut *ctx.key_work, semantic_bytes: ctx.semantic_bytes, record_bytes: ctx.record_bytes, take_state_error: &mut ctx.take_state_error, cancelled: &mut ctx.cancelled };
            Ok(oasis7_world_state::incremental_world_native::read_current_module_admission_freeze_entry_v1(&key, sources.factory.lifecycle.native, sources.families.roots, ctx.transaction, &mut read, &mut factory.lifecycle.native_scan, &mut factory.lifecycle.native_read, &guard, live).map_err(overlay_error)?)
        }).map_err(|error| match error { FactoryScheduleReadErrorV1::Body(error) => error, FactoryScheduleReadErrorV1::World(_) => invalid() })?;
        Some(PreparedRollbackFreezeV1 { before, key })
    } else {
        None
    };
    Ok(CurrentModuleInstallRowsDecisionV1::Ready(
        PreparedCurrentModuleInstallRowsV1 {
            agent,
            after_stock,
            world_before,
            world_after,
            fee_kind: seed.fee_kind(),
            fee_amount: seed.fee_amount(),
            target_before,
            before_instance,
            instance,
            event_instance_id,
            stored_instance_key,
            rollback_freeze,
            wake: if rollback {
                None
            } else {
                Some(
                    if active
                        && manifest
                            .subscriptions
                            .iter()
                            .any(|s| s.resolved_stage() == crate::ModuleSubscriptionStage::Tick)
                    {
                        Some(now)
                    } else {
                        None
                    },
                )
            },
            next_before,
            next_after,
            selected: selected as *const _ as usize,
            outcome: outcome as *const _ as usize,
            action_id: selected.envelope().id,
            guard: ctx.transaction.guard(),
            core: common.work().identity(),
            now,
            _fee: fee,
        },
    ))
}
