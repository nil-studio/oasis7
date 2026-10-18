#[derive(Default)]
struct TickLifecycleSandbox {
    calls: Vec<oasis7_wasm_abi::ModuleCallRequest>,
    outputs: std::collections::VecDeque<oasis7_wasm_abi::ModuleOutput>,
}

impl TickLifecycleSandbox {
    fn with_outputs(outputs: Vec<oasis7_wasm_abi::ModuleOutput>) -> Self {
        Self {
            calls: Vec::new(),
            outputs: outputs.into(),
        }
    }
}

struct CaptureContextSandbox {
    requests: Vec<oasis7_wasm_abi::ModuleCallRequest>,
    outputs: std::collections::VecDeque<oasis7_wasm_abi::ModuleOutput>,
}

impl CaptureContextSandbox {
    fn with_outputs(outputs: Vec<oasis7_wasm_abi::ModuleOutput>) -> Self {
        Self {
            requests: Vec::new(),
            outputs: outputs.into(),
        }
    }
}

impl oasis7_wasm_abi::ModuleSandbox for CaptureContextSandbox {
    fn call(
        &mut self,
        request: &oasis7_wasm_abi::ModuleCallRequest,
    ) -> Result<oasis7_wasm_abi::ModuleOutput, oasis7_wasm_abi::ModuleCallFailure> {
        self.requests.push(request.clone());
        Ok(self
            .outputs
            .pop_front()
            .unwrap_or(oasis7_wasm_abi::ModuleOutput {
                new_state: None,
                effects: Vec::new(),
                emits: Vec::new(),
                tick_lifecycle: None,
                output_bytes: 0,
            }))
    }
}

include!("modules_registry.rs");
include!("modules_registry_shadow_validation.rs");
include!("modules_permissions.rs");
include!("modules_permissions_policy_hooks.rs");

#[test]
#[ignore = "local perf probe"]
fn perf_probe_runtime_module_routing_with_many_active_manifests() {
    const MODULE_COUNT: usize = 192;
    const ITERATIONS: usize = 80;

    let mut world = crate::World::new();
    world.set_policy(crate::PolicySet::allow_all());

    let wasm_bytes = b"module-perf-router";
    let wasm_hash = crate::util::sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    for idx in 0..MODULE_COUNT {
        activate_module_manifest(
            &mut world,
            crate::ModuleManifest {
                module_id: format!("m.perf-router.{idx:03}"),
                name: format!("PerfRouter{idx:03}"),
                version: "0.1.0".to_string(),
                kind: if idx % 5 == 0 {
                    crate::ModuleKind::Pure
                } else {
                    crate::ModuleKind::Reducer
                },
                role: crate::ModuleRole::Domain,
                wasm_hash: wasm_hash.clone(),
                interface_version: "wasm-1".to_string(),
                abi_contract: crate::ModuleAbiContract::default(),
                exports: vec![if idx % 5 == 0 { "call" } else { "reduce" }.to_string()],
                subscriptions: vec![
                    crate::ModuleSubscription {
                        event_kinds: vec!["domain.agent_registered".to_string()],
                        action_kinds: Vec::new(),
                        stage: Some(crate::ModuleSubscriptionStage::PostEvent),
                        filters: None,
                    },
                    crate::ModuleSubscription {
                        event_kinds: Vec::new(),
                        action_kinds: vec!["action.register_agent".to_string()],
                        stage: Some(crate::ModuleSubscriptionStage::PreAction),
                        filters: None,
                    },
                ],
                required_caps: Vec::new(),
                artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash.as_str())),
                limits: crate::ModuleLimits {
                    max_mem_bytes: 1024,
                    max_gas: 10_000,
                    max_call_rate: 1,
                    max_output_bytes: 1024,
                    max_effects: 0,
                    max_emits: 0,
                },
            },
        );
    }

    let event = crate::WorldEvent {
        id: 1,
        time: 1,
        caused_by: None,
        body: crate::WorldEventBody::Domain(
            crate::DomainEvent::AgentRegistered {
                agent_id: "agent-perf".to_string(),
                pos: pos(0, 0),
            },
        ),
    };
    let action = crate::ActionEnvelope {
        committed_recipe_origin: None,
        id: 1,
        action: crate::Action::RegisterAgent {
            agent_id: "agent-perf".to_string(),
            pos: pos(0, 0),
        },
    };

    let mut warmup_sandbox = CaptureContextSandbox::with_outputs(Vec::new());
    assert_eq!(
        world
            .route_event_to_modules(&event, &mut warmup_sandbox)
            .unwrap(),
        MODULE_COUNT
    );
    assert_eq!(
        world
            .route_action_to_modules(&action, &mut warmup_sandbox)
            .unwrap(),
        MODULE_COUNT
    );

    let event_started_at = std::time::Instant::now();
    let mut event_invoked = 0usize;
    for _ in 0..ITERATIONS {
        let mut sandbox = CaptureContextSandbox::with_outputs(Vec::new());
        event_invoked = event_invoked.saturating_add(std::hint::black_box(
            world.route_event_to_modules(&event, &mut sandbox).unwrap(),
        ));
    }
    let event_elapsed = event_started_at.elapsed();

    let action_started_at = std::time::Instant::now();
    let mut action_invoked = 0usize;
    for _ in 0..ITERATIONS {
        let mut sandbox = CaptureContextSandbox::with_outputs(Vec::new());
        action_invoked = action_invoked.saturating_add(std::hint::black_box(
            world
                .route_action_to_modules(&action, &mut sandbox)
                .unwrap(),
        ));
    }
    let action_elapsed = action_started_at.elapsed();

    eprintln!(
        "perf_probe_runtime_module_routing_with_many_active_manifests: modules={} iterations={} event_total_ms={:.3} event_avg_ms={:.3} action_total_ms={:.3} action_avg_ms={:.3} event_invoked={} action_invoked={}",
        MODULE_COUNT,
        ITERATIONS,
        event_elapsed.as_secs_f64() * 1000.0,
        event_elapsed.as_secs_f64() * 1000.0 / ITERATIONS as f64,
        action_elapsed.as_secs_f64() * 1000.0,
        action_elapsed.as_secs_f64() * 1000.0 / ITERATIONS as f64,
        event_invoked,
        action_invoked,
    );

    assert_eq!(event_invoked, MODULE_COUNT * ITERATIONS);
    assert_eq!(action_invoked, MODULE_COUNT * ITERATIONS);
}
