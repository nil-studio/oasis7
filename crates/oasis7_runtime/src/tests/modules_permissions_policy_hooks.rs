#[test]
fn module_call_pure_policy_hook_allows_effect_queueing() {
    let mut world = World::new();
    world.add_capability(CapabilityGrant::allow_all("cap.weather"));
    world.set_policy(PolicySet::allow_all());

    let source_bytes = b"module-source-weather";
    let source_hash = util::sha256_hex(source_bytes);
    world
        .register_module_artifact(source_hash.clone(), source_bytes)
        .unwrap();
    let policy_bytes = b"module-policy-allow";
    let policy_hash = util::sha256_hex(policy_bytes);
    world
        .register_module_artifact(policy_hash.clone(), policy_bytes)
        .unwrap();

    activate_module_manifest(
        &mut world,
        ModuleManifest {
            module_id: "m.weather".to_string(),
            name: "Weather".to_string(),
            version: "0.1.0".to_string(),
            kind: ModuleKind::Reducer,
            role: ModuleRole::Domain,
            wasm_hash: source_hash.clone(),
            interface_version: "wasm-1".to_string(),
            abi_contract: ModuleAbiContract {
                abi_version: Some(1),
                input_schema: Some("schema.input@1".to_string()),
                output_schema: Some("schema.output@1".to_string()),
                cap_slots: std::collections::BTreeMap::new(),
                policy_hooks: vec!["m.policy.allow".to_string()],
                gameplay: None,
                declarations: Default::default(),
            },
            exports: vec!["reduce".to_string()],
            subscriptions: Vec::new(),
            required_caps: vec!["cap.weather".to_string()],
            artifact_identity: Some(super::signed_test_artifact_identity(source_hash.as_str())),
            limits: ModuleLimits {
                max_mem_bytes: 1024,
                max_gas: 10_000,
                max_call_rate: 1,
                max_output_bytes: 1024,
                max_effects: 2,
                max_emits: 0,
            },
        },
    );

    activate_module_manifest(
        &mut world,
        ModuleManifest {
            module_id: "m.policy.allow".to_string(),
            name: "PolicyAllow".to_string(),
            version: "0.1.0".to_string(),
            kind: ModuleKind::Pure,
            role: ModuleRole::Domain,
            wasm_hash: policy_hash.clone(),
            interface_version: "wasm-1".to_string(),
            abi_contract: ModuleAbiContract::default(),
            exports: vec!["call".to_string()],
            subscriptions: Vec::new(),
            required_caps: Vec::new(),
            artifact_identity: Some(super::signed_test_artifact_identity(policy_hash.as_str())),
            limits: ModuleLimits {
                max_mem_bytes: 1024,
                max_gas: 10_000,
                max_call_rate: 1,
                max_output_bytes: 1024,
                max_effects: 0,
                max_emits: 1,
            },
        },
    );

    let mut sandbox = PurePolicyHookSandbox;
    world
        .execute_module_call("m.weather", "trace-policy-allow", vec![], &mut sandbox)
        .unwrap();
    assert_eq!(world.pending_effects_len(), 1);
}

#[test]
fn module_call_pure_policy_hook_can_deny_effect() {
    let mut world = World::new();
    world.add_capability(CapabilityGrant::allow_all("cap.weather"));
    world.set_policy(PolicySet::allow_all());

    let source_bytes = b"module-source-weather-deny";
    let source_hash = util::sha256_hex(source_bytes);
    world
        .register_module_artifact(source_hash.clone(), source_bytes)
        .unwrap();
    let policy_bytes = b"module-policy-deny";
    let policy_hash = util::sha256_hex(policy_bytes);
    world
        .register_module_artifact(policy_hash.clone(), policy_bytes)
        .unwrap();

    activate_module_manifest(
        &mut world,
        ModuleManifest {
            module_id: "m.weather".to_string(),
            name: "Weather".to_string(),
            version: "0.1.0".to_string(),
            kind: ModuleKind::Reducer,
            role: ModuleRole::Domain,
            wasm_hash: source_hash.clone(),
            interface_version: "wasm-1".to_string(),
            abi_contract: ModuleAbiContract {
                abi_version: Some(1),
                input_schema: Some("schema.input@1".to_string()),
                output_schema: Some("schema.output@1".to_string()),
                cap_slots: std::collections::BTreeMap::new(),
                policy_hooks: vec!["m.policy.deny".to_string()],
                gameplay: None,
                declarations: Default::default(),
            },
            exports: vec!["reduce".to_string()],
            subscriptions: Vec::new(),
            required_caps: vec!["cap.weather".to_string()],
            artifact_identity: Some(super::signed_test_artifact_identity(source_hash.as_str())),
            limits: ModuleLimits {
                max_mem_bytes: 1024,
                max_gas: 10_000,
                max_call_rate: 1,
                max_output_bytes: 1024,
                max_effects: 2,
                max_emits: 0,
            },
        },
    );

    activate_module_manifest(
        &mut world,
        ModuleManifest {
            module_id: "m.policy.deny".to_string(),
            name: "PolicyDeny".to_string(),
            version: "0.1.0".to_string(),
            kind: ModuleKind::Pure,
            role: ModuleRole::Domain,
            wasm_hash: policy_hash.clone(),
            interface_version: "wasm-1".to_string(),
            abi_contract: ModuleAbiContract::default(),
            exports: vec!["call".to_string()],
            subscriptions: Vec::new(),
            required_caps: Vec::new(),
            artifact_identity: Some(super::signed_test_artifact_identity(policy_hash.as_str())),
            limits: ModuleLimits {
                max_mem_bytes: 1024,
                max_gas: 10_000,
                max_call_rate: 1,
                max_output_bytes: 1024,
                max_effects: 0,
                max_emits: 1,
            },
        },
    );

    let mut sandbox = PurePolicyHookSandbox;
    let err = world
        .execute_module_call("m.weather", "trace-policy-deny", vec![], &mut sandbox)
        .unwrap_err();
    assert!(matches!(err, WorldError::ModuleCallFailed { .. }));
    assert_eq!(world.pending_effects_len(), 0);

    let failed = world
        .journal()
        .events
        .iter()
        .filter_map(|event| match &event.body {
            WorldEventBody::ModuleCallFailed(failure) => Some(failure),
            _ => None,
        })
        .next_back()
        .expect("failure event");
    assert_eq!(failed.code, ModuleCallErrorCode::PolicyDenied);
    assert!(failed.detail.contains("blocked_by_pure_policy"));
}

#[test]
fn module_call_pure_policy_hook_uses_policy_manifest_hash_in_context() {
    let mut world = World::new();
    world.add_capability(CapabilityGrant::allow_all("cap.weather"));
    world.set_policy(PolicySet::allow_all());

    let source_bytes = b"module-source-weather-policy-hash";
    let source_hash = util::sha256_hex(source_bytes);
    world
        .register_module_artifact(source_hash.clone(), source_bytes)
        .unwrap();
    let policy_bytes = b"module-policy-allow-policy-hash";
    let policy_hash = util::sha256_hex(policy_bytes);
    world
        .register_module_artifact(policy_hash.clone(), policy_bytes)
        .unwrap();

    let source_manifest = ModuleManifest {
        module_id: "m.weather".to_string(),
        name: "Weather".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: source_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract {
            abi_version: Some(1),
            input_schema: Some("schema.input@1".to_string()),
            output_schema: Some("schema.output@1".to_string()),
            cap_slots: std::collections::BTreeMap::new(),
            policy_hooks: vec!["m.policy.allow".to_string()],
            gameplay: None,
            declarations: Default::default(),
        },
        exports: vec!["reduce".to_string()],
        subscriptions: Vec::new(),
        required_caps: vec!["cap.weather".to_string()],
        artifact_identity: Some(super::signed_test_artifact_identity(source_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 1024,
            max_gas: 10_000,
            max_call_rate: 1,
            max_output_bytes: 1024,
            max_effects: 2,
            max_emits: 0,
        },
    };
    let policy_manifest = ModuleManifest {
        module_id: "m.policy.allow".to_string(),
        name: "PolicyAllow".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Pure,
        role: ModuleRole::Domain,
        wasm_hash: policy_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".to_string()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        artifact_identity: Some(super::signed_test_artifact_identity(policy_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 1024,
            max_gas: 10_000,
            max_call_rate: 1,
            max_output_bytes: 1024,
            max_effects: 0,
            max_emits: 1,
        },
    };
    activate_module_manifest(&mut world, source_manifest);
    activate_module_manifest(&mut world, policy_manifest.clone());

    let mut sandbox = CapturePurePolicyContextSandbox::default();
    world
        .execute_module_call("m.weather", "trace-policy-hash", vec![], &mut sandbox)
        .unwrap();

    let policy_request = sandbox
        .requests
        .iter()
        .find(|request| request.module_id == "m.policy.allow")
        .expect("policy request");
    let input: ModuleCallInput =
        serde_cbor::from_slice(&policy_request.input).expect("decode policy module input");
    assert_eq!(
        input.ctx.world_config_hash,
        Some(world.current_manifest_hash().unwrap())
    );
    assert_eq!(
        input.ctx.manifest_hash,
        Some(module_manifest_hash(&policy_manifest))
    );
}

#[derive(Default)]
struct CapturePurePolicyContextSandbox {
    requests: Vec<ModuleCallRequest>,
}

impl ModuleSandbox for CapturePurePolicyContextSandbox {
    fn call(&mut self, request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        self.requests.push(request.clone());
        match request.module_id.as_str() {
            "m.weather" => Ok(ModuleOutput {
                new_state: None,
                effects: vec![ModuleEffectIntent {
                    kind: "http.request".to_string(),
                    params: json!({"url": "https://example.com"}),
                    cap_ref: "cap.weather".to_string(),
                    cap_slot: None,
                }],
                emits: Vec::new(),
                tick_lifecycle: None,
                output_bytes: 64,
            }),
            "m.policy.allow" => Ok(ModuleOutput {
                new_state: None,
                effects: Vec::new(),
                emits: vec![ModuleEmit {
                    kind: "policy.allow".to_string(),
                    payload: json!({}),
                }],
                tick_lifecycle: None,
                output_bytes: 32,
            }),
            other => Err(ModuleCallFailure {
                module_id: request.module_id.clone(),
                trace_id: request.trace_id.clone(),
                code: ModuleCallErrorCode::Trap,
                detail: format!("unexpected module call {other}"),
            }),
        }
    }
}
