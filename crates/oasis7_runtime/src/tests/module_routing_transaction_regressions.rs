use super::super::*;
use super::modules::activate_module_manifest;
use super::signed_test_artifact_identity;
use oasis7_wasm_abi::{
    ModuleCallErrorCode, ModuleCallFailure, ModuleCallInput, ModuleCallRequest, ModuleEffectIntent,
    ModuleEmit, ModuleOutput, ModuleSandbox,
};

const EFFECT_CAP: &str = "cap.route.effect";

#[derive(Clone)]
struct RouteSandbox {
    output: ModuleOutput,
    calls: usize,
    fail_after_first: bool,
}

impl RouteSandbox {
    fn new(output: ModuleOutput, fail_after_first: bool) -> Self {
        Self {
            output,
            calls: 0,
            fail_after_first,
        }
    }
}

impl ModuleSandbox for RouteSandbox {
    fn call(&mut self, request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        let call = self.calls;
        self.calls = self.calls.saturating_add(1);
        if self.fail_after_first && call > 0 {
            return Err(ModuleCallFailure {
                module_id: request.module_id.clone(),
                trace_id: request.trace_id.clone(),
                code: ModuleCallErrorCode::Trap,
                detail: "late route failure after first module publication".to_string(),
            });
        }
        Ok(self.output.clone())
    }
}

#[derive(Default)]
struct CapturingRouteSandbox {
    outputs: Vec<ModuleOutput>,
    requests: Vec<ModuleCallRequest>,
}

impl CapturingRouteSandbox {
    fn with_outputs(outputs: Vec<ModuleOutput>) -> Self {
        Self {
            outputs,
            requests: Vec::new(),
        }
    }
}

impl ModuleSandbox for CapturingRouteSandbox {
    fn call(&mut self, request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        let call_index = self.requests.len();
        self.requests.push(request.clone());
        self.outputs
            .get(call_index)
            .cloned()
            .ok_or_else(|| ModuleCallFailure {
                module_id: request.module_id.clone(),
                trace_id: request.trace_id.clone(),
                code: ModuleCallErrorCode::Trap,
                detail: format!("missing captured output for call {call_index}"),
            })
    }
}

fn route_manifest(module_id: &str, wasm_hash: &str) -> ModuleManifest {
    ModuleManifest {
        module_id: module_id.to_string(),
        name: module_id.to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.to_string(),
        interface_version: "wasm-1".to_string(),
        exports: vec!["reduce".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: Vec::new(),
            action_kinds: vec!["action.register_agent".to_string()],
            stage: Some(ModuleSubscriptionStage::PreAction),
            filters: None,
        }],
        required_caps: vec![EFFECT_CAP.to_string()],
        artifact_identity: Some(signed_test_artifact_identity(wasm_hash)),
        limits: ModuleLimits::unbounded(),
        abi_contract: ModuleAbiContract::default(),
    }
}

fn route_artifact_hash() -> String {
    util::sha256_hex(b"module-routing-transaction")
}

fn route_output() -> ModuleOutput {
    ModuleOutput {
        new_state: Some(vec![0x52, 0x4f, 0x55, 0x54, 0x45]),
        effects: vec![ModuleEffectIntent {
            kind: "http.request".to_string(),
            params: serde_json::json!({"url": "https://example.com/route"}),
            cap_ref: EFFECT_CAP.to_string(),
            cap_slot: None,
        }],
        emits: vec![ModuleEmit {
            kind: "route.module.emit".to_string(),
            payload: serde_json::json!({"route": "late-failure"}),
        }],
        tick_lifecycle: None,
        output_bytes: 32,
    }
}

fn route_state_output(state: u8) -> ModuleOutput {
    ModuleOutput {
        new_state: Some(vec![state]),
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "route.module.state".to_string(),
            payload: serde_json::json!({"state": state}),
        }],
        tick_lifecycle: None,
        output_bytes: 1,
    }
}

fn route_action() -> ActionEnvelope {
    ActionEnvelope {
        committed_recipe_origin: None,
        id: 71,
        action: Action::RegisterAgent {
            agent_id: "route-target".to_string(),
            pos: super::pos(0, 0),
        },
    }
}

fn nonmatching_route_action() -> ActionEnvelope {
    ActionEnvelope {
        committed_recipe_origin: None,
        id: 72,
        action: Action::QueryObservation {
            agent_id: "route-target".to_string(),
        },
    }
}

fn world_with_route_modules() -> World {
    let wasm_bytes = b"module-routing-transaction";
    let wasm_hash = util::sha256_hex(wasm_bytes);
    let mut world = World::new();
    world.add_capability(CapabilityGrant::allow_all(EFFECT_CAP));
    world.set_policy(PolicySet::allow_all());
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .expect("register route artifact");
    activate_module_manifest(&mut world, route_manifest("m.route.a", wasm_hash.as_str()));
    activate_module_manifest(&mut world, route_manifest("m.route.b", wasm_hash.as_str()));
    world
}

fn decode_route_input(request: &ModuleCallRequest) -> ModuleCallInput {
    serde_cbor::from_slice(&request.input).expect("decode captured route input")
}

fn assert_route_business_unchanged(
    world: &World,
    snapshot_before: &Snapshot,
    root_before: &str,
    pending_before: usize,
    cache_before: usize,
) {
    assert_eq!(world.state(), &snapshot_before.state);
    assert_eq!(
        world.current_state_root_hash().expect("route state root"),
        root_before
    );
    assert_eq!(world.pending_effects_len(), pending_before);
    assert_eq!(world.module_cache_len(), cache_before);
}

#[test]
fn direct_action_route_without_matching_subscription_is_noop_and_preserves_failpoint() {
    let mut world = world_with_route_modules();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let root_before = world.current_state_root_hash().expect("initial route root");
    let backpressure_before = world.runtime_backpressure_stats().clone();
    let cache_before = world.module_cache_len();
    world.fail_next_append_after_publication_prepare_for_test();

    let mut no_match_sandbox = RouteSandbox::new(route_output(), false);
    assert_eq!(
        world
            .route_action_to_modules(&nonmatching_route_action(), &mut no_match_sandbox)
            .expect("nonmatching route is a no-op"),
        0
    );
    assert_eq!(no_match_sandbox.calls, 0);
    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());
    assert_eq!(world.runtime_backpressure_stats(), &backpressure_before);
    assert_eq!(
        world.current_state_root_hash().expect("route state root"),
        root_before
    );
    assert_eq!(world.module_cache_len(), cache_before);

    let error = world
        .route_action_to_modules(
            &route_action(),
            &mut RouteSandbox::new(route_output(), false),
        )
        .expect_err("a matching route must consume the pending failpoint");
    assert!(
        matches!(error, WorldError::ResourceBalanceInvalid { .. }),
        "unexpected infrastructure error: {error:?}"
    );
    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());
    assert_eq!(world.runtime_backpressure_stats(), &backpressure_before);
    assert_eq!(
        world.current_state_root_hash().expect("route state root"),
        root_before
    );
    assert_eq!(world.module_cache_len(), cache_before);
}

#[test]
fn direct_action_route_commits_sorted_staged_context_state_and_cache() {
    let mut world = world_with_route_modules();
    let action = route_action();
    let snapshot_before = world.snapshot();
    let root_before = world.current_state_root_hash().expect("initial route root");
    let journal_before = world.journal().clone();
    let initial_journal_len = journal_before.events.len();
    let initial_time = world.state().time;
    let world_config_hash = world.current_manifest_hash().expect("route config hash");
    let wasm_hash = route_artifact_hash();
    let manifest_hashes = [
        util::hash_json(&route_manifest("m.route.a", wasm_hash.as_str()))
            .expect("route a manifest hash"),
        util::hash_json(&route_manifest("m.route.b", wasm_hash.as_str()))
            .expect("route b manifest hash"),
    ];
    let mut sandbox = CapturingRouteSandbox::with_outputs(vec![
        route_state_output(0xa1),
        route_state_output(0xb2),
        route_state_output(0xc3),
        route_state_output(0xd4),
    ]);

    assert_eq!(
        world
            .route_action_to_modules(&action, &mut sandbox)
            .expect("first sorted route"),
        2
    );
    assert_eq!(world.module_cache_len(), 1);
    assert_eq!(
        world
            .route_action_to_modules(&action, &mut sandbox)
            .expect("second sorted route"),
        2
    );
    assert_eq!(world.module_cache_len(), 1);
    assert_eq!(sandbox.requests.len(), 4);

    let expected_modules = ["m.route.a", "m.route.b", "m.route.a", "m.route.b"];
    let expected_states = [
        Some(Vec::new()),
        Some(Vec::new()),
        Some(vec![0xa1]),
        Some(vec![0xb2]),
    ];
    let expected_heights = [
        initial_journal_len as u64,
        initial_journal_len as u64 + 2,
        initial_journal_len as u64 + 4,
        initial_journal_len as u64 + 6,
    ];
    let expected_action = util::to_canonical_cbor(&action).expect("canonical route action");
    for (index, request) in sandbox.requests.iter().enumerate() {
        let input = decode_route_input(request);
        assert_eq!(request.module_id, expected_modules[index]);
        assert_eq!(
            request.trace_id,
            format!("action-71-{}", expected_modules[index])
        );
        assert_eq!(request.entrypoint, "reduce");
        assert_eq!(request.wasm_hash, wasm_hash);
        assert_eq!(input.ctx.module_id, expected_modules[index]);
        assert_eq!(input.ctx.trace_id, request.trace_id);
        assert_eq!(input.ctx.time, initial_time);
        assert_eq!(input.ctx.origin.kind, "action");
        assert_eq!(input.ctx.origin.id, "71");
        assert_eq!(
            input.ctx.caller,
            oasis7_wasm_abi::ModuleCallCaller::LegacyUnspecified
        );
        assert_eq!(input.ctx.limits, ModuleLimits::unbounded());
        assert_eq!(input.ctx.stage.as_deref(), Some("pre_action"));
        assert_eq!(
            input.ctx.world_config_hash.as_deref(),
            Some(world_config_hash.as_str())
        );
        assert_eq!(
            input.ctx.manifest_hash.as_deref(),
            Some(manifest_hashes[index % 2].as_str())
        );
        assert_eq!(input.ctx.journal_height, Some(expected_heights[index]));
        assert_eq!(input.ctx.module_version.as_deref(), Some("0.1.0"));
        assert_eq!(input.ctx.module_kind.as_deref(), Some("reducer"));
        assert_eq!(input.ctx.module_role.as_deref(), Some("domain"));
        assert_eq!(input.event, None);
        assert_eq!(input.action, Some(expected_action.clone()));
        assert_eq!(input.state, expected_states[index]);
    }

    let expected_events = [
        ("m.route.a", "action-71-m.route.a", 0xa1),
        ("m.route.a", "action-71-m.route.a", 0xa1),
        ("m.route.b", "action-71-m.route.b", 0xb2),
        ("m.route.b", "action-71-m.route.b", 0xb2),
        ("m.route.a", "action-71-m.route.a", 0xc3),
        ("m.route.a", "action-71-m.route.a", 0xc3),
        ("m.route.b", "action-71-m.route.b", 0xd4),
        ("m.route.b", "action-71-m.route.b", 0xd4),
    ];
    let events = &world.journal().events;
    assert_eq!(
        events.len(),
        journal_before.events.len() + expected_events.len()
    );
    assert_eq!(
        &events[..journal_before.events.len()],
        journal_before.events.as_slice()
    );
    for (offset, (module_id, trace_id, state)) in expected_events.into_iter().enumerate() {
        let event = &events[journal_before.events.len() + offset];
        assert_eq!(event.id, snapshot_before.last_event_id + offset as u64 + 1);
        if offset % 2 == 0 {
            let WorldEventBody::ModuleStateUpdated(update) = &event.body else {
                panic!("expected staged module state event at offset {offset}")
            };
            assert_eq!(update.module_id, module_id);
            assert_eq!(update.trace_id, trace_id);
            assert_eq!(update.state, vec![state]);
        } else {
            let WorldEventBody::ModuleEmitted(emit) = &event.body else {
                panic!("expected staged module emit event at offset {offset}")
            };
            assert_eq!(emit.module_id, module_id);
            assert_eq!(emit.trace_id, trace_id);
            assert_eq!(emit.kind, "route.module.state");
            assert_eq!(emit.payload, serde_json::json!({"state": state}));
        }
    }
    assert_eq!(
        world.state().module_states.get("m.route.a").cloned(),
        Some(vec![0xc3])
    );
    assert_eq!(
        world.state().module_states.get("m.route.b").cloned(),
        Some(vec![0xd4])
    );
    let final_root = world.current_state_root_hash().expect("final route root");
    assert_ne!(final_root, root_before);
    assert_eq!(
        world
            .tick_consensus_records()
            .last()
            .expect("final route consensus")
            .block
            .header
            .state_root,
        final_root
    );
}

#[test]
fn direct_action_route_late_failure_does_not_publish_state_or_effect() {
    let mut world = world_with_route_modules();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let root_before = world.current_state_root_hash().expect("initial route root");
    let pending_before = world.pending_effects_len();
    let cache_before = world.module_cache_len();

    let error = world
        .route_action_to_modules(
            &route_action(),
            &mut RouteSandbox::new(route_output(), true),
        )
        .expect_err("late route failure must abort the complete direct route");
    let WorldError::ModuleCallFailed {
        module_id,
        trace_id,
        code,
        detail,
    } = error
    else {
        panic!("expected late module-call failure")
    };
    assert_eq!(module_id, "m.route.b");
    assert_eq!(trace_id, "action-71-m.route.b");
    assert_eq!(code, ModuleCallErrorCode::Trap);
    assert_eq!(detail, "late route failure after first module publication");
    assert_route_business_unchanged(
        &world,
        &snapshot_before,
        root_before.as_str(),
        pending_before,
        cache_before,
    );

    let events = &world.journal().events;
    assert_eq!(events.len(), journal_before.events.len() + 1);
    assert_eq!(
        &events[..journal_before.events.len()],
        journal_before.events.as_slice()
    );
    let Some(WorldEvent {
        id: audit_id,
        body: WorldEventBody::ModuleCallFailed(failure),
        ..
    }) = events.last()
    else {
        panic!("late route failure must retain one audit event")
    };
    assert_eq!(*audit_id, snapshot_before.last_event_id + 1);
    assert_eq!(failure.module_id, "m.route.b");
    assert_eq!(failure.trace_id, "action-71-m.route.b");
    assert_eq!(failure.code, ModuleCallErrorCode::Trap);
    assert_eq!(
        failure.detail,
        "late route failure after first module publication"
    );
    let snapshot_after = world.snapshot();
    assert_eq!(snapshot_after.state, snapshot_before.state);
    assert_eq!(
        snapshot_after.next_intent_id,
        snapshot_before.next_intent_id
    );
    assert_eq!(snapshot_after.intent_id_era, snapshot_before.intent_id_era);
    assert_eq!(
        snapshot_after.pending_effects,
        snapshot_before.pending_effects
    );

    let before_record = consensus_before.last().expect("initial route consensus");
    let after_record = world
        .tick_consensus_records()
        .last()
        .expect("route failure consensus");
    assert_eq!(
        after_record.block.ordered_event_ids.len(),
        before_record.block.ordered_event_ids.len() + 1
    );
    assert_eq!(
        &after_record.block.ordered_event_ids[..before_record.block.ordered_event_ids.len()],
        before_record.block.ordered_event_ids.as_slice()
    );
    assert_eq!(after_record.block.ordered_event_ids.last(), Some(audit_id));
    assert_eq!(after_record.block.header.state_root, root_before);
}

#[test]
fn direct_action_route_infrastructure_failure_is_atomic_and_retry_replays_exactly() {
    let mut world = world_with_route_modules();
    let action = route_action();
    let output = route_output();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let root_before = world.current_state_root_hash().expect("initial route root");
    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .route_action_to_modules(&action, &mut RouteSandbox::new(output.clone(), false))
        .expect_err("post-prepare route failure must abort before retry");
    assert!(
        matches!(&error, WorldError::ResourceBalanceInvalid { .. }),
        "unexpected infrastructure error: {error:?}"
    );
    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());
    assert_eq!(
        world.current_state_root_hash().expect("route state root"),
        root_before
    );
    assert!(
        !world
            .journal()
            .events
            .iter()
            .any(|event| matches!(event.body, WorldEventBody::ModuleCallFailed(_)))
    );

    let replay_base = world.snapshot();
    let mut control = world_with_route_modules();
    control
        .route_action_to_modules(&action, &mut RouteSandbox::new(output.clone(), false))
        .expect("control route");
    world
        .route_action_to_modules(&action, &mut RouteSandbox::new(output, false))
        .expect("retry route");
    assert_eq!(world.snapshot(), control.snapshot());
    assert_eq!(world.journal(), control.journal());
    assert_eq!(
        world.tick_consensus_records(),
        control.tick_consensus_records()
    );
    assert_eq!(
        world.runtime_backpressure_stats(),
        control.runtime_backpressure_stats()
    );

    let replayed =
        World::from_snapshot(replay_base, world.journal().clone()).expect("replay direct route");
    assert_eq!(replayed.snapshot(), world.snapshot());
    assert_eq!(replayed.journal(), world.journal());
    assert_eq!(
        replayed.tick_consensus_records(),
        world.tick_consensus_records()
    );
    assert_eq!(
        replayed.runtime_backpressure_stats(),
        world.runtime_backpressure_stats()
    );
}
