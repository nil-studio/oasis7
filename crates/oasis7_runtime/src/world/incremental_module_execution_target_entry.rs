//! Real tick invocation -> typed sealed VM -> original output kernel consumers.
use super::*;
use crate::world::incremental_module_execution_effects::heap::{
    EffectCloneWorkV1, json_clone_bound_v1,
};
use oasis7_wasm_abi::{ModuleCallCaller, ModuleCallInput, ModuleCallOrigin, ModuleContext};
use oasis7_wasm_executor::bounded_worker::{WorkerExecutableV1, WorkerLimitsV1};

pub struct RootModuleExecutionContextV1<
    'a,
    't,
    's,
    'v,
    M,
    R,
    E,
    C,
    W,
    F,
    O: OriginalObjectStreamReader,
> {
    pub families: RootModuleExecutionFamiliesV1<'a>,
    pub now: u64,
    pub state: &'a M,
    pub ctx: &'a mut WorldFieldOverlayContextV1<'t, 's, 'v, M, R, E, C>,
    pub members: &'a mut MemberRecordWriteContext<'v, W, C>,
    pub fees: &'a mut RootModuleFeeWriteContextV1<'v, W, C>,
    pub events: &'a mut ModuleOutputEventContextV1<'v, R, W, C, F>,
    pub originals: &'a mut O,
    pub worker: &'a mut BoundedWorkerSandboxV1<'v, RootModuleWorkerAllocatorV1>,
    pub manifest_cache: &'a ReservedValue<AcceptedWorldManifestHashV1>,
    pub manifest_work: &'a mut RootWorldManifestHashWorkV1,
    pub work: &'a mut RootModuleExecutionWorkV1,
    #[cfg(test)]
    pub input_observation: Option<&'a mut Option<(usize, [u8; 32])>>,
}
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
    /// Component executor only: the final private driver establishes the real
    /// due/command authorization and Native baseline before invoking this API.
    /// No complete execution permit or accepted state is returned here.
    pub fn execute_tick_invocation_v1(
        &mut self,
        invocation_id: &str,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<Option<ReservedValue<ModuleOutput>>, RootModuleExecutionErrorV1<O::Error>> {
        let result = self.execute_invocation_inner(invocation_id, None, live_owner);
        match result {
            Ok(output) => Ok(output),
            Err(error) => {
                // An inner owner may already have poisoned this transaction.
                // Rejection must retain its first concrete error rather than
                // replace it with contribute's subsequent Aborted sentinel.
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                Err(error)
            }
        }
    }
    pub(in crate::world) fn execute_pre_action_invocation_v1(
        &mut self,
        invocation_id: &str,
        action: &crate::world::incremental_action_execution::SelectedPendingActionV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<Option<ReservedValue<ModuleOutput>>, RootModuleExecutionErrorV1<O::Error>> {
        let result = self.ctx.transaction.contribute(|tx| {
            action
                .validate(tx)
                .map_err(RootModuleExecutionErrorV1::from)
        });
        result?;
        let result = self.execute_invocation_inner(invocation_id, Some(action), live_owner);
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }
    fn select(
        &mut self,
        invocation_id: &str,
        live: &ExecutionFence,
    ) -> Result<Option<PreparedRootModuleInvocationV1>, RootModuleExecutionErrorV1<O::Error>> {
        let guard = self.ctx.transaction.guard();
        prepare_root_module_invocation_v1(
            ModuleInvocationPreparationRequestV1 {
                invocation_id,
                parent_roots: self.families.roots,
                instances: self.families.instances,
                config: self.families.config,
                content: self.families.content,
                current_active: None,
                current_record: None,
                guard: &guard,
                live,
            },
            self.state,
            self.ctx,
            &mut self.work.selection_scan,
            &mut self.work.selection,
            self.originals,
            &mut self.work.original,
            &mut self.work.selection_limits,
        )
        .map_err(RootModuleExecutionErrorV1::Invocation)
    }
    fn execute_invocation_inner(
        &mut self,
        invocation_id: &str,
        action: Option<&crate::world::incremental_action_execution::SelectedPendingActionV1>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<Option<ReservedValue<ModuleOutput>>, RootModuleExecutionErrorV1<O::Error>> {
        let live = live_owner();
        self.ctx
            .transaction
            .validate_guard(&self.ctx.transaction.guard(), &live)?;
        self.events
            .world_events
            .validate(self.ctx.transaction)
            .map_err(RootModuleExecutionErrorV1::Body)?;
        crate::world::incremental_clock::check_clock(
            self.now,
            self.ctx.transaction,
            &mut self.events.mutation,
        )
        .map_err(RootModuleExecutionErrorV1::Body)?;
        let hash = prepare_current_world_manifest_hash_v1(
            self.families.parent,
            self.families.head,
            self.families.roots,
            self.manifest_cache,
            None,
            self.families.config,
            self.state,
            self.ctx,
            self.manifest_work,
        )
        .map_err(RootModuleExecutionErrorV1::Manifest)?;
        let Some(first) = self.select(invocation_id, &live)? else {
            return Ok(None);
        };
        if action.is_none()
            && !crate::world::module_tick_runtime::module_has_tick_subscription(first.manifest())
        {
            return Ok(None);
        }
        let memory = self.ctx.values.memory;
        // A pre-action owns both the nested action bytes and the final input
        // at the same time. Reserve both Vec capacities before either encode.
        let input_copies = if action.is_some() { 2 } else { 1 };
        let input_bytes = self.work.max_input_bytes.checked_mul(input_copies).ok_or(
            ValueTreeError::ResourceLimited("root module input workspace"),
        )?;
        let phase_bytes = self
            .work
            .max_state_bytes
            .checked_add(input_bytes)
            .and_then(|n| {
                self.work
                    .selection_limits
                    .max_text_bytes
                    .checked_mul(32)
                    .and_then(|text| n.checked_add(text))
            })
            .and_then(|n| n.checked_add(32 << 10))
            .ok_or(ValueTreeError::ResourceLimited(
                "root module input workspace",
            ))?;
        let _input_phase = memory
            .reserve(phase_bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let (origin_kind, origin_id, trace_id) = match first.install_target() {
            oasis7_agent_api::world_action::ModuleInstallTarget::SelfAgent => (
                "tick".to_owned(),
                self.now.to_string(),
                format!("tick-{}-{}", self.now, first.instance_id()),
            ),
            oasis7_agent_api::world_action::ModuleInstallTarget::LocationInfrastructure {
                location_id,
            } => {
                let location_id = location_id.trim();
                if location_id.is_empty() {
                    (
                        "tick".to_owned(),
                        self.now.to_string(),
                        format!("tick-{}-{}", self.now, first.instance_id()),
                    )
                } else {
                    (
                        "infrastructure_tick".to_owned(),
                        format!("{}:{}", location_id, self.now),
                        format!(
                            "infra-tick-{}-{}-{}",
                            self.now,
                            location_id,
                            first.instance_id()
                        ),
                    )
                }
            }
        };
        let state = if first.manifest().kind == crate::ModuleKind::Reducer {
            let mut bytes = Vec::with_capacity(self.work.max_state_bytes);
            crate::world::incremental_module_state::read_iws_module_state(
                self.families.state,
                first.instance_id(),
                self.ctx.transaction,
                &mut self.ctx.values,
                &mut self.ctx.cancelled,
                self.work.max_state_bytes as u64,
                &mut bytes,
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            Some(bytes)
        } else {
            None
        };
        let mut input = ModuleCallInput {
            ctx: ModuleContext {
                v: "wasm-1".to_owned(),
                module_id: first.module_id().to_owned(),
                trace_id: trace_id.clone(),
                time: self.now,
                origin: ModuleCallOrigin {
                    kind: origin_kind,
                    id: origin_id,
                },
                caller: ModuleCallCaller::System {
                    system_id: "module_tick".to_owned(),
                },
                limits: first.manifest().limits.clone(),
                stage: Some("tick".to_owned()),
                world_config_hash: Some(hash.hash().to_owned()),
                manifest_hash: Some(hex::encode(first.manifest_sha256())),
                journal_height: Some(
                    self.events
                        .world_events
                        .journal_height(self.ctx.transaction)
                        .map_err(RootModuleExecutionErrorV1::Body)?,
                ),
                module_version: Some(first.manifest().version.clone()),
                module_kind: Some(
                    crate::world::module_runtime_labels::module_kind_label(&first.manifest().kind)
                        .to_owned(),
                ),
                module_role: Some(
                    crate::world::module_runtime_labels::module_role_label(&first.manifest().role)
                        .to_owned(),
                ),
            },
            event: None,
            action: None,
            state,
        };
        if let Some(action) = action {
            // The strict router owns its admitted arena through matching, never
            // constructs a serde_json BTreeMap or uses the process-wide regex
            // cache. All selected invocations share this work across actions.
            let subscribed = oasis7_wasm_router::bounded::pre_action_matches_serialized_bounded_v1(
                action.envelope(),
                &first.manifest().subscriptions,
                &first.manifest().module_id,
                crate::world::module_runtime_labels::action_kind_label(&action.envelope().action),
                &mut self.work.routing,
                &mut self.ctx.cancelled,
                |bytes| memory.reserve(bytes, MemoryClass::Ordinary),
            )
            .map_err(RootModuleExecutionErrorV1::Routing)?;
            if !subscribed {
                return Ok(None);
            }
            input.ctx.trace_id = format!("action-{}-{}", action.envelope().id, first.instance_id());
            input.ctx.origin = ModuleCallOrigin {
                kind: "action".into(),
                id: action.envelope().id.to_string(),
            };
            input.ctx.caller = ModuleCallCaller::LegacyUnspecified;
            input.ctx.stage = Some("pre_action".into());
            input.action = Some(encode_input(
                action.envelope(),
                memory,
                &mut self.work.input,
                self.work.max_input_bytes,
                &mut self.ctx.cancelled,
            )?);
        }
        let trace_id = input.ctx.trace_id.clone();
        let input = encode_input(
            &input,
            memory,
            &mut self.work.input,
            self.work.max_input_bytes,
            &mut self.ctx.cancelled,
        )?;
        // Real second selection binds the strict post-input-read revision. No
        // caller boolean/guard replacement blesses the stale first selection.
        let live = live_owner();
        let selected = self
            .select(invocation_id, &live)?
            .ok_or(ValueTreeError::Invalid(
                "root module disappeared during input preparation",
            ))?;
        if first.module_id() != selected.module_id()
            || first.instance_id() != selected.instance_id()
            || first.install_target() != selected.install_target()
            || first.manifest_sha256() != selected.manifest_sha256()
            || first.manifest().wasm_hash != selected.manifest().wasm_hash
        {
            return Err(
                ValueTreeError::Invalid("root module changed during input preparation").into(),
            );
        }
        drop(first);
        self.ctx
            .transaction
            .validate_guard(selected.guard(), &live)?;
        let input_bytes = input.len() as u64;
        let request = ModuleCallRequest {
            module_id: selected.module_id().to_owned(),
            wasm_hash: selected.manifest().wasm_hash.clone(),
            trace_id: trace_id.clone(),
            entrypoint: selected.manifest().kind.entrypoint().to_owned(),
            input,
            limits: selected.manifest().limits.clone(),
            wasm_bytes: Arc::clone(&selected.wasm),
        };
        #[cfg(test)]
        if let Some(observation) = self.input_observation.as_mut() {
            **observation = Some((
                request.input.len(),
                *blake3::hash(&request.input).as_bytes(),
            ));
        }
        let output = match execute_root_module_worker_v1(
            self.ctx.transaction,
            self.worker,
            &request,
            memory,
            &mut self.work.output,
            &mut self.ctx.cancelled,
        ) {
            Ok(output) => output,
            Err(RootModuleWorkerErrorV1::Output(RootModuleOutputErrorV1::Sandbox(failure))) => {
                let bytes = failure
                    .module_id
                    .len()
                    .checked_add(failure.trace_id.len())
                    .and_then(|n| n.checked_add(failure.detail.len()))
                    .and_then(|n| n.checked_mul(2))
                    .and_then(|n| n.checked_add(8192))
                    .ok_or(ValueTreeError::ResourceLimited("module VM failure owner"))?;
                let charge = memory
                    .reserve(bytes, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?;
                return Err(RootModuleExecutionErrorV1::Business(charge.retain(
                    WorldError::ModuleCallFailed {
                        module_id: failure.module_id.clone(),
                        trace_id: failure.trace_id.clone(),
                        code: failure.code.clone(),
                        detail: failure.detail.clone(),
                    },
                )));
            }
            Err(error) => return Err(RootModuleExecutionErrorV1::Worker(error)),
        };
        drop(request);
        let live = live_owner();
        self.ctx
            .transaction
            .validate_guard(selected.guard(), &live)?;
        let bytes = application_admission(
            &selected,
            &output,
            &trace_id,
            self.work,
            &mut self.ctx.cancelled,
        )?;
        let phase = memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let legacy_executable: WorkerExecutableV1<'v> = match &self.worker.executable {
            WorkerExecutableV1::SealedFd(file) => WorkerExecutableV1::SealedFd(file),
            WorkerExecutableV1::FixturePath(path) => WorkerExecutableV1::FixturePath(path),
        };
        // The shared old kernel carries its sandbox argument to the target.
        // Supply the same real backend, never an FnOk/fake execution callback.
        let mut legacy = BoundedWorkerSandboxV1 {
            executable: legacy_executable,
            allocator: self.worker.allocator,
            cancelled: self.worker.cancelled,
            limits: WorkerLimitsV1 {
                address_space_bytes: self.worker.limits.address_space_bytes,
                max_request_bytes: self.worker.limits.max_request_bytes,
                max_output_bytes: self.worker.limits.max_output_bytes,
                wall_time_ms: self.worker.limits.wall_time_ms,
            },
        };
        let hook_slots = self.work.max_policy_selections;
        let effect_slots = self.work.max_prepared_effects;
        let clone_slots = hook_slots
            .checked_mul(2)
            .ok_or(ValueTreeError::ResourceLimited("root module clone slots"))?;
        let mut target = Target {
            owners: RefCell::new(Owners {
                families: self.families,
                selected: &selected,
                now: self.now,
                state: self.state,
                ctx: &mut *self.ctx,
                members: &mut *self.members,
                fees: &mut *self.fees,
                events: &mut *self.events,
                originals: &mut *self.originals,
                worker: &mut *self.worker,
                world_hash: &hash,
                work: &mut *self.work,
                live,
                live_owner,
                hooks: Vec::with_capacity(hook_slots),
                intents: Vec::with_capacity(effect_slots),
                clones: Vec::with_capacity(clone_slots),
                sticky: None,
            }),
        };
        let result = crate::world::module_runtime::process_module_output_for_target(
            &mut target,
            selected.module_id(),
            selected.instance_id(),
            &trace_id,
            selected.manifest(),
            input_bytes,
            &output,
            &mut legacy,
        );
        let mut owners = target.owners.into_inner();
        if let Some(error) = owners.sticky.take() {
            return Err(error);
        }
        owners.poll_live()?;
        match result {
            Ok(()) => {
                if !owners.intents.is_empty() {
                    return Err(ValueTreeError::Invalid(
                        "root module unpublished prepared intents",
                    )
                    .into());
                }
                Ok(Some(output))
            }
            Err(error) => Err(RootModuleExecutionErrorV1::Business(phase.retain(error))),
        }
    }
}

fn application_admission(
    selected: &PreparedRootModuleInvocationV1,
    output: &ModuleOutput,
    trace: &str,
    work: &mut RootModuleExecutionWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<usize, ValueTreeError> {
    let mut fee = selected
        .manifest
        .reserved_bytes()
        .checked_add(selected.manifest.admitted_json_bytes_v1()?)
        .and_then(|n| n.checked_mul(16))
        .and_then(|n| n.checked_add(32 << 10))
        .ok_or(ValueTreeError::ResourceLimited(
            "root module output application",
        ))?;
    let mut add = |n: usize| -> Result<(), ValueTreeError> {
        fee = fee.checked_add(n).ok_or(ValueTreeError::ResourceLimited(
            "root module application heap",
        ))?;
        Ok(())
    };
    add(work
        .max_input_bytes
        .checked_mul(2)
        .ok_or(ValueTreeError::ResourceLimited("root module hook inputs"))?)?;
    add(work
        .max_prepared_effects
        .checked_mul(std::mem::size_of::<ReservedValue<PreparedRootModuleEffectV1>>())
        .ok_or(ValueTreeError::ResourceLimited("root module effect slots"))?)?;
    add(work
        .max_policy_selections
        .checked_mul(
            std::mem::size_of::<PreparedRootModuleInvocationV1>()
                + 2 * std::mem::size_of::<MemoryReservation>(),
        )
        .ok_or(ValueTreeError::ResourceLimited("root module hook slots"))?)?;
    add(trace
        .len()
        .checked_add(selected.module_id().len())
        .and_then(|n| n.checked_mul(32))
        .ok_or(ValueTreeError::ResourceLimited("root module trace copies"))?)?;
    if let Some(state) = &output.new_state {
        add(state
            .len()
            .checked_mul(4)
            .ok_or(ValueTreeError::ResourceLimited("root module state copies"))?)?;
    }
    let mut shape = EffectCloneWorkV1 {
        nodes: work.effects.clone_nodes_remaining,
        bytes: work.effects.clone_bytes_remaining,
        max_depth: work.effects.max_json_depth,
    };
    let result = (|| {
        for effect in &output.effects {
            add(
                json_clone_bound_v1(&effect.params, &mut shape, 0, cancelled)?
                    .checked_mul(16)
                    .ok_or(ValueTreeError::ResourceLimited("root module effect copies"))?,
            )?;
            add(effect
                .kind
                .len()
                .checked_add(effect.cap_ref.len())
                .and_then(|n| n.checked_add(effect.cap_slot.as_ref().map_or(0, String::len)))
                .and_then(|n| n.checked_mul(32))
                .ok_or(ValueTreeError::ResourceLimited(
                    "root module capability copies",
                ))?)?;
        }
        for emit in &output.emits {
            add(
                json_clone_bound_v1(&emit.payload, &mut shape, 0, cancelled)?
                    .checked_mul(16)
                    .ok_or(ValueTreeError::ResourceLimited("root module emit copies"))?,
            )?;
            add(emit
                .kind
                .len()
                .checked_mul(32)
                .ok_or(ValueTreeError::ResourceLimited("root module emit labels"))?)?;
        }
        Ok::<(), ValueTreeError>(())
    })();
    work.effects.clone_nodes_remaining = shape.nodes;
    work.effects.clone_bytes_remaining = shape.bytes;
    result?;
    if fee > work.transient_heap_bytes {
        return Err(ValueTreeError::ResourceLimited(
            "root module application admission",
        ));
    }
    Ok(fee)
}

#[path = "incremental_module_execution_target_due.rs"]
mod due;
pub use due::RootModuleTickOutcomeV1;
