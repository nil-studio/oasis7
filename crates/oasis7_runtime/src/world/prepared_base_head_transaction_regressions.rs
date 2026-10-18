use super::super::{WorldEvent, WorldEventBody};
use super::capability_authorization_command_stage::TrustedCommandStage;
use super::{World, WorldError};
use crate::{CapabilityEffectReceiptLink, CapabilityGrant};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_wasm_abi::{ModuleArtifact, ModuleStateUpdate};
use std::sync::Arc;

fn cache_artifact(wasm_hash: &str, bytes: &'static [u8]) -> ModuleArtifact {
    ModuleArtifact {
        wasm_hash: wasm_hash.to_string(),
        bytes: Arc::from(bytes),
    }
}

fn prepare_empty(
    world: &World,
) -> super::capability_authorization_command_stage::PreparedTrustedCommand {
    let staged = TrustedCommandStage::new(world).expect("stage");
    let state_root = world.current_state_root_hash().expect("state root");
    staged.prepare(state_root).expect("prepare")
}

fn assert_stale_after(label: &str, world: &mut World, mutate: impl FnOnce(&mut World)) {
    let prepared = prepare_empty(world);
    mutate(world);
    let result = prepared.install(world);
    assert!(
        matches!(
            &result,
            Err(WorldError::DistributedValidationFailed { reason })
                if reason == "prepared trusted command base head is stale"
        ),
        "{label}: unexpected install result: {result:?}"
    );
}

#[test]
fn stale_prepared_install_preserves_live_sidecars_and_local_telemetry() {
    let mut world = World::new();
    world
        .module_tick_schedule
        .insert("live-base".to_string(), 3);
    world.capability_authorization_root = "authority-base".to_string();
    world
        .module_cache
        .insert(cache_artifact("cache-base", b"cache-base"));
    world.module_tick_routing_metrics.last_route_duration_ms = 11;
    world.module_tick_routing_metrics.max_route_duration_ms = 17;
    world
        .module_tick_routing_metrics
        .cumulative_route_duration_ms = 29;

    let mut staged = TrustedCommandStage::new(&world).expect("stage");
    staged.schedule_tick("staged-only".to_string(), 7);
    let state_root = world.current_state_root_hash().expect("state root");
    let prepared = staged.prepare(state_root).expect("prepare staged command");

    world
        .module_tick_schedule
        .insert("live-after-prepare".to_string(), 19);
    world.capability_authorization_root = "authority-after-prepare".to_string();
    world
        .module_cache
        .insert(cache_artifact("cache-live", b"cache-live"));
    world.module_tick_routing_metrics.last_route_duration_ms = 101;
    world.module_tick_routing_metrics.max_route_duration_ms = 107;
    world
        .module_tick_routing_metrics
        .cumulative_route_duration_ms = 129;

    let snapshot_before_install = world.snapshot();
    let journal_before_install = world.journal().clone();
    let schedule_before_install = world.module_tick_schedule.clone();
    let authority_before_install = world.capability_authorization_root.clone();
    let cache_before_install = world.module_cache.clone();
    let metrics_before_install = world.module_tick_routing_metrics.clone();

    assert!(matches!(
        prepared.install(&mut world),
        Err(WorldError::DistributedValidationFailed { .. })
    ));

    assert_eq!(world.snapshot(), snapshot_before_install);
    assert_eq!(world.journal(), &journal_before_install);
    assert_eq!(world.module_tick_schedule, schedule_before_install);
    assert_eq!(
        world.capability_authorization_root,
        authority_before_install
    );
    assert_eq!(world.module_cache, cache_before_install);
    assert_eq!(world.module_tick_routing_metrics, metrics_before_install);
}

#[test]
fn stale_head_covers_each_durable_projection_group() {
    assert_stale_after("journal", &mut World::new(), |world| {
        world.journal.events.push(WorldEvent {
            id: 1,
            time: world.state.time,
            caused_by: None,
            body: WorldEventBody::ModuleStateUpdated(ModuleStateUpdate {
                module_id: "journal-drift".to_string(),
                trace_id: "journal-drift-trace".to_string(),
                state: vec![1],
            }),
        });
    });
    assert_stale_after("authorization-root", &mut World::new(), |world| {
        world.capability_authorization_root.push_str("-drift");
    });
    assert_stale_after("legacy-capability", &mut World::new(), |world| {
        world.capabilities.insert(
            "legacy-drift".to_string(),
            CapabilityGrant::allow_all("legacy-drift"),
        );
    });
    assert_stale_after("v2-grant", &mut World::new(), |world| {
        world
            .capability_grants_v2
            .insert("v2-drift".to_string(), serde_json::json!({"v": 1}));
    });
    assert_stale_after("authority", &mut World::new(), |world| {
        world.capability_revocation_state.epoch =
            world.capability_revocation_state.epoch.saturating_add(1);
    });
    assert_stale_after("receipt-link", &mut World::new(), |world| {
        world.capability_effect_receipt_links.insert(
            "link-drift".to_string(),
            CapabilityEffectReceiptLink {
                authorization_receipt_id: "receipt-drift".to_string(),
            },
        );
    });
    assert_stale_after("policy", &mut World::new(), |world| {
        world.scheduler_cursor = Some("policy-drift".to_string());
    });
    assert_stale_after("artifacts", &mut World::new(), |world| {
        world.module_artifacts.insert("artifact-drift".to_string());
    });
    assert_stale_after("artifact-bytes", &mut World::new(), |world| {
        world.module_artifact_bytes.insert(
            "artifact-bytes-drift".to_string(),
            Arc::from(b"artifact-bytes-drift".as_slice()),
        );
    });
    assert_stale_after("state", &mut World::new(), |world| {
        world.state.time = world.state.time.saturating_add(1);
    });
    assert_stale_after("resources", &mut World::new(), |world| {
        world.state.resources.insert(ResourceKind::Data, 1);
    });
    assert_stale_after("allocator", &mut World::new(), |world| {
        world.next_event_id = world.next_event_id.saturating_add(1);
    });
    assert_stale_after("config", &mut World::new(), |world| {
        world.module_limits_max.max_gas ^= 1;
    });
    assert_stale_after("consensus", &mut World::new(), |world| {
        world
            .tick_consensus_authority_source
            .push_str("-consensus-drift");
    });
}

#[test]
fn same_head_install_merges_cache_and_preserves_local_telemetry() {
    let mut world = World::new();
    let prepared = prepare_empty(&world);
    world
        .module_cache
        .insert(cache_artifact("live-cache", b"live-cache"));
    world.module_tick_routing_metrics.last_route_duration_ms = 77;
    world.module_tick_routing_metrics.max_route_duration_ms = 88;
    world
        .module_tick_routing_metrics
        .cumulative_route_duration_ms = 99;
    let cache_before = world.module_cache.clone();
    assert!(prepared.install(&mut world).is_ok());
    assert_eq!(world.module_cache, cache_before);
    assert_eq!(world.module_tick_routing_metrics.last_route_duration_ms, 77);
    assert_eq!(world.module_tick_routing_metrics.max_route_duration_ms, 88);
    assert_eq!(
        world
            .module_tick_routing_metrics
            .cumulative_route_duration_ms,
        99
    );
}
