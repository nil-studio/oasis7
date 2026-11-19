//! Unique sealed-worker input and application kernel for selected calls.
use super::*;
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
    pub(super) fn execute_captured_input_invocation_inner(
        &mut self,
        invocation_id: &str,
        action: Option<PreActionEnvelopeV1<'_>>,
        capture: Option<&RefCell<rule_capture::RuleOutputCaptureV1>>,
        route: Option<(
            oasis7_wasm_abi::ModuleSubscriptionStage,
            Option<&crate::world::incremental_module_execution_event::PersistedWorldEventV1<'_>>,
            Option<u64>,
        )>,
        product: Option<&EconomyInputV1<'_, '_>>,
        captured_tick: Option<(
            crate::world::incremental_normal_gameplay_cycles::DueGameplayTickSlotV1,
            oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
        )>,
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
        // Economy calls resolve the actual active registry before their
        // world-manifest input, preserving the original inactive-module error.
        let product_first = if product.is_some() {
            match self.select_registry(invocation_id, &live)? {
                Some(selected) => Some(selected),
                None => {
                    let bytes = "module not active "
                        .len()
                        .checked_add(invocation_id.len())
                        .ok_or(ValueTreeError::ResourceLimited(
                            "product inactive module diagnostic",
                        ))?;
                    self.work.input.visits_remaining =
                        self.work.input.visits_remaining.checked_sub(1).ok_or(
                            ValueTreeError::ResourceLimited("product inactive module visits"),
                        )?;
                    self.work.input.bytes_remaining = self
                        .work
                        .input
                        .bytes_remaining
                        .checked_sub(bytes as u64)
                        .ok_or(ValueTreeError::ResourceLimited(
                            "product inactive module bytes",
                        ))?;
                    let fee = self
                        .ctx
                        .values
                        .memory
                        .reserve(
                            bytes.checked_add(std::mem::size_of::<WorldError>()).ok_or(
                                ValueTreeError::ResourceLimited("product inactive module layout"),
                            )?,
                            MemoryClass::Ordinary,
                        )
                        .map_err(ValueTreeError::Memory)?;
                    return Err(RootModuleExecutionErrorV1::Business(fee.retain(
                        WorldError::ModuleChangeInvalid {
                            reason: format!("module not active {invocation_id}"),
                        },
                    )));
                }
            }
        } else {
            None
        };
        let ordinary_hash;
        let captured_hash;
        let (first, hash) = if let Some((slot, core)) = captured_tick {
            self.work.input.visits_remaining =
                self.work.input.visits_remaining.checked_sub(2).ok_or(
                    ValueTreeError::ResourceLimited("captured Tick artifact source visits"),
                )?;
            self.work.input.bytes_remaining =
                self.work.input.bytes_remaining.checked_sub(64).ok_or(
                    ValueTreeError::ResourceLimited("captured Tick artifact source bytes"),
                )?;
            let (selected, owner) = crate::world::incremental_module_invocation::finish_captured_gameplay_tick_manifest_v1(
                slot, core, self.ctx, self.originals, &mut self.work.original,
                &mut self.work.selection_limits, &live,
            ).map_err(RootModuleExecutionErrorV1::Invocation)?;
            captured_hash = owner.ok_or_else(|| {
                RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                    "captured gameplay Tick world hash",
                ))
            })?;
            (
                selected.map(InputFirstSelectionV1::Captured),
                captured_hash.hash_owner(),
            )
        } else {
            ordinary_hash = prepare_current_world_manifest_hash_v1(
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
            let first = if product.is_some() {
                product_first
            } else {
                self.select(invocation_id, &live)?
            };
            (first.map(InputFirstSelectionV1::Ordinary), &ordinary_hash)
        };
        let Some(first) = first else {
            return Ok(None);
        };
        if let Some((_, Some(event), _)) = route {
            event.validate(self.ctx.transaction)?;
        }
        if action.is_none()
            && product.is_none()
            && route.is_none()
            && !crate::world::module_tick_runtime::module_has_tick_subscription(first.manifest())
        {
            return Ok(None);
        }
        let memory = self.ctx.values.memory;
        // A pre-action owns both the nested action bytes and the final input
        // at the same time. Reserve both Vec capacities before either encode.
        let input_copies = 1
            + usize::from(product.is_some())
            + usize::from(action.is_some())
            + usize::from(route.is_some_and(|(_, event, _)| event.is_some()));
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
        let (origin_kind, origin_id, trace_id) = if let Some(product) = product {
            let trace = match product {
                EconomyInputV1::Due(product) => format!(
                    "action-{}-{}-product-{}",
                    product.job().job_id,
                    first.module_id(),
                    product.output_index()
                ),
                EconomyInputV1::Factory { call, .. } => call.trace_id().to_owned(),
            };
            ("action".to_owned(), trace.clone(), trace)
        } else {
            match first.install_target() {
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
                caller: if product.is_some() {
                    ModuleCallCaller::LegacyUnspecified
                } else {
                    ModuleCallCaller::System {
                        system_id: "module_tick".to_owned(),
                    }
                },
                limits: first.manifest().limits.clone(),
                stage: Some(
                    if product.is_some() {
                        "economy_action"
                    } else {
                        "tick"
                    }
                    .to_owned(),
                ),
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
        if let Some(product) = product {
            input.action = Some(match product {
                EconomyInputV1::Due(product) => {
                    let request =
                        crate::world::economy::ProductValidationModuleCallRequest::from_request(
                            product.request(),
                        );
                    encode_input(
                        &request,
                        memory,
                        &mut self.work.input,
                        self.work.max_input_bytes,
                        &mut self.ctx.cancelled,
                    )?
                }
                EconomyInputV1::Factory { call, .. } => encode_input(
                    call.request(),
                    memory,
                    &mut self.work.input,
                    self.work.max_input_bytes,
                    &mut self.ctx.cancelled,
                )?,
            });
        }
        if let Some(action) = action {
            oasis7_world_state::incremental_world_event_decode::verify_module_output_layout_inventory_v1(memory,MemoryClass::Ordinary)?;
            // The strict router owns its admitted arena through matching, never
            // constructs a serde_json BTreeMap or uses the process-wide regex
            // cache. All selected invocations share this work across actions.
            // Retain bounded parent diagnostics if a child/process error escapes.
            let error_fee = memory
                .reserve(16 << 10, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let mut regex =
                crate::world::incremental_module_execution_worker::RootRegexBackendV1::new(
                    &mut *self.worker,
                    memory,
                );
            let subscribed =
                oasis7_wasm_router::bounded::subscription_matches_serialized_bounded_with_regex_v1(
                    &action,
                    &first.manifest().subscriptions,
                    &first.manifest().module_id,
                    crate::world::module_runtime_labels::action_kind_label(action.action),
                    route.map_or(oasis7_wasm_abi::ModuleSubscriptionStage::PreAction, |r| r.0),
                    &mut self.work.routing,
                    &mut self.ctx.cancelled,
                    |bytes| memory.reserve(bytes, MemoryClass::Ordinary),
                    &mut regex,
                )
                .map_err(|error| RootModuleExecutionErrorV1::Routing(error_fee.retain(error)))?;
            if !subscribed {
                return Ok(None);
            }
            input.ctx.trace_id = format!("action-{}-{}", action.original.id, first.instance_id());
            input.ctx.origin = ModuleCallOrigin {
                kind: "action".into(),
                id: action.original.id.to_string(),
            };
            input.ctx.caller = ModuleCallCaller::LegacyUnspecified;
            input.ctx.stage = Some(
                if route.is_some() {
                    "post_action"
                } else {
                    "pre_action"
                }
                .into(),
            );
            input.action = Some(encode_input(
                &action,
                memory,
                &mut self.work.input,
                self.work.max_input_bytes,
                &mut self.ctx.cancelled,
            )?);
        }
        if let Some((stage, Some(event), era)) = route {
            let event = event.event();
            if stage == oasis7_wasm_abi::ModuleSubscriptionStage::PostEvent {
                let error_fee = memory
                    .reserve(16 << 10, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?;
                let mut regex =
                    crate::world::incremental_module_execution_worker::RootRegexBackendV1::new(
                        &mut *self.worker,
                        memory,
                    );
                if !oasis7_wasm_router::bounded::subscription_matches_serialized_bounded_with_regex_v1(
                    event, &first.manifest().subscriptions, &first.manifest().module_id,
                    crate::world::module_runtime_labels::event_kind_label(&event.body), stage,
                    &mut self.work.routing, &mut self.ctx.cancelled,
                    |bytes| memory.reserve(bytes, MemoryClass::Ordinary), &mut regex,
                ).map_err(|error| RootModuleExecutionErrorV1::Routing(error_fee.retain(error)))? { return Ok(None); }
                input.ctx.trace_id = era.map_or_else(
                    || format!("event-{}-{}", event.id, first.instance_id()),
                    |era| {
                        crate::world::economy_product_validation::product_validation_event_trace_id(
                            era,
                            event.id,
                            first.instance_id(),
                        )
                    },
                );
                input.ctx.time = event.time;
                input.ctx.journal_height = Some(event.id);
                input.ctx.origin = ModuleCallOrigin {
                    kind: "event".into(),
                    id: event.id.to_string(),
                };
                input.ctx.caller = ModuleCallCaller::LegacyUnspecified;
                input.ctx.stage = Some("post_event".into());
            }
            input.event = Some(encode_input(
                event,
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
        let selected = match first {
            InputFirstSelectionV1::Captured(first) => {
                self.work.input.visits_remaining =
                    self.work.input.visits_remaining.checked_sub(2).ok_or(
                        ValueTreeError::ResourceLimited("captured Tick input handoff visits"),
                    )?;
                self.work.input.bytes_remaining =
                    self.work.input.bytes_remaining.checked_sub(64).ok_or(
                        ValueTreeError::ResourceLimited("captured Tick input handoff bytes"),
                    )?;
                first
                    .finish_after_input(self.ctx, &live)
                    .map_err(RootModuleExecutionErrorV1::Body)?
            }
            InputFirstSelectionV1::Ordinary(first) => {
                let selected = (if product.is_some() {
                    self.select_registry(invocation_id, &live)?
                } else {
                    self.select(invocation_id, &live)?
                })
                .ok_or(ValueTreeError::Invalid(
                    "root module disappeared during input preparation",
                ))?;
                if first.module_id() != selected.module_id()
                    || first.instance_id() != selected.instance_id()
                    || first.install_target() != selected.install_target()
                    || first.manifest_sha256() != selected.manifest_sha256()
                    || first.manifest().wasm_hash != selected.manifest().wasm_hash
                {
                    return Err(ValueTreeError::Invalid(
                        "root module changed during input preparation",
                    )
                    .into());
                }
                drop(first);
                selected
            }
        };
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
        #[cfg(any(test, feature = "test-support"))]
        if let Some(observation) = self.input_observation.as_mut() {
            **observation = Some((
                request.input.len(),
                *blake3::hash(&request.input).as_bytes(),
            ));
        }
        scoped::verify_scoped_native_protocol_v1(self.worker, &request, memory, &self.work.output)
            .map_err(RootModuleExecutionErrorV1::Worker)?;
        self.ctx
            .transaction
            .validate_guard(selected.guard(), &live_owner())?;
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
                let failure = charge.retain(WorldError::ModuleCallFailed {
                    module_id: failure.module_id.clone(),
                    trace_id: failure.trace_id.clone(),
                    code: failure.code.clone(),
                    detail: failure.detail.clone(),
                });
                if let Some(product) = product {
                    *product.failure().borrow_mut() = Some(failure);
                    return Ok(None);
                }
                return Err(RootModuleExecutionErrorV1::Business(failure));
            }
            Err(error) => return Err(RootModuleExecutionErrorV1::Worker(error)),
        };
        let capture_slot = if let Some(capture) = capture {
            Some(
                capture
                    .borrow_mut()
                    .begin(selected.module_id(), &trace_id)?,
            )
        } else {
            None
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
            WorkerExecutableV1::NativeBoundSealedFd { file, descriptor } => {
                WorkerExecutableV1::NativeBoundSealedFd { file, descriptor }
            }
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
                world_hash: hash,
                work: &mut *self.work,
                live,
                live_owner,
                hooks: Vec::with_capacity(hook_slots),
                intents: Vec::with_capacity(effect_slots),
                clones: Vec::with_capacity(clone_slots),
                sticky: None,
                capture,
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
                if let (Some(capture), Some(slot)) = (capture, capture_slot) {
                    capture.borrow_mut().finish(slot, output)?;
                    Ok(None)
                } else {
                    Ok(Some(output))
                }
            }
            Err(error) => Err(RootModuleExecutionErrorV1::Business(phase.retain(error))),
        }
    }
}
