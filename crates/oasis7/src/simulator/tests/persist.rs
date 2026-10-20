use super::*;
use crate::runtime::{
    FactoryBuildJobState, FactoryModuleSpec, MaterialLedgerId, MaterialStack, World as RuntimeWorld,
};

#[path = "persist/kernel_roundtrip.rs"]
mod kernel_roundtrip;

#[test]
fn kernel_snapshot_roundtrip_preserves_agent_long_term_memories() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let (mut kernel, _) = initialize_kernel(config, init).expect("init ok");

    let entry = LongTermMemoryEntry::new("mem-4", 12, "factory alpha stalled").with_tag("factory");
    kernel
        .set_agent_long_term_memory("agent-0", vec![entry.clone()])
        .expect("set memory");

    let snapshot = kernel.snapshot();
    let journal = kernel.journal_snapshot();
    let restored = WorldKernel::from_snapshot(snapshot, journal).expect("restore ok");

    let restored_entries = restored
        .long_term_memory_for_agent("agent-0")
        .expect("memory restored");
    assert_eq!(restored_entries.len(), 1);
    assert_eq!(restored_entries[0], entry);
}

#[test]
fn kernel_snapshot_roundtrip_preserves_player_auth_nonce_state() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let (mut kernel, _) = initialize_kernel(config, init).expect("init ok");

    kernel
        .consume_player_auth_nonce("player-a", 11)
        .expect("consume nonce a");
    kernel
        .consume_player_auth_nonce("player-b", 4)
        .expect("consume nonce b");

    let snapshot = kernel.snapshot();
    let journal = kernel.journal_snapshot();
    let mut restored = WorldKernel::from_snapshot(snapshot, journal).expect("restore ok");

    assert_eq!(restored.player_auth_last_nonce("player-a"), Some(11));
    assert_eq!(restored.player_auth_last_nonce("player-b"), Some(4));

    let replay_err = restored
        .consume_player_auth_nonce("player-a", 11)
        .expect_err("replay should fail");
    assert!(replay_err.contains("replay"));
}

#[test]
fn kernel_snapshot_roundtrip_preserves_intel_ttl_configuration() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let (mut kernel, _) = initialize_kernel(config, init).expect("init ok");

    kernel.set_intel_ttl_ticks(7);

    let snapshot = kernel.snapshot();
    let journal = kernel.journal_snapshot();
    let restored = WorldKernel::from_snapshot(snapshot, journal).expect("restore ok");

    assert_eq!(restored.intel_ttl_ticks(), 7);
}
#[test]
#[expect(clippy::field_reassign_with_default, reason = "fixture defaults")]
fn kernel_snapshot_roundtrip_keeps_fragment_profile() {
    let mut config = WorldConfig::default();
    config.space = SpaceConfig {
        width_cm: 200_000,
        depth_cm: 200_000,
        height_cm: 200_000,
    };
    config.asteroid_fragment.base_density_per_km3 = 5.0;
    config.asteroid_fragment.voxel_size_km = 1;
    config.asteroid_fragment.cluster_noise = 0.0;
    config.asteroid_fragment.layer_scale_height_km = 0.0;
    config.asteroid_fragment.radius_min_cm = 120;
    config.asteroid_fragment.radius_max_cm = 120;

    let mut init = WorldInitConfig::default();
    init.seed = 31;
    init.agents.count = 0;

    let (kernel, _) = initialize_kernel(config, init).expect("kernel init");
    let snapshot = kernel.snapshot();
    let journal = kernel.journal_snapshot();
    let restored = WorldKernel::from_snapshot(snapshot, journal).expect("restore from snapshot");

    let fragment_before = kernel
        .model()
        .locations
        .values()
        .find(|loc| loc.id.starts_with("frag-"))
        .expect("fragment before");
    let profile_before = fragment_before
        .fragment_profile
        .clone()
        .expect("profile before");
    let budget_before = fragment_before
        .fragment_budget
        .clone()
        .expect("budget before");
    let fragment_after = restored
        .model()
        .locations
        .values()
        .find(|loc| loc.id.starts_with("frag-"))
        .expect("fragment after");
    let profile_after = fragment_after
        .fragment_profile
        .clone()
        .expect("profile after");
    let budget_after = fragment_after
        .fragment_budget
        .clone()
        .expect("budget after");

    assert_eq!(profile_after, profile_before);
    assert_eq!(budget_after, budget_before);
    assert_eq!(
        restored.model().chunk_resource_budgets,
        kernel.model().chunk_resource_budgets
    );
}

#[test]
fn kernel_persist_and_restore() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.step_until_empty();

    let tmp_dir = std::env::temp_dir().join("oasis7-kernel-test");
    if tmp_dir.exists() {
        fs::remove_dir_all(&tmp_dir).unwrap();
    }
    kernel.save_to_dir(&tmp_dir).unwrap();

    let loaded = WorldKernel::load_from_dir(&tmp_dir).unwrap();
    assert_eq!(loaded.time(), kernel.time());
    assert_eq!(loaded.model(), kernel.model());

    fs::remove_dir_all(&tmp_dir).unwrap();
}

#[cfg(feature = "test_tier_full")]
#[test]
fn kernel_loads_tracked_llm_baseline_fixture_state() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture_dir = repo_root.join("fixtures/llm_baseline/state_01");
    let snapshot_path = fixture_dir.join("snapshot.json");
    let journal_path = fixture_dir.join("journal.json");

    assert!(
        snapshot_path.is_file(),
        "missing tracked baseline snapshot fixture: {}",
        snapshot_path.display()
    );
    assert!(
        journal_path.is_file(),
        "missing tracked baseline journal fixture: {}",
        journal_path.display()
    );

    let kernel = WorldKernel::load_from_dir(&fixture_dir).expect("load tracked baseline fixture");
    assert!(
        kernel.time() >= 80,
        "baseline world age should already pass industrial bootstrap stage"
    );
    assert_eq!(kernel.model().agents.len(), 5);
    assert!(
        kernel.model().locations.len() >= 10,
        "baseline should contain multiple generated locations"
    );
    assert!(
        kernel.model().factories.len() >= 1,
        "baseline should already contain at least one factory"
    );
    assert!(
        kernel.journal().len() >= 90,
        "baseline journal should contain rich bootstrapping events"
    );
}

#[test]
fn restore_rejects_mismatched_journal_len() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.step_until_empty();

    let mut snapshot = kernel.snapshot();
    let journal = kernel.journal_snapshot();
    snapshot.journal_len = journal.events.len() + 1;

    let err = WorldKernel::from_snapshot(snapshot, journal).unwrap_err();
    assert!(matches!(err, PersistError::SnapshotMismatch { .. }));
}

#[test]
fn snapshot_version_validation_rejects_unknown() {
    let kernel = WorldKernel::new();
    let mut snapshot = kernel.snapshot();
    snapshot.version = SNAPSHOT_VERSION.saturating_add(1);
    let err = snapshot.validate_version().unwrap_err();
    assert!(matches!(
        err,
        PersistError::UnsupportedVersion {
            kind,
            version,
            expected
        } if kind == "snapshot" && version == snapshot.version && expected == SNAPSHOT_VERSION
    ));
}
#[test]
#[expect(clippy::field_reassign_with_default, reason = "fixture defaults")]
fn journal_version_validation_rejects_unknown() {
    let mut journal = WorldJournal::default();
    journal.version = JOURNAL_VERSION.saturating_add(1);
    let err = journal.validate_version().unwrap_err();
    assert!(matches!(
        err,
        PersistError::UnsupportedVersion {
            kind,
            version,
            expected
        } if kind == "journal" && version == journal.version && expected == JOURNAL_VERSION
    ));
}

#[test]
fn snapshot_version_validation_accepts_legacy_and_defaults_chunk_schema() {
    let kernel = WorldKernel::new();
    let snapshot = kernel.snapshot();

    let mut value: serde_json::Value =
        serde_json::from_str(&snapshot.to_json().expect("snapshot to json"))
            .expect("parse snapshot json");
    value["version"] = serde_json::Value::from(SNAPSHOT_VERSION.saturating_sub(1));
    if let serde_json::Value::Object(map) = &mut value {
        map.remove("chunk_generation_schema_version");
    }

    let migrated = WorldSnapshot::from_json(
        &serde_json::to_string(&value).expect("serialize migrated snapshot"),
    )
    .expect("load legacy snapshot");

    assert_eq!(migrated.version, SNAPSHOT_VERSION.saturating_sub(1));
    assert_eq!(
        migrated.chunk_generation_schema_version,
        CHUNK_GENERATION_SCHEMA_VERSION
    );
}

#[test]
fn snapshot_loads_legacy_float_geo_positions() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-legacy".to_string(),
        name: "legacy".to_string(),
        pos: GeoPos::new(10, 20, 30),
        profile: LocationProfile::default(),
    });
    kernel.step_until_empty();

    let mut value: serde_json::Value =
        serde_json::from_str(&kernel.snapshot().to_json().expect("snapshot to json"))
            .expect("parse snapshot json");

    let pos = value
        .get_mut("model")
        .and_then(|model| model.get_mut("locations"))
        .and_then(|locations| locations.get_mut("loc-legacy"))
        .and_then(|location| location.get_mut("pos"))
        .and_then(|pos| pos.as_object_mut())
        .expect("legacy location position");
    pos.insert("x_cm".to_string(), serde_json::json!(10.0));
    pos.insert("y_cm".to_string(), serde_json::json!(20.0));
    pos.insert("z_cm".to_string(), serde_json::json!(30.0));

    let migrated = WorldSnapshot::from_json(
        &serde_json::to_string(&value).expect("serialize migrated snapshot"),
    )
    .expect("load legacy float snapshot");

    let location = migrated
        .model
        .locations
        .get("loc-legacy")
        .expect("restored legacy location");
    assert_eq!(location.pos, GeoPos::new(10, 20, 30));
}

#[test]
fn snapshot_rejects_non_integral_legacy_float_geo_positions() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-invalid".to_string(),
        name: "invalid".to_string(),
        pos: GeoPos::new(10, 20, 30),
        profile: LocationProfile::default(),
    });
    kernel.step_until_empty();

    let mut value: serde_json::Value =
        serde_json::from_str(&kernel.snapshot().to_json().expect("snapshot to json"))
            .expect("parse snapshot json");

    let pos = value
        .get_mut("model")
        .and_then(|model| model.get_mut("locations"))
        .and_then(|locations| locations.get_mut("loc-invalid"))
        .and_then(|location| location.get_mut("pos"))
        .and_then(|pos| pos.as_object_mut())
        .expect("invalid location position");
    pos.insert("x_cm".to_string(), serde_json::json!(10.5));

    let err = WorldSnapshot::from_json(
        &serde_json::to_string(&value).expect("serialize migrated snapshot"),
    )
    .expect_err("non-integral legacy float snapshot should fail");

    assert!(
        matches!(err, PersistError::Serde(message) if message.contains("integer centimeter value"))
    );
}

#[test]
fn snapshot_agent_kinematics_defaults_when_legacy_field_is_missing() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.step_until_empty();

    let snapshot = kernel.snapshot();
    let mut value: serde_json::Value =
        serde_json::from_str(&snapshot.to_json().expect("snapshot to json"))
            .expect("parse snapshot json");

    let agents = value
        .get_mut("model")
        .and_then(|model| model.get_mut("agents"))
        .and_then(|agents| agents.as_object_mut())
        .expect("agents object");
    for agent in agents.values_mut() {
        let map = agent.as_object_mut().expect("agent object");
        map.remove("kinematics");
    }

    let migrated = WorldSnapshot::from_json(
        &serde_json::to_string(&value).expect("serialize migrated snapshot"),
    )
    .expect("load legacy snapshot without kinematics");

    let restored = migrated
        .model
        .agents
        .get("agent-1")
        .expect("restored agent exists");
    assert_eq!(restored.kinematics, AgentKinematics::default());
}

#[test]
fn snapshot_intel_ttl_ticks_defaults_when_legacy_field_is_missing() {
    let mut kernel = WorldKernel::new();
    kernel.set_intel_ttl_ticks(9);

    let snapshot = kernel.snapshot();
    let mut value: serde_json::Value =
        serde_json::from_str(&snapshot.to_json().expect("snapshot to json"))
            .expect("parse snapshot json");
    value
        .as_object_mut()
        .expect("snapshot object")
        .remove("intel_ttl_ticks");

    let migrated = WorldSnapshot::from_json(
        &serde_json::to_string(&value).expect("serialize migrated snapshot"),
    )
    .expect("load legacy snapshot without intel ttl");
    let restored =
        WorldKernel::from_snapshot(migrated, kernel.journal_snapshot()).expect("restore");

    assert_eq!(restored.intel_ttl_ticks(), 0);
}

#[test]
fn snapshot_runtime_snapshot_accepts_stringified_numeric_map_keys() {
    let mut snapshot = WorldKernel::new().snapshot();
    let mut runtime_snapshot = RuntimeWorld::default().snapshot();
    runtime_snapshot.state.pending_factory_builds.insert(
        6,
        FactoryBuildJobState {
            job_id: 6,
            builder_agent_id: "agent-0".to_string(),
            site_id: "site-0".to_string(),
            spec: FactoryModuleSpec {
                factory_id: "factory.smelter.mk1".to_string(),
                display_name: "Smelter MK1".to_string(),
                tier: 2,
                tags: vec!["smelter".to_string()],
                build_cost: vec![MaterialStack::new("structural_frame", 1)],
                build_time_ticks: 1,
                base_power_draw: 20,
                recipe_slots: 2,
                throughput_bps: 10_000,
                maintenance_per_tick: 1,
            },
            consume_ledger: MaterialLedgerId::world(),
            ready_at: 2,
            contract_version: 0,
            site_authority_revision: None,
            site_location_id: None,
            location_anchor_revision: None,
            construction_power_obligation: None,
        },
    );
    snapshot.runtime_snapshot = Some(runtime_snapshot);

    let restored = WorldSnapshot::from_json(&snapshot.to_json().expect("snapshot to json"))
        .expect("restore world snapshot");

    let runtime_snapshot = restored.runtime_snapshot.expect("runtime snapshot");
    assert!(
        runtime_snapshot
            .state
            .pending_factory_builds
            .contains_key(&6)
    );
}

#[test]
#[expect(clippy::field_reassign_with_default, reason = "fixture defaults")]
fn journal_version_validation_accepts_legacy() {
    let mut journal = WorldJournal::default();
    journal.version = JOURNAL_VERSION.saturating_sub(1);
    assert!(journal.validate_version().is_ok());
}

#[test]
#[expect(clippy::field_reassign_with_default, reason = "fixture defaults")]
fn initialize_kernel_records_chunk_generated_init_events() {
    let mut config = WorldConfig::default();
    config.asteroid_fragment.base_density_per_km3 = 0.0;

    let mut init = WorldInitConfig::default();
    init.seed = 41;
    init.agents.count = 1;

    let (kernel, _) = initialize_kernel(config, init).expect("kernel init");
    let init_chunk_events = kernel
        .journal()
        .iter()
        .filter(|event| {
            matches!(
                event.kind,
                WorldEventKind::ChunkGenerated {
                    cause: ChunkGenerationCause::Init,
                    ..
                }
            )
        })
        .count();

    assert!(init_chunk_events > 0);
    let snapshot = kernel.snapshot();
    assert!(snapshot.chain_resource_manifest.is_schema_current());
    assert!(!snapshot.chain_resource_manifest.generated_chunks.is_empty());
    assert!(snapshot.latest_chain_resource_delta.is_schema_current());
    assert!(!snapshot.latest_chain_resource_delta.entries.is_empty());
    assert_eq!(
        snapshot.latest_chain_resource_delta.resulting_manifest_hash,
        snapshot.chain_resource_manifest.manifest_hash
    );
}

#[test]
#[expect(clippy::field_reassign_with_default, reason = "fixture defaults")]
fn chain_resource_snapshot_records_generated_chunk_and_latest_delta() {
    let mut config = WorldConfig::default();
    config.asteroid_fragment.base_density_per_km3 = 0.0;

    let mut init = WorldInitConfig::default();
    init.seed = 73;
    init.agents.count = 1;

    let (kernel, _) = initialize_kernel(config, init).expect("kernel init");
    let snapshot = kernel.snapshot();
    let journal = kernel.journal_snapshot();
    let restored = WorldKernel::from_snapshot(snapshot.clone(), journal).expect("restore ok");
    let restored_snapshot = restored.snapshot();

    assert_eq!(
        snapshot.chain_resource_manifest.generated_chunks,
        restored_snapshot.chain_resource_manifest.generated_chunks
    );
    assert_eq!(
        snapshot.latest_chain_resource_delta.entries,
        restored_snapshot.latest_chain_resource_delta.entries
    );
    assert!(
        snapshot
            .chain_resource_manifest
            .generated_chunks
            .values()
            .all(|entry| !entry.chunk_budget_total_hash.is_empty()
                && !entry.chunk_budget_remaining_hash.is_empty()
                && entry.commit_ref.event_id.is_some())
    );
}

#[test]
#[expect(clippy::field_reassign_with_default, reason = "fixture defaults")]
fn replay_from_snapshot_rebuilds_and_validates_chunk_generated_events() {
    let mut config = WorldConfig::default();
    config.move_cost_per_km_electricity = 0;
    config.physics.max_move_distance_cm_per_tick = i64::MAX;
    config.physics.max_move_speed_cm_per_s = i64::MAX;
    config.asteroid_fragment.base_density_per_km3 = 0.0;

    let mut init = WorldInitConfig::default();
    init.seed = 97;
    init.agents.count = 1;

    let (mut kernel, _) = initialize_kernel(config.clone(), init).expect("init kernel");
    let snapshot = kernel.snapshot();

    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-far".to_string(),
        name: "far".to_string(),
        pos: GeoPos {
            x_cm: 100_000,
            y_cm: 100_000,
            z_cm: 0,
        },
        profile: LocationProfile::default(),
    });
    kernel.step().expect("register far location");

    kernel.submit_action(Action::MoveAgent {
        agent_id: "agent-0".to_string(),
        to: "loc-far".to_string(),
    });
    kernel.step().expect("move to far location");

    let journal = kernel.journal_snapshot();
    let chunk_event_index = journal
        .events
        .iter()
        .enumerate()
        .skip(snapshot.journal_len)
        .find_map(|(idx, event)| match event.kind {
            WorldEventKind::ChunkGenerated {
                cause: ChunkGenerationCause::Action,
                ..
            } => Some(idx),
            _ => None,
        })
        .expect("action chunk generation event exists");

    let replayed = WorldKernel::replay_from_snapshot(snapshot.clone(), journal.clone())
        .expect("replay with chunk-generated event");
    assert_eq!(replayed.model(), kernel.model());

    let mut tampered = journal;
    if let WorldEventKind::ChunkGenerated { block_count, .. } =
        &mut tampered.events[chunk_event_index].kind
    {
        *block_count = block_count.saturating_add(1);
    }

    let err = WorldKernel::replay_from_snapshot(snapshot, tampered).unwrap_err();
    assert!(matches!(err, PersistError::ReplayConflict { .. }));
}

#[test]
fn kernel_replay_from_snapshot() {
    let config = WorldConfig {
        move_cost_per_km_electricity: 0,
        ..Default::default()
    };
    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-2".to_string(),
        name: "outpost".to_string(),
        pos: pos(1, 1),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.step_until_empty();

    let snapshot = kernel.snapshot();
    let mut journal = kernel.journal_snapshot();

    kernel.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: "loc-2".to_string(),
    });
    let event = kernel.step().unwrap();
    journal.events.push(event);

    let replayed = WorldKernel::replay_from_snapshot(snapshot, journal).unwrap();
    let agent = replayed.model().agents.get("agent-1").unwrap();
    assert_eq!(agent.location_id, "loc-2");
}

#[test]
#[expect(clippy::field_reassign_with_default, reason = "fixture defaults")]
fn replay_from_snapshot_applies_compound_refined_event() {
    let mut config = WorldConfig::default();
    config.economy.refine_electricity_cost_per_kg = 3;
    config.economy.refine_hardware_yield_ppm = 2_000;

    let mut kernel = WorldKernel::with_config(config);
    let mut profile = LocationProfile::default();
    profile.radiation_emission_per_tick = 120;
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-refine".to_string(),
        name: "refine".to_string(),
        pos: pos(0, 0),
        profile,
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-refiner".to_string(),
        location_id: "loc-refine".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::HarvestRadiation {
        agent_id: "agent-refiner".to_string(),
        max_amount: 50,
    });
    kernel.step().expect("seed electricity");
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "agent-refiner".to_string(),
        },
        ResourceKind::Data,
        2_500,
    );
    let snapshot = kernel.snapshot();

    kernel.submit_action(Action::RefineCompound {
        owner: ResourceOwner::Agent {
            agent_id: "agent-refiner".to_string(),
        },
        compound_mass_g: 2_500,
    });
    kernel.step().expect("refine");

    let journal = kernel.journal_snapshot();
    let replayed = WorldKernel::replay_from_snapshot(snapshot, journal).expect("replay");

    let agent = replayed
        .model()
        .agents
        .get("agent-refiner")
        .expect("agent exists");
    assert_eq!(agent.resources.get(ResourceKind::Electricity), 41);
    assert_eq!(agent.resources.get(ResourceKind::Data), 5);
}

#[test]
#[expect(clippy::field_reassign_with_default, reason = "fixture defaults")]
fn replay_from_snapshot_applies_compound_mined_event() {
    let mut config = WorldConfig::default();
    config.economy.mine_electricity_cost_per_kg = 2;
    config.economy.mine_compound_max_per_action_g = 2_000;
    config.economy.mine_compound_max_per_location_g = 10_000;
    config.space = SpaceConfig {
        width_cm: 200_000,
        depth_cm: 200_000,
        height_cm: 200_000,
    };
    config.asteroid_fragment.base_density_per_km3 = 5.0;
    config.asteroid_fragment.voxel_size_km = 1;
    config.asteroid_fragment.cluster_noise = 0.0;
    config.asteroid_fragment.layer_scale_height_km = 0.0;
    config.asteroid_fragment.radius_min_cm = 120;
    config.asteroid_fragment.radius_max_cm = 120;

    let mut init = WorldInitConfig::default();
    init.seed = 98;
    init.agents.count = 0;

    let (mut kernel, _) = initialize_kernel(config.clone(), init).expect("init kernel");
    let location_id = kernel
        .model()
        .locations
        .values()
        .find(|loc| loc.id.starts_with("frag-"))
        .map(|loc| loc.id.clone())
        .expect("fragment exists");

    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-miner".to_string(),
        location_id: location_id.clone(),
    });
    kernel.step().expect("register miner");
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "agent-miner".to_string(),
        },
        ResourceKind::Electricity,
        30,
    );

    let snapshot = kernel.snapshot();
    kernel.submit_action(Action::MineCompound {
        owner: ResourceOwner::Agent {
            agent_id: "agent-miner".to_string(),
        },
        location_id: location_id.clone(),
        compound_mass_g: 1_200,
    });
    kernel.step().expect("mine");

    let journal = kernel.journal_snapshot();
    let replayed = WorldKernel::replay_from_snapshot(snapshot, journal).expect("replay");

    let agent = replayed
        .model()
        .agents
        .get("agent-miner")
        .expect("agent exists");
    assert_eq!(agent.resources.get(ResourceKind::Data), 1_200);
    assert_eq!(agent.resources.get(ResourceKind::Electricity), 26);
    let location = replayed
        .model()
        .locations
        .get(&location_id)
        .expect("location exists");
    assert_eq!(location.mined_compound_g, 1_200);
}

#[test]
fn replay_from_snapshot_applies_debug_resource_granted_event() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-debug".to_string(),
        name: "debug".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-debug".to_string(),
        location_id: "loc-debug".to_string(),
    });
    kernel.step_until_empty();

    let snapshot = kernel.snapshot();
    kernel.submit_action(Action::DebugGrantResource {
        owner: ResourceOwner::Agent {
            agent_id: "agent-debug".to_string(),
        },
        kind: ResourceKind::Data,
        amount: 99,
    });
    kernel.step().expect("debug grant");

    let journal = kernel.journal_snapshot();
    let replayed = WorldKernel::replay_from_snapshot(snapshot, journal).expect("replay");
    let agent = replayed
        .model()
        .agents
        .get("agent-debug")
        .expect("agent exists");
    assert_eq!(agent.resources.get(ResourceKind::Data), 99);
}

#[test]
#[expect(clippy::field_reassign_with_default, reason = "fixture defaults")]
fn replay_with_budget_caps_keeps_chunk_generated_consistent() {
    let mut config = WorldConfig::default();
    config.move_cost_per_km_electricity = 0;
    config.physics.max_move_distance_cm_per_tick = i64::MAX;
    config.physics.max_move_speed_cm_per_s = i64::MAX;
    config.asteroid_fragment.base_density_per_km3 = 20.0;
    config.asteroid_fragment.voxel_size_km = 10;
    config.asteroid_fragment.cluster_noise = 0.0;
    config.asteroid_fragment.layer_scale_height_km = 0.0;
    config.asteroid_fragment.min_fragment_spacing_cm = 0;
    config.asteroid_fragment.radius_min_cm = 2_500;
    config.asteroid_fragment.radius_max_cm = 2_500;
    config.asteroid_fragment.max_fragments_per_chunk = 2;
    config.asteroid_fragment.max_blocks_per_fragment = 2;
    config.asteroid_fragment.max_blocks_per_chunk = 3;

    let mut init = WorldInitConfig::default();
    init.seed = 197;
    init.agents.count = 1;

    let (mut kernel, _) = initialize_kernel(config.clone(), init).expect("init kernel");
    let snapshot = kernel.snapshot();

    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-budget".to_string(),
        name: "budget".to_string(),
        pos: GeoPos {
            x_cm: 2_500_000,
            y_cm: 2_500_000,
            z_cm: 0,
        },
        profile: LocationProfile::default(),
    });
    kernel.step().expect("register location");

    kernel.submit_action(Action::MoveAgent {
        agent_id: "agent-0".to_string(),
        to: "loc-budget".to_string(),
    });
    kernel.step().expect("move agent");

    let journal = kernel.journal_snapshot();
    let capped_action_event = journal
        .events
        .iter()
        .find_map(|event| match event.kind {
            WorldEventKind::ChunkGenerated {
                cause: ChunkGenerationCause::Action,
                fragment_count,
                block_count,
                ..
            } => Some((fragment_count, block_count)),
            _ => None,
        })
        .expect("action chunk generated event");

    assert!(capped_action_event.0 <= 2);
    assert!(capped_action_event.1 <= 3);

    let replayed = WorldKernel::replay_from_snapshot(snapshot, journal).expect("replay");
    assert_eq!(replayed.model(), kernel.model());
}

#[test]
#[expect(clippy::field_reassign_with_default, reason = "fixture defaults")]
fn replay_from_snapshot_applies_fragment_replenished_event() {
    let mut config = WorldConfig::default();
    config.asteroid_fragment.base_density_per_km3 = 0.0;
    config.asteroid_fragment.min_fragments_per_chunk = 0;
    config.asteroid_fragment.max_fragments_per_chunk = 100;
    config.asteroid_fragment.replenish_interval_ticks = 5;
    config.asteroid_fragment.replenish_percent_ppm = 10_000;

    let target_chunk = ChunkCoord { x: 0, y: 0, z: 0 };
    let mut init = WorldInitConfig::default();
    init.seed = 456;
    init.agents.count = 0;
    init.asteroid_fragment.bootstrap_chunks = vec![target_chunk];

    let (mut kernel, _) = initialize_kernel(config.clone(), init).expect("init kernel");
    let snapshot = kernel.snapshot();

    for i in 0..5 {
        kernel.submit_action(Action::RegisterLocation {
            location_id: format!("replay-replenish-loc-{i}"),
            name: format!("replay-replenish-loc-{i}"),
            pos: GeoPos {
                x_cm: 10_000 + i as i64,
                y_cm: 20_000 + i as i64,
                z_cm: 30_000,
            },
            profile: LocationProfile::default(),
        });
        kernel.step().expect("step");
    }

    let journal = kernel.journal_snapshot();
    assert!(
        journal
            .events
            .iter()
            .any(|event| { matches!(event.kind, WorldEventKind::FragmentsReplenished { .. }) })
    );

    let replayed = WorldKernel::replay_from_snapshot(snapshot, journal).expect("replay");
    assert_eq!(replayed.model(), kernel.model());
}

#[test]
fn replay_from_snapshot_applies_agent_prompt_updated_event() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let (mut kernel, _) = initialize_kernel(config, init).expect("init kernel");
    let snapshot = kernel.snapshot();

    let mut profile = AgentPromptProfile::for_agent("agent-0");
    profile.system_prompt_override = Some("system-v1".to_string());
    profile.short_term_goal_override = Some("goal-v1".to_string());
    profile.version = 1;
    profile.updated_at_tick = kernel.time();
    profile.updated_by = "persist-test".to_string();

    kernel.apply_agent_prompt_profile_update(
        profile.clone(),
        PromptUpdateOperation::Apply,
        vec![
            "system_prompt_override".to_string(),
            "short_term_goal_override".to_string(),
        ],
        "digest-v1".to_string(),
        None,
    );

    let journal = kernel.journal_snapshot();
    let replayed =
        WorldKernel::replay_from_snapshot(snapshot, journal).expect("replay with prompt update");

    assert_eq!(replayed.model(), kernel.model());
    assert_eq!(
        replayed.model().agent_prompt_profiles.get("agent-0"),
        Some(&profile)
    );
}

#[test]
fn replay_from_snapshot_applies_agent_player_bound_event() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let (mut kernel, _) = initialize_kernel(config, init).expect("init kernel");
    let snapshot = kernel.snapshot();

    let bind_event = kernel
        .bind_agent_player("agent-0", "player-a", Some("pubkey-a"))
        .expect("bind agent to player");
    assert!(bind_event.is_some());

    let journal = kernel.journal_snapshot();
    let replayed =
        WorldKernel::replay_from_snapshot(snapshot, journal).expect("replay with player binding");

    assert_eq!(replayed.model(), kernel.model());
    assert_eq!(
        replayed.model().agent_player_bindings.get("agent-0"),
        Some(&"player-a".to_string())
    );
    assert_eq!(
        replayed
            .model()
            .agent_player_public_key_bindings
            .get("agent-0"),
        Some(&"pubkey-a".to_string())
    );
}

#[test]
fn replay_from_snapshot_applies_legacy_agent_player_bound_event_without_public_key() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let (mut kernel, _) = initialize_kernel(config, init).expect("init kernel");
    let snapshot = kernel.snapshot();

    let bind_event = kernel
        .bind_agent_player("agent-0", "player-a", None)
        .expect("bind agent to player");
    assert!(bind_event.is_some());

    let journal = kernel.journal_snapshot();
    let replayed =
        WorldKernel::replay_from_snapshot(snapshot, journal).expect("replay with player binding");

    assert_eq!(replayed.model(), kernel.model());
    assert_eq!(
        replayed.model().agent_player_bindings.get("agent-0"),
        Some(&"player-a".to_string())
    );
    assert_eq!(
        replayed
            .model()
            .agent_player_public_key_bindings
            .get("agent-0"),
        None
    );
}
