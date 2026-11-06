//! Concrete owner bundle used by the shared original module-output kernel.
//! This is a child of invocation so actual selected WASM ownership stays private.
use super::*;
use crate::world::incremental_body::BodyMutationContext;
use crate::world::{
    capability_authorization_command_stage::ModuleCallTarget,
    incremental_module_execution_effects::*,
    incremental_module_execution_emit::*,
    incremental_module_execution_event::ModuleOutputEventContextV1,
    incremental_module_execution_fee::contribute_root_module_fee_and_event_v1,
    incremental_module_execution_manifest_hash::*,
    incremental_module_execution_output::{RootModuleOutputErrorV1, RootModuleOutputWorkV1},
    incremental_module_execution_visual_decode::RootModuleVisualDecodeWorkV1,
    incremental_module_execution_worker::*,
    incremental_module_fee_accounts::*,
    incremental_module_fee_contribution::RootModuleFeeWriteContextV1,
    incremental_module_state::{ModuleStateEventContext, contribute_module_state_event},
    incremental_module_state_family::ModuleStateFamilyQualificationV1,
};
use oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1;
use oasis7_formal_protocol::incremental_value::ValueNodeWriter;
use oasis7_proto::memory_budget::MemoryReservation;
use oasis7_wasm_abi::{
    ModuleCallErrorCode, ModuleCallFailure, ModuleCallRequest, ModuleOutput, ModuleSandbox,
};
use oasis7_wasm_executor::bounded_worker::BoundedWorkerSandboxV1;
use oasis7_world_state::{
    WorldError, WorldEventBody,
    effect::{EffectIntent, EffectOrigin},
    incremental_agent_authority_records::AgentAuthorityFamilyQualificationV1,
    incremental_agent_subtree::AgentSubtreeStructureQualificationV1,
    incremental_field_member::MemberRecordWriteContext,
    incremental_module_marketplace::{
        ModuleMarketplaceFamilyQualificationV1, ModuleVisualOverlayWorkV1,
    },
    incremental_runtime_effect_records::{
        RuntimeEffectFamilyQualificationV1, RuntimeEffectRecordsWorkV1,
    },
    incremental_runtime_history_source::AcceptedRuntimeHistorySourceV1,
    incremental_runtime_module_authorization::{
        ModuleAuthorizationFamilyQualificationV1, ModuleAuthorizationReadWorkV1,
    },
};
use std::cell::RefCell;

#[derive(Debug)]
pub enum RootModuleExecutionErrorV1<OE> {
    Value(ValueTreeError),
    Invocation(ModuleInvocationPreparationErrorV1<OE>),
    Fee(RootModuleFeeErrorV1),
    Effect(RootModuleEffectErrorV1),
    Emit(RootModuleEmitErrorV1),
    Worker(RootModuleWorkerErrorV1),
    Body(BodyMutationError),
    Manifest(RootWorldManifestHashErrorV1),
    Routing(
        ReservedValue<
            oasis7_wasm_router::bounded::BoundedRouterErrorV1<
                oasis7_proto::memory_budget::MemoryLimitExceeded,
                oasis7_wasm_executor::bounded_worker::RegexWorkerErrorV1,
            >,
        >,
    ),
    Business(ReservedValue<WorldError>),
}
impl<OE> From<ValueTreeError> for RootModuleExecutionErrorV1<OE> {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
impl<OE> From<oasis7_formal_protocol::incremental_execution::ExecutionError>
    for RootModuleExecutionErrorV1<OE>
{
    fn from(e: oasis7_formal_protocol::incremental_execution::ExecutionError) -> Self {
        Self::Body(e.into())
    }
}

/// Continuous work belongs to the caller, including rejected attempts.
pub struct RootModuleExecutionWorkV1 {
    pub input: oasis7_wasm_abi::CanonicalCborWorkV1,
    pub routing: oasis7_wasm_router::bounded::RouterWorkV1,
    pub max_input_bytes: usize,
    pub max_policy_selections: usize,
    pub max_prepared_effects: usize,
    pub lookup_steps_remaining: u64,
    pub transient_heap_bytes: usize,
    pub max_state_bytes: usize,
    pub max_owner_bytes: usize,
    pub anchor_scan: StateScanBudget,
    pub anchor_cells_remaining: u64,
    pub visual: ModuleVisualOverlayWorkV1,
    pub visual_decode: RootModuleVisualDecodeWorkV1,
    pub effects: RootModuleEffectWorkV1,
    pub effect_records: RuntimeEffectRecordsWorkV1,
    pub authorization: ModuleAuthorizationReadWorkV1,
    pub output: RootModuleOutputWorkV1,
    pub selection_scan: StateScanBudget,
    pub selection: ArtifactConfigReadWorkV1,
    pub original: RuntimeArtifactOriginalWorkV1,
    pub selection_limits: ModuleInvocationPreparationLimitsV1,
}

/// Qualified inputs, never caller-provided DTO substitutes for owner reads.
#[derive(Clone, Copy)]
pub struct RootModuleExecutionFamiliesV1<'a> {
    pub roots: &'a RootSetV1,
    pub parent: &'a ReservedValue<AcceptedRuntimeHistorySourceV1>,
    pub head: &'a VerifiedFormalCommitV1,
    pub instances: &'a ReservedValue<ModuleInstanceFamilyQualificationV1>,
    pub config: &'a ReservedValue<ArtifactConfigFamilyQualificationV1>,
    pub content: &'a ReservedValue<RuntimeArtifactContentQualificationV1>,
    pub agents: &'a ReservedValue<AgentSubtreeStructureQualificationV1>,
    pub authorization: &'a ReservedValue<ModuleAuthorizationFamilyQualificationV1>,
    pub effects: &'a ReservedValue<RuntimeEffectFamilyQualificationV1>,
    pub state: &'a ReservedValue<ModuleStateFamilyQualificationV1>,
    pub agent_authority: &'a ReservedValue<AgentAuthorityFamilyQualificationV1>,
    pub marketplace: &'a ReservedValue<ModuleMarketplaceFamilyQualificationV1>,
}

struct Owners<'a, 't, 's, 'v, M, R, E, C, W, F, O: OriginalObjectStreamReader> {
    families: RootModuleExecutionFamiliesV1<'a>,
    selected: &'a PreparedRootModuleInvocationV1,
    now: u64,
    state: &'a M,
    ctx: &'a mut WorldFieldOverlayContextV1<'t, 's, 'v, M, R, E, C>,
    members: &'a mut MemberRecordWriteContext<'v, W, C>,
    fees: &'a mut RootModuleFeeWriteContextV1<'v, W, C>,
    events: &'a mut ModuleOutputEventContextV1<'v, R, W, C, F>,
    originals: &'a mut O,
    worker: &'a mut BoundedWorkerSandboxV1<'v, RootModuleWorkerAllocatorV1>,
    world_hash: &'a ReservedValue<CurrentWorldManifestHashV1>,
    work: &'a mut RootModuleExecutionWorkV1,
    live: ExecutionFence,
    live_owner: &'a mut dyn FnMut() -> ExecutionFence,
    hooks: Vec<PreparedRootModuleInvocationV1>,
    intents: Vec<ReservedValue<PreparedRootModuleEffectV1>>,
    clones: Vec<MemoryReservation>,
    sticky: Option<RootModuleExecutionErrorV1<O::Error>>,
    capture: Option<&'a RefCell<rule_capture::RuleOutputCaptureV1>>,
}
struct Target<'a, 't, 's, 'v, M, R, E, C, W, F, O: OriginalObjectStreamReader> {
    owners: RefCell<Owners<'a, 't, 's, 'v, M, R, E, C, W, F, O>>,
}

impl<'a, 't, 's, 'v, M, R, E, C, W, F, O> Owners<'a, 't, 's, 'v, M, R, E, C, W, F, O>
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
    fn poll_live(&mut self) -> Result<(), RootModuleExecutionErrorV1<O::Error>> {
        if (self.ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        self.live = (self.live_owner)();
        self.ctx
            .transaction
            .validate_guard(&self.ctx.transaction.guard(), &self.live)?;
        Ok(())
    }
    fn failed(&mut self, error: RootModuleExecutionErrorV1<O::Error>) -> WorldError {
        if self.sticky.is_none() {
            self.sticky = Some(error);
        }
        WorldError::Serde("root module execution resource/source failure".to_owned())
    }
    fn failure(&self, module_id: &str, trace_id: &str) -> ModuleCallFailure {
        ModuleCallFailure {
            module_id: module_id.to_owned(),
            trace_id: trace_id.to_owned(),
            code: ModuleCallErrorCode::InvalidOutput,
            detail: "root module execution rejected".to_owned(),
        }
    }
    fn select_hook(
        &mut self,
        module_id: &str,
    ) -> Result<usize, RootModuleExecutionErrorV1<O::Error>> {
        self.poll_live()?;
        if self.hooks.len() >= self.work.max_policy_selections {
            return Err(ValueTreeError::ResourceLimited("root module policy selections").into());
        }
        let guard = self.ctx.transaction.guard();
        let selected = prepare_root_module_registry_invocation_v1(
            ModuleInvocationPreparationRequestV1 {
                invocation_id: module_id,
                parent_roots: self.families.roots,
                instances: self.families.instances,
                config: self.families.config,
                content: self.families.content,
                current_active: None,
                current_record: None,
                guard: &guard,
                live: &self.live,
            },
            self.state,
            self.ctx,
            &mut self.work.selection_scan,
            &mut self.work.selection,
            self.originals,
            &mut self.work.original,
            &mut self.work.selection_limits,
        )
        .map_err(RootModuleExecutionErrorV1::Invocation)?
        .ok_or(ValueTreeError::Invalid("active Pure policy module missing"))?;
        // The actual selected decoder's JSON charges are independently retained;
        // the getter's clone admission includes them as well as its typed DTO.
        let bytes = selected
            .manifest
            .admitted_json_bytes_v1()?
            .checked_add(selected.manifest.reserved_bytes())
            .and_then(|n| n.checked_mul(2))
            .ok_or(ValueTreeError::ResourceLimited(
                "selected manifest clone admission",
            ))?;
        let fee = self
            .ctx
            .values
            .memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(|e| RootModuleExecutionErrorV1::Value(ValueTreeError::Memory(e)))?;
        self.clones.push(fee);
        self.hooks.push(selected);
        Ok(self.hooks.len() - 1)
    }
    fn charge(
        &mut self,
        module_id: &str,
        trace_id: &str,
        manifest: &crate::ModuleManifest,
        input_bytes: u64,
        output: &ModuleOutput,
    ) -> Result<(), RootModuleExecutionErrorV1<O::Error>> {
        self.poll_live()?;
        if module_id != self.selected.module_id() || manifest != self.selected.manifest() {
            return Err(ValueTreeError::Invalid("root module fee selected manifest").into());
        }
        let guard = self.ctx.transaction.guard();
        let prepared = prepare_root_module_fee_v1(
            RootModuleFeeRequestV1 {
                parent_roots: self.families.roots,
                agents: self.families.agents,
                guard: &guard,
                live: &self.live,
                module_id,
                trace_id,
                manifest,
                input_bytes,
                max_owner_bytes: self.work.max_owner_bytes,
            },
            output,
            self.ctx,
        )
        .map_err(RootModuleExecutionErrorV1::Fee)?;
        match prepared {
            RootModuleFeeOutcomeV1::Free => Ok(()),
            RootModuleFeeOutcomeV1::Denied(failure) => {
                // A retained business error is produced by the caller below;
                // no fee owner write or event has occurred in this branch.
                Err(RootModuleExecutionErrorV1::Business(
                    self.ctx
                        .values
                        .memory
                        .reserve(self.work.transient_heap_bytes, MemoryClass::Ordinary)
                        .map_err(ValueTreeError::Memory)?
                        .retain(WorldError::ModuleCallFailed {
                            module_id: failure.module_id.clone(),
                            trace_id: failure.trace_id.clone(),
                            code: failure.code.clone(),
                            detail: failure.detail.clone(),
                        }),
                ))
            }
            RootModuleFeeOutcomeV1::Prepared(prepared) => contribute_root_module_fee_and_event_v1(
                &prepared,
                self.families.agents,
                self.now,
                self.ctx,
                self.fees,
                self.events,
                &self.live,
            )
            .map_err(RootModuleExecutionErrorV1::Fee),
        }
    }
    fn effect(
        &mut self,
        kind: &str,
        params: &serde_json::Value,
        cap_ref: &str,
        origin: &EffectOrigin,
    ) -> Result<EffectIntent, RootModuleExecutionErrorV1<O::Error>> {
        self.poll_live()?;
        if !matches!(origin, EffectOrigin::Module { module_id } if module_id == self.selected.module_id())
        {
            return Err(ValueTreeError::Invalid("root module effect origin").into());
        }
        if self.intents.len() >= self.work.max_prepared_effects {
            return Err(ValueTreeError::ResourceLimited("root module prepared effects").into());
        }
        let prepared = prepare_root_module_effect_v1(
            RootModuleEffectRequestV1 {
                parent: self.families.parent,
                head: self.families.head,
                selected: self.selected,
                kind,
                params,
                cap_ref,
            },
            self.now,
            self.families.authorization,
            self.ctx,
            self.events,
            &mut self.work.authorization,
            &mut self.work.effects,
            &self.live,
        )
        .map_err(RootModuleExecutionErrorV1::Effect)?;
        let intent = prepared.intent().clone();
        self.intents.push(prepared);
        Ok(intent)
    }
    fn append(
        &mut self,
        body: WorldEventBody,
    ) -> Result<u64, RootModuleExecutionErrorV1<O::Error>> {
        self.poll_live()?;
        let sequence = crate::world::World::read_iws_allocator(
            crate::world::incremental_allocators::RuntimeAllocatorV1::Event,
            self.ctx.transaction,
            self.events.values,
            &mut self.events.mutation,
        )
        .map_err(RootModuleExecutionErrorV1::Body)?;
        let (id, _, _) = crate::world::World::preview_next_event_id(sequence.next, sequence.era);
        match body {
            WorldEventBody::EffectQueued(intent) => {
                let mut found = None;
                for (index, prepared) in self.intents.iter().enumerate() {
                    self.work.lookup_steps_remaining = self
                        .work
                        .lookup_steps_remaining
                        .checked_sub(1)
                        .ok_or(ValueTreeError::ResourceLimited("root module intent lookup"))?;
                    if prepared.intent() == &intent {
                        found = Some(index);
                        break;
                    }
                }
                let index =
                    found.ok_or(ValueTreeError::Invalid("root module unprepared intent"))?;
                let prepared = &self.intents[index];
                contribute_prepared_root_module_effect_v1(
                    prepared,
                    self.families.authorization,
                    self.families.effects,
                    self.ctx,
                    self.members,
                    self.events,
                    &mut self.work.authorization,
                    &mut self.work.effects,
                    &mut self.work.effect_records,
                    &self.live,
                )
                .map_err(RootModuleExecutionErrorV1::Effect)?;
                self.intents.remove(index);
            }
            WorldEventBody::ModuleEmitted(event) => {
                contribute_root_module_emit_v1(
                    self.selected,
                    &event,
                    self.now,
                    self.families.roots,
                    self.families.agents,
                    self.families.agent_authority,
                    self.families.marketplace,
                    self.state,
                    self.ctx,
                    self.members,
                    self.events,
                    &mut self.work.visual_decode,
                    &mut self.work.anchor_scan,
                    &mut self.work.anchor_cells_remaining,
                    self.work.max_owner_bytes,
                    &mut self.work.visual,
                    &self.live,
                )
                .map_err(RootModuleExecutionErrorV1::Emit)?;
            }
            WorldEventBody::ModuleStateUpdated(update) => {
                if update.module_id != self.selected.instance_id() {
                    return Err(ValueTreeError::Invalid("root module state key").into());
                }
                let before = self.ctx.transaction.ordered_fact_count();
                self.events
                    .world_events
                    .validate(self.ctx.transaction)
                    .map_err(RootModuleExecutionErrorV1::Body)?;
                let placeholder = oasis7_wasm_abi::CanonicalCborWorkV1 {
                    visits_remaining: 0,
                    bytes_remaining: 0,
                    max_depth: self.events.original_work.max_depth,
                    max_map_keys: self.events.original_work.max_map_keys,
                    max_key_bytes: self.events.original_work.max_key_bytes,
                };
                let mut context = ModuleStateEventContext {
                    qualification: self.families.state,
                    values: self.events.values,
                    mutation: BodyMutationContext {
                        sink: &mut *self.events.mutation.sink,
                        memory: self.events.mutation.memory,
                        io: &mut *self.events.mutation.io,
                        cancelled: &mut self.events.mutation.cancelled,
                    },
                    persist_original: &mut self.events.persist_original,
                    max_event_bytes: self.events.max_event_bytes,
                    original_work: std::mem::replace(&mut self.events.original_work, placeholder),
                };
                let result = contribute_module_state_event(
                    self.now,
                    &crate::WorldEvent {
                        id,
                        time: self.now,
                        caused_by: None,
                        body: WorldEventBody::ModuleStateUpdated(update),
                    },
                    self.ctx.transaction,
                    &mut context,
                );
                self.events.original_work = context.original_work;
                result.map_err(RootModuleExecutionErrorV1::Body)?;
                self.events
                    .world_events
                    .note_success(self.ctx.transaction, before)
                    .map_err(RootModuleExecutionErrorV1::Body)?;
            }
            _ => return Err(ValueTreeError::Invalid("shared module output event family").into()),
        }
        Ok(id)
    }
}

impl<'a, 't, 's, 'v, M, R, E, C, W, F, O> ModuleCallTarget
    for Target<'a, 't, 's, 'v, M, R, E, C, W, F, O>
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
    fn encode_module_input_for_call<T: serde::Serialize>(
        &self,
        input: &T,
    ) -> Result<Vec<u8>, WorldError> {
        let mut owners = self.owners.borrow_mut();
        let Owners { ctx, work, .. } = &mut *owners;
        let result = encode_input(
            input,
            ctx.values.memory,
            &mut work.input,
            work.max_input_bytes,
            &mut ctx.cancelled,
        );
        match result {
            Ok(bytes) => Ok(bytes),
            Err(error) => Err(owners.failed(error.into())),
        }
    }
    fn module_manifest_hash_for_call(
        &self,
        manifest: &crate::ModuleManifest,
    ) -> Result<String, WorldError> {
        let mut owners = self.owners.borrow_mut();
        let mut found = None;
        for index in (0..owners.hooks.len()).rev() {
            let Some(left) = owners.work.lookup_steps_remaining.checked_sub(1) else {
                return Err(owners.failed(
                    ValueTreeError::ResourceLimited("root policy manifest lookup").into(),
                ));
            };
            owners.work.lookup_steps_remaining = left;
            if owners.hooks[index].manifest() == manifest {
                found = Some(index);
                break;
            }
        }
        match found {
            Some(index) => Ok(hex::encode(owners.hooks[index].manifest_sha256())),
            None => {
                Err(owners
                    .failed(ValueTreeError::Invalid("root policy manifest hash source").into()))
            }
        }
    }
    fn active_manifest_for_call(
        &self,
        module_id: &str,
    ) -> Result<crate::ModuleManifest, WorldError> {
        let mut owners = self.owners.borrow_mut();
        match owners.select_hook(module_id) {
            Ok(index) => Ok(owners.hooks[index].manifest().clone()),
            Err(error) => Err(owners.failed(error)),
        }
    }
    fn current_manifest_hash_for_call(&self) -> Result<String, WorldError> {
        Ok(self.owners.borrow().world_hash.hash().to_owned())
    }
    fn state_time_for_call(&self) -> u64 {
        self.owners.borrow().now
    }
    fn journal_height_for_call(&self) -> u64 {
        let mut owners = self.owners.borrow_mut();
        match owners
            .events
            .world_events
            .journal_height(owners.ctx.transaction)
        {
            Ok(height) => height,
            Err(error) => {
                owners.failed(RootModuleExecutionErrorV1::Body(error));
                0
            }
        }
    }
    fn call_module_raw_for_call(
        &mut self,
        module_id: &str,
        trace_id: &str,
        input: Vec<u8>,
        manifest: &crate::ModuleManifest,
        _legacy_sandbox: &mut dyn ModuleSandbox,
    ) -> Result<ModuleOutput, ModuleCallFailure> {
        let mut owners = self.owners.borrow_mut();
        let result = (|| {
            let mut found = None;
            for index in (0..owners.hooks.len()).rev() {
                owners.work.lookup_steps_remaining = owners
                    .work
                    .lookup_steps_remaining
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited("root policy source lookup"))?;
                let selected = &owners.hooks[index];
                if selected.module_id() == module_id && selected.manifest() == manifest {
                    found = Some(index);
                    break;
                }
            }
            let index = found.ok_or(ValueTreeError::Invalid(
                "root policy call missing true active selection",
            ))?;
            let Owners {
                hooks,
                ctx,
                worker,
                work,
                live,
                live_owner,
                ..
            } = &mut *owners;
            let selected = &hooks[index];
            ctx.transaction.validate_guard(&selected.guard, live)?;
            let fee = input
                .len()
                .checked_add(module_id.len())
                .and_then(|n| n.checked_add(trace_id.len()))
                .and_then(|n| {
                    n.checked_add(manifest.wasm_hash.len() + manifest.kind.entrypoint().len() + 512)
                })
                .ok_or(ValueTreeError::ResourceLimited("root policy request"))?;
            let _request = ctx
                .values
                .memory
                .reserve(fee, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let request = ModuleCallRequest {
                module_id: module_id.to_owned(),
                wasm_hash: manifest.wasm_hash.clone(),
                trace_id: trace_id.to_owned(),
                entrypoint: manifest.kind.entrypoint().to_owned(),
                input,
                limits: manifest.limits.clone(),
                wasm_bytes: Arc::clone(&selected.wasm),
            };
            let output = execute_root_module_worker_v1(
                ctx.transaction,
                worker,
                &request,
                ctx.values.memory,
                &mut work.output,
                &mut ctx.cancelled,
            )
            .map_err(RootModuleExecutionErrorV1::Worker)?;
            *live = (live_owner)();
            ctx.transaction.validate_guard(&selected.guard, live)?;
            Ok::<_, RootModuleExecutionErrorV1<O::Error>>(output)
        })();
        match result {
            Ok(output) => {
                // The shared hook kernel takes an owned DTO. The real admitted
                // output's full retained fee is reserved before this clone and
                // held by target until every hook/application has completed.
                let charge = match owners
                    .ctx
                    .values
                    .memory
                    .reserve(output.reserved_bytes(), MemoryClass::Ordinary)
                {
                    Ok(charge) => charge,
                    Err(error) => {
                        owners.failed(RootModuleExecutionErrorV1::Value(ValueTreeError::Memory(
                            error,
                        )));
                        return Err(owners.failure(module_id, trace_id));
                    }
                };
                owners.clones.push(charge);
                let cloned = (*output).clone();
                if let Some(capture) = owners.capture {
                    let result = (|| {
                        let mut capture = capture.borrow_mut();
                        let slot = capture.begin(module_id, trace_id)?;
                        capture.finish(slot, output)
                    })();
                    if let Err(error) = result {
                        owners.failed(RootModuleExecutionErrorV1::Value(error));
                        return Err(owners.failure(module_id, trace_id));
                    }
                }
                Ok(cloned)
            }
            Err(RootModuleExecutionErrorV1::Worker(RootModuleWorkerErrorV1::Output(
                RootModuleOutputErrorV1::Sandbox(failure),
            ))) => Err((*failure).clone()),
            Err(error) => {
                owners.failed(error);
                Err(owners.failure(module_id, trace_id))
            }
        }
    }
    fn module_call_failed_for_call(
        &mut self,
        failure: ModuleCallFailure,
    ) -> Result<(), WorldError> {
        Err(WorldError::ModuleCallFailed {
            module_id: failure.module_id,
            trace_id: failure.trace_id,
            code: failure.code,
            detail: failure.detail,
        })
    }
    fn append_module_event_for_call(&mut self, body: WorldEventBody) -> Result<u64, WorldError> {
        let mut owners = self.owners.borrow_mut();
        match owners.append(body) {
            Ok(id) => Ok(id),
            Err(error) => Err(owners.failed(error)),
        }
    }
    fn try_charge_module_runtime_for_call(
        &mut self,
        module_id: &str,
        trace_id: &str,
        manifest: &crate::ModuleManifest,
        input_bytes: u64,
        output: &ModuleOutput,
    ) -> Result<(), ModuleCallFailure> {
        let mut owners = self.owners.borrow_mut();
        match owners.charge(module_id, trace_id, manifest, input_bytes, output) {
            Ok(()) => Ok(()),
            Err(error) => {
                owners.failed(error);
                Err(owners.failure(module_id, trace_id))
            }
        }
    }
    fn build_effect_intent_for_call(
        &mut self,
        kind: String,
        params: serde_json::Value,
        cap_ref: String,
        origin: EffectOrigin,
    ) -> Result<EffectIntent, WorldError> {
        let mut owners = self.owners.borrow_mut();
        match owners.effect(&kind, &params, &cap_ref, &origin) {
            Ok(intent) => Ok(intent),
            Err(RootModuleExecutionErrorV1::Effect(RootModuleEffectErrorV1::Business(error))) => {
                Err((*error).clone())
            }
            Err(error) => Err(owners.failed(error)),
        }
    }
}

fn encode_input<T: serde::Serialize>(
    input: &T,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    work: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
    max_bytes: usize,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<Vec<u8>, ValueTreeError> {
    use std::io::{self, Write};
    struct Output {
        bytes: Vec<u8>,
        max: usize,
    }
    impl Write for Output {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let count = bytes.len().min(64 << 10);
            if self
                .bytes
                .len()
                .checked_add(count)
                .is_none_or(|len| len > self.max)
            {
                return Err(io::Error::other("bounded root module input"));
            }
            self.bytes.extend_from_slice(&bytes[..count]);
            Ok(count)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    // Match util::to_canonical_cbor at the legacy ModuleCallTarget boundary.
    // ActionOriginal identities deliberately use the separate untagged ABI
    // encoder; VM inputs (including nested action/event inputs) retain tag 55799.
    const SELF_DESCRIBE: &[u8] = &[0xd9, 0xd9, 0xf7];
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if max_bytes < SELF_DESCRIBE.len() {
        return Err(ValueTreeError::ResourceLimited("root module input bytes"));
    }
    work.bytes_remaining = work
        .bytes_remaining
        .checked_sub(SELF_DESCRIBE.len() as u64)
        .ok_or(ValueTreeError::ResourceLimited("root module input bytes"))?;
    // Caller retains max_bytes through the shared kernel's owned Vec API.
    let allocator = RootModuleWorkerAllocatorV1::new(memory);
    let placeholder = oasis7_wasm_abi::CanonicalCborWorkV1 {
        visits_remaining: 0,
        bytes_remaining: 0,
        max_depth: work.max_depth,
        max_map_keys: work.max_map_keys,
        max_key_bytes: work.max_key_bytes,
    };
    let mut context = oasis7_wasm_abi::CanonicalCborStreamContextV1::new(
        &allocator,
        std::mem::replace(work, placeholder),
        cancelled,
    );
    let mut output = Output {
        bytes: Vec::with_capacity(max_bytes),
        max: max_bytes,
    };
    output.bytes.extend_from_slice(SELF_DESCRIBE);
    let result = oasis7_wasm_abi::stream_canonical_cbor_v1(input, &mut context, &mut output);
    *work = context.work;
    result.map_err(|error| match error {
        oasis7_wasm_abi::CanonicalCborStreamError::Cancelled => ValueTreeError::Cancelled,
        oasis7_wasm_abi::CanonicalCborStreamError::ResourceLimited(message) => {
            ValueTreeError::ResourceLimited(message)
        }
        oasis7_wasm_abi::CanonicalCborStreamError::Io(_) => {
            ValueTreeError::ResourceLimited("root module input bytes")
        }
        _ => ValueTreeError::Invalid("root module canonical input"),
    })?;
    Ok(output.bytes)
}

#[path = "incremental_module_execution_target_entry.rs"]
mod entry;
pub use entry::{
    RootModuleExecutionContextV1, RootModuleTickOutcomeV1, RootRuleOverrideAuditV1,
    RootRulePhaseOutcomeV1, RootRulePhaseWorkV1,
};

#[path = "incremental_module_execution_rule_capture.rs"]
mod rule_capture;

pub(in crate::world) use entry::PreparedRuleOverrideEventV1;
