//! Actual sparse Current Apply. Only a validated certificate/source owner can
//! enter this writer. It never reconstructs or mutates a borrowed World.
use super::governance_finality::ValidatedCurrentModuleApplyV1;
use super::*;
use crate::world::{
    incremental_body_event::materials::CorePolicyMaterialContextV1,
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::incremental_value::{
    ValueNodeReader, ValueNodeWriter, ValueReadContext,
};
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1, incremental_runtime_artifact_config::*,
};

pub(super) struct CommittedAppliedModuleStateV1 {
    source: ValidatedCurrentModuleApplyV1,
    bodies: Vec<crate::WorldEventBody>,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    now: u64,
    _fee: MemoryReservation,
}
impl CommittedAppliedModuleStateV1 {
    pub(super) fn proposal_id(&self) -> u64 {
        self.source.prefix.proposal.original_id()
    }
    pub(super) fn applied_hash(&self) -> &str {
        &self.source.prefix.applied_hash
    }
    pub(super) fn event_bodies(&self) -> &[crate::WorldEventBody] {
        &self.bodies
    }
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        core: WorkIdentityV1,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.core != core || self.now != now || !self._fee.uses_budget(memory) {
            return Err(invalid());
        }
        let proposal = self.source.prefix.proposal.value().ok_or_else(invalid)?;
        let oasis7_world_state::ProposalStatus::Approved { manifest_hash, .. } = &proposal.status
        else {
            return Err(invalid());
        };
        self.source
            .prefix
            .proposal
            .validate_provenance(tx, memory, live)
            .map_err(overlay_error)?;
        self.source.finality.validate_provenance(
            tx,
            memory,
            core,
            now,
            self.source.prefix.proposal.original_id(),
            manifest_hash,
            self.source.prefix.epoch,
            live,
        )
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn apply_validated_current_module_v1<M, P, V, W, D, E, C, PE, PC, CW, ME>(
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    source: ValidatedCurrentModuleApplyV1,
    sources: &ModuleLifecycleSourcesV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    module: &mut ModuleLifecycleWorkV1<'_>,
    factory: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<CommittedAppliedModuleStateV1, BodyMutationError>
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
{
    let now = step.time();
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
        |step, read, parent, write, _materials, factory, module, tail| {
            source.validate_current_v1(step.transaction, tail, now, live)?;
            selected.validate(step.transaction)?;
            let action = outcome
                .override_action_for_selected_v1(selected, step.transaction)?
                .unwrap_or(&selected.envelope().action);
            let actor = action.actor_id().ok_or_else(invalid)?;
            let changes = source.prefix.changes.as_ref();
            let n = if let Some(c) = changes {
                c.register
                    .len()
                    .checked_add(c.upgrade.len())
                    .and_then(|n| n.checked_add(c.activate.len()))
                    .and_then(|n| n.checked_add(c.deactivate.len()))
                    .ok_or_else(invalid)?
            } else {
                0
            };
            let slots = n.checked_add(2).ok_or_else(invalid)?;
            let heap = source
                .prefix
                .fee
                .bytes()
                .checked_mul(4)
                .and_then(|n| n.checked_add(source.certificate.reserved_bytes().checked_mul(2)?))
                .and_then(|n| {
                    let each = std::mem::size_of::<crate::WorldEventBody>().checked_add(actor.len())?.checked_add(4096)?;
                    n.checked_add(slots.checked_mul(each)?)
                })
                .ok_or_else(invalid)?;
            tail.charge(heap)?;
            let fee = tail
                .memory()
                .reserve(heap, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let id = source.prefix.proposal.original_id();
            let mut bodies = Vec::with_capacity(slots);
            // These are the original four sorted cohorts, including equal-key stable
            // ordering. The event values also supply the exact business writer below.
            if let Some(c) = changes {
                let mut register = c.register.clone();
                register.sort_by(|a, b| a.module_id.cmp(&b.module_id));
                for module in register {
                    bodies.push(crate::WorldEventBody::ModuleEvent(crate::ModuleEvent {
                        proposal_id: id,
                        kind: crate::ModuleEventKind::RegisterModule {
                            module,
                            registered_by: actor.to_owned(),
                        },
                    }));
                }
                let mut upgrade = c.upgrade.clone();
                upgrade.sort_by(|a, b| a.module_id.cmp(&b.module_id));
                for u in upgrade {
                    bodies.push(crate::WorldEventBody::ModuleEvent(crate::ModuleEvent {
                        proposal_id: id,
                        kind: crate::ModuleEventKind::UpgradeModule {
                            module_id: u.module_id,
                            from_version: u.from_version,
                            to_version: u.to_version,
                            wasm_hash: u.manifest.wasm_hash.clone(),
                            manifest: u.manifest,
                            upgraded_by: actor.to_owned(),
                        },
                    }));
                }
                let mut activate = c.activate.clone();
                activate.sort_by(|a, b| a.module_id.cmp(&b.module_id));
                for a in activate {
                    bodies.push(crate::WorldEventBody::ModuleEvent(crate::ModuleEvent {
                        proposal_id: id,
                        kind: crate::ModuleEventKind::ActivateModule {
                            module_id: a.module_id,
                            version: a.version,
                            activated_by: actor.to_owned(),
                        },
                    }));
                }
                let mut deactivate = c.deactivate.clone();
                deactivate.sort_by(|a, b| a.module_id.cmp(&b.module_id));
                for d in deactivate {
                    bodies.push(crate::WorldEventBody::ModuleEvent(crate::ModuleEvent {
                        proposal_id: id,
                        kind: crate::ModuleEventKind::DeactivateModule {
                            module_id: d.module_id,
                            reason: d.reason,
                            deactivated_by: actor.to_owned(),
                        },
                    }));
                }
            }
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
            for body in &bodies {
                let crate::WorldEventBody::ModuleEvent(event) = body else {
                    return Err(invalid());
                };
                tail.debit(1, 0)?;
                write_module_event_v1(
                    event, sources, &mut ctx, parent, write, factory, module, tail, now, live,
                )?;
            }
            let before = open_current_config_v1(
                ArtifactConfigFieldV1::Manifest,
                ArtifactConfigRecordKeyV1::Global,
                sources,
                &mut ctx,
                parent,
                factory,
            )?
            .ok_or_else(invalid)?;
            let guard = ctx.transaction.guard();
            replace_current_module_lifecycle_artifact_config_v1(
                ModuleLifecycleArtifactConfigEditV1::Manifest(&source.prefix.applied_manifest),
                sources.families.config,
                Some(&before),
                &mut ctx,
                write,
                module.config_write,
                &guard,
                live,
            )
            .map_err(overlay_error)?;
            let before = &source.prefix.proposal;
            let mut proposal = before.value().ok_or_else(invalid)?.clone();
            proposal.status = oasis7_world_state::ProposalStatus::Applied {
                manifest_hash: source.prefix.applied_hash.to_string(),
            };
            let guard = ctx.transaction.guard();
            oasis7_world_state::incremental_runtime_proposal::replace_current_runtime_proposal_v1(
                before,
                &proposal,
                &mut ctx,
                write,
                module.proposal_write,
                &guard,
                live,
            )
            .map_err(overlay_error)?;
            bodies.push(crate::WorldEventBody::ManifestUpdated(
                crate::ManifestUpdate {
                    manifest: source.prefix.applied_manifest.clone(),
                    manifest_hash: source.prefix.applied_hash.to_string(),
                },
            ));
            bodies.push(crate::WorldEventBody::Governance(
                crate::GovernanceEvent::Applied {
                    proposal_id: id,
                    manifest_hash: Some(source.prefix.applied_hash.to_string()),
                    consensus_height: Some(source.certificate.consensus_height),
                    threshold: Some(source.certificate.effective_min_unique_signers()),
                    signer_node_ids: source.certificate.signatures.keys().cloned().collect(),
                },
            ));
            Ok(CommittedAppliedModuleStateV1 {
                source,
                bodies,
                guard: ctx.transaction.guard(),
                core: tail.identity(),
                now,
                _fee: fee,
            })
        },
    )
}
fn open_current_config_v1<M, P, R, V, E, C, PE, PC>(
    field: ArtifactConfigFieldV1,
    key: ArtifactConfigRecordKeyV1<'_>,
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    factory: &mut FactoryScheduleWorkV1,
) -> Result<Option<ReservedValue<ArtifactConfigBodyQualificationV1>>, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    open_artifact_config_body_overlay_v1(
        ArtifactConfigSelectedBodyRequestV1 {
            field,
            key,
            family: sources.families.config,
            parent_roots: sources.families.roots,
            current: None,
        },
        parent.state,
        ctx,
        &mut factory.config_scan,
        &mut factory.config,
    )
    .map_err(overlay_error)
}
#[allow(clippy::too_many_arguments)]
fn write_module_event_v1<M, P, R, V, E, C, PE, PC, W, CW>(
    event: &crate::ModuleEvent,
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    tail: &mut crate::world::incremental_normal_tail::NormalTailWorkV1<'_, '_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader
        + ValueNodeWriter
        + oasis7_formal_protocol::incremental_state::StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    let record = match &event.kind {
        crate::ModuleEventKind::RegisterModule {
            module,
            registered_by,
        } => Some((
            module.module_id.as_str(),
            module.version.as_str(),
            module,
            registered_by,
        )),
        crate::ModuleEventKind::UpgradeModule {
            module_id,
            to_version,
            manifest,
            upgraded_by,
            ..
        } => Some((
            module_id.as_str(),
            to_version.as_str(),
            manifest,
            upgraded_by,
        )),
        _ => None,
    };
    if let Some((id, version, manifest, actor)) = record {
        let key = crate::ModuleRegistry::record_key(id, version);
        let before = open_current_config_v1(
            ArtifactConfigFieldV1::RegistryManifest,
            ArtifactConfigRecordKeyV1::Member(&key),
            sources,
            ctx,
            parent,
            factory,
        )?;
        let after = crate::ModuleRecord {
            manifest: manifest.clone(),
            registered_at: now,
            registered_by: actor.clone(),
            audit_event_id: None,
        };
        let guard = ctx.transaction.guard();
        replace_current_module_lifecycle_artifact_config_v1(
            ModuleLifecycleArtifactConfigEditV1::RegistryRecord {
                original_key: &key,
                after: &after,
            },
            sources.families.config,
            before.as_ref(),
            ctx,
            write,
            module.config_write,
            &guard,
            live,
        )
        .map_err(overlay_error)?;
        use oasis7_world_state::incremental_runtime_artifact_content::*;
        let before = match read_runtime_artifact_content_overlay_v1(
            RuntimeArtifactContentFieldV1::Registered,
            &manifest.wasm_hash,
            ctx,
        )
        .map_err(overlay_error)?
        {
            RuntimeArtifactContentValueV1::Registered(value) => value,
            _ => return Err(invalid()),
        };
        let guard = ctx.transaction.guard();
        let mut forbidden = |_, _: &[u8]| {
            Err(ValueTreeError::Invalid(
                "registration must not create an original",
            ))
        };
        change_runtime_artifact_content_overlay_v1(
            RuntimeArtifactContentChangeV1::Registered {
                key: &manifest.wasm_hash,
                expected: before,
                after: true,
            },
            ctx,
            write,
            module.content,
            &mut forbidden,
            &guard,
            live,
        )
        .map_err(overlay_error)?;
    }
    match &event.kind {
        crate::ModuleEventKind::ActivateModule {
            module_id, version, ..
        } => {
            let before = open_current_config_v1(
                ArtifactConfigFieldV1::RegistryActive,
                ArtifactConfigRecordKeyV1::Member(module_id),
                sources,
                ctx,
                parent,
                factory,
            )?;
            let key = crate::ModuleRegistry::record_key(module_id, version);
            let body = open_current_config_v1(
                ArtifactConfigFieldV1::RegistryManifest,
                ArtifactConfigRecordKeyV1::Member(&key),
                sources,
                ctx,
                parent,
                factory,
            )?
            .ok_or_else(invalid)?;
            let mut read = WorldFieldReadContextV1 {
                root: body.root(),
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
            let selected = read_selected_module_manifest_v1(
                &body,
                &mut read,
                &mut factory.config,
                &mut factory.config_scan,
                &SelectedModuleManifestLimitsV1 {
                    heap_bytes: module.max_selected_heap,
                    json_heap_bytes: module.max_selected_heap,
                    max_collection_items: module.max_selected_heap as u64,
                },
            )?;
            let tick = selected
                .manifest()
                .subscriptions
                .iter()
                .any(|s| s.resolved_stage() == crate::ModuleSubscriptionStage::Tick);
            let guard = ctx.transaction.guard();
            replace_current_module_lifecycle_artifact_config_v1(
                ModuleLifecycleArtifactConfigEditV1::Active {
                    module_id,
                    after: Some(version),
                },
                sources.families.config,
                before.as_ref(),
                ctx,
                write,
                module.config_write,
                &guard,
                live,
            )
            .map_err(overlay_error)?;
            write_wake_v1(
                module_id,
                if tick { Some(now) } else { None },
                ctx,
                write,
                tail,
                now,
            )?;
        }
        crate::ModuleEventKind::DeactivateModule { module_id, .. } => {
            let before = open_current_config_v1(
                ArtifactConfigFieldV1::RegistryActive,
                ArtifactConfigRecordKeyV1::Member(module_id),
                sources,
                ctx,
                parent,
                factory,
            )?;
            let guard = ctx.transaction.guard();
            replace_current_module_lifecycle_artifact_config_v1(
                ModuleLifecycleArtifactConfigEditV1::Active {
                    module_id,
                    after: None,
                },
                sources.families.config,
                before.as_ref(),
                ctx,
                write,
                module.config_write,
                &guard,
                live,
            )
            .map_err(overlay_error)?;
            write_wake_v1(module_id, None, ctx, write, tail, now)?;
        }
        _ => {}
    }
    Ok(())
}
// The original SchedulePageWriter default validates and persists the actual
// paid delta through this same node sink. It does not manufacture a schedule.
struct ScheduleSink<'a, W>(&'a mut W);
impl<W: ValueNodeWriter> ValueNodeWriter for ScheduleSink<'_, W> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.0.put_node(id, bytes)
    }
}
impl<W: ValueNodeReader> ValueNodeReader for ScheduleSink<'_, W> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        class: MemoryClass,
    ) -> Result<std::sync::Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.0.read_node(id, max, memory, class)
    }
}
impl<W: ValueNodeWriter> oasis7_formal_protocol::incremental_schedule::SchedulePageWriter
    for ScheduleSink<'_, W>
{
}
pub(super) fn write_wake_v1<M, R, E, C, W, CW>(
    id: &str,
    wake: Option<u64>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    _tail: &mut crate::world::incremental_normal_tail::NormalTailWorkV1<'_, '_>,
    now: u64,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    let mut sink = ScheduleSink(&mut *write.sink);
    let mut mutation = crate::world::incremental_body::BodyMutationContext {
        sink: &mut sink,
        memory: write.memory,
        io: &mut *write.io,
        cancelled: || (ctx.cancelled)() || (write.cancelled)(),
    };
    crate::world::World::contribute_iws_module_wake_at(
        now,
        id,
        wake,
        ctx.transaction,
        ctx.values.source,
        &mut mutation,
    )
}

#[path = "incremental_normal_action_module_lifecycle_governance_apply_event.rs"]
mod publication;
pub(in crate::world) use publication::PreparedAppliedModuleEventV1;
pub(super) use publication::publish_applied_module_events_v1;
