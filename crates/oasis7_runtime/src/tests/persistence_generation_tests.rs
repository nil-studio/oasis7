use super::*;

#[test]
fn load_from_dir_uses_tick_archive_bound_to_selected_sidecar_generation() {
    let mut older_world = World::new();
    for _ in 0..140 {
        older_world.step().expect("older step");
    }

    let mut newer_world = World::new();
    for _ in 0..141 {
        newer_world.step().expect("newer step");
    }

    let dir = temp_dir("persist-generation-bound-tick-archive");
    older_world
        .save_to_dir(&dir)
        .expect("save older generation");
    let index_path = dir
        .join(".distfs-state")
        .join("sidecar-generations")
        .join("index.json");
    let older_generation_id = serde_json::from_slice::<serde_json::Value>(
        &fs::read(&index_path).expect("read older generation index"),
    )
    .expect("decode older generation index")["latest_generation"]
        .as_str()
        .expect("older generation id")
        .to_string();

    newer_world
        .save_to_dir(&dir)
        .expect("save newer generation");

    // Simulate a rollback selector choosing the older generation while the
    // directory-level archive still contains the newer world's records.
    let mut index: serde_json::Value =
        serde_json::from_slice(&fs::read(&index_path).expect("read generation index"))
            .expect("decode generation index");
    index["latest_generation"] = serde_json::Value::String(older_generation_id);
    fs::write(
        &index_path,
        serde_json::to_vec_pretty(&index).expect("encode generation index"),
    )
    .expect("select older generation");

    let restored = World::load_from_dir(&dir).expect("restore selected generation");
    assert_eq!(
        restored.tick_consensus_records(),
        older_world.tick_consensus_records()
    );
    restored
        .verify_tick_consensus_chain()
        .expect("verify selected generation tick consensus chain");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn authoritative_loader_hydrates_tick_archive_after_normal_save() {
    let mut world = World::new();
    for _ in 0..140 {
        world.step().expect("authoritative step");
    }

    let dir = temp_dir("persist-authoritative-tick-archive");
    world
        .save_authoritative_recovery_generation(&dir, b"authoritative-receipt")
        .expect("save authoritative generation");
    world.step().expect("advance inherited generation");
    world
        .save_to_dir(&dir)
        .expect("save normal generation with inherited metadata");

    let restored = World::load_authoritative_recovery_generation(&dir)
        .expect("load authoritative generation")
        .expect("authoritative generation");
    assert_eq!(
        restored.world.tick_consensus_records(),
        world.tick_consensus_records()
    );
    restored
        .world
        .verify_tick_consensus_chain()
        .expect("verify hydrated authoritative tick consensus chain");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn indexed_rollback_generation_wins_over_mutable_external_json_context() {
    let mut older_world = World::new();
    for _ in 0..140 {
        older_world.step().expect("older step");
    }

    let mut newer_world = World::new();
    newer_world.submit_action(Action::RegisterAgent {
        agent_id: "newer-generation-agent".to_string(),
        pos: pos(7, 7),
    });
    newer_world.step().expect("register newer agent");
    for _ in 0..139 {
        newer_world.step().expect("newer step");
    }

    let dir = temp_dir("persist-indexed-rollback-context");
    older_world
        .save_to_dir_with_chain_resource_context(
            &dir,
            ChainResourceDerivationContext {
                world_id: "world-a",
                chain_id: "chain-a",
                genesis_ref: Some("genesis-a"),
                created_at_height: 140,
                manifest_height: 140,
                commit_block_hash: Some("block-a"),
                tick: older_world.state().time,
            },
            "world-a-config",
            "world-a-generation",
        )
        .expect("save older generation");
    let index_path = dir
        .join(".distfs-state")
        .join("sidecar-generations")
        .join("index.json");
    let older_generation_id = serde_json::from_slice::<serde_json::Value>(
        &fs::read(&index_path).expect("read older generation index"),
    )
    .expect("decode older generation index")["latest_generation"]
        .as_str()
        .expect("older generation id")
        .to_string();
    newer_world
        .save_to_dir_with_chain_resource_context(
            &dir,
            ChainResourceDerivationContext {
                world_id: "world-b",
                chain_id: "chain-b",
                genesis_ref: Some("genesis-b"),
                created_at_height: 140,
                manifest_height: 140,
                commit_block_hash: Some("block-b"),
                tick: newer_world.state().time,
            },
            "world-b-config",
            "world-b-generation",
        )
        .expect("save newer generation");

    let mut index: serde_json::Value =
        serde_json::from_slice(&fs::read(&index_path).expect("read generation index"))
            .expect("decode generation index");
    index["latest_generation"] = serde_json::Value::String(older_generation_id);
    fs::write(
        &index_path,
        serde_json::to_vec_pretty(&index).expect("encode generation index"),
    )
    .expect("select older generation");

    let restored = World::load_from_dir(&dir).expect("restore selected indexed generation");
    assert_eq!(
        restored.snapshot().chain_resource_manifest.world_id,
        "world-a"
    );
    assert_eq!(
        restored.tick_consensus_records(),
        older_world.tick_consensus_records()
    );
    assert!(
        !restored
            .state()
            .agents
            .contains_key("newer-generation-agent")
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn indexed_sidecar_generation_does_not_load_newer_top_level_module_state() {
    let mut older_world = World::new();
    let older_wasm_hash = super::install_test_module(
        &mut older_world,
        "m.persistence.generation.older",
        b"generation-older-module",
    );

    let mut newer_world = World::new();
    let newer_wasm_hash = super::install_test_module(
        &mut newer_world,
        "m.persistence.generation.newer",
        b"generation-newer-module",
    );

    let dir = temp_dir("persist-indexed-generation-module-state");
    older_world
        .save_to_dir(&dir)
        .expect("save older generation");
    let index_path = dir
        .join(".distfs-state")
        .join("sidecar-generations")
        .join("index.json");
    let older_generation_id = serde_json::from_slice::<serde_json::Value>(
        &fs::read(&index_path).expect("read older generation index"),
    )
    .expect("decode older generation index")["latest_generation"]
        .as_str()
        .expect("older generation id")
        .to_string();

    newer_world
        .save_to_dir(&dir)
        .expect("save newer generation");

    let mut index: serde_json::Value =
        serde_json::from_slice(&fs::read(&index_path).expect("read generation index"))
            .expect("decode generation index");
    index["latest_generation"] = serde_json::Value::String(older_generation_id);
    fs::write(
        &index_path,
        serde_json::to_vec_pretty(&index).expect("encode generation index"),
    )
    .expect("select older generation");

    let mut restored = World::load_from_dir(&dir).expect("restore selected generation");
    let older_key = ModuleRegistry::record_key("m.persistence.generation.older", "0.1.0");
    let newer_key = ModuleRegistry::record_key("m.persistence.generation.newer", "0.1.0");
    assert!(restored.module_registry().records.contains_key(&older_key));
    assert!(!restored.module_registry().records.contains_key(&newer_key));
    assert_eq!(
        restored.snapshot().module_artifacts,
        older_world.snapshot().module_artifacts
    );
    let restored_older_artifact = restored
        .load_module(&older_wasm_hash)
        .expect("selected generation keeps its module artifact");
    assert_eq!(
        restored_older_artifact.bytes,
        b"generation-older-module".to_vec().into()
    );
    assert!(restored.load_module(&newer_wasm_hash).is_err());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn verify_tick_consensus_archive_uses_selected_indexed_generation() {
    let mut older_world = World::new();
    for _ in 0..140 {
        older_world.step().expect("older step");
    }
    let mut newer_world = World::new();
    for _ in 0..141 {
        newer_world.step().expect("newer step");
    }

    let dir = temp_dir("persist-verify-indexed-tick-archive");
    older_world
        .save_to_dir(&dir)
        .expect("save older generation");
    let index_path = dir
        .join(".distfs-state")
        .join("sidecar-generations")
        .join("index.json");
    let older_generation_id = serde_json::from_slice::<serde_json::Value>(
        &fs::read(&index_path).expect("read older generation index"),
    )
    .expect("decode older generation index")["latest_generation"]
        .as_str()
        .expect("older generation id")
        .to_string();
    newer_world
        .save_to_dir(&dir)
        .expect("save newer generation");

    let mut index: serde_json::Value =
        serde_json::from_slice(&fs::read(&index_path).expect("read generation index"))
            .expect("decode generation index");
    index["latest_generation"] = serde_json::Value::String(older_generation_id);
    fs::write(
        &index_path,
        serde_json::to_vec_pretty(&index).expect("encode generation index"),
    )
    .expect("select older generation");
    fs::remove_file(dir.join("tick-consensus.archive.index.json"))
        .expect("remove mutable top-level archive index");
    fs::write(dir.join("tick-consensus.archive.json"), b"not-json")
        .expect("corrupt mutable top-level archive");

    World::verify_tick_consensus_archive_from_dir(&dir)
        .expect("verify selected generation archive from CAS");

    let _ = fs::remove_dir_all(&dir);
}
