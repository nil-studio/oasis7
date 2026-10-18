use super::super::*;

fn lifecycle_manifest(module_id: &str, version: &str, wasm_hash: String) -> ModuleManifest {
    ModuleManifest {
        module_id: module_id.to_string(),
        name: format!("LifecycleTransaction-{module_id}-{version}"),
        version: version.to_string(),
        kind: ModuleKind::Pure,
        role: ModuleRole::AgentInternal,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".to_string()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        artifact_identity: Some(super::signed_test_artifact_identity(&wasm_hash)),
        limits: ModuleLimits::default(),
    }
}

fn register_artifact(world: &mut World, label: &str) -> String {
    let bytes = format!("module-lifecycle-matrix-{label}").into_bytes();
    let wasm_hash = util::sha256_hex(&bytes);
    world
        .register_module_artifact(wasm_hash.clone(), &bytes)
        .unwrap();
    wasm_hash
}

fn apply_fixture_changes(world: &mut World, changes: &ModuleChangeSet) {
    world
        .apply_module_changes_for_test(0, changes, "fixture")
        .unwrap();
}

fn assert_governed_failure_rolls_back<F>(
    mut world: World,
    changes: ModuleChangeSet,
    assert_staged_mutation: F,
) where
    F: FnOnce(&World),
{
    // Prove the operation reaches and mutates its intended surface before the
    // injected publication failure, so a skipped lifecycle event cannot pass.
    let mut staged = world.clone();
    apply_fixture_changes(&mut staged, &changes);
    assert_staged_mutation(&staged);

    let mut content = serde_json::Map::new();
    content.insert(
        "module_changes".to_string(),
        serde_json::to_value(&changes).unwrap(),
    );
    let proposal_id = world
        .propose_manifest_update(
            Manifest {
                version: world.manifest().version.saturating_add(1),
                content: serde_json::Value::Object(content),
            },
            "alice",
        )
        .unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();

    // The approved proposal retains its builtin-authority commitment. Switching
    // authority fails publication after apply_module_event mutated `staged`.
    world
        .bind_node_identity(
            "authority.lifecycle.matrix",
            "authority-lifecycle-matrix-key",
        )
        .unwrap();
    world
        .set_tick_consensus_authority_source("authority.lifecycle.matrix")
        .unwrap();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();

    let error = world
        .apply_proposal(proposal_id)
        .expect_err("authority drift must fail publication");
    assert!(matches!(
        error,
        WorldError::DistributedValidationFailed { .. }
    ));
    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
}

#[test]
fn failed_register_proposal_does_not_publish_partial_world() {
    let mut world = World::new();
    let manifest = lifecycle_manifest(
        "m.lifecycle.register",
        "1.0.0",
        register_artifact(&mut world, "register"),
    );
    let key = ModuleRegistry::record_key(&manifest.module_id, &manifest.version);
    assert_governed_failure_rolls_back(
        world,
        ModuleChangeSet {
            register: vec![manifest],
            ..ModuleChangeSet::default()
        },
        move |staged| assert!(staged.module_registry().records.contains_key(&key)),
    );
}

#[test]
fn failed_upgrade_proposal_does_not_publish_partial_world() {
    let mut world = World::new();
    let module_id = "m.lifecycle.upgrade";
    let old = lifecycle_manifest(
        module_id,
        "1.0.0",
        register_artifact(&mut world, "upgrade-old"),
    );
    apply_fixture_changes(
        &mut world,
        &ModuleChangeSet {
            register: vec![old.clone()],
            ..ModuleChangeSet::default()
        },
    );
    let new = lifecycle_manifest(
        module_id,
        "2.0.0",
        register_artifact(&mut world, "upgrade-new"),
    );
    let key = ModuleRegistry::record_key(module_id, "2.0.0");
    let upgrade = ModuleUpgrade {
        module_id: module_id.to_string(),
        from_version: old.version,
        to_version: new.version.clone(),
        wasm_hash: new.wasm_hash.clone(),
        manifest: new,
    };
    assert_governed_failure_rolls_back(
        world,
        ModuleChangeSet {
            upgrade: vec![upgrade],
            ..ModuleChangeSet::default()
        },
        move |staged| assert!(staged.module_registry().records.contains_key(&key)),
    );
}

#[test]
fn failed_activate_proposal_does_not_publish_partial_world() {
    let mut world = World::new();
    let module_id = "m.lifecycle.activate";
    let manifest = lifecycle_manifest(
        module_id,
        "1.0.0",
        register_artifact(&mut world, "activate"),
    );
    apply_fixture_changes(
        &mut world,
        &ModuleChangeSet {
            register: vec![manifest.clone()],
            ..ModuleChangeSet::default()
        },
    );
    let changes = ModuleChangeSet {
        activate: vec![ModuleActivation {
            module_id: module_id.to_string(),
            version: manifest.version,
        }],
        ..ModuleChangeSet::default()
    };
    assert_governed_failure_rolls_back(world, changes, move |staged| {
        assert_eq!(
            staged
                .module_registry()
                .active
                .get(module_id)
                .map(String::as_str),
            Some("1.0.0")
        );
    });
}

#[test]
fn failed_deactivate_proposal_does_not_publish_partial_world() {
    let mut world = World::new();
    let module_id = "m.lifecycle.deactivate";
    let manifest = lifecycle_manifest(
        module_id,
        "1.0.0",
        register_artifact(&mut world, "deactivate"),
    );
    apply_fixture_changes(
        &mut world,
        &ModuleChangeSet {
            register: vec![manifest.clone()],
            activate: vec![ModuleActivation {
                module_id: module_id.to_string(),
                version: manifest.version,
            }],
            ..ModuleChangeSet::default()
        },
    );
    let changes = ModuleChangeSet {
        deactivate: vec![ModuleDeactivation {
            module_id: module_id.to_string(),
            reason: "matrix".to_string(),
        }],
        ..ModuleChangeSet::default()
    };
    assert_governed_failure_rolls_back(world, changes, move |staged| {
        assert!(!staged.module_registry().active.contains_key(module_id));
    });
}

#[test]
fn governance_apply_post_prepare_failure_publishes_no_typed_projection() {
    let mut world = World::new();
    let module = lifecycle_manifest(
        "m.lifecycle.post-prepare",
        "1.0.0",
        register_artifact(&mut world, "post-prepare"),
    );
    let module_key = ModuleRegistry::record_key(&module.module_id, &module.version);
    let mut content = serde_json::Map::new();
    content.insert(
        "module_changes".to_string(),
        serde_json::to_value(ModuleChangeSet {
            register: vec![module],
            ..ModuleChangeSet::default()
        })
        .unwrap(),
    );
    let proposal_id = world
        .propose_manifest_update(
            Manifest {
                version: 2,
                content: serde_json::Value::Object(content),
            },
            "alice",
        )
        .unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();
    let certificate = world.build_local_finality_certificate(proposal_id).unwrap();

    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let cache_len_before = world.module_cache_len();
    world.fail_next_append_after_publication_prepare_for_test();

    let error = world
        .apply_proposal_with_finality(proposal_id, &certificate)
        .expect_err("post-prepare failure must publish no proposal effects");
    assert!(matches!(error, WorldError::ResourceBalanceInvalid { .. }));
    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());
    assert_eq!(world.module_cache_len(), cache_len_before);
    assert!(!world.module_registry().records.contains_key(&module_key));
    assert!(matches!(
        world
            .proposals()
            .get(&proposal_id)
            .map(|proposal| &proposal.status),
        Some(ProposalStatus::Approved { .. })
    ));
}

#[test]
fn governed_mixed_lifecycle_batch_publishes_one_complete_projection() {
    let mut world = World::new();
    let upgrade_id = "m.lifecycle.mixed-upgrade";
    let deactivate_id = "m.lifecycle.mixed-deactivate";
    let upgrade_old = lifecycle_manifest(
        upgrade_id,
        "1.0.0",
        register_artifact(&mut world, "mixed-upgrade-old"),
    );
    let deactivate = lifecycle_manifest(
        deactivate_id,
        "1.0.0",
        register_artifact(&mut world, "mixed-deactivate"),
    );
    apply_fixture_changes(
        &mut world,
        &ModuleChangeSet {
            register: vec![upgrade_old.clone(), deactivate.clone()],
            activate: vec![
                ModuleActivation {
                    module_id: upgrade_id.to_string(),
                    version: upgrade_old.version.clone(),
                },
                ModuleActivation {
                    module_id: deactivate_id.to_string(),
                    version: deactivate.version.clone(),
                },
            ],
            ..ModuleChangeSet::default()
        },
    );

    let register_id = "m.lifecycle.mixed-register";
    let mut register = lifecycle_manifest(
        register_id,
        "1.0.0",
        register_artifact(&mut world, "mixed-register"),
    );
    register.subscriptions.push(ModuleSubscription {
        event_kinds: Vec::new(),
        action_kinds: Vec::new(),
        stage: Some(ModuleSubscriptionStage::Tick),
        filters: None,
    });
    let upgrade_new = lifecycle_manifest(
        upgrade_id,
        "2.0.0",
        register_artifact(&mut world, "mixed-upgrade-new"),
    );
    let changes = ModuleChangeSet {
        register: vec![register.clone()],
        upgrade: vec![ModuleUpgrade {
            module_id: upgrade_id.to_string(),
            from_version: upgrade_old.version,
            to_version: upgrade_new.version.clone(),
            wasm_hash: upgrade_new.wasm_hash.clone(),
            manifest: upgrade_new.clone(),
        }],
        activate: vec![ModuleActivation {
            module_id: register_id.to_string(),
            version: register.version.clone(),
        }],
        deactivate: vec![ModuleDeactivation {
            module_id: deactivate_id.to_string(),
            reason: "mixed-batch".to_string(),
        }],
    };
    let mut content = serde_json::Map::new();
    content.insert(
        "module_changes".to_string(),
        serde_json::to_value(&changes).unwrap(),
    );
    let proposal_id = world
        .propose_manifest_update(
            Manifest {
                version: world.manifest().version.saturating_add(1),
                content: serde_json::Value::Object(content),
            },
            "alice",
        )
        .unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();
    let certificate = world.build_local_finality_certificate(proposal_id).unwrap();
    let journal_len_before = world.journal().events.len();

    let applied_hash = world
        .apply_proposal_with_finality(proposal_id, &certificate)
        .unwrap();

    let published = &world.journal().events[journal_len_before..];
    assert_eq!(published.len(), 6);
    assert!(matches!(
        &published[0].body,
        WorldEventBody::ModuleEvent(ModuleEvent {
            kind: ModuleEventKind::RegisterModule { module, .. },
            ..
        }) if module.module_id == register_id
    ));
    assert!(matches!(
        &published[1].body,
        WorldEventBody::ModuleEvent(ModuleEvent {
            kind: ModuleEventKind::UpgradeModule { module_id, .. },
            ..
        }) if module_id == upgrade_id
    ));
    assert!(matches!(
        &published[2].body,
        WorldEventBody::ModuleEvent(ModuleEvent {
            kind: ModuleEventKind::ActivateModule { module_id, .. },
            ..
        }) if module_id == register_id
    ));
    assert!(matches!(
        &published[3].body,
        WorldEventBody::ModuleEvent(ModuleEvent {
            kind: ModuleEventKind::DeactivateModule { module_id, .. },
            ..
        }) if module_id == deactivate_id
    ));
    assert!(matches!(
        &published[4].body,
        WorldEventBody::ManifestUpdated(update) if update.manifest_hash == applied_hash
    ));
    assert!(matches!(
        &published[5].body,
        WorldEventBody::Governance(GovernanceEvent::Applied {
            proposal_id: event_proposal_id,
            manifest_hash: Some(manifest_hash),
            ..
        }) if *event_proposal_id == proposal_id && manifest_hash == &applied_hash
    ));
    assert!(
        published
            .windows(2)
            .all(|pair| pair[1].id == pair[0].id.saturating_add(1))
    );

    let register_key = ModuleRegistry::record_key(register_id, "1.0.0");
    let upgrade_key = ModuleRegistry::record_key(upgrade_id, "2.0.0");
    assert_eq!(
        world
            .module_registry()
            .records
            .get(&register_key)
            .map(|record| &record.manifest),
        Some(&register)
    );
    assert_eq!(
        world
            .module_registry()
            .records
            .get(&upgrade_key)
            .map(|record| &record.manifest),
        Some(&upgrade_new)
    );
    assert_eq!(
        world
            .module_registry()
            .active
            .get(register_id)
            .map(String::as_str),
        Some("1.0.0")
    );
    assert!(!world.module_registry().active.contains_key(deactivate_id));
    assert_eq!(
        world.snapshot().module_tick_schedule.get(register_id),
        Some(&world.state().time)
    );
    assert_eq!(
        world
            .tick_consensus_records()
            .last()
            .map(|record| record.block.header.tick),
        Some(world.state().time)
    );
}
