use super::pos;
use crate::{
    Action, AgentLocationAuthorityV1, CapabilityGrant, DomainEvent, FactoryConstructionPowerMode,
    FactoryConstructionPowerProfileV1, FactoryProfileV1, FactorySiteAuthorityV1, LocationAnchorV1,
    MaterialLedgerId, ModuleAbiContract, ModuleActivation, ModuleChangeSet, ModuleKind,
    ModuleLimits, ModuleManifest, ModuleRegistry, ModuleRole, PolicySet, ProposalDecision,
    RejectReason, World, WorldEventBody, util,
};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_wasm_abi::{
    FactoryBuildDecision, FactoryBuildRequest, FactoryModuleSpec, MaterialStack, ModuleCallFailure,
    ModuleCallInput, ModuleCallRequest, ModuleEmit, ModuleOutput, ModuleSandbox,
    RecipeExecutionPlan, RecipeExecutionRequest,
};
use std::collections::VecDeque;

fn factory_spec(factory_id: &str, build_time_ticks: u32, recipe_slots: u16) -> FactoryModuleSpec {
    FactoryModuleSpec {
        factory_id: factory_id.to_string(),
        display_name: "Test Factory".to_string(),
        tier: 1,
        tags: vec!["assembly".to_string()],
        build_cost: vec![
            MaterialStack::new("steel_plate", 10),
            MaterialStack::new("circuit_board", 2),
        ],
        build_time_ticks,
        base_power_draw: 5,
        recipe_slots,
        throughput_bps: 10_000,
        maintenance_per_tick: 1,
    }
}

fn prepare_factory_build(
    world: &mut World,
    builder_agent_id: &str,
    site_id: &str,
    factory_id: &str,
    source_module_id: Option<&str>,
) {
    let location_id = format!("location-{site_id}");
    world
        .set_location_anchor(LocationAnchorV1 {
            location_id: location_id.clone(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        })
        .expect("install builder location anchor");
    world
        .set_agent_location_authority(AgentLocationAuthorityV1 {
            agent_id: builder_agent_id.to_string(),
            location_id: location_id.clone(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        })
        .expect("install builder location authority");
    world
        .set_factory_site_authority(FactorySiteAuthorityV1 {
            site_id: site_id.to_string(),
            location_id,
            owner_agent_id: builder_agent_id.to_string(),
            authorized_agent_ids: Vec::new(),
            chunk_ready: true,
            active: true,
            authority_revision: 1,
            registered_at: 0,
        })
        .expect("install factory site authority");
    world
        .set_factory_construction_power_profile(FactoryConstructionPowerProfileV1 {
            factory_id: factory_id.to_string(),
            factory_kind: "test".to_string(),
            source_module_id: source_module_id.map(str::to_string),
            electricity_amount: 10,
            mode: FactoryConstructionPowerMode::StartOnlySink,
            authority_revision: 1,
            active: true,
        })
        .expect("install construction power profile");
    world
        .upsert_factory_profile(FactoryProfileV1 {
            factory_id: factory_id.to_string(),
            tier: 1,
            recipe_slots: 1,
            tags: vec!["assembly".to_string()],
        })
        .expect("install factory capability profile");
    world
        .set_agent_resource_balance(builder_agent_id, ResourceKind::Electricity, 10)
        .expect("seed construction power");
}

fn activate_pure_module(world: &mut World, module_id: &str, wasm_seed: &[u8]) {
    world.set_policy(PolicySet::allow_all());
    world.add_capability(CapabilityGrant::allow_all("cap.economy"));

    let wasm_hash = util::sha256_hex(wasm_seed);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_seed)
        .expect("register module artifact");

    let manifest = ModuleManifest {
        module_id: module_id.to_string(),
        name: format!("module-{module_id}"),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Pure,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".to_string()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 1024 * 1024,
            max_gas: 1_000_000,
            max_call_rate: 1024,
            max_output_bytes: 1024 * 1024,
            max_effects: 0,
            max_emits: 8,
        },
    };

    let changes = ModuleChangeSet {
        register: vec![manifest.clone()],
        activate: vec![ModuleActivation {
            module_id: manifest.module_id.clone(),
            version: manifest.version.clone(),
        }],
        ..ModuleChangeSet::default()
    };

    let mut content = serde_json::Map::new();
    content.insert(
        "module_changes".to_string(),
        serde_json::to_value(changes).expect("serialize module changes"),
    );
    let proposal_id = world
        .propose_manifest_update(
            crate::Manifest {
                version: 2,
                content: serde_json::Value::Object(content),
            },
            "tester",
        )
        .expect("propose module activation");
    world
        .shadow_proposal(proposal_id)
        .expect("shadow module proposal");
    world
        .approve_proposal(proposal_id, "tester", ProposalDecision::Approve)
        .expect("approve module proposal");
    world
        .apply_proposal(proposal_id)
        .expect("apply module proposal");
}

fn stack_amount(stacks: &[MaterialStack], kind: &str) -> i64 {
    stacks
        .iter()
        .find(|stack| stack.kind == kind)
        .map(|stack| stack.amount)
        .unwrap_or_default()
}

fn decode_captured_action_request<T: serde::de::DeserializeOwned>(
    request: &ModuleCallRequest,
) -> T {
    let input = decode_captured_module_input(request);
    let action = input.action.expect("module call action bytes");
    serde_cbor::from_slice(&action).expect("decode economy request payload")
}

fn decode_captured_module_input(request: &ModuleCallRequest) -> ModuleCallInput {
    serde_cbor::from_slice(&request.input).expect("decode module call input")
}

struct CaptureEconomyRequestSandbox {
    requests: Vec<ModuleCallRequest>,
    outputs: VecDeque<ModuleOutput>,
}

impl CaptureEconomyRequestSandbox {
    fn with_outputs(outputs: Vec<ModuleOutput>) -> Self {
        Self {
            requests: Vec::new(),
            outputs: outputs.into(),
        }
    }
}

impl ModuleSandbox for CaptureEconomyRequestSandbox {
    fn call(&mut self, request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        self.requests.push(request.clone());
        Ok(self.outputs.pop_front().unwrap_or(ModuleOutput {
            new_state: None,
            effects: Vec::new(),
            emits: Vec::new(),
            tick_lifecycle: None,
            output_bytes: 0,
        }))
    }
}

#[test]
fn build_factory_with_module_request_exposes_available_inputs_by_ledger() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register agent");

    world
        .set_ledger_material_balance(MaterialLedgerId::agent("builder-a"), "steel_plate", 12)
        .expect("seed builder steel");
    world
        .set_ledger_material_balance(MaterialLedgerId::agent("builder-a"), "circuit_board", 3)
        .expect("seed builder circuits");
    world
        .set_material_balance("steel_plate", 100)
        .expect("seed world steel");
    world
        .set_material_balance("circuit_board", 200)
        .expect("seed world circuits");

    activate_pure_module(&mut world, "m4.factory.capture", b"factory-capture-module");
    prepare_factory_build(
        &mut world,
        "builder-a",
        "site-1",
        "factory.capture",
        Some("m4.factory.capture"),
    );
    world.submit_action(Action::BuildFactoryWithModule {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        module_id: "m4.factory.capture".to_string(),
        spec: factory_spec("factory.capture", 1, 1),
    });

    let output = ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "economy.factory_build_decision".to_string(),
            payload: serde_json::to_value(FactoryBuildDecision::accepted(
                vec![
                    MaterialStack::new("steel_plate", 10),
                    MaterialStack::new("circuit_board", 2),
                ],
                1,
            ))
            .expect("serialize factory build decision"),
        }],
        tick_lifecycle: None,
        output_bytes: 256,
    };
    let mut sandbox = CaptureEconomyRequestSandbox::with_outputs(vec![output]);
    world
        .step_with_modules(&mut sandbox)
        .expect("start module build with captured request");

    assert_eq!(sandbox.requests.len(), 1);
    let input = decode_captured_module_input(&sandbox.requests[0]);
    let request: FactoryBuildRequest = decode_captured_action_request(&sandbox.requests[0]);
    let key = ModuleRegistry::record_key("m4.factory.capture", "0.1.0");
    let manifest = world
        .module_registry()
        .records
        .get(&key)
        .expect("active factory module record")
        .manifest
        .clone();
    assert_eq!(
        input.ctx.world_config_hash,
        Some(world.current_manifest_hash().unwrap())
    );
    assert_eq!(
        input.ctx.manifest_hash,
        Some(util::hash_json(&manifest).expect("hash economy module manifest"))
    );
    assert_eq!(stack_amount(&request.available_inputs, "steel_plate"), 12);
    assert_eq!(stack_amount(&request.available_inputs, "circuit_board"), 3);

    let by_ledger = request
        .available_inputs_by_ledger
        .expect("ledger-aware material view");
    let builder_inputs = by_ledger
        .get("agent:builder-a")
        .expect("builder ledger entry");
    assert_eq!(stack_amount(builder_inputs, "steel_plate"), 12);
    assert_eq!(stack_amount(builder_inputs, "circuit_board"), 3);

    let world_inputs = by_ledger.get("world").expect("world ledger entry");
    assert_eq!(stack_amount(world_inputs, "steel_plate"), 100);
    assert_eq!(stack_amount(world_inputs, "circuit_board"), 200);
}

#[test]
fn schedule_recipe_with_module_request_exposes_available_inputs_by_ledger() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register agent");

    world
        .set_material_balance("steel_plate", 10)
        .expect("seed steel");
    world
        .set_material_balance("circuit_board", 2)
        .expect("seed circuits");
    world
        .set_ledger_material_balance(MaterialLedgerId::agent("builder-a"), "steel_plate", 10)
        .expect("seed builder steel");
    world
        .set_ledger_material_balance(MaterialLedgerId::agent("builder-a"), "circuit_board", 2)
        .expect("seed builder circuits");
    prepare_factory_build(
        &mut world,
        "builder-a",
        "site-1",
        "factory.recipe.capture",
        None,
    );
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec: factory_spec("factory.recipe.capture", 1, 1),
    });
    world.step().expect("start build");
    world.step().expect("build complete");

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 6)
        .expect("seed site ingot");
    world
        .set_material_balance("iron_ingot", 9)
        .expect("seed world ingot");
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 30)
        .expect("seed builder electricity");
    world.set_resource_balance(ResourceKind::Electricity, 0);
    activate_pure_module(&mut world, "m4.recipe.capture", b"recipe-capture-module");

    world.submit_action(Action::ScheduleRecipeWithModule {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.recipe.capture".to_string(),
        recipe_id: "recipe.motor.mk1".to_string(),
        module_id: "m4.recipe.capture".to_string(),
        desired_batches: 2,
        deterministic_seed: 20260214,
    });

    let output = ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "economy.recipe_execution_plan".to_string(),
            payload: serde_json::to_value(RecipeExecutionPlan::accepted(
                2,
                vec![MaterialStack::new("iron_ingot", 6)],
                vec![MaterialStack::new("motor_mk1", 2)],
                vec![MaterialStack::new("metal_scrap", 1)],
                9,
                1,
            ))
            .expect("serialize recipe execution plan"),
        }],
        tick_lifecycle: None,
        output_bytes: 256,
    };
    let mut sandbox = CaptureEconomyRequestSandbox::with_outputs(vec![output]);
    world
        .step_with_modules(&mut sandbox)
        .expect("start recipe with captured request");

    assert_eq!(sandbox.requests.len(), 1);
    let input = decode_captured_module_input(&sandbox.requests[0]);
    let request: RecipeExecutionRequest = decode_captured_action_request(&sandbox.requests[0]);
    let key = ModuleRegistry::record_key("m4.recipe.capture", "0.1.0");
    let manifest = world
        .module_registry()
        .records
        .get(&key)
        .expect("active recipe module record")
        .manifest
        .clone();
    assert_eq!(
        input.ctx.world_config_hash,
        Some(world.current_manifest_hash().unwrap())
    );
    assert_eq!(
        input.ctx.manifest_hash,
        Some(util::hash_json(&manifest).expect("hash recipe module manifest"))
    );
    assert_eq!(request.desired_batches, 2);
    assert_eq!(request.deterministic_seed, 20260214);
    assert_eq!(
        request.available_power, 30,
        "recipe module quote must see the factory builder's electricity ledger"
    );
    assert_eq!(stack_amount(&request.available_inputs, "iron_ingot"), 6);

    let by_ledger = request
        .available_inputs_by_ledger
        .expect("ledger-aware material view");
    let site_inputs = by_ledger.get("site:site-1").expect("site ledger entry");
    assert_eq!(stack_amount(site_inputs, "iron_ingot"), 6);

    let world_inputs = by_ledger.get("world").expect("world ledger entry");
    assert_eq!(stack_amount(world_inputs, "iron_ingot"), 9);
    assert_eq!(
        world.pending_recipe_jobs_len(),
        1,
        "accepted module plan must schedule against the owner power ledger"
    );
    assert_eq!(
        world
            .agent_resource_balance("builder-a", ResourceKind::Electricity)
            .expect("builder electricity after module schedule"),
        21
    );
    assert_eq!(world.resource_balance(ResourceKind::Electricity), 0);
}

#[test]
fn schedule_recipe_with_module_does_not_fallback_to_world_materials() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register agent");
    let spec = factory_spec("factory.recipe.no-world-fallback", 1, 1);
    let builder_ledger = MaterialLedgerId::agent("builder-a");
    for stack in &spec.build_cost {
        world
            .set_ledger_material_balance(builder_ledger.clone(), stack.kind.as_str(), stack.amount)
            .expect("seed builder construction material");
    }
    prepare_factory_build(
        &mut world,
        "builder-a",
        "site-1",
        spec.factory_id.as_str(),
        None,
    );
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec,
    });
    world.step().expect("start build");
    world.step().expect("complete build");

    world
        .set_material_balance("iron_ingot", 1)
        .expect("seed only world recipe input");
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 30)
        .expect("seed recipe power");
    activate_pure_module(
        &mut world,
        "m4.recipe.no-world-fallback",
        b"recipe-no-fallback",
    );
    world.submit_action(Action::ScheduleRecipeWithModule {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.recipe.no-world-fallback".to_string(),
        recipe_id: "recipe.no-world-fallback".to_string(),
        module_id: "m4.recipe.no-world-fallback".to_string(),
        desired_batches: 1,
        deterministic_seed: 20260902,
    });

    let output = ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "economy.recipe_execution_plan".to_string(),
            payload: serde_json::to_value(RecipeExecutionPlan::accepted(
                1,
                vec![MaterialStack::new("iron_ingot", 1)],
                vec![MaterialStack::new("motor_mk1", 1)],
                Vec::new(),
                1,
                1,
            ))
            .expect("serialize recipe plan"),
        }],
        tick_lifecycle: None,
        output_bytes: 256,
    };
    let mut sandbox = CaptureEconomyRequestSandbox::with_outputs(vec![output]);
    world
        .step_with_modules(&mut sandbox)
        .expect("world-only recipe input should be rejected structurally");

    let request: RecipeExecutionRequest = decode_captured_action_request(&sandbox.requests[0]);
    assert_eq!(stack_amount(&request.available_inputs, "iron_ingot"), 0);
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::site("site-1"), "iron_ingot"),
        0
    );
    assert_eq!(world.material_balance("iron_ingot"), 1);
    assert_eq!(world.pending_recipe_jobs_len(), 0);
    assert!(world.journal().events.iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                reason: RejectReason::InsufficientMaterial { material_kind, .. },
                ..
            }) if material_kind == "iron_ingot"
        )
    }));
}
