const VALID_COMMAND_SCHEMA_HASH: &str =
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn command_manifest(world: &mut World, namespace: &str) -> ModuleManifest {
    let wasm_bytes = format!("dummy-wasm-command-{namespace}").into_bytes();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), &wasm_bytes)
        .unwrap();

    ModuleManifest {
        module_id: "m.commands".to_string(),
        name: "Commands".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Pure,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract {
            declarations: oasis7_wasm_abi::ModuleSchemaDeclarations {
                commands: vec![oasis7_wasm_abi::ModuleCommandDeclaration {
                    namespace: namespace.to_string(),
                    name: "open_account".to_string(),
                    schema_version: 1,
                    schema_hash: VALID_COMMAND_SCHEMA_HASH.to_string(),
                    max_payload_bytes: 4096,
                }],
            },
            ..ModuleAbiContract::default()
        },
        exports: vec!["call".to_string()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits::default(),
    }
}

fn command_proposal(world: &mut World, module_manifest: ModuleManifest) -> u64 {
    let changes = ModuleChangeSet {
        register: vec![module_manifest],
        ..ModuleChangeSet::default()
    };
    let mut content = serde_json::Map::new();
    content.insert(
        "module_changes".to_string(),
        serde_json::to_value(&changes).unwrap(),
    );
    world
        .propose_manifest_update(
            Manifest {
                version: 2,
                content: serde_json::Value::Object(content),
            },
            "alice",
        )
        .unwrap()
}

#[test]
fn shadow_accepts_valid_module_command_declaration() {
    let mut world = World::new();
    let module_manifest = command_manifest(&mut world, "bank.acme");
    let proposal_id = command_proposal(&mut world, module_manifest);

    world.shadow_proposal(proposal_id).unwrap();
}

#[test]
fn shadow_rejects_reserved_module_command_namespace_before_mutation() {
    let mut world = World::new();
    let module_manifest = command_manifest(&mut world, "core");
    let proposal_id = command_proposal(&mut world, module_manifest);

    let err = world.shadow_proposal(proposal_id).unwrap_err();
    let WorldError::ModuleChangeInvalid { reason } = err else {
        panic!("expected ModuleChangeInvalid");
    };
    assert!(reason.contains("reserved"), "unexpected reason: {reason}");
    assert!(world.module_registry().records.is_empty());
    assert!(world.module_registry().active.is_empty());
}

#[test]
fn shadow_rejects_missing_module_artifact() {
    let mut world = World::new();
    world.add_capability(CapabilityGrant::allow_all("cap.weather"));

    let module_manifest = ModuleManifest {
        module_id: "m.weather".to_string(),
        name: "Weather".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: "missing-hash".to_string(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: Vec::new(),
        required_caps: vec!["cap.weather".to_string()],
        artifact_identity: Some(super::signed_test_artifact_identity("missing-hash")),
        limits: ModuleLimits::default(),
    };

    let changes = ModuleChangeSet {
        register: vec![module_manifest],
        ..ModuleChangeSet::default()
    };

    let mut content = serde_json::Map::new();
    content.insert(
        "module_changes".to_string(),
        serde_json::to_value(&changes).unwrap(),
    );
    let manifest = Manifest {
        version: 2,
        content: serde_json::Value::Object(content),
    };

    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    let err = world.shadow_proposal(proposal_id).unwrap_err();
    assert!(matches!(err, WorldError::ModuleChangeInvalid { .. }));
}

#[test]
fn shadow_rejects_incomplete_module_artifact_identity() {
    let mut world = World::new();
    world.add_capability(CapabilityGrant::allow_all("cap.weather"));
    let wasm_bytes = b"dummy-wasm-weather-identity";
    let wasm_hash = util::sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.weather".to_string(),
        name: "Weather".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: Vec::new(),
        required_caps: vec!["cap.weather".to_string()],
        artifact_identity: Some(ModuleArtifactIdentity {
            source_hash: String::new(),
            build_manifest_hash: "build-hash".to_string(),
            signer_node_id: "node-1".to_string(),
            signature_scheme: "ed25519".to_string(),
            artifact_signature: "unsigned:dummy:src:build".to_string(),
        }),
        limits: ModuleLimits::default(),
    };

    let changes = ModuleChangeSet {
        register: vec![module_manifest],
        ..ModuleChangeSet::default()
    };

    let mut content = serde_json::Map::new();
    content.insert(
        "module_changes".to_string(),
        serde_json::to_value(&changes).unwrap(),
    );
    let manifest = Manifest {
        version: 2,
        content: serde_json::Value::Object(content),
    };

    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    let err = world.shadow_proposal(proposal_id).unwrap_err();
    let WorldError::ModuleChangeInvalid { reason } = err else {
        panic!("expected ModuleChangeInvalid");
    };
    assert!(reason.contains("artifact_identity is incomplete"));
}

#[test]
fn shadow_rejects_module_artifact_identity_signature_mismatch() {
    let mut world = World::new();
    world.add_capability(CapabilityGrant::allow_all("cap.weather"));
    let wasm_bytes = b"dummy-wasm-weather-identity-mismatch";
    let wasm_hash = util::sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.weather".to_string(),
        name: "Weather".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: Vec::new(),
        required_caps: vec!["cap.weather".to_string()],
        artifact_identity: Some(ModuleArtifactIdentity {
            source_hash: "src-hash".to_string(),
            build_manifest_hash: "build-hash".to_string(),
            signer_node_id: "node-1".to_string(),
            signature_scheme: "ed25519".to_string(),
            artifact_signature: "unsigned:different-wasm-hash:src-hash:build-hash".to_string(),
        }),
        limits: ModuleLimits::default(),
    };

    let changes = ModuleChangeSet {
        register: vec![module_manifest],
        ..ModuleChangeSet::default()
    };

    let mut content = serde_json::Map::new();
    content.insert(
        "module_changes".to_string(),
        serde_json::to_value(&changes).unwrap(),
    );
    let manifest = Manifest {
        version: 2,
        content: serde_json::Value::Object(content),
    };

    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    let err = world.shadow_proposal(proposal_id).unwrap_err();
    let WorldError::ModuleChangeInvalid { reason } = err else {
        panic!("expected ModuleChangeInvalid");
    };
    assert!(reason.contains("unsigned signature is forbidden"));
}

#[test]
fn shadow_rejects_identity_hash_signature_when_release_policy_disables_fallback() {
    let mut world = World::new();
    world.enable_production_release_policy();
    world.add_capability(CapabilityGrant::allow_all("cap.weather"));
    let wasm_bytes = b"dummy-wasm-weather-identity-hash";
    let wasm_hash = util::sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let source_hash = util::sha256_hex(b"weather-src");
    let build_manifest_hash = util::sha256_hex(b"weather-build");
    let identity_hash =
        util::sha256_hex(format!("m.weather:{source_hash}:{build_manifest_hash}").as_bytes());
    let module_manifest = ModuleManifest {
        module_id: "m.weather".to_string(),
        name: "Weather".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: Vec::new(),
        required_caps: vec!["cap.weather".to_string()],
        artifact_identity: Some(ModuleArtifactIdentity {
            source_hash,
            build_manifest_hash,
            signer_node_id: "builtin.module.release.signer".to_string(),
            signature_scheme: "identity_hash_v1".to_string(),
            artifact_signature: format!("idhash:{identity_hash}"),
        }),
        limits: ModuleLimits::default(),
    };
    let changes = ModuleChangeSet {
        register: vec![module_manifest],
        ..ModuleChangeSet::default()
    };
    let mut content = serde_json::Map::new();
    content.insert(
        "module_changes".to_string(),
        serde_json::to_value(&changes).unwrap(),
    );
    let manifest = Manifest {
        version: 2,
        content: serde_json::Value::Object(content),
    };

    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    let err = world.shadow_proposal(proposal_id).unwrap_err();
    let WorldError::ModuleChangeInvalid { reason } = err else {
        panic!("expected ModuleChangeInvalid");
    };
    assert!(reason.contains("signature_scheme identity_hash_v1 is disabled"));
}

#[test]
fn shadow_rejects_unsupported_module_abi_version() {
    let mut world = World::new();
    world.add_capability(CapabilityGrant::allow_all("cap.weather"));
    let wasm_bytes = b"dummy-wasm-weather-abi-version";
    let wasm_hash = util::sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.weather".to_string(),
        name: "Weather".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract {
            abi_version: Some(2),
            input_schema: Some("schema.input@1".to_string()),
            output_schema: Some("schema.output@1".to_string()),
            cap_slots: Default::default(),
            policy_hooks: Vec::new(),
            gameplay: None,
            declarations: Default::default(),
        },
        exports: vec!["reduce".to_string()],
        subscriptions: Vec::new(),
        required_caps: vec!["cap.weather".to_string()],
        artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits::default(),
    };

    let changes = ModuleChangeSet {
        register: vec![module_manifest],
        ..ModuleChangeSet::default()
    };

    let mut content = serde_json::Map::new();
    content.insert(
        "module_changes".to_string(),
        serde_json::to_value(&changes).unwrap(),
    );
    let manifest = Manifest {
        version: 2,
        content: serde_json::Value::Object(content),
    };

    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    let err = world.shadow_proposal(proposal_id).unwrap_err();
    let WorldError::ModuleChangeInvalid { reason } = err else {
        panic!("expected ModuleChangeInvalid");
    };
    assert!(reason.contains("abi_version unsupported"));
}

#[test]
fn shadow_rejects_partial_module_schema_contract() {
    let mut world = World::new();
    world.add_capability(CapabilityGrant::allow_all("cap.weather"));
    let wasm_bytes = b"dummy-wasm-weather-schema-contract";
    let wasm_hash = util::sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.weather".to_string(),
        name: "Weather".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract {
            abi_version: Some(1),
            input_schema: Some("schema.input@1".to_string()),
            output_schema: None,
            cap_slots: Default::default(),
            policy_hooks: Vec::new(),
            gameplay: None,
            declarations: Default::default(),
        },
        exports: vec!["reduce".to_string()],
        subscriptions: Vec::new(),
        required_caps: vec!["cap.weather".to_string()],
        artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits::default(),
    };

    let changes = ModuleChangeSet {
        register: vec![module_manifest],
        ..ModuleChangeSet::default()
    };

    let mut content = serde_json::Map::new();
    content.insert(
        "module_changes".to_string(),
        serde_json::to_value(&changes).unwrap(),
    );
    let manifest = Manifest {
        version: 2,
        content: serde_json::Value::Object(content),
    };

    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    let err = world.shadow_proposal(proposal_id).unwrap_err();
    let WorldError::ModuleChangeInvalid { reason } = err else {
        panic!("expected ModuleChangeInvalid");
    };
    assert!(reason.contains("input_schema/output_schema pair"));
}

#[test]
fn shadow_rejects_cap_slot_binding_to_unknown_required_cap() {
    let mut world = World::new();
    world.add_capability(CapabilityGrant::allow_all("cap.weather"));
    let wasm_bytes = b"dummy-wasm-weather-cap-slot";
    let wasm_hash = util::sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.weather".to_string(),
        name: "Weather".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract {
            abi_version: Some(1),
            input_schema: Some("schema.input@1".to_string()),
            output_schema: Some("schema.output@1".to_string()),
            cap_slots: std::collections::BTreeMap::from([(
                "weather_api".to_string(),
                "cap.not-required".to_string(),
            )]),
            policy_hooks: Vec::new(),
            gameplay: None,
            declarations: Default::default(),
        },
        exports: vec!["reduce".to_string()],
        subscriptions: Vec::new(),
        required_caps: vec!["cap.weather".to_string()],
        artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits::default(),
    };

    let changes = ModuleChangeSet {
        register: vec![module_manifest],
        ..ModuleChangeSet::default()
    };

    let mut content = serde_json::Map::new();
    content.insert(
        "module_changes".to_string(),
        serde_json::to_value(&changes).unwrap(),
    );
    let manifest = Manifest {
        version: 2,
        content: serde_json::Value::Object(content),
    };

    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    let err = world.shadow_proposal(proposal_id).unwrap_err();
    let WorldError::ModuleChangeInvalid { reason } = err else {
        panic!("expected ModuleChangeInvalid");
    };
    assert!(reason.contains("binds unknown cap_ref"));
}
