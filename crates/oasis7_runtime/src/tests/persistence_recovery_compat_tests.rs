use super::*;

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
fn json_and_indexed_distfs_restore_the_same_tick_consensus() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-cross-format".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("commit agent registration");
    world.step().expect("commit successor tick");
    let expected = world
        .latest_tick_consensus_record()
        .expect("generated world has a consensus record");

    let dir = temp_dir("persist-json-distfs-parity");
    world.save_to_dir(&dir).expect("save both representations");

    // Make the JSON snapshot unavailable so the indexed sidecar has to supply
    // the state. Keep the real JSON bytes for the separate recovery below.
    let json_path = dir.join("snapshot.json");
    let json = fs::read(&json_path).expect("read generated JSON snapshot");
    fs::remove_file(&json_path).expect("exclude JSON snapshot from indexed restore");
    let distfs = World::load_from_dir(&dir).expect("restore from indexed DistFS");
    let audit: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join("distfs.recovery.audit.json")).expect("read DistFS recovery audit"),
    )
    .expect("decode DistFS recovery audit");
    assert_eq!(audit["status"].as_str(), Some("distfs_restored"));

    // Keep the same serialization of the original world and remove every
    // indexed generation and CAS blob, so JSON cannot reuse the first path.
    fs::write(&json_path, json).expect("restore generated JSON input bytes");
    fs::remove_dir_all(dir.join(".distfs-state")).expect("exclude DistFS from JSON restore");
    let json = World::load_from_dir(&dir).expect("restore from JSON");

    for restored in [&distfs, &json] {
        restored
            .verify_tick_consensus_chain()
            .expect("verify restored consensus chain");
        let record = restored
            .latest_tick_consensus_record()
            .expect("restored world has a consensus record");
        assert_eq!(
            record.block.header.state_root,
            expected.block.header.state_root
        );
        assert_eq!(
            restored.tick_consensus_records(),
            world.tick_consensus_records()
        );
    }
    assert_eq!(distfs.state(), json.state());
    assert_eq!(distfs.state(), world.state());

    let _ = fs::remove_dir_all(&dir);
}
