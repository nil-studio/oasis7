use super::*;
use oasis7_agent_api::world_action::ResourceKind;

fn activate_product(world: &mut World, module_id: &str, version: &str, hash: &str) {
    let manifest = ModuleManifest {
        module_id: module_id.into(),
        name: "product code identity fixture".into(),
        version: version.into(),
        kind: ModuleKind::Pure,
        role: ModuleRole::Domain,
        wasm_hash: hash.into(),
        interface_version: "wasm-1".into(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".into()],
        subscriptions: vec![],
        required_caps: vec![],
        artifact_identity: Some(
            m4_builtin_module_artifact_identity(module_id, hash)
                .unwrap_or_else(|_| crate::tests::signed_test_artifact_identity(hash)),
        ),
        limits: ModuleLimits {
            max_mem_bytes: 64 * 1024 * 1024,
            max_gas: 2_000_000,
            max_call_rate: 64,
            max_output_bytes: 1024,
            max_effects: 0,
            max_emits: 1,
        },
    };
    world
        .apply_module_changes_for_test(
            0,
            &ModuleChangeSet {
                register: vec![manifest],
                activate: vec![ModuleActivation {
                    module_id: module_id.into(),
                    version: version.into(),
                }],
                ..Default::default()
            },
            "fixture",
        )
        .unwrap();
}

fn standard_hash(module_id: &str) -> String {
    m4_builtin_manifest_hash_tokens(module_id).unwrap()[0].clone()
}

fn batch_world() -> World {
    let mut world = recipe_fixture();
    // Exact active code identity fixture; this does not execute WASM. The
    // separate full-tier integration test exercises actual standard artifacts.
    activate_product(
        &mut world,
        M4_PRODUCT_IRON_INGOT_MODULE_ID,
        "test",
        &standard_hash(M4_PRODUCT_IRON_INGOT_MODULE_ID),
    );
    let mut allowance = grant();
    allowance.object_id = "factory".into();
    allowance.action_kinds = vec!["ScheduleRecipe".into()];
    allowance.limit_units = 10000;
    world.install_agent_delegation_grant(allowance).unwrap();
    world
        .set_agent_resource_balance(AGENT_ID, ResourceKind::Electricity, 10000)
        .unwrap();
    let ledger = world.state().factories["factory"].input_ledger.clone();
    world
        .set_ledger_material_balance(ledger.clone(), "iron_ore", 10000)
        .unwrap();
    world
        .set_ledger_material_balance(ledger, "carbon_fuel", 10000)
        .unwrap();
    world
}

#[test]
fn known_product_stack_limit_rejects_167_batch_quote_without_mutation() {
    let world = batch_world();
    let before = serde_json::to_value(world.snapshot()).unwrap();
    let journal = world.journal().clone();
    let legal = world
        .builtin_recipe_quote(AGENT_ID, "factory", "recipe.smelter.iron_ingot", 166)
        .unwrap();
    assert!(legal.rejection.is_none(), "{:?}", legal.rejection);
    assert_eq!(
        legal.preview.unwrap().produce,
        vec![MaterialStack::new("iron_ingot", 498)]
    );
    let denied = world
        .builtin_recipe_quote(AGENT_ID, "factory", "recipe.smelter.iron_ingot", 167)
        .unwrap();
    assert!(
        denied.rejection.is_some(),
        "501 iron must be rejected before resource sinks"
    );
    assert!(denied.preview.is_none());
    let reason = format!("{:?}", denied.rejection);
    for detail in ["iron_ingot", "501", "500"] {
        assert!(reason.contains(detail), "{reason}");
    }
    assert_eq!(serde_json::to_value(world.snapshot()).unwrap(), before);
    assert_eq!(world.journal(), &journal);
}

#[test]
fn known_product_stack_limit_native_submit_rejects_before_sinks() {
    let mut world = batch_world();
    let quote = world
        .builtin_recipe_quote(AGENT_ID, "factory", "recipe.smelter.iron_ingot", 167)
        .unwrap();
    let before = world.state().clone();
    let grants_before = world.agent_delegation_authorizations(AGENT_ID).unwrap();
    let start = world.journal().events.len();
    let prepared = prepare(
        &mut world,
        "oversize-production",
        quote.action,
        1336,
        Default::default(),
    );
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .unwrap();
    let settled = serde_json::to_value(world.snapshot()).unwrap();
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .unwrap();
    assert_eq!(serde_json::to_value(world.snapshot()).unwrap(), settled);

    assert_eq!(world.state().material_ledgers, before.material_ledgers);
    assert_eq!(world.state().industry_progress, before.industry_progress);
    assert_eq!(
        world.agent_delegation_authorizations(AGENT_ID).unwrap(),
        grants_before
    );

    assert_eq!(
        world.state().pending_recipe_jobs,
        before.pending_recipe_jobs
    );
    assert_eq!(
        world.state().factories["factory"].production,
        before.factories["factory"].production
    );
    assert_eq!(
        world.state().agents[AGENT_ID]
            .state
            .resources
            .get(ResourceKind::Electricity),
        before.agents[AGENT_ID]
            .state
            .resources
            .get(ResourceKind::Electricity)
    );
    assert!(world.journal().events[start..].iter().any(|event| matches!(&event.body,
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason: RejectReason::RuleDenied { notes }, .. }) if notes.iter().any(|note| note.contains("stack limit")))));
    assert!(
        !world.journal().events[start..].iter().any(|event| matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::RecipeStarted { .. })
        ))
    );
}

#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
#[test]
fn known_product_stack_limits_match_real_standard_validators() {
    let mut world = recipe_fixture();
    let mut wasm =
        oasis7_wasm_executor::WasmExecutor::new(crate::tests::test_wasm_executor_config()).unwrap();
    for (kind, module, limit) in [
        ("iron_ingot", M4_PRODUCT_IRON_INGOT_MODULE_ID, 500),
        ("alloy_plate", M4_PRODUCT_ALLOY_PLATE_MODULE_ID, 400),
        ("control_chip", M4_PRODUCT_CONTROL_CHIP_MODULE_ID, 200),
        ("motor_mk1", M4_PRODUCT_MOTOR_MODULE_ID, 128),
        ("logistics_drone", M4_PRODUCT_LOGISTICS_DRONE_MODULE_ID, 32),
        ("sensor_pack", M4_PRODUCT_SENSOR_PACK_MODULE_ID, 96),
        ("module_rack", M4_PRODUCT_MODULE_RACK_MODULE_ID, 48),
        ("factory_core", M4_PRODUCT_FACTORY_CORE_MODULE_ID, 24),
    ] {
        let bytes =
            crate::m4_builtin_wasm_artifact::m4_builtin_wasm_module_artifact_bytes(module).unwrap();
        let hash = crate::util::sha256_hex(&bytes);
        assert_eq!(
            hash,
            standard_hash(module),
            "must execute the exact admitted code"
        );
        world
            .register_module_artifact(hash.clone(), &bytes)
            .unwrap();
        activate_product(&mut world, module, "actual-standard", &hash);
        for (amount, accepted) in [(limit, true), (limit + 1, false)] {
            let start = world.journal().events.len();
            world.submit_action(Action::ValidateProductWithModule {
                requester_agent_id: AGENT_ID.into(),
                module_id: module.into(),
                stack: MaterialStack::new(kind, amount),
                deterministic_seed: 1,
            });
            world.step_with_modules(&mut wasm).unwrap();
            let events = &world.journal().events[start..];
            assert_eq!(events.iter().any(|event| matches!(&event.body,
                WorldEventBody::Domain(DomainEvent::ProductValidated { stack, stack_limit, .. })
                    if stack.kind == kind && stack.amount == amount && i64::from(*stack_limit) == limit)), accepted, "{kind} {amount}");
            assert_eq!(
                events.iter().any(|event| matches!(
                    &event.body,
                    WorldEventBody::Domain(DomainEvent::ActionRejected { .. })
                )),
                !accepted,
                "{kind} {amount}"
            );
        }
    }
}

#[test]
fn known_product_stack_limit_absent_and_unknown_code_preserve_unknown_boundary() {
    let mut world = batch_world();
    world
        .apply_module_changes_for_test(
            0,
            &ModuleChangeSet {
                deactivate: vec![oasis7_wasm_abi::ModuleDeactivation {
                    module_id: M4_PRODUCT_IRON_INGOT_MODULE_ID.into(),
                    reason: "fixture absent validator".into(),
                }],
                ..Default::default()
            },
            "fixture",
        )
        .unwrap();
    assert!(
        world
            .builtin_recipe_quote(AGENT_ID, "factory", "recipe.smelter.iron_ingot", 167)
            .unwrap()
            .rejection
            .is_none()
    );
    activate_product(
        &mut world,
        M4_PRODUCT_IRON_INGOT_MODULE_ID,
        "custom",
        &"e".repeat(64),
    );
    assert!(
        world
            .builtin_recipe_quote(AGENT_ID, "factory", "recipe.smelter.iron_ingot", 167)
            .unwrap()
            .rejection
            .is_none()
    );
}

#[test]
fn known_product_stack_limit_applies_to_alloy_without_iron_batch_hardcoding() {
    let mut world = batch_world();
    activate_product(
        &mut world,
        M4_PRODUCT_ALLOY_PLATE_MODULE_ID,
        "test",
        &standard_hash(M4_PRODUCT_ALLOY_PLATE_MODULE_ID),
    );
    let ledger = world.state().factories["factory"].input_ledger.clone();
    for kind in ["iron_ingot", "copper_wire"] {
        world
            .set_ledger_material_balance(ledger.clone(), kind, 10000)
            .unwrap();
    }
    let accepted = world
        .builtin_recipe_quote(AGENT_ID, "factory", "recipe.smelter.alloy_plate", 200)
        .unwrap();
    assert!(accepted.rejection.is_none(), "{:?}", accepted.rejection);
    assert_eq!(
        accepted.preview.unwrap().produce,
        vec![MaterialStack::new("alloy_plate", 400)]
    );
    let rejected = world
        .builtin_recipe_quote(AGENT_ID, "factory", "recipe.smelter.alloy_plate", 201)
        .unwrap();
    assert!(
        matches!(rejected.rejection, Some(RejectReason::RuleDenied { notes }) if notes.iter().any(|note| note.contains("alloy_plate") && note.contains("402") && note.contains("400")))
    );
}

#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
#[test]
fn known_product_stack_limit_166_batch_receipt_matches_quote_once_with_real_validator() {
    let mut world = batch_world();
    let bytes = crate::m4_builtin_wasm_artifact::m4_builtin_wasm_module_artifact_bytes(
        M4_PRODUCT_IRON_INGOT_MODULE_ID,
    )
    .unwrap();
    let hash = crate::util::sha256_hex(&bytes);
    assert_eq!(hash, standard_hash(M4_PRODUCT_IRON_INGOT_MODULE_ID));
    world.register_module_artifact(hash, &bytes).unwrap();
    let quote = world
        .builtin_recipe_quote(AGENT_ID, "factory", "recipe.smelter.iron_ingot", 166)
        .unwrap();
    let preview = quote.preview.clone().unwrap();
    let prepared = prepare(
        &mut world,
        "legal-166-production",
        quote.action,
        1328,
        Default::default(),
    );
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .unwrap();
    assert_eq!(
        world.pending_recipe_jobs_len(),
        1,
        "native start must create exactly one job; outcomes={:?}",
        world
            .journal()
            .events
            .iter()
            .rev()
            .take(5)
            .map(|event| &event.body)
            .collect::<Vec<_>>()
    );
    let mut wasm =
        oasis7_wasm_executor::WasmExecutor::new(crate::tests::test_wasm_executor_config()).unwrap();
    for _ in 0..preview.duration_ticks.saturating_add(2) {
        if world.pending_recipe_jobs_len() == 0 {
            break;
        }
        world.step_with_modules(&mut wasm).unwrap();
    }
    assert_eq!(
        world.ledger_material_balance(&preview.output_ledger, "iron_ingot"),
        498,
        "actual completion must match quote; pending={:?} receipts={:?}",
        world.state().pending_recipe_jobs,
        world.state().product_validation_receipts
    );
    assert_eq!(
        world.ledger_material_balance(&preview.output_ledger, "slag"),
        166
    );
    assert_eq!(
        world.state().factories["factory"].production.completed_jobs,
        1
    );
    assert_eq!(world.state().recipe_completion_receipts.len(), 1);
    let receipts = world.state().product_validation_receipts.clone();
    assert_eq!(receipts.values().map(Vec::len).sum::<usize>(), 1);
    assert!(
        receipts
            .values()
            .flatten()
            .all(|receipt| receipt.decision.accepted
                && receipt.stack == MaterialStack::new("iron_ingot", 498))
    );
    let mut restored = World::from_snapshot(world.snapshot(), world.journal().clone()).unwrap();
    restored
        .finalize_cognition_commit(&prepared.commit_id)
        .unwrap();
    restored.step_with_modules(&mut wasm).unwrap();
    assert_eq!(
        restored.ledger_material_balance(&preview.output_ledger, "iron_ingot"),
        498
    );
    assert_eq!(restored.state().recipe_completion_receipts.len(), 1);
    assert_eq!(restored.state().product_validation_receipts, receipts);
}

#[test]
fn known_product_stack_limit_module_plan_rejects_before_sinks() {
    let mut world = recipe_fixture();
    activate_product(
        &mut world,
        M4_PRODUCT_IRON_INGOT_MODULE_ID,
        "test",
        &standard_hash(M4_PRODUCT_IRON_INGOT_MODULE_ID),
    );
    let recipe_module = "m4.recipe.fixture.oversize";
    let bytes = b"fixture recipe plan; not a real WASM validator";
    let hash = crate::util::sha256_hex(bytes);
    world.register_module_artifact(hash.clone(), bytes).unwrap();
    activate_product(&mut world, recipe_module, "test", &hash);
    world
        .set_agent_resource_balance(AGENT_ID, ResourceKind::Electricity, 10000)
        .unwrap();
    let ledger = world.state().factories["factory"].input_ledger.clone();
    for kind in ["iron_ore", "carbon_fuel"] {
        world
            .set_ledger_material_balance(ledger.clone(), kind, 10000)
            .unwrap();
    }
    let plan = builtin_recipe_plan("recipe.smelter.iron_ingot", 167).unwrap();
    let mut sandbox = oasis7_wasm_executor::FixedSandbox::succeed(oasis7_wasm_abi::ModuleOutput {
        new_state: None,
        effects: vec![],
        emits: vec![oasis7_wasm_abi::ModuleEmit {
            kind: "economy.recipe_execution_plan".into(),
            payload: serde_json::to_value(plan).unwrap(),
        }],
        tick_lifecycle: None,
        output_bytes: 512,
    });
    let before = world.state().clone();
    let start = world.journal().events.len();
    world.submit_action(Action::ScheduleRecipeWithModule {
        requester_agent_id: AGENT_ID.into(),
        factory_id: "factory".into(),
        recipe_id: "recipe.smelter.iron_ingot".into(),
        module_id: recipe_module.into(),
        desired_batches: 167,
        deterministic_seed: 1,
    });
    world.step_with_modules(&mut sandbox).unwrap();
    assert_eq!(world.state().material_ledgers, before.material_ledgers);
    assert_eq!(
        world.state().pending_recipe_jobs,
        before.pending_recipe_jobs
    );
    assert_eq!(world.state().industry_progress, before.industry_progress);
    assert_eq!(
        world.state().agents[AGENT_ID]
            .state
            .resources
            .get(ResourceKind::Electricity),
        before.agents[AGENT_ID]
            .state
            .resources
            .get(ResourceKind::Electricity)
    );
    assert!(world.journal().events[start..].iter().any(|event| matches!(&event.body,
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason: RejectReason::RuleDenied { notes }, .. }) if notes.iter().any(|note| note.contains("stack limit") && note.contains("501")))));
}
