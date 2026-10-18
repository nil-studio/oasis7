use super::*;
use crate::state::ModuleInstanceState;
use crate::tests::signed_test_artifact_identity;
use crate::{
    ModuleAbiContract, ModuleRecord, ModuleRole, ModuleSubscription, ModuleSubscriptionStage,
};

fn manifest_with_subscription(wasm_hash: &str, event_kind: &str) -> ModuleManifest {
    ModuleManifest {
        module_id: "m.cache".to_string(),
        name: "Cache".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.to_string(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: vec![event_kind.to_string()],
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::PostEvent),
            filters: None,
        }],
        required_caps: Vec::new(),
        artifact_identity: Some(signed_test_artifact_identity(wasm_hash)),
        limits: ModuleLimits::unbounded(),
    }
}

fn tick_manifest(module_id: &str, wasm_hash: &str) -> ModuleManifest {
    ModuleManifest {
        module_id: module_id.to_string(),
        name: "TickModule".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.to_string(),
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
        artifact_identity: Some(signed_test_artifact_identity(wasm_hash)),
        limits: ModuleLimits::unbounded(),
    }
}

#[derive(Default)]
struct CountingSandbox {
    calls: usize,
}

impl ModuleSandbox for CountingSandbox {
    fn call(&mut self, _request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        self.calls += 1;
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
fn prepared_subscription_cache_key_tracks_manifest_identity() {
    let base = manifest_with_subscription("hash-a", "world.tick");
    let changed_hash = manifest_with_subscription("hash-b", "world.tick");
    let changed_subscription = manifest_with_subscription("hash-a", "world.event");

    let base_key = prepared_subscription_cache_key(&base).expect("base key");
    assert_ne!(
        base_key,
        prepared_subscription_cache_key(&changed_hash).expect("hash key")
    );
    assert_ne!(
        base_key,
        prepared_subscription_cache_key(&changed_subscription).expect("subscription key")
    );
}

#[test]
fn prepared_subscription_cache_lookup_key_uses_stable_record_identity() {
    let base = manifest_with_subscription("hash-a", "world.tick");
    let changed_hash = manifest_with_subscription("hash-b", "world.tick");
    let changed_subscription = manifest_with_subscription("hash-a", "world.event");

    let base_lookup_key = prepared_subscription_lookup_key(&base);
    assert_ne!(
        base_lookup_key,
        prepared_subscription_lookup_key(&changed_hash)
    );
    assert_eq!(
        base_lookup_key,
        prepared_subscription_lookup_key(&changed_subscription)
    );
}

#[test]
fn prepared_subscription_cache_entry_keeps_full_subscription_fingerprint() {
    let manifest = manifest_with_subscription("hash-a", "world.tick");
    let expected_fingerprint = prepared_subscription_cache_key(&manifest).expect("fingerprint");
    let lookup_key = prepared_subscription_lookup_key(&manifest);
    let mut world = World::new();

    let prepared = world
        .prepared_subscriptions_for_manifest(&manifest)
        .expect("prepared subscriptions");
    let entry = world
        .prepared_subscription_cache
        .get(&lookup_key)
        .expect("cache entry");

    assert_eq!(entry._subscription_fingerprint, expected_fingerprint);
    assert_eq!(entry.subscriptions, manifest.subscriptions);
    assert_eq!(entry.prepared.len(), prepared.len());
}

#[test]
fn prepared_subscription_cache_refreshes_when_subscription_fingerprint_changes() {
    let base = manifest_with_subscription("hash-a", "world.tick");
    let changed_subscription = manifest_with_subscription("hash-a", "world.event");
    let lookup_key = prepared_subscription_lookup_key(&base);
    let mut world = World::new();

    let _ = world
        .prepared_subscriptions_for_manifest(&base)
        .expect("prepare base subscriptions");
    let base_fingerprint = world
        .prepared_subscription_cache
        .get(&lookup_key)
        .expect("base cache entry")
        ._subscription_fingerprint
        .clone();

    let _ = world
        .prepared_subscriptions_for_manifest(&changed_subscription)
        .expect("prepare changed subscriptions");
    let changed_fingerprint = world
        .prepared_subscription_cache
        .get(&lookup_key)
        .expect("changed cache entry")
        ._subscription_fingerprint
        .clone();

    assert_ne!(base_fingerprint, changed_fingerprint);
    assert_eq!(
        changed_fingerprint,
        prepared_subscription_cache_key(&changed_subscription).expect("changed fingerprint")
    );
}

#[test]
fn active_module_invocation_for_id_resolves_due_instance_only() {
    let due_manifest = manifest_with_subscription("hash-due", "world.tick");
    let other_manifest = manifest_with_subscription("hash-other", "world.tick");
    let mut world = World::new();
    world.state.module_instances.insert(
        "inst-due".to_string(),
        ModuleInstanceState {
            instance_id: "inst-due".to_string(),
            module_id: "m.due".to_string(),
            module_version: "0.1.0".to_string(),
            wasm_hash: "hash-due".to_string(),
            owner_agent_id: "owner".to_string(),
            install_target: ModuleInstallTarget::SelfAgent,
            active: true,
            installed_at: 1,
        },
    );
    world.state.module_instances.insert(
        "inst-other".to_string(),
        ModuleInstanceState {
            instance_id: "inst-other".to_string(),
            module_id: "m.other".to_string(),
            module_version: "0.1.0".to_string(),
            wasm_hash: "hash-other".to_string(),
            owner_agent_id: "owner".to_string(),
            install_target: ModuleInstallTarget::SelfAgent,
            active: true,
            installed_at: 1,
        },
    );
    world.module_registry.records.insert(
        ModuleRegistry::record_key("m.due", "0.1.0"),
        ModuleRecord {
            manifest: due_manifest,
            registered_at: 1,
            registered_by: "owner".to_string(),
            audit_event_id: None,
        },
    );
    world.module_registry.records.insert(
        ModuleRegistry::record_key("m.other", "0.1.0"),
        ModuleRecord {
            manifest: other_manifest,
            registered_at: 1,
            registered_by: "owner".to_string(),
            audit_event_id: None,
        },
    );

    let invocation = world
        .active_module_invocation_for_id("inst-due")
        .expect("lookup succeeds")
        .expect("due invocation");
    assert_eq!(invocation.instance_id, "inst-due");
    assert_eq!(invocation.module_id, "m.due");
}

#[test]
fn active_module_invocation_for_id_errors_only_for_referenced_missing_record() {
    let mut world = World::new();
    world.state.module_instances.insert(
        "inst-due".to_string(),
        ModuleInstanceState {
            instance_id: "inst-due".to_string(),
            module_id: "m.due".to_string(),
            module_version: "0.1.0".to_string(),
            wasm_hash: "hash-due".to_string(),
            owner_agent_id: "owner".to_string(),
            install_target: ModuleInstallTarget::SelfAgent,
            active: true,
            installed_at: 1,
        },
    );

    let err = world
        .active_module_invocation_for_id("inst-due")
        .expect_err("due missing record errors");
    assert!(matches!(err, WorldError::ModuleChangeInvalid { .. }));

    assert!(
        world
            .active_module_invocation_for_id("not-scheduled")
            .expect("unreferenced missing invocation skips")
            .is_none()
    );
}

#[test]
fn route_tick_to_modules_reports_due_and_overdue_routing_metrics() {
    let mut world = World::new();
    world.state.time = 10;
    let wasm_bytes = b"tick-routing-observability";
    let wasm_hash = crate::util::sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .expect("register artifact");
    world.state.module_instances.insert(
        "inst-due".to_string(),
        ModuleInstanceState {
            instance_id: "inst-due".to_string(),
            module_id: "m.due".to_string(),
            module_version: "0.1.0".to_string(),
            wasm_hash: wasm_hash.clone(),
            owner_agent_id: "owner".to_string(),
            install_target: ModuleInstallTarget::SelfAgent,
            active: true,
            installed_at: 1,
        },
    );
    world.module_registry.records.insert(
        ModuleRegistry::record_key("m.due", "0.1.0"),
        ModuleRecord {
            manifest: tick_manifest("m.due", &wasm_hash),
            registered_at: 1,
            registered_by: "owner".to_string(),
            audit_event_id: None,
        },
    );
    world.module_tick_schedule.insert("inst-due".to_string(), 4);
    world
        .module_tick_schedule
        .insert("not-active".to_string(), 7);
    world.module_tick_schedule.insert("future".to_string(), 20);

    let mut sandbox = CountingSandbox::default();
    assert_eq!(
        world
            .route_tick_to_modules(&mut sandbox)
            .expect("route due modules"),
        1
    );

    let metrics = world.module_tick_routing_metrics_snapshot();
    assert_eq!(metrics.schedule_len, 1);
    assert_eq!(metrics.last_due_count, 2);
    assert_eq!(metrics.last_invoked_count, 1);
    assert_eq!(metrics.last_missing_invocation_count, 1);
    assert_eq!(metrics.missing_invocation_count, 1);
    assert_eq!(metrics.oldest_overdue_ticks, Some(6));
    assert_eq!(metrics.routing_count, 1);
}

#[test]
fn module_tick_routing_metrics_survive_world_persistence() {
    let mut world = World::new();
    world.state.time = 10;
    let wasm_bytes = b"tick-routing-observability-persist";
    let wasm_hash = crate::util::sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .expect("register artifact");
    world.state.module_instances.insert(
        "inst-due".to_string(),
        ModuleInstanceState {
            instance_id: "inst-due".to_string(),
            module_id: "m.due".to_string(),
            module_version: "0.1.0".to_string(),
            wasm_hash: wasm_hash.clone(),
            owner_agent_id: "owner".to_string(),
            install_target: ModuleInstallTarget::SelfAgent,
            active: true,
            installed_at: 1,
        },
    );
    world.module_registry.records.insert(
        ModuleRegistry::record_key("m.due", "0.1.0"),
        ModuleRecord {
            manifest: tick_manifest("m.due", &wasm_hash),
            registered_at: 1,
            registered_by: "owner".to_string(),
            audit_event_id: None,
        },
    );
    world.module_tick_schedule.insert("inst-due".to_string(), 4);
    world
        .module_tick_schedule
        .insert("not-active".to_string(), 7);

    let mut sandbox = CountingSandbox::default();
    world
        .route_tick_to_modules(&mut sandbox)
        .expect("route due modules");

    let dir = std::env::temp_dir().join(format!(
        "oasis7-module-routing-metrics-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    world.save_to_dir(&dir).expect("save world");
    let restored = World::load_from_dir(&dir).expect("load world");
    let _ = std::fs::remove_dir_all(&dir);

    let metrics = restored.module_tick_routing_metrics_snapshot();
    assert_eq!(metrics.last_due_count, 2);
    assert_eq!(metrics.last_invoked_count, 1);
    assert_eq!(metrics.last_missing_invocation_count, 1);
    assert_eq!(metrics.missing_invocation_count, 1);
    assert_eq!(metrics.oldest_overdue_ticks, Some(6));
    assert_eq!(metrics.routing_count, 1);
    assert_eq!(metrics.last_route_duration_ms, 0);
    assert_eq!(metrics.max_route_duration_ms, 0);
    assert_eq!(metrics.cumulative_route_duration_ms, 0);
    assert_eq!(
        metrics.duration_buckets.lt_1ms
            + metrics.duration_buckets.ms_1_to_5
            + metrics.duration_buckets.ms_5_to_25
            + metrics.duration_buckets.ms_25_to_100
            + metrics.duration_buckets.ge_100ms,
        0,
        "wall-clock duration buckets must not persist into canonical snapshots"
    );
}

#[test]
fn route_tick_to_modules_keeps_schedule_when_due_record_is_missing() {
    let mut world = World::new();
    world.state.time = 7;
    let wasm_bytes = b"due-tick-module";
    let wasm_hash = crate::util::sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .expect("register artifact");
    world.state.module_instances.insert(
        "inst-due".to_string(),
        ModuleInstanceState {
            instance_id: "inst-due".to_string(),
            module_id: "m.due".to_string(),
            module_version: "0.1.0".to_string(),
            wasm_hash: wasm_hash.clone(),
            owner_agent_id: "owner".to_string(),
            install_target: ModuleInstallTarget::SelfAgent,
            active: true,
            installed_at: 1,
        },
    );
    world.module_tick_schedule.insert("inst-due".to_string(), 1);

    let err = world
        .route_tick_to_modules(&mut CountingSandbox::default())
        .expect_err("due missing record errors");
    assert!(matches!(err, WorldError::ModuleChangeInvalid { .. }));
    assert_eq!(world.module_tick_schedule.get("inst-due"), Some(&1));

    world.module_registry.records.insert(
        ModuleRegistry::record_key("m.due", "0.1.0"),
        ModuleRecord {
            manifest: tick_manifest("m.due", &wasm_hash),
            registered_at: 1,
            registered_by: "owner".to_string(),
            audit_event_id: None,
        },
    );
    let mut sandbox = CountingSandbox::default();
    assert_eq!(
        world
            .route_tick_to_modules(&mut sandbox)
            .expect("due invocation runs after record is restored"),
        1
    );
    assert_eq!(sandbox.calls, 1);
    assert_eq!(world.module_tick_schedule.get("inst-due"), None);
    let metrics = world.module_tick_routing_metrics_snapshot();
    assert_eq!(metrics.schedule_len, 0);
    assert_eq!(metrics.last_due_count, 1);
    assert_eq!(metrics.last_invoked_count, 1);
    assert_eq!(metrics.missing_invocation_count, 1);
    assert_eq!(metrics.last_missing_invocation_count, 0);
    assert_eq!(metrics.oldest_overdue_ticks, Some(6));
    assert_eq!(metrics.routing_count, 2);
}

#[test]
fn route_tick_to_modules_preflights_due_records_before_side_effects() {
    let mut world = World::new();
    world.state.time = 7;
    let wasm_bytes = b"first-due-tick-module";
    let wasm_hash = crate::util::sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .expect("register artifact");
    world.state.module_instances.insert(
        "inst-a".to_string(),
        ModuleInstanceState {
            instance_id: "inst-a".to_string(),
            module_id: "m.a".to_string(),
            module_version: "0.1.0".to_string(),
            wasm_hash: wasm_hash.clone(),
            owner_agent_id: "owner".to_string(),
            install_target: ModuleInstallTarget::SelfAgent,
            active: true,
            installed_at: 1,
        },
    );
    world.state.module_instances.insert(
        "inst-z".to_string(),
        ModuleInstanceState {
            instance_id: "inst-z".to_string(),
            module_id: "m.z".to_string(),
            module_version: "0.1.0".to_string(),
            wasm_hash: "missing-record-hash".to_string(),
            owner_agent_id: "owner".to_string(),
            install_target: ModuleInstallTarget::SelfAgent,
            active: true,
            installed_at: 1,
        },
    );
    world.module_registry.records.insert(
        ModuleRegistry::record_key("m.a", "0.1.0"),
        ModuleRecord {
            manifest: tick_manifest("m.a", &wasm_hash),
            registered_at: 1,
            registered_by: "owner".to_string(),
            audit_event_id: None,
        },
    );
    world.module_tick_schedule.insert("inst-a".to_string(), 1);
    world.module_tick_schedule.insert("inst-z".to_string(), 1);

    let mut sandbox = CountingSandbox::default();
    let err = world
        .route_tick_to_modules(&mut sandbox)
        .expect_err("later due missing record errors before earlier due executes");
    assert!(matches!(err, WorldError::ModuleChangeInvalid { .. }));
    assert_eq!(sandbox.calls, 0);
    assert_eq!(world.module_tick_schedule.get("inst-a"), Some(&1));
    assert_eq!(world.module_tick_schedule.get("inst-z"), Some(&1));
    let metrics = world.module_tick_routing_metrics_snapshot();
    assert_eq!(metrics.schedule_len, 2);
    assert_eq!(metrics.last_due_count, 2);
    assert_eq!(metrics.last_invoked_count, 0);
    assert_eq!(metrics.last_missing_invocation_count, 1);
    assert_eq!(metrics.missing_invocation_count, 1);
    assert_eq!(metrics.oldest_overdue_ticks, Some(6));
    assert_eq!(metrics.routing_count, 1);
    assert_eq!(
        metrics.duration_buckets.lt_1ms
            + metrics.duration_buckets.ms_1_to_5
            + metrics.duration_buckets.ms_5_to_25
            + metrics.duration_buckets.ms_25_to_100
            + metrics.duration_buckets.ge_100ms,
        1
    );
}
