use super::*;
use oasis7_distfs::assemble_snapshot;

const ISSUE160_SAMPLE_WORLD_ENV: &str = "OASIS7_VERIFY_ISSUE160_SAMPLE_WORLD";

#[test]
fn load_from_dir_ignores_invalid_legacy_payload_when_indexed_sidecar_is_valid() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    let dir = temp_dir("persist-distfs-fallback");
    world.save_to_dir(&dir).expect("save world");
    fs::write(
        dir.join("snapshot.manifest.json"),
        b"{\"manifest\":\"broken\"}",
    )
    .expect("tamper sidecar");

    let restored = World::load_from_dir(&dir).expect("restore indexed sidecar");
    assert_eq!(restored.state(), world.state());
    let audit_value: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join("distfs.recovery.audit.json")).expect("read indexed recovery audit"),
    )
    .expect("decode distfs fallback audit");
    assert_eq!(
        audit_value.get("status").and_then(|value| value.as_str()),
        Some("distfs_restored")
    );
    assert!(
        audit_value
            .get("reason")
            .and_then(|value| value.as_str())
            .map(|reason| reason.contains("legacy_payload_ignored"))
            .unwrap_or(false)
    );
    assert!(
        audit_value
            .get("timestamp_ms")
            .and_then(|value| value.as_i64())
            .is_some()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn snapshot_json_without_era_fields_keeps_backward_compatibility() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-legacy".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("step");

    let snapshot = world.snapshot();
    let mut value = serde_json::to_value(&snapshot).expect("encode snapshot");
    let object = value.as_object_mut().expect("snapshot object");
    object.remove("event_id_era");
    object.remove("action_id_era");
    object.remove("intent_id_era");
    object.remove("proposal_id_era");

    let legacy_json = serde_json::to_string(&value).expect("legacy json");
    let restored = Snapshot::from_json(&legacy_json).expect("decode legacy snapshot");
    assert_eq!(restored.event_id_era, 0);
    assert_eq!(restored.action_id_era, 0);
    assert_eq!(restored.intent_id_era, 0);
    assert_eq!(restored.proposal_id_era, 0);
}

#[test]
fn sample_issue160_execution_world_matches_tick_consensus_after_distfs_restore() {
    if std::env::var_os(ISSUE160_SAMPLE_WORLD_ENV).is_none() {
        return;
    }

    let dir = std::path::Path::new(
        "output/chain-runtime/viewer-live-node-playtest-issue160-trust-refresh-fix1/reward-runtime-execution-world",
    );
    assert!(
        dir.exists(),
        "set {ISSUE160_SAMPLE_WORLD_ENV}=1 only when the sample world exists at {}",
        dir.display()
    );

    let snapshot_json = Snapshot::load_json(dir.join("snapshot.json")).expect("load snapshot json");
    let journal = Journal::load_json(dir.join("journal.json")).expect("load journal json");
    let manifest: oasis7_proto::distributed::SnapshotManifest =
        crate::util::read_json_from_path(dir.join("snapshot.manifest.json").as_path())
            .expect("load distfs snapshot manifest");
    let store = LocalCasStore::new(dir.join(".distfs-state"));
    let snapshot_distfs: Snapshot =
        assemble_snapshot(&manifest, &store).expect("assemble distfs snapshot");

    let world_from_json =
        World::from_snapshot(snapshot_json.clone(), journal.clone()).expect("world from json");
    let world_from_distfs =
        World::from_snapshot(snapshot_distfs.clone(), journal).expect("world from distfs");

    let json_tick_root = snapshot_json
        .tick_consensus_records
        .last()
        .map(|record| record.block.header.state_root.clone())
        .expect("json tick consensus record");
    let distfs_tick_root = snapshot_distfs
        .tick_consensus_records
        .last()
        .map(|record| record.block.header.state_root.clone())
        .expect("distfs tick consensus record");
    let json_world_tick_root = world_from_json
        .latest_tick_consensus_record()
        .map(|record| record.block.header.state_root.clone())
        .expect("json world tick root");
    let distfs_world_tick_root = world_from_distfs
        .latest_tick_consensus_record()
        .map(|record| record.block.header.state_root.clone())
        .expect("distfs world tick root");

    assert_eq!(json_tick_root, json_world_tick_root);
    assert_eq!(distfs_tick_root, distfs_world_tick_root);
}
