use super::super::*;
use super::World;
use std::sync::Arc;

fn manifest(module_id: &str, version: &str, wasm_hash: &str) -> ModuleManifest {
    ModuleManifest {
        module_id: module_id.into(),
        name: format!("Metadata {module_id} {version}"),
        version: version.into(),
        kind: ModuleKind::Pure,
        role: ModuleRole::AgentInternal,
        wasm_hash: wasm_hash.into(),
        interface_version: "wasm-1".into(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".into()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        artifact_identity: None,
        limits: ModuleLimits::default(),
    }
}

fn event(kind: ModuleEventKind) -> ModuleEvent {
    ModuleEvent {
        proposal_id: 37,
        kind,
    }
}

fn seed_record(world: &mut World, module: ModuleManifest, active: bool) {
    let module_id = module.module_id.clone();
    let version = module.version.clone();
    world
        .apply_module_event(
            &event(ModuleEventKind::RegisterModule {
                module,
                registered_by: "seed".into(),
            }),
            world.state.time,
        )
        .expect("seed module record");
    if active {
        world
            .apply_module_event(
                &event(ModuleEventKind::ActivateModule {
                    module_id,
                    version,
                    activated_by: "seed".into(),
                }),
                world.state.time,
            )
            .expect("seed active module");
    }
}

fn seed_subscription_cache(world: &mut World, record_key: String, marker: &str) -> String {
    let key = format!("{record_key}|{marker}");
    world.prepared_subscription_cache.insert(
        key.clone(),
        super::PreparedSubscriptionCacheEntry {
            subscriptions: Vec::new(),
            _subscription_fingerprint: marker.into(),
            prepared: Arc::<[super::PreparedSubscription]>::from([]),
        },
    );
    key
}

fn assert_world_unchanged(world: &World, expected: &World, expected_root: &str) {
    assert_eq!(world.snapshot(), expected.snapshot());
    assert_eq!(world.manifest, expected.manifest);
    assert_eq!(world.module_registry, expected.module_registry);
    assert_eq!(world.module_artifacts, expected.module_artifacts);
    assert_eq!(world.module_artifact_bytes, expected.module_artifact_bytes);
    assert_eq!(world.module_cache, expected.module_cache);
    assert_eq!(
        format!("{:?}", world.prepared_subscription_cache),
        format!("{:?}", expected.prepared_subscription_cache)
    );
    assert_eq!(world.module_tick_schedule, expected.module_tick_schedule);
    assert_eq!(world.journal(), expected.journal());
    assert_eq!(
        (world.next_event_id, world.next_event_id_era),
        (expected.next_event_id, expected.next_event_id_era)
    );
    assert_eq!(
        world.runtime_backpressure_stats(),
        expected.runtime_backpressure_stats()
    );
    assert_eq!(
        world.tick_consensus_records(),
        expected.tick_consensus_records()
    );
    assert_eq!(world.state.time, expected.state.time);
    assert_eq!(
        world.current_state_root_hash().expect("current state root"),
        expected_root
    );
}

fn assert_injected(error: &WorldError) {
    assert!(
        matches!(
            error,
            WorldError::ResourceBalanceInvalid { reason }
                if reason == "injected append_event failure after publication preparation"
        ),
        "unexpected error: {error:?}"
    );
}

#[test]
fn every_raw_module_event_post_prepare_failure_preserves_publication_state() {
    let cases = ["register", "upgrade", "activate", "deactivate"];
    for case in cases {
        let mut world = World::new();
        let target = manifest("m.metadata.target", "1.0.0", "target-hash");
        if case != "register" {
            seed_record(&mut world, target.clone(), case == "deactivate");
        }
        let target_key = ModuleRegistry::record_key("m.metadata.target", "1.0.0");
        let unrelated_key = ModuleRegistry::record_key("m.metadata.unrelated", "1.0.0");
        let _ = seed_subscription_cache(&mut world, target_key, "target-cache");
        let _ = seed_subscription_cache(&mut world, unrelated_key, "unrelated-cache");
        let kind = match case {
            "register" => ModuleEventKind::RegisterModule {
                module: target,
                registered_by: "actor".into(),
            },
            "upgrade" => ModuleEventKind::UpgradeModule {
                module_id: "m.metadata.target".into(),
                from_version: "1.0.0".into(),
                to_version: "2.0.0".into(),
                wasm_hash: "upgrade-informational-hash".into(),
                manifest: manifest("m.metadata.target", "2.0.0", "upgrade-manifest-hash"),
                upgraded_by: "actor".into(),
            },
            "activate" => ModuleEventKind::ActivateModule {
                module_id: "m.metadata.target".into(),
                version: "1.0.0".into(),
                activated_by: "actor".into(),
            },
            "deactivate" => ModuleEventKind::DeactivateModule {
                module_id: "m.metadata.target".into(),
                reason: "fixture".into(),
                deactivated_by: "actor".into(),
            },
            _ => unreachable!(),
        };
        let before = world.clone();
        let root_before = world.current_state_root_hash().expect("state root before");
        world.fail_next_append_after_publication_prepare_for_test();

        let error = world
            .append_event(WorldEventBody::ModuleEvent(event(kind)), None)
            .expect_err("post-prepare fault must reject raw module event");

        assert_injected(&error);
        assert_world_unchanged(&world, &before, &root_before);
    }
}

#[test]
fn missing_record_activation_is_exact_and_does_not_leak_active_state() {
    let mut world = World::new();
    let before = world.clone();
    let root_before = world.current_state_root_hash().expect("state root before");
    let error = world
        .append_event(
            WorldEventBody::ModuleEvent(event(ModuleEventKind::ActivateModule {
                module_id: "m.metadata.missing".into(),
                version: "9.9.9".into(),
                activated_by: "actor".into(),
            })),
            None,
        )
        .expect_err("missing activation record must be rejected");

    assert!(matches!(
        error,
        WorldError::ModuleChangeInvalid { reason }
            if reason == "module record missing m.metadata.missing@9.9.9"
    ));
    assert_world_unchanged(&world, &before, &root_before);
}

#[test]
fn raw_manifest_update_post_prepare_failure_preserves_publication_state() {
    let mut world = World::new();
    let before = world.clone();
    let root_before = world.current_state_root_hash().expect("state root before");
    let mut content = serde_json::Map::new();
    content.insert("fixture".into(), serde_json::json!(true));
    let update = ManifestUpdate {
        manifest: Manifest {
            version: world.manifest.version.saturating_add(1),
            content: serde_json::Value::Object(content),
        },
        manifest_hash: "informational-hash".into(),
    };
    world.fail_next_append_after_publication_prepare_for_test();

    let error = world
        .append_event(WorldEventBody::ManifestUpdated(update), None)
        .expect_err("post-prepare fault must reject raw manifest update");

    assert_injected(&error);
    assert_world_unchanged(&world, &before, &root_before);
}

#[test]
fn register_and_upgrade_preserve_overwrite_asymmetry_and_targeted_cache_invalidation() {
    let mut world = World::new();
    world.state.time = 44;
    let register_key = ModuleRegistry::record_key("m.metadata.register", "1.0.0");
    let upgrade_key = ModuleRegistry::record_key("m.metadata.upgrade-key", "2.0.0");
    let unrelated_key = ModuleRegistry::record_key("m.metadata.unrelated", "1.0.0");
    let old_register_hash = "old-register-hash";
    let old_upgrade_hash = "old-upgrade-hash";
    world.module_registry.records.insert(
        register_key.clone(),
        ModuleRecord {
            manifest: manifest("m.metadata.register", "1.0.0", old_register_hash),
            registered_at: 3,
            registered_by: "old-registrar".into(),
            audit_event_id: Some(11),
        },
    );
    world.module_registry.records.insert(
        upgrade_key.clone(),
        ModuleRecord {
            manifest: manifest("m.metadata.upgrade-key", "2.0.0", old_upgrade_hash),
            registered_at: 5,
            registered_by: "old-upgrader".into(),
            audit_event_id: Some(13),
        },
    );
    world.module_artifacts.insert(old_register_hash.into());
    world.module_artifacts.insert(old_upgrade_hash.into());
    let register_cache_key = seed_subscription_cache(&mut world, register_key.clone(), "register");
    let upgrade_cache_key = seed_subscription_cache(&mut world, upgrade_key.clone(), "upgrade");
    let unrelated_cache_key =
        seed_subscription_cache(&mut world, unrelated_key.clone(), "unrelated");

    let registered = manifest("m.metadata.register", "1.0.0", "registered-hash");
    world
        .append_event(
            WorldEventBody::ModuleEvent(event(ModuleEventKind::RegisterModule {
                module: registered.clone(),
                registered_by: "registrar".into(),
            })),
            None,
        )
        .expect("register raw module event");
    let asymmetric = manifest("m.metadata.payload", "9.9.9", "payload-hash");
    world
        .append_event(
            WorldEventBody::ModuleEvent(event(ModuleEventKind::UpgradeModule {
                module_id: "m.metadata.upgrade-key".into(),
                from_version: "1.0.0".into(),
                to_version: "2.0.0".into(),
                wasm_hash: "informational-upgrade-hash".into(),
                manifest: asymmetric.clone(),
                upgraded_by: "upgrader".into(),
            })),
            None,
        )
        .expect("upgrade raw module event");

    let registered_record = &world.module_registry.records[&register_key];
    assert_eq!(registered_record.manifest, registered);
    assert_eq!(registered_record.registered_at, 44);
    assert_eq!(registered_record.registered_by, "registrar");
    assert_eq!(registered_record.audit_event_id, None);
    let upgraded_record = &world.module_registry.records[&upgrade_key];
    assert_eq!(upgraded_record.manifest, asymmetric);
    assert_eq!(upgraded_record.registered_at, 44);
    assert_eq!(upgraded_record.registered_by, "upgrader");
    assert_eq!(upgraded_record.audit_event_id, None);
    for hash in [
        old_register_hash,
        old_upgrade_hash,
        "registered-hash",
        "payload-hash",
    ] {
        assert!(
            world.module_artifacts.contains(hash),
            "missing artifact {hash}"
        );
    }
    assert!(
        !world
            .prepared_subscription_cache
            .contains_key(&register_cache_key)
    );
    assert!(
        !world
            .prepared_subscription_cache
            .contains_key(&upgrade_cache_key)
    );
    assert!(
        world
            .prepared_subscription_cache
            .contains_key(&unrelated_cache_key)
    );
}

#[test]
fn activation_and_deactivation_preserve_tick_and_unknown_module_compatibility() {
    let mut world = World::new();
    let mut tick = manifest("m.metadata.tick", "1.0.0", "tick-hash");
    tick.subscriptions.push(ModuleSubscription {
        event_kinds: vec!["Tick".into()],
        action_kinds: Vec::new(),
        stage: Some(ModuleSubscriptionStage::Tick),
        filters: None,
    });
    seed_record(&mut world, tick, false);
    seed_record(
        &mut world,
        manifest("m.metadata.non-tick", "1.0.0", "non-tick-hash"),
        false,
    );
    for module_id in ["m.metadata.tick", "m.metadata.non-tick"] {
        world
            .append_event(
                WorldEventBody::ModuleEvent(event(ModuleEventKind::ActivateModule {
                    module_id: module_id.into(),
                    version: "1.0.0".into(),
                    activated_by: "actor".into(),
                })),
                None,
            )
            .expect("activate raw module event");
    }
    assert_eq!(
        world.module_tick_schedule["m.metadata.tick"],
        world.state.time
    );
    assert!(
        !world
            .module_tick_schedule
            .contains_key("m.metadata.non-tick")
    );

    world
        .append_event(
            WorldEventBody::ModuleEvent(event(ModuleEventKind::DeactivateModule {
                module_id: "m.metadata.unknown".into(),
                reason: "no-op".into(),
                deactivated_by: "actor".into(),
            })),
            None,
        )
        .expect("unknown deactivation remains accepted");
    world
        .append_event(
            WorldEventBody::ModuleEvent(event(ModuleEventKind::DeactivateModule {
                module_id: "m.metadata.tick".into(),
                reason: "stop".into(),
                deactivated_by: "actor".into(),
            })),
            None,
        )
        .expect("deactivate tick module");
    assert!(!world.module_registry.active.contains_key("m.metadata.tick"));
    assert!(!world.module_tick_schedule.contains_key("m.metadata.tick"));
}

#[test]
fn informational_manifest_hash_remains_accepted_with_live_replay_root_equality() {
    let mut world = World::new();
    let baseline = world.snapshot();
    let mut content = serde_json::Map::new();
    content.insert("metadata".into(), serde_json::json!("replacement"));
    let replacement = Manifest {
        version: 41,
        content: serde_json::Value::Object(content),
    };
    world
        .append_event(
            WorldEventBody::ManifestUpdated(ManifestUpdate {
                manifest: replacement.clone(),
                manifest_hash: "deliberately-not-the-canonical-hash".into(),
            }),
            None,
        )
        .expect("informational manifest hash remains accepted");

    assert_eq!(world.manifest, replacement);
    let replay = World::from_snapshot(baseline, world.journal().clone()).expect("replay manifest");
    assert_eq!(replay.manifest, world.manifest);
    assert_eq!(
        replay.current_state_root_hash().expect("replay root"),
        world.current_state_root_hash().expect("live root")
    );
}
