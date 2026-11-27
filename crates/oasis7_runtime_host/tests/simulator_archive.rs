#![cfg(not(target_arch = "wasm32"))]

use oasis7_agent_api::world_resource::ChainResourceDelta;
use oasis7_runtime::{Action, World, WorldState};
use oasis7_runtime_host::simulator_archive::archive::WorldEvent;
use oasis7_runtime_host::simulator_archive::kernel_host::WorldKernel;
use oasis7_runtime_host::simulator_archive::persist::{PersistError, WorldJournal, WorldSnapshot};
use oasis7_simulator::{GeoPos, WorldEventKind};

fn runtime_fixture() -> World {
    let mut world = oasis7_runtime_host::world::new_with_state(WorldState::default());
    world.submit_action(Action::RegisterAgent {
        agent_id: "archive-agent".into(),
        pos: GeoPos::new(10, 20, 30),
    });
    world.step().unwrap();
    world
}

#[test]
fn snapshot_split_rejoin_preserves_nonempty_host_extensions_and_exact_codecs() {
    let mut original = WorldKernel::new().snapshot();
    original.runtime_snapshot = Some(runtime_fixture().snapshot());
    original.player_gameplay = Some(
        serde_json::from_value(serde_json::json!({
            "stage_id": "first_session_loop", "stage_status": "active",
            "execution_state": "accepted", "goal_id": "archive-goal",
            "goal_kind": "create_first_world_feedback", "goal_title": "Goal",
            "objective": "Keep this objective", "progress_detail": "Recorded progress",
            "next_step_hint": "Resume original goal"
        }))
        .unwrap(),
    );
    original.chain_resource_manifest.world_id = "archive-world".into();
    original.chain_resource_manifest.genesis_ref = Some("archive-genesis".into());
    original.chain_resource_manifest.refresh_hashes();
    original.latest_chain_resource_delta = ChainResourceDelta::empty_for_manifest(
        "archive-world",
        original.chain_resource_manifest.manifest_hash.clone(),
        7,
        9,
    );
    let json = serde_json::to_vec(&original).unwrap();
    let cbor = serde_cbor::to_vec(&original).unwrap();
    let (kernel, extensions) = original.clone().into_kernel_parts();
    let native = serde_json::to_value(&kernel).unwrap();
    for key in [
        "runtime_snapshot",
        "player_gameplay",
        "chain_resource_manifest",
        "latest_chain_resource_delta",
    ] {
        assert!(
            native.get(key).is_none(),
            "native kernel must not own {key}"
        );
    }
    assert!(extensions.runtime_snapshot.is_some());
    assert!(extensions.player_gameplay.is_some());
    let restored = WorldSnapshot::from_kernel_parts(kernel, extensions);
    assert_eq!(restored, original);
    assert_eq!(serde_json::to_vec(&restored).unwrap(), json);
    assert_eq!(serde_cbor::to_vec(&restored).unwrap(), cbor);
}

#[test]
fn event_split_rejoin_preserves_original_runtime_event_and_exact_codecs() {
    let world = runtime_fixture();
    let original = WorldEvent {
        id: 7,
        time: 9,
        kind: WorldEventKind::AgentRegistered {
            agent_id: "archive-agent".into(),
            location_id: "origin".into(),
            pos: GeoPos::new(10, 20, 30),
        },
        runtime_event: Some(world.journal().events.last().unwrap().clone()),
    };
    let json = serde_json::to_vec(&original).unwrap();
    let cbor = serde_cbor::to_vec(&original).unwrap();
    let (kernel, runtime) = original.clone().into_kernel_parts();
    assert!(
        serde_json::to_value(&kernel)
            .unwrap()
            .get("runtime_event")
            .is_none()
    );
    assert!(runtime.is_some());
    let restored = WorldEvent::from_kernel_parts(kernel, runtime);
    assert_eq!(restored, original);
    assert_eq!(serde_json::to_vec(&restored).unwrap(), json);
    assert_eq!(serde_cbor::to_vec(&restored).unwrap(), cbor);
}

#[test]
fn native_exports_match_the_kernel_portion_of_the_legacy_archive() {
    let kernel = WorldKernel::new();
    let native = kernel.kernel_snapshot();
    let (archived, _) = kernel.snapshot().into_kernel_parts();
    assert_eq!(native, archived);
    let native_journal = kernel.kernel_journal_snapshot();
    assert_eq!(native_journal.version, kernel.journal_snapshot().version);
    assert_eq!(native_journal.events.len(), kernel.journal().len());
}

#[test]
fn native_replay_keeps_original_host_event_bytes_on_archive_rewrite() {
    let world = runtime_fixture();
    let snapshot = WorldKernel::new().snapshot();
    let journal = WorldJournal {
        version: oasis7_simulator::JOURNAL_VERSION,
        events: vec![WorldEvent {
            id: 0,
            time: 9,
            kind: WorldEventKind::RuntimeEvent {
                kind: "runtime.fixture".into(),
                domain_kind: Some("original-domain".into()),
            },
            runtime_event: Some(world.journal().events.last().unwrap().clone()),
        }],
    };
    let json = serde_json::to_vec(&journal).unwrap();
    let cbor = serde_cbor::to_vec(&journal).unwrap();
    let replayed = WorldKernel::replay_from_snapshot(snapshot, journal.clone()).unwrap();
    assert_eq!(replayed.kernel_snapshot().time, 9);
    assert_eq!(
        replayed.kernel_journal_snapshot().events[0].kind,
        journal.events[0].kind
    );
    let restored =
        WorldKernel::from_snapshot(replayed.snapshot(), replayed.journal_snapshot()).unwrap();
    assert_eq!(
        serde_json::to_vec(&restored.journal_snapshot()).unwrap(),
        json
    );
    assert_eq!(
        serde_cbor::to_vec(&restored.journal_snapshot()).unwrap(),
        cbor
    );
}

#[test]
fn host_archive_extensions_survive_pending_actions_and_pre_action_hooks() {
    let world = runtime_fixture();
    let mut snapshot = WorldKernel::new().snapshot();
    snapshot.runtime_snapshot = Some(world.snapshot());
    snapshot.chain_resource_manifest.world_id = "committed-world".into();
    snapshot.chain_resource_manifest.chain_id = "committed-chain".into();
    snapshot.chain_resource_manifest.genesis_ref = Some("original-genesis".into());
    snapshot.chain_resource_manifest.created_at_block_hash = Some("original-block".into());
    snapshot.chain_resource_manifest.refresh_hashes();
    snapshot.latest_chain_resource_delta.commit_block_hash = Some("original-block".into());
    let expected_extensions = snapshot.clone().into_kernel_parts().1;
    let journal = WorldJournal {
        version: oasis7_simulator::JOURNAL_VERSION,
        events: vec![WorldEvent {
            id: 0,
            time: 9,
            kind: WorldEventKind::RuntimeEvent {
                kind: "original-runtime-event".into(),
                domain_kind: None,
            },
            runtime_event: Some(world.journal().events.last().unwrap().clone()),
        }],
    };
    let original_runtime = journal.events[0].runtime_event.clone();
    let mut kernel = WorldKernel::replay_from_snapshot(snapshot, journal).unwrap();
    let hook_called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observed = hook_called.clone();
    kernel.add_pre_action_rule_hook(move |id, _, view: &WorldKernel| {
        assert_eq!(
            view.journal().get(0).unwrap().runtime_event.cloned(),
            original_runtime
        );
        observed.store(true, std::sync::atomic::Ordering::SeqCst);
        oasis7_simulator::KernelRuleDecision::allow(id)
    });
    kernel.submit_action(oasis7_simulator::Action::RegisterLocation {
        location_id: "host-boundary".into(),
        name: "Host boundary".into(),
        pos: GeoPos::new(0, 0, 0),
        profile: Default::default(),
    });
    assert_eq!(kernel.snapshot().into_kernel_parts().1, expected_extensions);
    kernel.step().unwrap();
    assert!(hook_called.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(kernel.snapshot().into_kernel_parts().1, expected_extensions);
    assert_eq!(
        kernel.journal().len(),
        kernel.kernel_journal_snapshot().events.len()
    );
    assert!(kernel.journal().get(0).unwrap().runtime_event.is_some());
    assert!(kernel.journal().last().unwrap().runtime_event.is_none());
    let encoded = serde_json::to_vec(&kernel).unwrap();
    let restored: WorldKernel = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(serde_json::to_vec(&restored).unwrap(), encoded);
    assert_eq!(restored.journal(), kernel.journal());
    let cbor = serde_cbor::to_vec(&kernel).unwrap();
    let restored: WorldKernel = serde_cbor::from_slice(&cbor).unwrap();
    assert_eq!(serde_cbor::to_vec(&restored).unwrap(), cbor);
}

#[test]
fn restored_snapshot_keeps_external_cut_exactly_through_no_journal_mutations() {
    let mut snapshot = WorldKernel::new().snapshot();
    snapshot.runtime_snapshot = Some(runtime_fixture().snapshot());
    snapshot.player_gameplay = Some(serde_json::from_value(serde_json::json!({
        "stage_id":"first_session_loop", "stage_status":"active", "execution_state":"accepted",
        "goal_id":"retained-goal", "goal_kind":"create_first_world_feedback", "goal_title":"Goal",
        "objective":"Original objective", "progress_detail":"Historical progress", "next_step_hint":"Resume"
    })).unwrap());
    snapshot.chain_resource_manifest.world_id = "external-cut".into();
    snapshot.chain_resource_manifest.created_at_block_hash = Some("original-block".into());
    snapshot.chain_resource_manifest.refresh_hashes();
    let json = serde_json::to_vec(&snapshot).unwrap();
    let cbor = serde_cbor::to_vec(&snapshot).unwrap();
    let mut host =
        WorldKernel::from_snapshot(snapshot, WorldKernel::new().journal_snapshot()).unwrap();
    assert_eq!(serde_json::to_vec(&host.snapshot()).unwrap(), json);
    assert_eq!(serde_cbor::to_vec(&host.snapshot()).unwrap(), cbor);
    let explicit = host.snapshot_with_chain_resource_context(
        oasis7_agent_api::world_resource::ChainResourceDerivationContext::simulator_default(
            host.time(),
        ),
    );
    assert_ne!(explicit.chain_resource_manifest.world_id, "external-cut");
    assert_eq!(explicit.runtime_snapshot, host.snapshot().runtime_snapshot);
    assert_eq!(explicit.player_gameplay, host.snapshot().player_gameplay);
    let ttl = host.kernel_snapshot().intel_ttl_ticks;
    host.set_intel_ttl_ticks(ttl + 1);
    assert_eq!(host.journal().len(), 0);
    host.set_intel_ttl_ticks(ttl);
    assert_eq!(serde_json::to_vec(&host.snapshot()).unwrap(), json);
    assert_eq!(serde_cbor::to_vec(&host.snapshot()).unwrap(), cbor);
}

#[test]
fn default_chain_evidence_refreshes_after_restore_and_replay() {
    let origin = WorldKernel::new().snapshot();
    let journal = WorldKernel::new().journal_snapshot();
    let mut original = WorldKernel::from_snapshot(origin.clone(), journal.clone()).unwrap();
    original.submit_action(oasis7_simulator::Action::RegisterLocation {
        location_id: "new-location".into(),
        name: "New location".into(),
        pos: GeoPos::new(0, 0, 0),
        profile: Default::default(),
    });
    original.step().unwrap();
    let default = original.snapshot_with_chain_resource_context(
        oasis7_agent_api::world_resource::ChainResourceDerivationContext::simulator_default(
            original.time(),
        ),
    );
    assert_eq!(original.snapshot(), default);
    let replay = WorldKernel::replay_from_snapshot(origin, original.journal_snapshot()).unwrap();
    assert_eq!(replay.snapshot(), default);
    let restored =
        WorldKernel::from_snapshot(default.clone(), original.journal_snapshot()).unwrap();
    assert_eq!(restored.snapshot(), default);
}

#[test]
fn journal_view_borrows_native_payload_and_survives_sidecar_tail_append() {
    let original = runtime_fixture();
    let mut host = WorldKernel::new();
    host.submit_action(oasis7_simulator::Action::RegisterLocation {
        location_id: "origin".into(),
        name: "Origin".into(),
        pos: GeoPos::new(0, 0, 0),
        profile: Default::default(),
    });
    host.step().unwrap();
    host.submit_action(oasis7_simulator::Action::RegisterAgent {
        agent_id: "before-restore".into(),
        location_id: "origin".into(),
    });
    host.step().unwrap();
    let mut journal = host.journal_snapshot();
    journal.events[1].runtime_event = Some(original.journal().events.last().unwrap().clone());
    let mut host = WorldKernel::from_snapshot(host.snapshot(), journal.clone()).unwrap();
    let native: &oasis7_simulator::WorldKernel = &host;
    assert!(std::ptr::eq(
        host.journal().get(0).unwrap().kind,
        &native.journal()[0].kind
    ));
    for i in 0..8 {
        host.submit_action(oasis7_simulator::Action::RegisterAgent {
            agent_id: format!("agent-{i}"),
            location_id: "origin".into(),
        });
        host.step().unwrap();
    }
    assert_eq!(host.journal().get(0).unwrap().to_owned(), journal.events[0]);
    assert_eq!(host.journal().get(1).unwrap().to_owned(), journal.events[1]);
    assert!(host.journal().get(0).unwrap().runtime_event.is_none());
    assert!(host.journal().get(1).unwrap().runtime_event.is_some());
    assert!(
        host.journal()
            .iter()
            .skip(2)
            .all(|event| event.runtime_event.is_none())
    );
    let owned = host.journal_snapshot();
    assert_eq!(
        serde_json::to_vec(&host.journal()).unwrap(),
        serde_json::to_vec(&owned.events).unwrap()
    );
    assert_eq!(
        serde_cbor::to_vec(&host.journal()).unwrap(),
        serde_cbor::to_vec(&owned.events).unwrap()
    );
    assert_eq!(
        host.journal()
            .iter()
            .map(|e| e.to_owned())
            .collect::<Vec<_>>(),
        owned.events
    );
}

#[test]
fn invalid_journal_version_precedes_short_prefix_error_in_restore_and_replay() {
    let mut snapshot = WorldKernel::new().snapshot();
    snapshot.journal_len = 1;
    let mut journal = WorldKernel::new().journal_snapshot();
    journal.version = u32::MAX;
    for result in [
        WorldKernel::from_snapshot(snapshot.clone(), journal.clone()),
        WorldKernel::replay_from_snapshot(snapshot.clone(), journal.clone()),
    ] {
        assert!(
            matches!(result, Err(PersistError::UnsupportedVersion { kind, version: u32::MAX, .. }) if kind == "journal")
        );
    }
}
