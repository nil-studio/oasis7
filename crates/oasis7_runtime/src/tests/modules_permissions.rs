#[cfg(not(feature = "wasmtime"))]
#[test]
fn wasm_executor_skeleton_reports_unavailable() {
    let mut sandbox =
        WasmExecutor::new(WasmExecutorConfig::default()).expect("initialize wasm executor");
    let request = ModuleCallRequest {
        module_id: "m.test".to_string(),
        wasm_hash: "hash".to_string(),
        trace_id: "trace-1".to_string(),
        entrypoint: "call".to_string(),
        input: vec![],
        limits: ModuleLimits::default(),
        wasm_bytes: Vec::new().into(),
    };

    let err = sandbox.call(&request).unwrap_err();
    assert_eq!(err.code, ModuleCallErrorCode::SandboxUnavailable);
    assert_eq!(err.module_id, "m.test");
    assert_eq!(err.trace_id, "trace-1");
}

#[test]
fn step_with_modules_routes_domain_events() {
    let mut world = World::new();
    world.set_policy(PolicySet::allow_all());

    let wasm_bytes = b"module-router";
    let wasm_hash = util::sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.router".to_string(),
        name: "Router".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: vec!["domain.agent_registered".to_string()],
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::PostEvent),
            filters: None,
        }],
        required_caps: Vec::new(),
        artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 1024,
            max_gas: 10_000,
            max_call_rate: 1,
            max_output_bytes: 1024,
            max_effects: 0,
            max_emits: 1,
        },
    };

    let changes = ModuleChangeSet {
        register: vec![module_manifest.clone()],
        activate: vec![ModuleActivation {
            module_id: module_manifest.module_id.clone(),
            version: module_manifest.version.clone(),
        }],
        ..ModuleChangeSet::default()
    };

    let mut content = serde_json::Map::new();
    content.insert(
        "module_changes".to_string(),
        serde_json::to_value(&changes).unwrap(),
    );
    let manifest = Manifest {
        version: 2,
        content: serde_json::Value::Object(content),
    };

    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();
    world.apply_proposal(proposal_id).unwrap();

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });

    let output = ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "AgentRegistered".to_string(),
            payload: json!({"ok": true}),
        }],
        tick_lifecycle: None,
        output_bytes: 64,
    };
    let mut sandbox = CaptureContextSandbox::with_outputs(vec![output]);
    world.step_with_modules(&mut sandbox).unwrap();

    assert_eq!(sandbox.requests.len(), 1);
    let input: ModuleCallInput =
        serde_cbor::from_slice(&sandbox.requests[0].input).expect("decode event module input");
    assert_eq!(
        input.ctx.world_config_hash,
        Some(world.current_manifest_hash().unwrap())
    );
    assert_eq!(
        input.ctx.manifest_hash,
        Some(module_manifest_hash(&module_manifest))
    );

    let has_emit = world
        .journal()
        .events
        .iter()
        .any(|event| matches!(event.body, WorldEventBody::ModuleEmitted(_)));
    assert!(has_emit);
}

#[test]
fn step_with_modules_routes_actions() {
    let mut world = World::new();
    world.set_policy(PolicySet::allow_all());

    let wasm_bytes = b"module-action-router";
    let wasm_hash = util::sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.action-router".to_string(),
        name: "ActionRouter".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: Vec::new(),
            action_kinds: vec!["action.register_agent".to_string()],
            stage: Some(ModuleSubscriptionStage::PreAction),
            filters: None,
        }],
        required_caps: Vec::new(),
        artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 1024,
            max_gas: 10_000,
            max_call_rate: 1,
            max_output_bytes: 1024,
            max_effects: 0,
            max_emits: 1,
        },
    };

    let changes = ModuleChangeSet {
        register: vec![module_manifest.clone()],
        activate: vec![ModuleActivation {
            module_id: module_manifest.module_id.clone(),
            version: module_manifest.version.clone(),
        }],
        ..ModuleChangeSet::default()
    };

    let mut content = serde_json::Map::new();
    content.insert(
        "module_changes".to_string(),
        serde_json::to_value(&changes).unwrap(),
    );
    let manifest = Manifest {
        version: 2,
        content: serde_json::Value::Object(content),
    };

    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();
    world.apply_proposal(proposal_id).unwrap();

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });

    let output = ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "ActionSeen".to_string(),
            payload: json!({"agent": "agent-1"}),
        }],
        tick_lifecycle: None,
        output_bytes: 64,
    };
    let mut sandbox = CaptureContextSandbox::with_outputs(vec![output]);
    world.step_with_modules(&mut sandbox).unwrap();

    assert_eq!(sandbox.requests.len(), 1);
    let input: ModuleCallInput =
        serde_cbor::from_slice(&sandbox.requests[0].input).expect("decode action module input");
    assert_eq!(
        input.ctx.world_config_hash,
        Some(world.current_manifest_hash().unwrap())
    );
    assert_eq!(
        input.ctx.manifest_hash,
        Some(module_manifest_hash(&module_manifest))
    );

    let mut action_emit_index = None;
    let mut domain_event_index = None;
    for (idx, event) in world.journal().events.iter().enumerate() {
        match &event.body {
            WorldEventBody::ModuleEmitted(emit) if emit.trace_id.starts_with("action-") => {
                action_emit_index = Some(idx);
            }
            WorldEventBody::Domain(DomainEvent::AgentRegistered { agent_id, .. })
                if agent_id == "agent-1" =>
            {
                domain_event_index = Some(idx);
            }
            _ => {}
        }
    }

    let action_emit_index = action_emit_index.expect("expected action subscription emit");
    let domain_event_index = domain_event_index.expect("expected agent registration event");
    assert!(action_emit_index < domain_event_index);
}

#[test]
fn step_with_modules_routes_post_action_with_effective_action_and_result_event() {
    let mut world = World::new();
    world.set_policy(PolicySet::allow_all());

    let rule_wasm_bytes = b"module-post-action-override-rule";
    let rule_wasm_hash = util::sha256_hex(rule_wasm_bytes);
    world
        .register_module_artifact(rule_wasm_hash.clone(), rule_wasm_bytes)
        .unwrap();
    activate_module_manifest(
        &mut world,
        ModuleManifest {
            module_id: "m.rule.override".to_string(),
            name: "OverrideRule".to_string(),
            version: "0.1.0".to_string(),
            kind: ModuleKind::Reducer,
            role: ModuleRole::Rule,
            wasm_hash: rule_wasm_hash.clone(),
            interface_version: "wasm-1".to_string(),
            abi_contract: ModuleAbiContract::default(),
            exports: vec!["reduce".to_string()],
            subscriptions: vec![ModuleSubscription {
                event_kinds: Vec::new(),
                action_kinds: vec!["action.move_agent".to_string()],
                stage: Some(ModuleSubscriptionStage::PreAction),
                filters: None,
            }],
            required_caps: Vec::new(),
            artifact_identity: Some(super::signed_test_artifact_identity(
                rule_wasm_hash.as_str(),
            )),
            limits: ModuleLimits {
                max_mem_bytes: 1024,
                max_gas: 10_000,
                max_call_rate: 1,
                max_output_bytes: 1024,
                max_effects: 0,
                max_emits: 1,
            },
        },
    );

    let observer_wasm_bytes = b"module-post-action-observer";
    let observer_wasm_hash = util::sha256_hex(observer_wasm_bytes);
    world
        .register_module_artifact(observer_wasm_hash.clone(), observer_wasm_bytes)
        .unwrap();
    activate_module_manifest(
        &mut world,
        ModuleManifest {
            module_id: "m.post-action.observer".to_string(),
            name: "PostActionObserver".to_string(),
            version: "0.1.0".to_string(),
            kind: ModuleKind::Pure,
            role: ModuleRole::Domain,
            wasm_hash: observer_wasm_hash.clone(),
            interface_version: "wasm-1".to_string(),
            abi_contract: ModuleAbiContract::default(),
            exports: vec!["call".to_string()],
            subscriptions: vec![ModuleSubscription {
                event_kinds: Vec::new(),
                action_kinds: vec!["action.move_agent".to_string()],
                stage: Some(ModuleSubscriptionStage::PostAction),
                filters: None,
            }],
            required_caps: Vec::new(),
            artifact_identity: Some(super::signed_test_artifact_identity(
                observer_wasm_hash.as_str(),
            )),
            limits: ModuleLimits {
                max_mem_bytes: 1024,
                max_gas: 10_000,
                max_call_rate: 1,
                max_output_bytes: 1024,
                max_effects: 0,
                max_emits: 0,
            },
        },
    );

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    let action_id = world.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: pos(1, 0),
    });
    let override_action = Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: pos(9, 0),
    };
    let rule_output = ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "rule.decision".to_string(),
            payload: serde_json::to_value(RuleDecision {
                action_id,
                verdict: RuleVerdict::Modify,
                override_action: Some(override_action.clone()),
                cost: ResourceDelta::default(),
                notes: vec!["override".to_string()],
            })
            .unwrap(),
        }],
        tick_lifecycle: None,
        output_bytes: 128,
    };
    let observer_output = ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: Vec::new(),
        tick_lifecycle: None,
        output_bytes: 0,
    };
    let mut sandbox = CaptureContextSandbox::with_outputs(vec![rule_output, observer_output]);
    world.step_with_modules(&mut sandbox).unwrap();

    assert_eq!(sandbox.requests.len(), 2);
    let observer_input: ModuleCallInput =
        serde_cbor::from_slice(&sandbox.requests[1].input).expect("decode observer input");
    assert_eq!(observer_input.ctx.stage.as_deref(), Some("post_action"));
    let observed_action: ActionEnvelope = serde_cbor::from_slice(
        observer_input
            .action
            .as_deref()
            .expect("post_action action bytes"),
    )
    .expect("decode effective action");
    assert_eq!(observed_action.id, action_id);
    match observed_action.action {
        Action::MoveAgent { agent_id, to } => {
            assert_eq!(agent_id, "agent-1");
            assert_eq!(to, pos(9, 0));
        }
        other => panic!("unexpected observed action: {other:?}"),
    }

    let observed_event: WorldEvent = serde_cbor::from_slice(
        observer_input
            .event
            .as_deref()
            .expect("post_action result event bytes"),
    )
    .expect("decode post_action event");
    match observed_event.body {
        WorldEventBody::Domain(DomainEvent::AgentMoved { agent_id, to, .. }) => {
            assert_eq!(agent_id, "agent-1");
            assert_eq!(to, pos(9, 0));
        }
        other => panic!("unexpected post_action result event: {other:?}"),
    }
    assert_eq!(
        world.state().agents.get("agent-1").unwrap().state.pos,
        pos(9, 0)
    );
}

impl ModuleSandbox for TickLifecycleSandbox {
    fn call(&mut self, request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        self.calls.push(request.clone());
        Ok(self.outputs.pop_front().unwrap_or(ModuleOutput {
            new_state: None,
            effects: Vec::new(),
            emits: Vec::new(),
            tick_lifecycle: Some(ModuleTickLifecycleDirective::Suspend),
            output_bytes: 0,
        }))
    }
}

#[derive(Default)]
struct InstanceStateSandbox {
    traces: Vec<String>,
    states: Vec<Option<Vec<u8>>>,
    origin_kinds: Vec<String>,
}

impl ModuleSandbox for InstanceStateSandbox {
    fn call(&mut self, request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        let input: ModuleCallInput = serde_cbor::from_slice(&request.input).expect("decode input");
        self.traces.push(request.trace_id.clone());
        self.states.push(input.state.clone());
        self.origin_kinds.push(input.ctx.origin.kind);
        Ok(ModuleOutput {
            new_state: Some(request.trace_id.as_bytes().to_vec()),
            effects: Vec::new(),
            emits: Vec::new(),
            tick_lifecycle: Some(ModuleTickLifecycleDirective::WakeAfterTicks { ticks: 1 }),
            output_bytes: request.trace_id.len() as u64,
        })
    }
}

#[test]
fn step_with_modules_routes_tick_lifecycle_with_wake_and_suspend() {
    let mut world = World::new();
    world.set_policy(PolicySet::allow_all());

    let wasm_bytes = b"module-tick-router";
    let wasm_hash = util::sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.tick-router".to_string(),
        name: "TickRouter".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: Vec::new(),
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::Tick),
            filters: None,
        }],
        required_caps: Vec::new(),
        artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 1024,
            max_gas: 10_000,
            max_call_rate: 1,
            max_output_bytes: 1024,
            max_effects: 0,
            max_emits: 0,
        },
    };

    let changes = ModuleChangeSet {
        register: vec![module_manifest.clone()],
        activate: vec![ModuleActivation {
            module_id: module_manifest.module_id.clone(),
            version: module_manifest.version.clone(),
        }],
        ..ModuleChangeSet::default()
    };
    let mut content = serde_json::Map::new();
    content.insert(
        "module_changes".to_string(),
        serde_json::to_value(&changes).unwrap(),
    );
    let manifest = Manifest {
        version: 2,
        content: serde_json::Value::Object(content),
    };
    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();
    world.apply_proposal(proposal_id).unwrap();

    let mut sandbox = TickLifecycleSandbox::with_outputs(vec![
        ModuleOutput {
            new_state: None,
            effects: Vec::new(),
            emits: Vec::new(),
            tick_lifecycle: Some(ModuleTickLifecycleDirective::WakeAfterTicks { ticks: 2 }),
            output_bytes: 0,
        },
        ModuleOutput {
            new_state: None,
            effects: Vec::new(),
            emits: Vec::new(),
            tick_lifecycle: Some(ModuleTickLifecycleDirective::Suspend),
            output_bytes: 0,
        },
    ]);

    world.step_with_modules(&mut sandbox).expect("tick 1");
    world.step_with_modules(&mut sandbox).expect("tick 2");
    world.step_with_modules(&mut sandbox).expect("tick 3");
    world.step_with_modules(&mut sandbox).expect("tick 4");

    assert_eq!(
        sandbox.calls.len(),
        2,
        "tick module should run at t=1 and t=3"
    );
    let first_input: ModuleCallInput =
        serde_cbor::from_slice(&sandbox.calls[0].input).expect("decode first tick input");
    let second_input: ModuleCallInput =
        serde_cbor::from_slice(&sandbox.calls[1].input).expect("decode second tick input");
    let expected_manifest_hash = module_manifest_hash(&module_manifest);
    assert_eq!(first_input.ctx.stage.as_deref(), Some("tick"));
    assert_eq!(first_input.ctx.origin.kind, "tick");
    assert_eq!(
        first_input.ctx.world_config_hash,
        Some(world.current_manifest_hash().unwrap())
    );
    assert_eq!(
        first_input.ctx.manifest_hash,
        Some(expected_manifest_hash.clone())
    );
    assert_eq!(second_input.ctx.stage.as_deref(), Some("tick"));
    assert_eq!(second_input.ctx.origin.kind, "tick");
    assert_eq!(
        second_input.ctx.world_config_hash,
        Some(world.current_manifest_hash().unwrap())
    );
    assert_eq!(second_input.ctx.manifest_hash, Some(expected_manifest_hash));
}

#[test]
fn step_with_modules_routes_location_infrastructure_install_as_infrastructure_tick() {
    let mut world = World::new();
    world.set_policy(PolicySet::allow_all());

    world.submit_action(Action::RegisterAgent {
        agent_id: "installer-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register installer");
    world
        .set_agent_resource_balance("installer-1", ResourceKind::Electricity, 128)
        .expect("seed installer electricity");

    let wasm_bytes = b"module-infra-tick-router";
    let wasm_hash = util::sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.tick-router.infrastructure".to_string(),
        name: "TickRouterInfrastructure".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: Vec::new(),
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::Tick),
            filters: None,
        }],
        required_caps: Vec::new(),
        artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 1024,
            max_gas: 10_000,
            max_call_rate: 1,
            max_output_bytes: 1024,
            max_effects: 0,
            max_emits: 0,
        },
    };

    world.submit_action(Action::InstallModuleToTargetFromArtifact {
        installer_agent_id: "installer-1".to_string(),
        manifest: module_manifest.clone(),
        activate: true,
        install_target: ModuleInstallTarget::LocationInfrastructure {
            location_id: "loc-hub-1".to_string(),
        },
    });
    world
        .step()
        .expect("install module to infrastructure target");

    assert_eq!(
        world
            .state()
            .installed_module_targets
            .get(module_manifest.module_id.as_str()),
        Some(&ModuleInstallTarget::LocationInfrastructure {
            location_id: "loc-hub-1".to_string(),
        })
    );

    let mut sandbox = TickLifecycleSandbox::with_outputs(vec![ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: Vec::new(),
        tick_lifecycle: Some(ModuleTickLifecycleDirective::Suspend),
        output_bytes: 0,
    }]);

    world
        .step_with_modules(&mut sandbox)
        .expect("tick with modules");
    assert_eq!(sandbox.calls.len(), 1);
    let input: ModuleCallInput =
        serde_cbor::from_slice(&sandbox.calls[0].input).expect("decode infrastructure tick input");
    assert_eq!(input.ctx.origin.kind, "infrastructure_tick");
    assert_eq!(input.ctx.origin.id, format!("loc-hub-1:{}", input.ctx.time));
    assert_eq!(input.ctx.stage.as_deref(), Some("tick"));
}

#[test]
fn step_with_modules_routes_same_module_id_as_isolated_instances() {
    let mut world = World::new();
    world.set_policy(PolicySet::allow_all());
    world.submit_action(Action::RegisterAgent {
        agent_id: "installer-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register installer");
    world
        .set_agent_resource_balance("installer-1", ResourceKind::Electricity, 256)
        .expect("seed installer resources");
    world
        .set_agent_resource_balance("installer-1", ResourceKind::Data, 256)
        .expect("seed installer data");

    let wasm_bytes = b"module-instance-tick-router";
    let wasm_hash = util::sha256_hex(wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "installer-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes: wasm_bytes.to_vec(),
    });
    world.step().expect("deploy artifact");

    let module_manifest = ModuleManifest {
        module_id: "m.tick-router.instance".to_string(),
        name: "TickRouterInstance".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: Vec::new(),
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::Tick),
            filters: None,
        }],
        required_caps: Vec::new(),
        artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 1024,
            max_gas: 10_000,
            max_call_rate: 1,
            max_output_bytes: 1024,
            max_effects: 0,
            max_emits: 0,
        },
    };

    world.submit_action(Action::InstallModuleFromArtifact {
        installer_agent_id: "installer-1".to_string(),
        manifest: module_manifest.clone(),
        activate: true,
    });
    world.step().expect("install self instance");
    world.submit_action(Action::InstallModuleToTargetFromArtifact {
        installer_agent_id: "installer-1".to_string(),
        manifest: module_manifest.clone(),
        activate: true,
        install_target: ModuleInstallTarget::LocationInfrastructure {
            location_id: "loc-instance".to_string(),
        },
    });
    world.step().expect("install infrastructure instance");

    assert_eq!(world.state().module_instances.len(), 2);

    let mut sandbox = InstanceStateSandbox::default();
    world
        .step_with_modules(&mut sandbox)
        .expect("first tick with module instances");
    assert_eq!(sandbox.traces.len(), 2);
    assert_eq!(sandbox.states.len(), 2);
    assert!(sandbox
        .states
        .iter()
        .all(|state| state.as_ref().is_none_or(Vec::is_empty)));
    assert!(sandbox.origin_kinds.iter().any(|kind| kind == "tick"));
    assert!(sandbox
        .origin_kinds
        .iter()
        .any(|kind| kind == "infrastructure_tick"));
    let first_tick_traces = sandbox.traces.clone();

    world
        .step_with_modules(&mut sandbox)
        .expect("second tick with module instances");
    assert_eq!(sandbox.traces.len(), 4);
    assert_eq!(sandbox.states.len(), 4);
    assert_eq!(
        sandbox.states[2].clone().expect("instance-1 state"),
        first_tick_traces[0].as_bytes().to_vec()
    );
    assert_eq!(
        sandbox.states[3].clone().expect("instance-2 state"),
        first_tick_traces[1].as_bytes().to_vec()
    );
}

#[derive(Default)]
struct CaptureEntrypointSandbox {
    entrypoints: Vec<String>,
}

impl ModuleSandbox for CaptureEntrypointSandbox {
    fn call(&mut self, request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        self.entrypoints.push(request.entrypoint.clone());
        Ok(ModuleOutput {
            new_state: None,
            effects: Vec::new(),
            emits: Vec::new(),
            tick_lifecycle: None,
            output_bytes: 0,
        })
    }
}

#[test]
fn module_calls_use_entrypoint_for_kind() {
    let mut world = World::new();
    world.set_policy(PolicySet::allow_all());

    let reducer_bytes = b"module-reducer";
    let reducer_hash = util::sha256_hex(reducer_bytes);
    world
        .register_module_artifact(reducer_hash.clone(), reducer_bytes)
        .unwrap();

    let pure_bytes = b"module-pure";
    let pure_hash = util::sha256_hex(pure_bytes);
    world
        .register_module_artifact(pure_hash.clone(), pure_bytes)
        .unwrap();

    let reducer_manifest = ModuleManifest {
        module_id: "m.reducer".to_string(),
        name: "Reducer".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: reducer_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: vec!["domain.agent_registered".to_string()],
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::PostEvent),
            filters: None,
        }],
        required_caps: Vec::new(),
        artifact_identity: Some(super::signed_test_artifact_identity(reducer_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 1024,
            max_gas: 10_000,
            max_call_rate: 1,
            max_output_bytes: 1024,
            max_effects: 0,
            max_emits: 0,
        },
    };

    let pure_manifest = ModuleManifest {
        module_id: "m.pure".to_string(),
        name: "Pure".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Pure,
        role: ModuleRole::Domain,
        wasm_hash: pure_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: vec!["domain.agent_registered".to_string()],
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::PostEvent),
            filters: None,
        }],
        required_caps: Vec::new(),
        artifact_identity: Some(super::signed_test_artifact_identity(pure_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 1024,
            max_gas: 10_000,
            max_call_rate: 1,
            max_output_bytes: 1024,
            max_effects: 0,
            max_emits: 0,
        },
    };

    let changes = ModuleChangeSet {
        register: vec![reducer_manifest.clone(), pure_manifest.clone()],
        activate: vec![
            ModuleActivation {
                module_id: reducer_manifest.module_id.clone(),
                version: reducer_manifest.version.clone(),
            },
            ModuleActivation {
                module_id: pure_manifest.module_id.clone(),
                version: pure_manifest.version.clone(),
            },
        ],
        ..ModuleChangeSet::default()
    };

    let mut content = serde_json::Map::new();
    content.insert(
        "module_changes".to_string(),
        serde_json::to_value(&changes).unwrap(),
    );
    let manifest = Manifest {
        version: 2,
        content: serde_json::Value::Object(content),
    };

    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();
    world.apply_proposal(proposal_id).unwrap();

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });

    let mut sandbox = CaptureEntrypointSandbox::default();
    world.step_with_modules(&mut sandbox).unwrap();

    assert!(sandbox.entrypoints.contains(&"reduce".to_string()));
    assert!(sandbox.entrypoints.contains(&"call".to_string()));
}
