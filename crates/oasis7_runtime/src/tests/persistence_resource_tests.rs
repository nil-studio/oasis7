use super::*;

#[test]
fn persist_runtime_snapshot_records_committed_resource_chunk_and_delta() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-resource-chunk".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register agent");

    let dir = temp_dir("persist-runtime-resource-chunk");
    world.save_to_dir(&dir).expect("save world");

    let snapshot = Snapshot::load_json(dir.join("snapshot.json")).expect("load snapshot");
    assert!(snapshot.chain_resource_manifest.is_schema_current());
    assert_eq!(snapshot.chain_resource_manifest.generated_chunks.len(), 1);
    assert!(
        snapshot
            .chain_resource_manifest
            .generated_chunks
            .values()
            .any(|chunk| chunk.chunk_status == ChainChunkResourceStatus::Committed)
    );
    let delta = snapshot
        .latest_chain_resource_delta
        .expect("resource delta");
    assert!(delta.is_schema_current());
    assert!(
        !delta.entries.is_empty(),
        "runtime resource delta should expose a non-empty chunk commit stream"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn persist_empty_runtime_snapshot_records_starter_resource_chunk_and_chain_context() {
    let world = World::new();

    let dir = temp_dir("persist-empty-runtime-starter-resource-chunk");
    world
        .save_to_dir_with_chain_resource_context(
            &dir,
            ChainResourceDerivationContext {
                world_id: "testnet-world",
                chain_id: "testnet-chain",
                genesis_ref: Some("genesis-ref"),
                created_at_height: 7,
                manifest_height: 7,
                commit_block_hash: Some("block-h7"),
                tick: world.state().time,
            },
            "testnet-world-config",
            "testnet-generation-algorithm",
        )
        .expect("save empty world with chain context");

    let snapshot = Snapshot::load_json(dir.join("snapshot.json")).expect("load snapshot");
    let manifest = snapshot.chain_resource_manifest;
    assert!(manifest.is_schema_current());
    assert_eq!(manifest.world_id, "testnet-world");
    assert_eq!(manifest.chain_id, "testnet-chain");
    assert_eq!(manifest.genesis_ref.as_deref(), Some("genesis-ref"));
    assert_eq!(manifest.manifest_height, 7);
    assert_eq!(manifest.created_at_block_hash.as_deref(), Some("block-h7"));
    assert_ne!(manifest.world_seed, 0);
    assert_eq!(manifest.generated_chunks.len(), 1);
    let chunk = manifest
        .generated_chunks
        .values()
        .next()
        .expect("starter chunk");
    assert_eq!(chunk.chunk_status, ChainChunkResourceStatus::Committed);
    assert!(!chunk.total_by_element_g.is_empty());
    assert_eq!(chunk.total_by_element_g, chunk.remaining_by_element_g);
    assert!(chunk.total_by_element_g.values().any(|amount| *amount > 0));

    let delta = snapshot
        .latest_chain_resource_delta
        .expect("resource delta");
    assert!(delta.is_schema_current());
    assert_eq!(delta.world_id, "testnet-world");
    assert_eq!(delta.chain_id, "testnet-chain");
    assert_eq!(delta.block_height, 7);
    assert_eq!(delta.ordering_key.height, 7);
    assert_eq!(delta.commit_block_hash.as_deref(), Some("block-h7"));
    assert_eq!(delta.resulting_manifest_hash, manifest.manifest_hash);
    assert!(!delta.entries.is_empty());
    assert!(delta.entries.iter().any(|entry| match entry {
        ChainResourceDeltaEntry::ChunkResource {
            total_delta_g,
            remaining_delta_g,
            resulting_remaining_g,
            ..
        } => *total_delta_g > 0 || *remaining_delta_g > 0 || *resulting_remaining_g > 0,
        _ => false,
    }));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn load_from_dir_preserves_chain_resource_context_in_runtime_snapshot() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-chain-context".to_string(),
        pos: pos(2, 2),
    });
    world.step().expect("register agent");

    let dir = temp_dir("load-preserves-chain-resource-context");
    world
        .save_to_dir_with_chain_resource_context(
            &dir,
            ChainResourceDerivationContext {
                world_id: "testnet-world",
                chain_id: "testnet-chain",
                genesis_ref: Some("genesis-ref"),
                created_at_height: 11,
                manifest_height: 11,
                commit_block_hash: Some("block-h11"),
                tick: world.state().time,
            },
            "testnet-world-config",
            "testnet-generation-algorithm",
        )
        .expect("save world with chain context");

    let persisted =
        Snapshot::load_json(dir.join("snapshot.json")).expect("load persisted snapshot");
    let restored = World::load_from_dir(&dir).expect("load world");
    let restored_snapshot = restored.snapshot();

    assert_eq!(
        restored_snapshot.chain_resource_manifest.world_id,
        "testnet-world"
    );
    assert_eq!(
        restored_snapshot.chain_resource_manifest.manifest_hash,
        persisted.chain_resource_manifest.manifest_hash
    );
    let restored_delta = restored_snapshot
        .latest_chain_resource_delta
        .expect("restored resource delta");
    let persisted_delta = persisted
        .latest_chain_resource_delta
        .expect("persisted resource delta");
    assert_eq!(restored_delta.world_id, "testnet-world");
    assert_eq!(
        restored_delta.resulting_manifest_hash,
        persisted_delta.resulting_manifest_hash
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn load_from_dir_keeps_indexed_sidecar_when_json_context_differs() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-stale-sidecar".to_string(),
        pos: pos(3, 3),
    });
    world.step().expect("register agent");

    let dir = temp_dir("load-prefers-json-stale-chain-resource-sidecar");
    world.save_to_dir(&dir).expect("save world with sidecar");
    let formal_snapshot = world.snapshot_with_chain_resource_context(
        ChainResourceDerivationContext {
            world_id: "testnet-world",
            chain_id: "testnet-chain",
            genesis_ref: Some("genesis-ref"),
            created_at_height: 21,
            manifest_height: 21,
            commit_block_hash: Some("block-h21"),
            tick: world.state().time,
        },
        "testnet-world-config",
        "testnet-generation-algorithm",
    );
    formal_snapshot
        .save_json(dir.join("snapshot.json"))
        .expect("overwrite json snapshot with formal chain context");

    let restored = World::load_from_dir(&dir).expect("load world");
    let restored_snapshot = restored.snapshot();

    assert_eq!(
        restored_snapshot.chain_resource_manifest.world_id,
        "runtime-world"
    );
    let audit: serde_json::Value = serde_json::from_slice(
        &std::fs::read(dir.join("distfs.recovery.audit.json")).expect("read distfs recovery audit"),
    )
    .expect("decode recovery audit");
    assert_eq!(
        audit.get("status").and_then(|value| value.as_str()),
        Some("distfs_restored")
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn load_from_dir_keeps_distfs_sidecar_when_json_chain_resource_height_is_older() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-fresh-sidecar".to_string(),
        pos: pos(4, 4),
    });
    world.step().expect("register agent");

    let dir = temp_dir("load-keeps-distfs-newer-chain-resource-sidecar");
    world
        .save_to_dir_with_chain_resource_context(
            &dir,
            ChainResourceDerivationContext {
                world_id: "testnet-world",
                chain_id: "testnet-chain",
                genesis_ref: Some("genesis-ref"),
                created_at_height: 30,
                manifest_height: 30,
                commit_block_hash: Some("block-h30"),
                tick: world.state().time,
            },
            "testnet-world-config",
            "testnet-generation-algorithm",
        )
        .expect("save sidecar with newer chain context");

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-json-newer-state".to_string(),
        pos: pos(5, 5),
    });
    world.step().expect("advance json state");
    let older_chain_context_snapshot = world.snapshot_with_chain_resource_context(
        ChainResourceDerivationContext {
            world_id: "testnet-world",
            chain_id: "testnet-chain",
            genesis_ref: Some("genesis-ref"),
            created_at_height: 21,
            manifest_height: 21,
            commit_block_hash: Some("block-h21"),
            tick: world.state().time,
        },
        "testnet-world-config",
        "testnet-generation-algorithm",
    );
    older_chain_context_snapshot
        .save_json(dir.join("snapshot.json"))
        .expect("overwrite json snapshot with older chain context");

    let restored = World::load_from_dir(&dir).expect("load world");
    let restored_snapshot = restored.snapshot();

    assert_eq!(
        restored_snapshot.chain_resource_manifest.manifest_height, 30,
        "newer distfs chain resource context must not be replaced by older JSON context"
    );
    assert!(
        !restored_snapshot
            .state
            .agents
            .contains_key("agent-json-newer-state"),
        "older chain resource context JSON must not win only because its state time is newer"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn persist_claimed_agent_runtime_snapshot_preserves_starter_chunk_resources() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "claimed-agent".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register claimed agent");

    let dir = temp_dir("persist-claimed-agent-starter-resource-chunk");
    world
        .save_to_dir_with_chain_resource_context(
            &dir,
            ChainResourceDerivationContext {
                world_id: "testnet-world",
                chain_id: "testnet-world",
                genesis_ref: None,
                created_at_height: 1,
                manifest_height: 2,
                commit_block_hash: Some("block-h2"),
                tick: world.state().time,
            },
            "testnet-world-config",
            "testnet-generation-algorithm",
        )
        .expect("save claimed agent world with chain context");

    let snapshot = Snapshot::load_json(dir.join("snapshot.json")).expect("load snapshot");
    let chunk = snapshot
        .chain_resource_manifest
        .generated_chunks
        .values()
        .next()
        .expect("claimed agent chunk");
    assert_eq!(chunk.chunk_status, ChainChunkResourceStatus::Committed);
    assert!(!chunk.total_by_element_g.is_empty());
    assert_eq!(chunk.total_by_element_g, chunk.remaining_by_element_g);
    assert!(chunk.fragment_refs.iter().any(|fragment| {
        fragment
            .fragment_id
            .starts_with("runtime-starter-fragment:")
    }));
    assert!(
        chunk
            .fragment_refs
            .iter()
            .any(|fragment| fragment.fragment_id == "runtime-agent:claimed-agent")
    );
    let delta = snapshot
        .latest_chain_resource_delta
        .expect("resource delta");
    assert!(delta.is_schema_current());
    assert!(
        delta.entries.is_empty(),
        "non-genesis claimed-agent snapshot should not replay starter resources"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn persist_non_starter_agent_chunk_does_not_mint_extra_starter_budget() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "claimed-agent-far".to_string(),
        pos: pos(2_500_000, 4_200_000),
    });
    world.step().expect("register far claimed agent");

    let dir = temp_dir("persist-far-agent-no-extra-starter-budget");
    world
        .save_to_dir_with_chain_resource_context(
            &dir,
            ChainResourceDerivationContext {
                world_id: "testnet-world",
                chain_id: "testnet-world",
                genesis_ref: None,
                created_at_height: 1,
                manifest_height: 1,
                commit_block_hash: Some("block-h1"),
                tick: world.state().time,
            },
            "testnet-world-config",
            "testnet-generation-algorithm",
        )
        .expect("save far claimed agent world with chain context");

    let snapshot = Snapshot::load_json(dir.join("snapshot.json")).expect("load snapshot");
    assert_eq!(snapshot.chain_resource_manifest.generated_chunks.len(), 2);
    let starter_chunk = snapshot
        .chain_resource_manifest
        .generated_chunks
        .get("chunk:0:0:0")
        .expect("starter chunk");
    assert!(!starter_chunk.total_by_element_g.is_empty());
    let agent_chunk = snapshot
        .chain_resource_manifest
        .generated_chunks
        .get("chunk:1:2:0")
        .expect("agent chunk");
    assert!(agent_chunk.total_by_element_g.is_empty());
    assert!(agent_chunk.remaining_by_element_g.is_empty());
    assert!(
        agent_chunk
            .fragment_refs
            .iter()
            .any(|fragment| fragment.fragment_id == "runtime-agent:claimed-agent-far")
    );

    let delta = snapshot
        .latest_chain_resource_delta
        .expect("resource delta");
    assert!(delta.entries.iter().all(|entry| match entry {
        ChainResourceDeltaEntry::ChunkResource { coord, .. } => *coord == starter_chunk.coord,
        _ => true,
    }));

    let _ = std::fs::remove_dir_all(&dir);
}
