use super::super::*;
use super::modules::activate_module_manifest;
use super::signed_test_artifact_identity;
use oasis7_wasm_abi::{
    ModuleCallErrorCode, ModuleCallFailure, ModuleCallInput, ModuleCallRequest, ModuleEffectIntent,
    ModuleEmit, ModuleOutput, ModuleSandbox,
};

const EFFECT_CAP: &str = "cap.event.route.effect";

#[derive(Clone)]
struct EventSandbox {
    output: ModuleOutput,
    calls: usize,
    fail_after_first: bool,
    requests: Vec<ModuleCallRequest>,
}

impl EventSandbox {
    fn new(output: ModuleOutput, fail_after_first: bool) -> Self {
        Self {
            output,
            calls: 0,
            fail_after_first,
            requests: Vec::new(),
        }
    }
}

impl ModuleSandbox for EventSandbox {
    fn call(&mut self, request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        let call = self.calls;
        self.calls = self.calls.saturating_add(1);
        self.requests.push(request.clone());
        if self.fail_after_first && call > 0 {
            return Err(ModuleCallFailure {
                module_id: request.module_id.clone(),
                trace_id: request.trace_id.clone(),
                code: ModuleCallErrorCode::Trap,
                detail: "late event route failure after first module publication".to_string(),
            });
        }
        Ok(self.output.clone())
    }
}

fn event_manifest(module_id: &str, wasm_hash: &str) -> ModuleManifest {
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
            event_kinds: vec!["domain.agent_registered".to_string()],
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::PostEvent),
            filters: None,
        }],
        required_caps: vec![EFFECT_CAP.to_string()],
        artifact_identity: Some(signed_test_artifact_identity(wasm_hash)),
        limits: ModuleLimits::unbounded(),
        abi_contract: ModuleAbiContract::default(),
    }
}

fn event_output() -> ModuleOutput {
    ModuleOutput {
        new_state: Some(vec![0x45, 0x56, 0x45, 0x4e, 0x54]),
        effects: vec![ModuleEffectIntent {
            kind: "http.request".to_string(),
            params: serde_json::json!({"url": "https://example.com/event"}),
            cap_ref: EFFECT_CAP.to_string(),
            cap_slot: None,
        }],
        emits: vec![ModuleEmit {
            kind: "event.module.emit".to_string(),
            payload: serde_json::json!({"route": "late-failure"}),
        }],
        tick_lifecycle: None,
        output_bytes: 32,
    }
}

fn routed_event() -> WorldEvent {
    WorldEvent {
        id: 900,
        time: 4,
        caused_by: None,
        body: WorldEventBody::Domain(DomainEvent::AgentRegistered {
            agent_id: "event-route-target".to_string(),
            pos: super::pos(0, 0),
        }),
    }
}

fn world_with_event_modules() -> World {
    let wasm_bytes = b"module-event-routing-transaction";
    let wasm_hash = util::sha256_hex(wasm_bytes);
    let mut world = World::new();
    world.add_capability(CapabilityGrant::allow_all(EFFECT_CAP));
    world.set_policy(PolicySet::allow_all());
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .expect("register event route artifact");
    activate_module_manifest(&mut world, event_manifest("m.event.a", wasm_hash.as_str()));
    activate_module_manifest(&mut world, event_manifest("m.event.b", wasm_hash.as_str()));
    world
}

fn assert_event_business_unchanged(
    world: &World,
    snapshot_before: &Snapshot,
    root_before: &str,
    cache_before: usize,
) {
    assert_eq!(world.state(), &snapshot_before.state);
    assert_eq!(
        world
            .current_state_root_hash()
            .expect("event route state root"),
        root_before
    );
    assert_eq!(
        world.pending_effects_len(),
        snapshot_before.pending_effects.len()
    );
    assert_eq!(world.module_cache_len(), cache_before);
}

#[test]
fn direct_event_route_preserves_context_and_stages_state_in_sorted_order() {
    let mut world = world_with_event_modules();
    let event = routed_event();
    let wasm_hash = util::sha256_hex(b"module-event-routing-transaction");
    let expected_event = util::to_canonical_cbor(&event).expect("canonical event bytes");
    let expected_world_config_hash = world.current_manifest_hash().expect("world config hash");
    let expected_manifest_hashes = [
        util::hash_json(&event_manifest("m.event.a", wasm_hash.as_str()))
            .expect("module a manifest hash"),
        util::hash_json(&event_manifest("m.event.b", wasm_hash.as_str()))
            .expect("module b manifest hash"),
    ];
    let mut sandbox = EventSandbox::new(event_output(), false);

    assert_eq!(
        world
            .route_event_to_modules(&event, &mut sandbox)
            .expect("event route"),
        2
    );
    assert_eq!(sandbox.calls, 2);
    assert_eq!(sandbox.requests.len(), 2);
    for (index, request) in sandbox.requests.iter().enumerate() {
        let input: ModuleCallInput =
            serde_cbor::from_slice(&request.input).expect("decode event route input");
        let expected_module_id = if index == 0 { "m.event.a" } else { "m.event.b" };
        assert_eq!(request.module_id, expected_module_id);
        assert_eq!(request.trace_id, format!("event-900-{expected_module_id}"));
        assert_eq!(input.event.as_deref(), Some(expected_event.as_slice()));
        assert_eq!(input.action, None);
        assert_eq!(input.ctx.origin.kind, "event");
        assert_eq!(input.ctx.origin.id, "900");
        assert_eq!(input.ctx.stage.as_deref(), Some("post_event"));
        assert_eq!(input.ctx.time, 4);
        assert_eq!(input.ctx.journal_height, Some(900));
        assert_eq!(
            input.ctx.world_config_hash,
            Some(expected_world_config_hash.clone())
        );
        assert_eq!(
            input.ctx.manifest_hash,
            Some(expected_manifest_hashes[index].clone())
        );
        assert_eq!(input.state, Some(Vec::new()));
    }
    let expected_state = event_output().new_state.expect("event output state");
    assert_eq!(
        world.snapshot().state.module_states.get("m.event.a"),
        Some(&expected_state)
    );
    assert_eq!(
        world.snapshot().state.module_states.get("m.event.b"),
        Some(&expected_state)
    );
}

#[test]
fn direct_event_route_late_failure_does_not_publish_state_or_effect() {
    let mut world = world_with_event_modules();
    let event = routed_event();
    let output = event_output();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let root_before = world
        .current_state_root_hash()
        .expect("initial event route root");
    let cache_before = world.module_cache_len();

    let error = world
        .route_event_to_modules(&event, &mut EventSandbox::new(output, true))
        .expect_err("late event route failure must abort the complete direct route");
    let WorldError::ModuleCallFailed {
        module_id,
        trace_id,
        code,
        detail,
    } = error
    else {
        panic!("expected late module-call failure")
    };
    assert_eq!(module_id, "m.event.b");
    assert_eq!(trace_id, "event-900-m.event.b");
    assert_eq!(code, ModuleCallErrorCode::Trap);
    assert_eq!(
        detail,
        "late event route failure after first module publication"
    );
    assert_event_business_unchanged(&world, &snapshot_before, root_before.as_str(), cache_before);

    let events = &world.journal().events;
    assert_eq!(
        &events[..journal_before.events.len()],
        journal_before.events.as_slice()
    );
    assert_eq!(events.len(), journal_before.events.len() + 1);
    let Some(WorldEvent {
        id: audit_id,
        body: WorldEventBody::ModuleCallFailed(failure),
        ..
    }) = events.last()
    else {
        panic!("late event route failure must retain one audit event")
    };
    assert_eq!(failure.module_id, "m.event.b");
    assert_eq!(failure.trace_id, "event-900-m.event.b");
    assert_eq!(failure.code, ModuleCallErrorCode::Trap);
    assert_eq!(
        failure.detail,
        "late event route failure after first module publication"
    );
    assert_eq!(*audit_id, snapshot_before.last_event_id + 1);
    assert_eq!(
        world.snapshot().last_event_id,
        snapshot_before.last_event_id + 1
    );
    let before_record = consensus_before.last().expect("initial event consensus");
    let after_record = world
        .tick_consensus_records()
        .last()
        .expect("event failure consensus");
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
fn direct_event_route_infrastructure_failure_is_atomic_and_retry_replays_exactly() {
    let mut world = world_with_event_modules();
    let event = routed_event();
    let output = event_output();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let root_before = world
        .current_state_root_hash()
        .expect("initial event route root");
    world.fail_next_append_after_publication_prepare_for_test();

    let error = world
        .route_event_to_modules(&event, &mut EventSandbox::new(output.clone(), false))
        .expect_err("post-prepare event route failure must abort before retry");
    assert!(
        matches!(&error, WorldError::ResourceBalanceInvalid { .. }),
        "unexpected infrastructure error: {error:?}"
    );
    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());
    assert_eq!(
        world
            .current_state_root_hash()
            .expect("event route state root"),
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
    let mut control = world_with_event_modules();
    control
        .route_event_to_modules(&event, &mut EventSandbox::new(output.clone(), false))
        .expect("control event route");
    world
        .route_event_to_modules(&event, &mut EventSandbox::new(output, false))
        .expect("retry event route");
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
        World::from_snapshot(replay_base, world.journal().clone()).expect("replay event route");
    assert_eq!(replayed.snapshot(), world.snapshot());
    assert_eq!(replayed.journal(), world.journal());
    assert_eq!(
        replayed.tick_consensus_records(),
        world.tick_consensus_records()
    );
}
