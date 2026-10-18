use super::super::*;
use super::World;
use std::sync::Arc;

fn register_artifact(world: &mut World, label: &str) -> String {
    let bytes = format!("module-change-batch-{label}").into_bytes();
    let hash = crate::util::sha256_hex(&bytes);
    world
        .register_module_artifact(hash.clone(), &bytes)
        .unwrap();
    hash
}

fn manifest(module_id: &str, version: &str, wasm_hash: String) -> ModuleManifest {
    ModuleManifest {
        module_id: module_id.into(),
        name: format!("Batch-{module_id}-{version}"),
        version: version.into(),
        kind: ModuleKind::Pure,
        role: ModuleRole::AgentInternal,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".into(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".into()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        artifact_identity: Some(crate::tests::signed_test_artifact_identity(
            &wasm_hash,
        )),
        limits: ModuleLimits::default(),
    }
}

fn assert_world_unchanged(world: &World, expected: &World) {
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
}

#[test]
fn second_register_tail_failure_preserves_whole_change_batch() {
    let mut world = World::new();
    let first = manifest("m.batch.a", "1.0.0", register_artifact(&mut world, "first"));
    let second = manifest(
        "m.batch.b",
        "1.0.0",
        register_artifact(&mut world, "second"),
    );
    let before = world.clone();
    world.fail_append_after_publication_prepare_on_nth_for_test(2);
    world
        .apply_module_changes_for_test(
            71,
            &ModuleChangeSet {
                register: vec![second, first],
                ..ModuleChangeSet::default()
            },
            "batch-actor",
        )
        .expect_err("second publication failure must reject the whole sorted batch");
    assert_world_unchanged(&world, &before);
}

#[test]
fn register_then_invalid_activate_preserves_whole_change_batch() {
    let mut world = World::new();
    let registered = manifest(
        "m.batch.natural",
        "1.0.0",
        register_artifact(&mut world, "natural"),
    );
    let before = world.clone();
    let error = world
        .apply_module_changes_for_test(
            72,
            &ModuleChangeSet {
                register: vec![registered],
                activate: vec![ModuleActivation {
                    module_id: "m.batch.natural".into(),
                    version: "9.9.9".into(),
                }],
                ..ModuleChangeSet::default()
            },
            "batch-actor",
        )
        .expect_err("invalid activation after register must reject the whole batch");
    assert!(
        format!("{error:?}").contains("record missing"),
        "unexpected error: {error:?}"
    );
    assert_world_unchanged(&world, &before);
}

fn seed_manifest(world: &mut World, manifest: ModuleManifest, active: bool) {
    let module_id = manifest.module_id.clone();
    let version = manifest.version.clone();
    world
        .apply_module_event(
            &ModuleEvent {
                proposal_id: 0,
                kind: ModuleEventKind::RegisterModule {
                    module: manifest,
                    registered_by: "seed".into(),
                },
            },
            world.state.time,
        )
        .unwrap();
    if active {
        world
            .apply_module_event(
                &ModuleEvent {
                    proposal_id: 0,
                    kind: ModuleEventKind::ActivateModule {
                        module_id,
                        version,
                        activated_by: "seed".into(),
                    },
                },
                world.state.time,
            )
            .unwrap();
    }
}

fn full_changes(world: &mut World) -> ModuleChangeSet {
    let register_a = manifest(
        "m.batch.register.a",
        "1.0.0",
        "initially-absent-register-a-hash".into(),
    );
    let register_b = manifest(
        "m.batch.register.b",
        "1.0.0",
        register_artifact(world, "register-b"),
    );
    let upgrade_old_a = manifest(
        "m.batch.upgrade.a",
        "1.0.0",
        register_artifact(world, "upgrade-old-a"),
    );
    let upgrade_old_b = manifest(
        "m.batch.upgrade.b",
        "1.0.0",
        register_artifact(world, "upgrade-old-b"),
    );
    seed_manifest(world, upgrade_old_a.clone(), false);
    seed_manifest(world, upgrade_old_b.clone(), false);
    let upgrade_new_a = manifest(
        "m.batch.upgrade.a",
        "2.0.0",
        register_artifact(world, "upgrade-new-a"),
    );
    let upgrade_new_b = manifest(
        "m.batch.upgrade.b",
        "2.0.0",
        register_artifact(world, "upgrade-new-b"),
    );
    for suffix in ["a", "b"] {
        let seeded = manifest(
            &format!("m.batch.deactivate.{suffix}"),
            "1.0.0",
            register_artifact(world, &format!("deactivate-{suffix}")),
        );
        seed_manifest(world, seeded, true);
    }
    ModuleChangeSet {
        register: vec![register_b, register_a],
        upgrade: vec![
            ModuleUpgrade {
                module_id: upgrade_new_b.module_id.clone(),
                from_version: "1.0.0".into(),
                to_version: "2.0.0".into(),
                wasm_hash: upgrade_new_b.wasm_hash.clone(),
                manifest: upgrade_new_b,
            },
            ModuleUpgrade {
                module_id: upgrade_new_a.module_id.clone(),
                from_version: "1.0.0".into(),
                to_version: "2.0.0".into(),
                wasm_hash: upgrade_new_a.wasm_hash.clone(),
                manifest: upgrade_new_a,
            },
        ],
        activate: vec![
            ModuleActivation {
                module_id: "m.batch.register.b".into(),
                version: "1.0.0".into(),
            },
            ModuleActivation {
                module_id: "m.batch.register.a".into(),
                version: "1.0.0".into(),
            },
        ],
        deactivate: vec![
            ModuleDeactivation {
                module_id: "m.batch.deactivate.b".into(),
                reason: "retire-b".into(),
            },
            ModuleDeactivation {
                module_id: "m.batch.deactivate.a".into(),
                reason: "retire-a".into(),
            },
        ],
    }
}

fn seed_subscription_cache(world: &mut World, key: String, marker: &str) {
    world.prepared_subscription_cache.insert(
        key,
        super::PreparedSubscriptionCacheEntry {
            subscriptions: Vec::new(),
            _subscription_fingerprint: marker.into(),
            prepared: Arc::<[super::PreparedSubscription]>::from([]),
        },
    );
}

fn event_identity(event: &WorldEvent) -> (&'static str, &str) {
    let WorldEventBody::ModuleEvent(event) = &event.body else {
        panic!("expected module event");
    };
    match &event.kind {
        ModuleEventKind::RegisterModule { module, .. } => ("register", &module.module_id),
        ModuleEventKind::UpgradeModule { module_id, .. } => ("upgrade", module_id),
        ModuleEventKind::ActivateModule { module_id, .. } => ("activate", module_id),
        ModuleEventKind::DeactivateModule { module_id, .. } => ("deactivate", module_id),
    }
}

#[test]
fn successful_batch_preserves_category_and_stable_module_order() {
    let mut world = World::new();
    let changes = full_changes(&mut world);
    let target_cache_key = format!(
        "{}|target",
        ModuleRegistry::record_key("m.batch.register.a", "1.0.0")
    );
    let unrelated_cache_key = "m.batch.unrelated@1.0.0|unrelated".to_string();
    seed_subscription_cache(&mut world, target_cache_key.clone(), "target");
    seed_subscription_cache(&mut world, unrelated_cache_key.clone(), "unrelated");
    assert!(
        !world
            .module_artifacts
            .contains("initially-absent-register-a-hash")
    );
    world
        .apply_module_changes_for_test(81, &changes, "batch-actor")
        .unwrap();
    let identities: Vec<_> = world.journal().events.iter().map(event_identity).collect();
    assert_eq!(
        identities,
        vec![
            ("register", "m.batch.register.a"),
            ("register", "m.batch.register.b"),
            ("upgrade", "m.batch.upgrade.a"),
            ("upgrade", "m.batch.upgrade.b"),
            ("activate", "m.batch.register.a"),
            ("activate", "m.batch.register.b"),
            ("deactivate", "m.batch.deactivate.a"),
            ("deactivate", "m.batch.deactivate.b"),
        ]
    );
    for event in &world.journal().events {
        assert_eq!(event.caused_by, None);
        let WorldEventBody::ModuleEvent(module_event) = &event.body else {
            unreachable!()
        };
        assert_eq!(module_event.proposal_id, 81);
        match &module_event.kind {
            ModuleEventKind::RegisterModule { registered_by, .. } => {
                assert_eq!(registered_by, "batch-actor")
            }
            ModuleEventKind::UpgradeModule {
                from_version,
                to_version,
                wasm_hash,
                manifest,
                upgraded_by,
                ..
            } => {
                assert_eq!(from_version, "1.0.0");
                assert_eq!(to_version, "2.0.0");
                assert_eq!(wasm_hash, &manifest.wasm_hash);
                assert_eq!(upgraded_by, "batch-actor");
            }
            ModuleEventKind::ActivateModule { activated_by, .. } => {
                assert_eq!(activated_by, "batch-actor")
            }
            ModuleEventKind::DeactivateModule {
                module_id,
                reason,
                deactivated_by,
            } => {
                assert_eq!(
                    reason,
                    &format!("retire-{}", module_id.rsplit('.').next().unwrap())
                );
                assert_eq!(deactivated_by, "batch-actor");
            }
        }
    }
    assert!(
        world
            .module_artifacts
            .contains("initially-absent-register-a-hash")
    );
    assert!(
        !world
            .prepared_subscription_cache
            .contains_key(&target_cache_key)
    );
    assert!(
        world
            .prepared_subscription_cache
            .contains_key(&unrelated_cache_key)
    );
    assert_eq!(world.module_registry.active["m.batch.register.a"], "1.0.0");
    assert!(
        !world
            .module_registry
            .active
            .contains_key("m.batch.deactivate.a")
    );
}

#[test]
fn failed_batch_preserves_targeted_and_unrelated_subscription_cache() {
    let mut world = World::new();
    let registered = manifest("m.batch.cache", "1.0.0", "absent-cache-artifact".into());
    let target = format!(
        "{}|target",
        ModuleRegistry::record_key("m.batch.cache", "1.0.0")
    );
    let unrelated = "m.batch.other@1.0.0|unrelated".to_string();
    seed_subscription_cache(&mut world, target, "target");
    seed_subscription_cache(&mut world, unrelated, "unrelated");
    let before = world.clone();
    world.fail_next_append_after_publication_prepare_for_test();
    world
        .apply_module_changes_for_test(
            86,
            &ModuleChangeSet {
                register: vec![registered],
                ..ModuleChangeSet::default()
            },
            "cache-actor",
        )
        .expect_err("cache invalidation must wait for batch publication");
    assert_world_unchanged(&world, &before);
}

#[test]
fn empty_batch_is_strict_noop_and_equal_ids_keep_input_order() {
    let mut world = World::new();
    let before = world.clone();
    world.fail_next_append_after_publication_prepare_for_test();
    world
        .apply_module_changes_for_test(82, &ModuleChangeSet::default(), "actor")
        .unwrap();
    assert_world_unchanged(&world, &before);

    let mut world = World::new();
    let version_two = manifest(
        "m.batch.same",
        "2.0.0",
        register_artifact(&mut world, "same-two"),
    );
    let version_one = manifest(
        "m.batch.same",
        "1.0.0",
        register_artifact(&mut world, "same-one"),
    );
    world
        .apply_module_changes_for_test(
            83,
            &ModuleChangeSet {
                register: vec![version_two, version_one],
                ..ModuleChangeSet::default()
            },
            "actor",
        )
        .unwrap();
    let versions: Vec<_> = world
        .journal()
        .events
        .iter()
        .map(|event| {
            let WorldEventBody::ModuleEvent(ModuleEvent {
                kind: ModuleEventKind::RegisterModule { module, .. },
                ..
            }) = &event.body
            else {
                unreachable!()
            };
            module.version.as_str()
        })
        .collect();
    assert_eq!(versions, ["2.0.0", "1.0.0"]);
}

#[test]
fn same_batch_register_activate_deactivate_and_retry_commit_once() {
    let mut world = World::new();
    let registered = manifest(
        "m.batch.compose",
        "1.0.0",
        register_artifact(&mut world, "compose"),
    );
    let changes = ModuleChangeSet {
        register: vec![registered],
        activate: vec![ModuleActivation {
            module_id: "m.batch.compose".into(),
            version: "1.0.0".into(),
        }],
        deactivate: vec![ModuleDeactivation {
            module_id: "m.batch.compose".into(),
            reason: "same batch".into(),
        }],
        ..ModuleChangeSet::default()
    };
    let before = world.clone();
    world.fail_append_after_publication_prepare_on_nth_for_test(3);
    assert!(
        world
            .apply_module_changes_for_test(84, &changes, "actor")
            .is_err()
    );
    assert_world_unchanged(&world, &before);
    world
        .apply_module_changes_for_test(84, &changes, "actor")
        .unwrap();
    assert_eq!(world.journal().events.len(), 3);
    assert!(!world.module_registry.active.contains_key("m.batch.compose"));
    assert!(!world.module_tick_schedule.contains_key("m.batch.compose"));
}

#[test]
fn batch_retention_era_root_and_replay_are_deterministic() {
    let mut world = World::new();
    let first = manifest(
        "m.batch.replay.a",
        "1.0.0",
        register_artifact(&mut world, "replay-a"),
    );
    let second = manifest(
        "m.batch.replay.b",
        "1.0.0",
        register_artifact(&mut world, "replay-b"),
    );
    world.runtime_memory_limits.max_journal_events = 1;
    world.next_event_id = u64::MAX;
    world.next_event_id_era = 6;
    world
        .apply_module_changes_for_test(
            85,
            &ModuleChangeSet {
                register: vec![second, first],
                ..ModuleChangeSet::default()
            },
            "actor",
        )
        .unwrap();
    assert_eq!(world.next_event_id_era, 7);
    assert_eq!(world.journal().events.len(), 1);
    assert_eq!(world.runtime_backpressure_stats.journal_events_evicted, 1);
    assert_eq!(
        world
            .tick_consensus_records()
            .last()
            .unwrap()
            .block
            .header
            .state_root,
        world.current_state_root_hash().unwrap()
    );

    let mut replay_world = World::new();
    let replay_first = manifest(
        "m.batch.replay.a",
        "1.0.0",
        register_artifact(&mut replay_world, "replay-a"),
    );
    let replay_second = manifest(
        "m.batch.replay.b",
        "1.0.0",
        register_artifact(&mut replay_world, "replay-b"),
    );
    let baseline = replay_world.snapshot();
    replay_world
        .apply_module_changes_for_test(
            85,
            &ModuleChangeSet {
                register: vec![replay_second, replay_first],
                ..ModuleChangeSet::default()
            },
            "actor",
        )
        .unwrap();
    let replay = World::from_snapshot(baseline, replay_world.journal().clone()).unwrap();
    assert_eq!(replay.module_registry, replay_world.module_registry);
    assert_eq!(replay.state, replay_world.state);
    assert_eq!(
        replay.current_state_root_hash(),
        replay_world.current_state_root_hash()
    );
}
