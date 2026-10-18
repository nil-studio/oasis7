use super::*;
use crate::GeoPos;
use crate::runtime::{Action, World, WorldState};
use crate::simulator::WorldKernel;
use oasis7_simulator::WorldEventKind;

fn runtime_fixture() -> World {
    let mut world = crate::runtime_host::world::new_with_state(WorldState::default());
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
    let journal = crate::simulator::WorldJournal {
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
    let journal = crate::simulator::WorldJournal {
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
        assert_eq!(view.journal()[0].runtime_event, original_runtime);
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
    assert!(kernel.journal()[0].runtime_event.is_some());
    assert!(kernel.journal().last().unwrap().runtime_event.is_none());
    let encoded = serde_json::to_vec(&kernel).unwrap();
    let restored: WorldKernel = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(serde_json::to_vec(&restored).unwrap(), encoded);
    assert_eq!(restored.journal(), kernel.journal());
    let cbor = serde_cbor::to_vec(&kernel).unwrap();
    let restored: WorldKernel = serde_cbor::from_slice(&cbor).unwrap();
    assert_eq!(serde_cbor::to_vec(&restored).unwrap(), cbor);
}
