use super::*;

fn planned_manifest() -> (PathBuf, PathBuf) {
    let (dir, path) = write_test_network_tier_manifest("invalid-runtime-hash");
    let mut source: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../doc/testing/templates/network-tier-persistent-preview-planned.example.json"
    ))
    .unwrap();
    source["runtime_refs"]["genesis_ref"] =
        serde_json::json!(dir.join("genesis.json").to_string_lossy());
    source["runtime_refs"]["bootstrap_peer_ref"] =
        serde_json::json!(dir.join("bootstrap.txt").to_string_lossy());
    source["runtime_refs"]["release_candidate_bundle_ref"] =
        serde_json::json!(dir.join("missing-bundle.json").to_string_lossy());
    fs::write(
        dir.join("genesis.json"),
        serde_json::to_vec(&serde_json::json!({
            "world_id": source["world_policy"]["world_id"], "chain_id": source["chain_id"]
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(&path, serde_json::to_vec(&source).unwrap()).unwrap();
    (dir, path)
}

#[test]
fn planned_policy_refuses_startup_before_bundle_or_execution_world_access() {
    let (dir, path) = planned_manifest();
    let execution_world = dir.join("must-not-create-world");
    let error = parse_options(
        [
            "--network-tier-manifest",
            path.to_str().unwrap(),
            "--execution-world-dir",
            execution_world.to_str().unwrap(),
        ]
        .into_iter(),
    )
    .expect_err("schema valid is not runtime support");
    assert!(
        error.contains("authority activation is planned and unsupported"),
        "{error}"
    );
    assert!(!execution_world.exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn planned_policy_cannot_report_ready_at_clean_genesis() {
    let (dir, path) = planned_manifest();
    let loaded = LoadedNetworkTierManifest::load(&path).unwrap();
    let snapshot = NodeSnapshot {
        node_id: "planned-node".into(),
        player_id: "planned-player".into(),
        world_id: "planned-world".into(),
        role: NodeRole::Sequencer,
        replication_enabled: false,
        running: true,
        tick_count: 0,
        last_tick_unix_ms: None,
        consensus: NodeConsensusSnapshot::default(),
        consensus_progress_observer_error: None,
        last_error: None,
    };
    let head = super::super::status_payload::build_network_head_status(&snapshot, 0, Some(&loaded));
    assert_eq!(head.source, "authority_activation_planned");
    assert_eq!(head.decision, "critical");
    assert_eq!(head.quorum_mode, "authority_activation_planned");
    assert!(!head.stake_quorum_met);
    fs::remove_dir_all(dir).unwrap();
}
