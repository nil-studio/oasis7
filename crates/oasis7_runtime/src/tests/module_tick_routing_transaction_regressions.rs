use super::super::*;
use super::modules::activate_module_manifest;
use super::signed_test_artifact_identity;
use oasis7_wasm_abi::{
    ModuleCallErrorCode, ModuleCallFailure, ModuleCallRequest, ModuleEffectIntent, ModuleOutput,
    ModuleSandbox, ModuleTickLifecycleDirective,
};

const EFFECT_CAP: &str = "cap.tick.route.effect";

#[derive(Clone)]
struct TickSandbox {
    output: ModuleOutput,
    calls: usize,
    fail_after_first: bool,
}

impl TickSandbox {
    fn new(output: ModuleOutput, fail_after_first: bool) -> Self {
        Self {
            output,
            calls: 0,
            fail_after_first,
        }
    }
}

impl ModuleSandbox for TickSandbox {
    fn call(&mut self, request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        let call = self.calls;
        self.calls = self.calls.saturating_add(1);
        if self.fail_after_first && call > 0 {
            return Err(ModuleCallFailure {
                module_id: request.module_id.clone(),
                trace_id: request.trace_id.clone(),
                code: ModuleCallErrorCode::Trap,
                detail: "late tick route failure after first module publication".to_string(),
            });
        }
        Ok(self.output.clone())
    }
}

fn tick_manifest(module_id: &str, wasm_hash: &str) -> ModuleManifest {
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
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::Tick),
            filters: None,
        }],
        required_caps: vec![EFFECT_CAP.to_string()],
        artifact_identity: Some(signed_test_artifact_identity(wasm_hash)),
        limits: ModuleLimits::unbounded(),
        abi_contract: ModuleAbiContract::default(),
    }
}

fn tick_output() -> ModuleOutput {
    ModuleOutput {
        new_state: Some(vec![0x54, 0x49, 0x43, 0x4b]),
        effects: vec![ModuleEffectIntent {
            kind: "http.request".to_string(),
            params: serde_json::json!({"url": "https://example.com/tick"}),
            cap_ref: EFFECT_CAP.to_string(),
            cap_slot: None,
        }],
        emits: Vec::new(),
        tick_lifecycle: Some(ModuleTickLifecycleDirective::WakeAfterTicks { ticks: 2 }),
        output_bytes: 4,
    }
}

fn suspend_tick_output() -> ModuleOutput {
    let mut output = tick_output();
    output.effects.clear();
    output.tick_lifecycle = Some(ModuleTickLifecycleDirective::Suspend);
    output
}

fn world_with_tick_modules() -> World {
    world_with_tick_module_kind(ModuleKind::Reducer)
}

fn world_with_tick_module_kind(kind: ModuleKind) -> World {
    let wasm_bytes = b"module-tick-routing-transaction";
    let wasm_hash = util::sha256_hex(wasm_bytes);
    let mut world = World::new();
    world.add_capability(CapabilityGrant::allow_all(EFFECT_CAP));
    world.set_policy(PolicySet::allow_all());
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .expect("register tick route artifact");
    for id in ["m.tick.a", "m.tick.b"] {
        let mut manifest = tick_manifest(id, wasm_hash.as_str());
        manifest.kind = kind.clone();
        manifest.exports = vec![kind.entrypoint().into()];
        activate_module_manifest(&mut world, manifest);
    }
    world
}

fn assert_tick_snapshot_unchanged(world: &World, before: &Snapshot, cache_before: usize) {
    assert_eq!(world.state(), &before.state);
    assert_eq!(world.pending_effects_len(), before.pending_effects.len());
    assert_eq!(world.module_cache_len(), cache_before);
    let after = world.snapshot();
    assert_eq!(after.module_tick_schedule, before.module_tick_schedule);
    assert_eq!(
        after.module_tick_routing_metrics,
        before.module_tick_routing_metrics
    );
}

#[test]
fn direct_tick_route_late_failure_rolls_back_schedule_metrics_and_business_state() {
    let mut world = world_with_tick_modules();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let root_before = world
        .current_state_root_hash()
        .expect("initial tick route root");
    let cache_before = world.module_cache_len();

    let error = world
        .route_tick_to_modules(&mut TickSandbox::new(tick_output(), true))
        .expect_err("late tick route failure must abort the complete direct route");
    let WorldError::ModuleCallFailed {
        module_id,
        trace_id,
        code,
        detail,
    } = error
    else {
        panic!("expected late tick module-call failure")
    };
    assert_eq!(module_id, "m.tick.b");
    assert_eq!(trace_id, "tick-0-m.tick.b");
    assert_eq!(code, ModuleCallErrorCode::Trap);
    assert_eq!(
        detail,
        "late tick route failure after first module publication"
    );
    assert_tick_snapshot_unchanged(&world, &snapshot_before, cache_before);
    assert_eq!(
        world
            .current_state_root_hash()
            .expect("tick route state root"),
        root_before
    );

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
        panic!("late tick route failure must retain one audit event")
    };
    assert_eq!(*audit_id, snapshot_before.last_event_id + 1);
    assert_eq!(failure.module_id, "m.tick.b");
    assert_eq!(failure.trace_id, "tick-0-m.tick.b");
    assert_eq!(failure.code, ModuleCallErrorCode::Trap);
    assert_eq!(
        failure.detail,
        "late tick route failure after first module publication"
    );

    let before_record = consensus_before.last().expect("initial tick consensus");
    let after_record = world
        .tick_consensus_records()
        .last()
        .expect("tick failure consensus");
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
fn direct_tick_route_infrastructure_failure_is_atomic_and_retry_replays_exactly() {
    let mut world = world_with_tick_modules();
    let output = tick_output();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let root_before = world
        .current_state_root_hash()
        .expect("initial tick route root");
    world.fail_next_append_after_publication_prepare_for_test();

    let error = world
        .route_tick_to_modules(&mut TickSandbox::new(output.clone(), false))
        .expect_err("post-prepare tick route failure must abort before retry");
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
            .expect("tick route state root"),
        root_before
    );

    let mut control = world_with_tick_modules();
    control
        .route_tick_to_modules(&mut TickSandbox::new(output.clone(), false))
        .expect("control tick route");
    world
        .route_tick_to_modules(&mut TickSandbox::new(output, false))
        .expect("retry tick route");
    assert_eq!(world.snapshot(), control.snapshot());
    assert_eq!(world.journal(), control.journal());
    assert_eq!(
        world.tick_consensus_records(),
        control.tick_consensus_records()
    );

    let authority_source = world.tick_consensus_authority_source().to_string();
    world
        .record_tick_consensus_authority_for_tick(0, authority_source.as_str())
        .expect("finalize tick route consensus before replay");
    control
        .record_tick_consensus_authority_for_tick(0, authority_source.as_str())
        .expect("finalize control tick route consensus before replay");

    let replayed = World::from_snapshot(world.snapshot(), world.journal().clone())
        .expect("restore tick route snapshot");
    assert_eq!(replayed.snapshot(), world.snapshot());
    assert_eq!(replayed.journal(), world.journal());
    assert_eq!(
        replayed.tick_consensus_records(),
        world.tick_consensus_records()
    );
}

#[test]
fn direct_tick_route_empty_schedule_only_updates_metrics_without_consensus() {
    let mut world = World::new();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    assert_eq!(
        world
            .route_tick_to_modules(&mut TickSandbox::new(tick_output(), false))
            .expect("empty tick route"),
        0
    );
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());
    let metrics = world.snapshot().module_tick_routing_metrics;
    assert_eq!(
        metrics.routing_count,
        snapshot_before.module_tick_routing_metrics.routing_count + 1
    );
    assert_eq!(metrics.last_due_count, 0);
    assert_eq!(metrics.last_invoked_count, 0);
    assert_eq!(metrics.last_missing_invocation_count, 0);
}

#[test]
fn direct_tick_route_missing_invocation_removes_only_stale_schedule_and_records_metrics() {
    let base = World::new();
    let mut snapshot = base.snapshot();
    snapshot
        .module_tick_schedule
        .insert("missing.instance".to_string(), 0);
    let mut world =
        World::from_snapshot(snapshot, base.journal().clone()).expect("missing tick world");
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    assert_eq!(
        world
            .route_tick_to_modules(&mut TickSandbox::new(tick_output(), false))
            .expect("missing tick route"),
        0
    );
    let after = world.snapshot();
    assert!(after.module_tick_schedule.is_empty());
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());
    assert_eq!(after.module_tick_routing_metrics.last_due_count, 1);
    assert_eq!(after.module_tick_routing_metrics.last_invoked_count, 0);
    assert_eq!(
        after
            .module_tick_routing_metrics
            .last_missing_invocation_count,
        1
    );
    assert_eq!(
        after.module_tick_routing_metrics.missing_invocation_count,
        1
    );
}

#[test]
fn tick_route_wake_then_suspend_persists_post_route_snapshot() {
    let mut world = world_with_tick_modules();
    let mut sandbox = TickSandbox::new(tick_output(), false);

    assert_eq!(
        world
            .route_tick_to_modules(&mut sandbox)
            .expect("wake tick route"),
        2
    );
    let after_wake = world.snapshot();
    assert_eq!(after_wake.module_tick_schedule.get("m.tick.a"), Some(&2));
    assert_eq!(after_wake.module_tick_schedule.get("m.tick.b"), Some(&2));

    sandbox.output = suspend_tick_output();
    world
        .step_with_modules(&mut sandbox)
        .expect("advance to suspended tick");
    world
        .step_with_modules(&mut sandbox)
        .expect("suspend tick route");
    let after_suspend = world.snapshot();
    assert!(after_suspend.module_tick_schedule.is_empty());
    assert_eq!(after_suspend.state.time, 2);
    assert_eq!(
        after_suspend.module_tick_routing_metrics.last_invoked_count,
        2
    );
    let restored = World::from_snapshot(after_suspend, world.journal().clone())
        .expect("restore post-route tick snapshot");
    assert_eq!(restored.snapshot(), world.snapshot());
}

#[test]
fn complete_delta_preserves_two_instance_pure_wake_clear_and_next_due_work() {
    let mut live = world_with_tick_module_kind(ModuleKind::Pure);
    let checkpoint = live.snapshot();
    let mut pure_output = tick_output();
    pure_output.new_state = None;
    pure_output.effects.clear();
    let mut sandbox = TickSandbox::new(pure_output.clone(), false);
    live.route_tick_to_modules(&mut sandbox).unwrap();
    let wake = live.snapshot();
    assert_eq!(wake.module_tick_schedule.len(), 2);
    assert_eq!(wake.state.module_states, checkpoint.state.module_states);
    let delta = SnapshotDeltaV1::between(&checkpoint, &wake).unwrap();
    let mut restored =
        World::from_snapshot(delta.apply(&checkpoint).unwrap(), live.journal().clone()).unwrap();
    let mut recovered_sandbox = TickSandbox::new(pure_output.clone(), false);
    for world in [&mut live, &mut restored] {
        world
            .step_with_modules(&mut TickSandbox::new(pure_output.clone(), false))
            .unwrap();
    }
    assert_eq!(live.snapshot(), restored.snapshot());
    assert_eq!(
        live.snapshot()
            .module_tick_routing_metrics
            .last_invoked_count,
        0
    );
    sandbox.output.tick_lifecycle = Some(ModuleTickLifecycleDirective::Suspend);
    recovered_sandbox.output.tick_lifecycle = Some(ModuleTickLifecycleDirective::Suspend);
    live.step_with_modules(&mut sandbox).unwrap();
    restored.step_with_modules(&mut recovered_sandbox).unwrap();
    assert_eq!(live.snapshot(), restored.snapshot());
    assert_eq!(
        live.snapshot()
            .module_tick_routing_metrics
            .last_invoked_count,
        2
    );
    let clear = SnapshotDeltaV1::between(&wake, &live.snapshot()).unwrap();
    let cleared = clear.apply(&wake).unwrap();
    assert!(cleared.module_tick_schedule.is_empty());
    let mut cleared_world = World::from_snapshot(cleared, live.journal().clone()).unwrap();
    let mut no_calls = TickSandbox::new(pure_output, false);
    cleared_world.step_with_modules(&mut no_calls).unwrap();
    assert_eq!(no_calls.calls, 0);
}
