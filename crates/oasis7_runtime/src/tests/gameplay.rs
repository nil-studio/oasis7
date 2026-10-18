use super::super::*;

fn register_artifact(world: &mut World, module_id: &str) -> String {
    let bytes = format!("gameplay-{module_id}").into_bytes();
    let wasm_hash = util::sha256_hex(&bytes);
    world
        .register_module_artifact(wasm_hash.clone(), &bytes)
        .expect("register artifact");
    wasm_hash
}

fn gameplay_manifest(
    module_id: &str,
    version: &str,
    wasm_hash: String,
    kind: GameplayModuleKind,
    game_modes: &[&str],
) -> ModuleManifest {
    ModuleManifest {
        module_id: module_id.to_string(),
        name: module_id.to_string(),
        version: version.to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Gameplay,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract {
            abi_version: Some(1),
            input_schema: Some("schema.input@1".to_string()),
            output_schema: Some("schema.output@1".to_string()),
            cap_slots: std::collections::BTreeMap::new(),
            policy_hooks: Vec::new(),
            gameplay: Some(GameplayContract {
                kind,
                game_modes: game_modes.iter().map(|mode| (*mode).to_string()).collect(),
                min_players: 1,
                max_players: None,
            }),
            declarations: Default::default(),
        },
        exports: vec!["reduce".to_string()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits::default(),
    }
}

fn apply_module_changes(world: &mut World, changes: ModuleChangeSet) -> Result<(), WorldError> {
    let mut content = serde_json::Map::new();
    content.insert(
        "module_changes".to_string(),
        serde_json::to_value(&changes).expect("serialize module changes"),
    );
    let manifest = Manifest {
        version: world.manifest().version.saturating_add(1),
        content: serde_json::Value::Object(content),
    };
    let proposal_id = world.propose_manifest_update(manifest, "alice")?;
    world.shadow_proposal(proposal_id)?;
    world.approve_proposal(proposal_id, "bob", ProposalDecision::Approve)?;
    world.apply_proposal(proposal_id)?;
    Ok(())
}

#[test]
fn gameplay_module_requires_gameplay_contract() {
    let mut world = World::new();
    let wasm_hash = register_artifact(&mut world, "g.war.alpha");
    let manifest = ModuleManifest {
        module_id: "g.war.alpha".to_string(),
        name: "WarAlpha".to_string(),
        version: "1.0.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Gameplay,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits::default(),
    };
    let err = apply_module_changes(
        &mut world,
        ModuleChangeSet {
            register: vec![manifest.clone()],
            activate: vec![ModuleActivation {
                module_id: manifest.module_id.clone(),
                version: manifest.version.clone(),
            }],
            ..ModuleChangeSet::default()
        },
    )
    .unwrap_err();
    let WorldError::ModuleChangeInvalid { reason } = err else {
        panic!("expected module change invalid");
    };
    assert!(reason.contains("gameplay contract missing"));
}

#[test]
fn non_gameplay_role_cannot_attach_gameplay_contract() {
    let mut world = World::new();
    let wasm_hash = register_artifact(&mut world, "m.domain.invalid_gameplay");
    let mut manifest = gameplay_manifest(
        "m.domain.invalid_gameplay",
        "1.0.0",
        wasm_hash,
        GameplayModuleKind::War,
        &["sandbox"],
    );
    manifest.role = ModuleRole::Domain;
    let err = apply_module_changes(
        &mut world,
        ModuleChangeSet {
            register: vec![manifest.clone()],
            activate: vec![ModuleActivation {
                module_id: manifest.module_id.clone(),
                version: manifest.version.clone(),
            }],
            ..ModuleChangeSet::default()
        },
    )
    .unwrap_err();
    let WorldError::ModuleChangeInvalid { reason } = err else {
        panic!("expected module change invalid");
    };
    assert!(reason.contains("requires gameplay role"));
}

#[test]
fn gameplay_slot_conflict_is_rejected_for_same_mode_and_kind() {
    let mut world = World::new();
    let manifest_a = gameplay_manifest(
        "g.war.alpha",
        "1.0.0",
        register_artifact(&mut world, "g.war.alpha"),
        GameplayModuleKind::War,
        &["sandbox"],
    );
    let manifest_b = gameplay_manifest(
        "g.war.beta",
        "1.0.0",
        register_artifact(&mut world, "g.war.beta"),
        GameplayModuleKind::War,
        &["sandbox"],
    );
    let err = apply_module_changes(
        &mut world,
        ModuleChangeSet {
            register: vec![manifest_a.clone(), manifest_b.clone()],
            activate: vec![
                ModuleActivation {
                    module_id: manifest_a.module_id.clone(),
                    version: manifest_a.version.clone(),
                },
                ModuleActivation {
                    module_id: manifest_b.module_id.clone(),
                    version: manifest_b.version.clone(),
                },
            ],
            ..ModuleChangeSet::default()
        },
    )
    .unwrap_err();
    let WorldError::ModuleChangeInvalid { reason } = err else {
        panic!("expected module change invalid");
    };
    assert!(reason.contains("gameplay slot conflict"));
}

#[test]
fn gameplay_mode_readiness_reports_coverage_and_missing_kinds() {
    let mut world = World::new();
    let manifests = vec![
        gameplay_manifest(
            "g.war.alpha",
            "1.0.0",
            register_artifact(&mut world, "g.war.alpha"),
            GameplayModuleKind::War,
            &["sandbox"],
        ),
        gameplay_manifest(
            "g.governance.alpha",
            "1.0.0",
            register_artifact(&mut world, "g.governance.alpha"),
            GameplayModuleKind::Governance,
            &["sandbox"],
        ),
        gameplay_manifest(
            "g.crisis.alpha",
            "1.0.0",
            register_artifact(&mut world, "g.crisis.alpha"),
            GameplayModuleKind::Crisis,
            &["sandbox"],
        ),
        gameplay_manifest(
            "g.economic.alpha",
            "1.0.0",
            register_artifact(&mut world, "g.economic.alpha"),
            GameplayModuleKind::Economic,
            &["sandbox"],
        ),
        gameplay_manifest(
            "g.meta.alpha",
            "1.0.0",
            register_artifact(&mut world, "g.meta.alpha"),
            GameplayModuleKind::Meta,
            &["sandbox"],
        ),
    ];

    apply_module_changes(
        &mut world,
        ModuleChangeSet {
            register: manifests.clone(),
            activate: manifests
                .iter()
                .map(|manifest| ModuleActivation {
                    module_id: manifest.module_id.clone(),
                    version: manifest.version.clone(),
                })
                .collect(),
            ..ModuleChangeSet::default()
        },
    )
    .expect("apply gameplay manifests");

    let sandbox_report = world.gameplay_mode_readiness("sandbox");
    assert!(sandbox_report.is_ready());
    assert!(sandbox_report.missing_kinds.is_empty());
    assert_eq!(sandbox_report.active_modules.len(), 5);
    assert_eq!(sandbox_report.coverage.len(), 5);
    assert!(
        sandbox_report
            .coverage
            .iter()
            .all(|entry| entry.active_count == 1)
    );

    let ranked_report = world.gameplay_mode_readiness("ranked");
    assert!(!ranked_report.is_ready());
    assert_eq!(ranked_report.active_modules.len(), 0);
    assert_eq!(ranked_report.missing_kinds.len(), 5);
}
