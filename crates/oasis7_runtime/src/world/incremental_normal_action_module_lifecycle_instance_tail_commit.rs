//! One fixed install commit: prevalidated business rows, exact Current CAS,
//! governed Apply and all originals remain inside the sole transaction.
use super::event::{ModuleLifecycleCommittedStateV1, PreparedNormalModuleLifecycleEventV1};
use super::governance_apply::{
    CommittedAppliedModuleStateV1, apply_validated_current_module_v1,
    publish_applied_module_events_v1,
};
use super::instance_plan::{
    CurrentInstallGovernanceSourceV1, CurrentModuleInstallTailV1,
    SelectedCurrentModuleInstallPlanV1,
};
use super::*;
use crate::world::{
    incremental_action_execution::SelectedActionAuditV1,
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::incremental_value::{
    ValueNodeReader, ValueNodeWriter, ValueReadContext,
};
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
};

pub(super) type CommittedCurrentModuleInstallV1 = PreparedNormalModuleLifecycleEventV1;
pub(super) struct CommittedModuleInstallStateV1 {
    tail: CurrentModuleInstallTailV1,
    applied: Option<CommittedAppliedModuleStateV1>,
    unchanged: Option<(
        super::governance_manifest::CurrentModuleWorldManifestV1,
        ReservedValue<String>,
    )>,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    now: u64,
    _fee: MemoryReservation,
}
impl CommittedModuleInstallStateV1 {
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        now: u64,
        core: WorkIdentityV1,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        self.tail
            .validate_evidence_v1(tx, memory, core, now, live)?;
        if self.core != core || self.now != now || !self._fee.uses_budget(memory) {
            return Err(invalid());
        }
        if let Some(applied) = &self.applied {
            applied.validate(tx, memory, core, now, live)?;
        }
        if let Some((manifest, hash)) = &self.unchanged {
            manifest.validate_evidence_v1(tx, memory, live)?;
            if !hash.uses_budget(memory) {
                return Err(invalid());
            }
        }
        Ok(())
    }
    pub(super) fn action_id(&self) -> u64 {
        self.tail.seed.action_id()
    }
    pub(super) fn release_source(
        &self,
    ) -> Option<&super::release_facts::ValidatedCurrentReleaseApplyV1> {
        match self.tail.seed.operation() {
            super::instance_plan::InstanceTailOperationV1::ReleaseInstall { source } => {
                Some(source)
            }
            _ => None,
        }
    }
    pub(super) fn event(&self) -> &crate::DomainEvent {
        &self.tail.event
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn commit_current_module_install_plan_v1<M, P, V, W, D, E, C, PE, PC, CW, ME, F>(
    plan: SelectedCurrentModuleInstallPlanV1,
    accepted: Option<SelectedActionAuditV1>,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    sources: &ModuleLifecycleSourcesV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    module: &mut ModuleLifecycleWorkV1<'_>,
    factory: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    max_event_bytes: usize,
    persist: &mut F,
    live: &ExecutionFence,
) -> Result<CommittedCurrentModuleInstallV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader
        + ValueNodeWriter
        + oasis7_formal_protocol::incremental_state::StatePageWriter,
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
        let now = step.time();
        plan.tail.rows.validate_selection(selected, outcome)?;
        {
            let loan = crate::world::incremental_normal_tail::NormalTailWorkV1::new(common);
            plan.validate_current_v1(step.transaction, &loan, now, live)?;
        }
        selected.validate(step.transaction)?;
        let SelectedCurrentModuleInstallPlanV1 { governance, tail } = plan;
        let (applied, unchanged) = match governance {
            CurrentInstallGovernanceSourceV1::Apply(source) => (
                Some(apply_validated_current_module_v1(
                    selected, outcome, source, sources, step, read, parent, write, materials,
                    module, factory, common, live,
                )?),
                None,
            ),
            CurrentInstallGovernanceSourceV1::Unchanged { manifest, hash } => {
                (None, Some((manifest, hash)))
            }
        };
        let state_fee = common
            .memory()
            .reserve(
                std::mem::size_of::<CommittedModuleInstallStateV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
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
            |step, read, parent, write, _materials, _factory, _module, work| {
                tail.validate_evidence_v1(
                    step.transaction,
                    work.memory(),
                    work.identity(),
                    now,
                    live,
                )?;
                work.charge(tail.rows.retained_heap())?;
                let rows = &tail.rows;
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
                // Only CAS the original fee changes; business debit validation already ran
                // before Apply, so a later mismatch is a failed whole transaction.
                if rows.fee_amount > 0 {
                    let guard = ctx.transaction.guard();
                    oasis7_world_state::incremental_inventory_resource_overlay::change_current_world_resource_v1(rows.fee_kind,rows.world_before,Some(rows.world_after),&mut ctx,write,&guard,live).map_err(overlay_error)?;
                }
                super::commit::write_market_agent_v1(
                    &rows.agent,
                    &rows.after_stock,
                    now,
                    sources,
                    &mut ctx,
                    write,
                    live,
                )?;
                let id = rows.stored_instance_key();
                {
                    let mut reader = crate::world::incremental_module_read::ModuleReadContext {
                        qualification: sources.families.instances,
                        values: ctx.values.source,
                        memory: ctx.values.memory,
                        io: &mut *ctx.values.io,
                        max_value_bytes: rows.retained_heap() as u64,
                        cancelled: &mut ctx.cancelled,
                    };
                    let actual = reader.read_instance(id, ctx.transaction)?;
                    use oasis7_world_state::incremental_module_instance_fields::ModuleInstanceFieldV1;
                    let equal = match (&rows.before_instance, &actual) {
                        (None, None) => true,
                        (Some(expected), Some(actual)) => ModuleInstanceFieldV1::ALL
                            .into_iter()
                            .all(|field| expected.field(field) == actual.field(field)),
                        _ => false,
                    };
                    if !equal {
                        return Err(invalid());
                    }
                }
                {
                    let mut mutation = crate::world::incremental_body::BodyMutationContext {
                        sink: &mut *write.sink,
                        memory: write.memory,
                        io: &mut *write.io,
                        cancelled: || (ctx.cancelled)() || (write.cancelled)(),
                    };
                    crate::world::incremental_module_instance_change::contribute_module_instance_change(sources.families.instances,id,Some(rows.instance()),ctx.transaction,ctx.values.source,&mut mutation,rows.retained_heap() as u64)?;
                }
                use oasis7_world_state::incremental_installed_module_targets::{
                    InstalledModuleTargetChangeV1, change_installed_module_target_overlay_v1,
                };
                let guard = ctx.transaction.guard();
                change_installed_module_target_overlay_v1(
                    InstalledModuleTargetChangeV1 {
                        key: &rows.instance().module_id,
                        expected: rows.target_before.as_ref().map(target_ref),
                        after: Some(target_ref(&rows.instance().install_target)),
                        max_current_location_bytes: rows.retained_heap() as u64,
                    },
                    &mut ctx,
                    write,
                    &guard,
                    live,
                )
                .map_err(overlay_error)?;
                let guard = ctx.transaction.guard();
                oasis7_world_state::incremental_world_u64::change_world_u64_overlay_v1(oasis7_world_state::incremental_world_u64::WorldU64FieldV1::NextModuleInstanceId,rows.next_before(),rows.next_after(),&mut ctx,write,&guard,live).map_err(overlay_error)?;
                // The original schedule uses raw event identity; stored instance identity
                // is separately trimmed by the unique installed-row kernel.
                if let Some(wake) = rows.wake {
                    super::governance_apply::write_wake_v1(
                        rows.event_instance_id(),
                        wake,
                        &mut ctx,
                        write,
                        work,
                        now,
                    )?;
                }
                if let Some(freeze) = &rows.rollback_freeze {
                    let crate::DomainEvent::ModuleRollbackApplied {
                        module_id,
                        from_module_version,
                        from_wasm_hash: Some(hash),
                        from_release_request_id,
                        proposal_id,
                        ..
                    } = &*tail.event
                    else {
                        return Err(invalid());
                    };
                    // Existing markers are retained exactly; only genuine
                    // absence creates the original rollback-stop marker.
                    let marker;
                    let after = if let Some(existing) = freeze.before.value() {
                        existing
                    } else {
                        marker = oasis7_world_state::state::ModuleAdmissionFreeze {
                            module_id: module_id.clone(),
                            module_version: from_module_version.clone(),
                            wasm_hash: hash.clone(),
                            rollback_proposal_id: *proposal_id,
                            source_release_request_id: *from_release_request_id,
                            reason: "rollback_stop_new_admission".to_owned(),
                        };
                        &marker
                    };
                    let descriptor=oasis7_world_state::incremental_world_native::world_native_field_descriptors_v1()?.into_iter().find(|field|field.name=="module_admission_freezes").ok_or_else(invalid)?;
                    work.debit(24, 0)?;
                    let oasis7_world_state::incremental_field_member::FieldDestination::State(
                        namespace,
                    ) = descriptor.destination
                    else {
                        return Err(invalid());
                    };
                    let mut native_read = WorldFieldReadContextV1 {
                        root: sources.families.roots.state_root(namespace),
                        state: parent.state,
                        values: ValueReadContext {
                            source: ctx.values.source,
                            memory: ctx.values.memory,
                            class: ctx.values.class,
                            io: &mut *ctx.values.io,
                        },
                        key_work: &mut *ctx.key_work,
                        semantic_bytes: ctx.semantic_bytes,
                        record_bytes: ctx.record_bytes,
                        take_state_error: &mut ctx.take_state_error,
                        cancelled: &mut ctx.cancelled,
                    };
                    oasis7_world_state::incremental_world_native::insert_current_module_admission_freeze_if_absent_v1(&freeze.before,&freeze.key,after,ctx.transaction,&mut native_read,write,&mut _factory.lifecycle.records_remaining,&mut _factory.lifecycle.native_write,live).map_err(overlay_error)?;
                }
                Ok(())
            },
        )?;
        // No business error remains beyond this point. Any failure in the original
        // journal or normalization poisons the entire provisional transaction.
        if let Some(applied) = &applied {
            publish_applied_module_events_v1(
                applied,
                step,
                read,
                materials,
                common,
                max_event_bytes,
                persist,
                live,
            )?;
        }
        let heap = tail
            .event
            .reserved_bytes()
            .checked_add(std::mem::size_of::<crate::WorldEvent>())
            .ok_or_else(invalid)?;
        pay(common, heap)?;
        let fee = common
            .memory()
            .reserve(heap, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let body = (*tail.event).clone();
        let id = {
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
            crate::world::incremental_normal_tail::event_preview::preview_current_normal_event_id_v1(&mut ctx,common)?
        };
        let state = CommittedModuleInstallStateV1 {
            tail,
            applied,
            unchanged,
            guard: step.transaction.guard(),
            core: common.work().identity(),
            now,
            _fee: state_fee,
        };
        let layout = common
            .memory()
            .reserve(
                std::mem::size_of::<PreparedNormalModuleLifecycleEventV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let cap = PreparedNormalModuleLifecycleEventV1 {
            event: fee.retain(crate::WorldEvent {
                id,
                time: now,
                caused_by: Some(crate::CausedBy::Action(selected.envelope().id)),
                body: crate::WorldEventBody::Domain(body),
            }),
            accepted,
            guard: step.transaction.guard(),
            core: common.work().identity(),
            action_id: selected.envelope().id,
            memory: common.memory().clone(),
            state: ModuleLifecycleCommittedStateV1::Install(state),
            _layout: layout,
            _prepare_fees: std::array::from_fn(|_| None),
        };
        step.append_prepared_module_lifecycle_event_v1(
            &cap,
            materials,
            common,
            live,
            max_event_bytes,
            persist,
        )?;
        Ok(cap)
    }));
    let result = match attempt {
        Ok(result) => result,
        Err(payload) => {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                step.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
            std::panic::resume_unwind(payload)
        }
    };
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            step.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}

fn target_ref(
    target: &oasis7_agent_api::world_action::ModuleInstallTarget,
) -> oasis7_world_state::incremental_installed_module_targets::ModuleInstallTargetRefV1<'_> {
    use oasis7_world_state::incremental_installed_module_targets::ModuleInstallTargetRefV1;
    match target {
        oasis7_agent_api::world_action::ModuleInstallTarget::SelfAgent => {
            ModuleInstallTargetRefV1::SelfAgent
        }
        oasis7_agent_api::world_action::ModuleInstallTarget::LocationInfrastructure {
            location_id,
        } => ModuleInstallTargetRefV1::LocationInfrastructure(location_id),
    }
}
