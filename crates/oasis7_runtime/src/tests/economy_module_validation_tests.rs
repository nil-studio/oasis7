use super::test_support::prepare_module_test_factory_build;
use super::*;
use crate::tests::signed_test_artifact_identity;
use crate::{
    FactoryProductionStatus, Manifest, MaterialLedgerId, ModuleSubscription,
    ModuleSubscriptionStage, WorldError, WorldEvent,
};
use oasis7_wasm_abi::{
    ModuleCallFailure, ModuleCallInput, ModuleCallRequest, ModuleOutput, ModuleSandbox,
};
use std::collections::VecDeque;

struct CaptureContextSandbox {
    requests: Vec<ModuleCallRequest>,
    outputs: VecDeque<ModuleOutput>,
}

impl CaptureContextSandbox {
    fn with_outputs(outputs: Vec<ModuleOutput>) -> Self {
        Self {
            requests: Vec::new(),
            outputs: outputs.into(),
        }
    }
}

impl ModuleSandbox for CaptureContextSandbox {
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

fn activate_module_manifest_for_test(world: &mut World, manifest: ModuleManifest) {
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
        serde_json::to_value(&changes).unwrap(),
    );
    let manifest_update = Manifest {
        version: 2,
        content: serde_json::Value::Object(content),
    };

    let proposal_id = world
        .propose_manifest_update(manifest_update, "alice")
        .unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();
    world.apply_proposal(proposal_id).unwrap();
}

fn logistics_drone_module_recipe_world(factory_id: &str) -> World {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register agent");
    world.set_material_balance("steel_plate", 10).unwrap();
    world.set_material_balance("circuit_board", 2).unwrap();
    let spec = factory_spec(factory_id, 1, 1);
    prepare_module_test_factory_build(&mut world, "builder-a", "site-1", &spec);
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec,
    });
    world.step().expect("start build");
    world.step().expect("build complete");
    for (kind, amount) in [("motor_mk1", 2), ("control_chip", 1), ("chassis_plate", 1)] {
        world.set_material_balance(kind, amount).unwrap();
        world
            .set_ledger_material_balance(MaterialLedgerId::site("site-1"), kind, amount)
            .unwrap();
    }
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 40)
        .unwrap();
    world.set_resource_balance(ResourceKind::Electricity, 40);
    activate_pure_module(&mut world, "m4.recipe.logistics_drone", b"recipe-module");
    activate_pure_module(&mut world, "m4.product.logistics_drone", b"product-module");
    world
}

fn rejected_logistics_drone_recipe_module_output() -> ModuleOutput {
    ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![
            ModuleEmit {
                kind: "economy.recipe_execution_plan".to_string(),
                payload: serde_json::to_value(RecipeExecutionPlan::accepted(
                    1,
                    vec![
                        MaterialStack::new("motor_mk1", 2),
                        MaterialStack::new("control_chip", 1),
                        MaterialStack::new("chassis_plate", 1),
                    ],
                    vec![MaterialStack::new("logistics_drone", 1)],
                    vec![MaterialStack::new("assembly_scrap", 1)],
                    10,
                    1,
                ))
                .expect("serialize recipe execution plan"),
            },
            ModuleEmit {
                kind: "economy.product_validation".to_string(),
                payload: serde_json::to_value(ProductValidationDecision::rejected(
                    "logistics_drone",
                    0,
                    true,
                    vec!["fleet_grade".to_string()],
                    vec!["stack exceeds limit".to_string()],
                ))
                .expect("serialize rejected product validation"),
            },
        ],
        tick_lifecycle: None,
        output_bytes: 512,
    }
}

fn settled_product_validation_rejection_world(factory_id: &str) -> World {
    let mut world = logistics_drone_module_recipe_world(factory_id);
    world.submit_action(Action::ScheduleRecipeWithModule {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: "recipe.assembler.logistics_drone".to_string(),
        module_id: "m4.recipe.logistics_drone".to_string(),
        desired_batches: 1,
        deterministic_seed: 20260214,
    });
    let mut sandbox = FixedSandbox::succeed(rejected_logistics_drone_recipe_module_output());
    world
        .step_with_modules(&mut sandbox)
        .expect("start recipe with module");
    for _ in 0..4 {
        if world.pending_recipe_jobs_len() == 0 {
            break;
        }
        world
            .step_with_modules(&mut sandbox)
            .expect("advance module recipe toward rejection settlement");
    }
    assert_eq!(world.pending_recipe_jobs_len(), 0);
    assert_eq!(
        world.state().factory_production_failure_dispositions.len(),
        1
    );
    world
}

fn product_validation_blocker_event(world: &World) -> DomainEvent {
    world
        .journal()
        .events
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(
                blocker @ DomainEvent::FactoryProductionBlocked { blocker_kind, .. },
            ) if blocker_kind == "product_validation" => Some(blocker.clone()),
            _ => None,
        })
        .expect("product-validation blocker event")
}

#[test]
fn schedule_recipe_with_module_rejection_emits_production_blocker_and_resets_candidate() {
    let factory_id = "factory.recipe.module-rejection-blocker";
    let mut world = logistics_drone_module_recipe_world(factory_id);
    world.submit_action(Action::ScheduleRecipeWithModule {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: "recipe.assembler.module-rejection-blocker".to_string(),
        module_id: "m4.recipe.logistics_drone".to_string(),
        desired_batches: 1,
        deterministic_seed: 20260902,
    });
    let journal_start = world.journal().events.len();
    let output = ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "economy.recipe_execution_plan".to_string(),
            payload: serde_json::to_value(RecipeExecutionPlan::rejected(
                "module intentionally denied this recipe",
            ))
            .expect("serialize rejected recipe plan"),
        }],
        tick_lifecycle: None,
        output_bytes: 256,
    };
    let mut sandbox = FixedSandbox::succeed(output);

    world
        .step_with_modules(&mut sandbox)
        .expect("module recipe rejection should remain a structured action result");

    assert_eq!(world.pending_recipe_jobs_len(), 0);
    assert!(world.journal().events[journal_start..].iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                reason: RejectReason::RuleDenied { notes },
                ..
            }) if notes.iter().any(|note| note.contains("recipe module denied"))
        )
    }));
    assert!(world.journal().events[journal_start..].iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::FactoryProductionBlocked {
                factory_id: blocked_factory,
                recipe_id: blocked_recipe,
                blocker_kind,
                blocker_detail,
                ..
            }) if blocked_factory == factory_id
                && blocked_recipe == "recipe.assembler.module-rejection-blocker"
                && blocker_kind == "governance_gate"
                && blocker_detail.contains("recipe module denied")
        )
    }));
    let factory = world
        .state()
        .factories
        .get(factory_id)
        .expect("factory after module rejection blocker");
    assert_eq!(factory.production.status, FactoryProductionStatus::Blocked);
    assert_eq!(factory.production.same_recipe_repeat_count, 0);
    assert_eq!(factory.production.last_completed_canonical_snapshot, None);
}

#[path = "economy_module_validation/identity_guard.rs"]
mod identity_guard;
#[path = "economy_module_validation/product_validation.rs"]
mod product_validation;
#[test]
fn validate_product_with_module_rejects_unknown_requester_before_module_call() {
    let mut world = World::new();
    activate_pure_module(
        &mut world,
        "m4.product.unknown-requester",
        b"unknown-requester-product-module",
    );
    world.submit_action(Action::ValidateProductWithModule {
        requester_agent_id: "missing-requester".to_string(),
        module_id: "m4.product.unknown-requester".to_string(),
        stack: MaterialStack::new("logistics_drone", 1),
        deterministic_seed: 20260903,
    });
    let mut sandbox = CaptureContextSandbox::with_outputs(Vec::new());

    world
        .step_with_modules(&mut sandbox)
        .expect("unknown requester should be a structured rejection");

    assert!(
        sandbox.requests.is_empty(),
        "product module must not run for an unknown requester"
    );
    assert!(world.journal().events.iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                reason: RejectReason::AgentNotFound { agent_id },
                ..
            }) if agent_id == "missing-requester"
        )
    }));
}

#[test]
fn schedule_recipe_with_module_blocks_atomic_commit_when_byproduct_validation_fails() {
    let mut world = logistics_drone_module_recipe_world("factory.recipe.byproduct-reject");
    activate_pure_module(&mut world, "m4.product.assembly_scrap", b"byproduct-module");

    world.submit_action(Action::ScheduleRecipeWithModule {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.recipe.byproduct-reject".to_string(),
        recipe_id: "recipe.assembler.logistics_drone".to_string(),
        module_id: "m4.recipe.logistics_drone".to_string(),
        desired_batches: 1,
        deterministic_seed: 20260819,
    });

    let mut sandbox = CaptureContextSandbox::with_outputs(vec![
        ModuleOutput {
            new_state: None,
            effects: Vec::new(),
            emits: vec![ModuleEmit {
                kind: "economy.recipe_execution_plan".to_string(),
                payload: serde_json::to_value(RecipeExecutionPlan::accepted(
                    1,
                    vec![
                        MaterialStack::new("motor_mk1", 2),
                        MaterialStack::new("control_chip", 1),
                        MaterialStack::new("chassis_plate", 1),
                    ],
                    vec![MaterialStack::new("logistics_drone", 1)],
                    vec![MaterialStack::new("assembly_scrap", 1)],
                    10,
                    1,
                ))
                .expect("serialize recipe execution plan"),
            }],
            tick_lifecycle: None,
            output_bytes: 256,
        },
        ModuleOutput {
            new_state: None,
            effects: Vec::new(),
            emits: vec![ModuleEmit {
                kind: "economy.product_validation".to_string(),
                payload: serde_json::to_value(ProductValidationDecision::accepted(
                    "logistics_drone",
                    32,
                    true,
                    vec!["fleet_grade".to_string()],
                ))
                .expect("serialize accepted main-product validation"),
            }],
            tick_lifecycle: None,
            output_bytes: 256,
        },
        ModuleOutput {
            new_state: None,
            effects: Vec::new(),
            emits: vec![ModuleEmit {
                kind: "economy.product_validation".to_string(),
                payload: serde_json::to_value(ProductValidationDecision::rejected(
                    "assembly_scrap",
                    0,
                    false,
                    vec!["recyclable".to_string()],
                    vec!["assembly_scrap is ineligible".to_string()],
                ))
                .expect("serialize rejected byproduct validation"),
            }],
            tick_lifecycle: None,
            output_bytes: 256,
        },
    ]);

    world
        .step_with_modules(&mut sandbox)
        .expect("start module recipe");
    assert_eq!(world.pending_recipe_jobs_len(), 1);
    let settlement_journal_start = world.journal().events.len();

    for _ in 0..4 {
        if world.pending_recipe_jobs_len() == 0 {
            break;
        }
        world
            .step_with_modules(&mut sandbox)
            .expect("advance module recipe toward byproduct rejection");
    }

    assert_eq!(world.pending_recipe_jobs_len(), 0);
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::site("site-1"), "logistics_drone"),
        0,
        "a rejected byproduct must prevent main-product credit"
    );
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::site("site-1"), "assembly_scrap"),
        0,
        "a rejected byproduct must not be credited"
    );
    assert!(
        !world.journal().events[settlement_journal_start..]
            .iter()
            .any(|event| matches!(
                event.body,
                WorldEventBody::Domain(DomainEvent::RecipeCompleted { .. })
            )),
        "a rejected byproduct must not emit RecipeCompleted"
    );

    let factory = world
        .state()
        .factories
        .get("factory.recipe.byproduct-reject")
        .expect("factory after rejected byproduct validation");
    assert_eq!(
        factory.production.status,
        crate::FactoryProductionStatus::Blocked
    );
    assert_eq!(
        factory.production.current_blocker_kind.as_deref(),
        Some("product_validation")
    );
    assert!(
        factory
            .production
            .current_blocker_detail
            .as_deref()
            .is_some_and(|detail| detail.contains("assembly_scrap")),
        "product-validation blocker must identify the rejected byproduct"
    );
    assert!(
        world.journal().events[settlement_journal_start..]
            .iter()
            .any(|event| matches!(
                &event.body,
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    reason: RejectReason::RuleDenied { notes },
                    ..
                }) if notes.iter().any(|note| note.contains("assembly_scrap is ineligible"))
            ))
    );

    let validation_modules: Vec<_> = sandbox.requests[1..]
        .iter()
        .map(|request| request.module_id.as_str())
        .collect();
    assert_eq!(
        validation_modules,
        vec!["m4.product.logistics_drone", "m4.product.assembly_scrap"],
        "validation order must be main outputs followed by byproducts"
    );
}

#[test]
fn industrial_integrity_schedule_recipe_with_module_rejects_plan_over_requested_batches_before_sink()
 {
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
        .set_material_balance("iron_ingot", 1)
        .expect("seed recipe input");
    world.set_resource_balance(ResourceKind::Electricity, 40);
    let spec = factory_spec("factory.recipe.batch-bound", 1, 1);
    prepare_module_test_factory_build(&mut world, "builder-a", "site-1", &spec);
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec,
    });
    world.step().expect("start build");
    world.step().expect("build complete");
    activate_pure_module(
        &mut world,
        "m4.recipe.batch-bound",
        b"batch-bound-recipe-module",
    );
    world.submit_action(Action::ScheduleRecipeWithModule {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.recipe.batch-bound".to_string(),
        recipe_id: "recipe.batch-bound".to_string(),
        module_id: "m4.recipe.batch-bound".to_string(),
        desired_batches: 1,
        deterministic_seed: 20260214,
    });

    let input_before = world.material_balance("iron_ingot");
    let power_before = world.resource_balance(ResourceKind::Electricity);
    let journal_start = world.journal().events.len();
    let output = ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "economy.recipe_execution_plan".to_string(),
            payload: serde_json::to_value(RecipeExecutionPlan::accepted(
                2,
                vec![MaterialStack::new("iron_ingot", 1)],
                vec![MaterialStack::new("gear", 2)],
                Vec::new(),
                10,
                1,
            ))
            .expect("serialize over-accepted recipe plan"),
        }],
        tick_lifecycle: None,
        output_bytes: 256,
    };
    let mut sandbox = FixedSandbox::succeed(output);
    world
        .step_with_modules(&mut sandbox)
        .expect("over-accepted recipe plan should produce a rejection event");

    assert_eq!(world.pending_recipe_jobs_len(), 0);
    assert_eq!(world.material_balance("iron_ingot"), input_before);
    assert_eq!(
        world.resource_balance(ResourceKind::Electricity),
        power_before,
        "over-accepted plan must reject before power sink"
    );
    assert!(
        !world.journal().events[journal_start..].iter().any(|event| {
            matches!(
                &event.body,
                WorldEventBody::Domain(DomainEvent::RecipeStarted { .. })
            )
        })
    );
    assert!(world.journal().events[journal_start..].iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                reason: RejectReason::RuleDenied { notes },
                ..
            }) if notes.iter().any(|note| {
                note.contains("accepted_batches") || note.contains("desired_batches")
            })
        )
    }));
}

#[test]
fn validate_product_with_module_uses_module_decision() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register agent");
    activate_pure_module(&mut world, "m4.product.logistics_drone", b"product-module");

    world.submit_action(Action::ValidateProductWithModule {
        requester_agent_id: "builder-a".to_string(),
        module_id: "m4.product.logistics_drone".to_string(),
        stack: MaterialStack::new("logistics_drone", 1),
        deterministic_seed: 20260214,
    });

    let output = ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "economy.product_validation".to_string(),
            payload: serde_json::to_value(ProductValidationDecision::accepted(
                "logistics_drone",
                32,
                true,
                vec!["fleet_grade".to_string()],
            ))
            .expect("serialize product validation decision"),
        }],
        tick_lifecycle: None,
        output_bytes: 256,
    };
    let mut sandbox = FixedSandbox::succeed(output);
    world
        .step_with_modules(&mut sandbox)
        .expect("validate product with module");

    let validated = world
        .journal()
        .events
        .last()
        .expect("product validated event");
    match &validated.body {
        WorldEventBody::Domain(DomainEvent::ProductValidated {
            requester_agent_id,
            module_id,
            stack,
            stack_limit,
            tradable,
            quality_levels,
            ..
        }) => {
            assert_eq!(requester_agent_id, "builder-a");
            assert_eq!(module_id, "m4.product.logistics_drone");
            assert_eq!(stack.kind, "logistics_drone");
            assert_eq!(stack.amount, 1);
            assert_eq!(*stack_limit, 32);
            assert!(*tradable);
            assert_eq!(quality_levels, &vec!["fleet_grade".to_string()]);
        }
        other => panic!("expected ProductValidated, got {other:?}"),
    }
    let preview_source = world
        .state()
        .latest_product_validation
        .as_ref()
        .expect("accepted validation is retained for the player preview");
    assert_eq!(preview_source.product_id, "logistics_drone");
    assert!(preview_source.tradable);
}

#[test]
fn validate_product_with_module_rejects_when_module_denies() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register agent");
    activate_pure_module(&mut world, "m4.product.logistics_drone", b"product-module");

    world.submit_action(Action::ValidateProductWithModule {
        requester_agent_id: "builder-a".to_string(),
        module_id: "m4.product.logistics_drone".to_string(),
        stack: MaterialStack::new("logistics_drone", 99),
        deterministic_seed: 20260214,
    });

    let output = ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "economy.product_validation".to_string(),
            payload: serde_json::to_value(ProductValidationDecision::rejected(
                "logistics_drone",
                32,
                true,
                vec!["fleet_grade".to_string()],
                vec!["stack exceeds limit".to_string()],
            ))
            .expect("serialize rejected product validation"),
        }],
        tick_lifecycle: None,
        output_bytes: 256,
    };
    let mut sandbox = FixedSandbox::succeed(output);
    world
        .step_with_modules(&mut sandbox)
        .expect("module denial should turn into action rejected");

    let rejected = world.journal().events.last().expect("rejection event");
    match &rejected.body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => {
            match reason {
                RejectReason::RuleDenied { notes } => {
                    assert!(notes
                    .iter()
                    .any(|note| note.contains("product module denied: stack exceeds limit")));
                }
                other => panic!("expected RuleDenied, got {other:?}"),
            }
        }
        other => panic!("expected ActionRejected, got {other:?}"),
    }
}

#[test]
fn product_validation_quote_previews_eligible_product_without_mutating_world() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register agent");

    let journal_len_before_quote = world.journal().events.len();
    let stage_before_quote = world.state().industry_progress.stage;
    let quote = world
        .product_validation_quote(
            "builder-a",
            "m4.product.logistics_drone",
            &MaterialStack::new("logistics_drone", 1),
            20260214,
        )
        .expect("eligible product has a deterministic pre-submit quote");

    assert_eq!(world.journal().events.len(), journal_len_before_quote);
    assert_eq!(world.state().industry_progress.stage, stage_before_quote);
    assert_eq!(quote.product_id, "logistics_drone");
    assert_eq!(quote.product_role, "explore");
    assert!(quote.tradable);
    assert_eq!(quote.stage_before, "bootstrap");
    assert_eq!(quote.stage_after, "bootstrap");
    assert!(!quote.unlock_or_value_class.is_empty());
    assert!(!quote.recommended_action.is_empty());
    assert!(quote.submission_allowed);
    assert_eq!(
        quote,
        world
            .product_validation_quote(
                "builder-a",
                "m4.product.logistics_drone",
                &MaterialStack::new("logistics_drone", 1),
                20260214,
            )
            .expect("same input has the same quote"),
    );
}

#[test]
fn schedule_recipe_marks_factory_blocked_and_resumes_after_inputs_recover() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register builder");

    world
        .set_material_balance("steel_plate", 20)
        .expect("seed build steel");
    world
        .set_material_balance("circuit_board", 4)
        .expect("seed build circuits");
    let spec = factory_spec("factory.blocked_resume", 1, 1);
    prepare_module_test_factory_build(&mut world, "builder-a", "site-1", &spec);
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec,
    });
    world.step().expect("start build");
    world.step().expect("finish build");

    let plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 2)],
        vec![MaterialStack::new("motor_mk1", 1)],
        Vec::new(),
        1,
        1,
    );
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 5)
        .expect("seed builder electricity");
    world.set_resource_balance(ResourceKind::Electricity, 5);
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.blocked_resume".to_string(),
        recipe_id: "recipe.blocked_resume".to_string(),
        plan: plan.clone(),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("blocked schedule");

    let blocked_event = world.journal().events.last().expect("blocked event");
    match &blocked_event.body {
        WorldEventBody::Domain(DomainEvent::FactoryProductionBlocked {
            factory_id,
            recipe_id,
            blocker_kind,
            blocker_detail,
            ..
        }) => {
            assert_eq!(factory_id, "factory.blocked_resume");
            assert_eq!(recipe_id, "recipe.blocked_resume");
            assert_eq!(blocker_kind, "material_shortage");
            assert!(blocker_detail.contains("iron_ingot"));
        }
        other => panic!("expected FactoryProductionBlocked, got {other:?}"),
    }

    let factory = world
        .state()
        .factories
        .get("factory.blocked_resume")
        .expect("factory state");
    assert_eq!(
        factory.production.status,
        crate::FactoryProductionStatus::Blocked
    );
    assert_eq!(factory.production.active_jobs, 0);
    assert_eq!(
        factory.production.current_blocker_kind.as_deref(),
        Some("material_shortage")
    );

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 2)
        .expect("seed recovery iron");
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.blocked_resume".to_string(),
        recipe_id: "recipe.blocked_resume".to_string(),
        plan,
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("resume schedule");

    let resumed_event = world.journal().events.last().expect("resumed event");
    match &resumed_event.body {
        WorldEventBody::Domain(DomainEvent::FactoryProductionResumed {
            factory_id,
            recipe_id,
            previous_blocker_kind,
            ..
        }) => {
            assert_eq!(factory_id, "factory.blocked_resume");
            assert_eq!(recipe_id, "recipe.blocked_resume");
            assert_eq!(previous_blocker_kind.as_deref(), Some("material_shortage"));
        }
        other => panic!("expected FactoryProductionResumed, got {other:?}"),
    }

    let running_factory = world
        .state()
        .factories
        .get("factory.blocked_resume")
        .expect("factory state after resume");
    assert_eq!(
        running_factory.production.status,
        crate::FactoryProductionStatus::Running
    );
    assert_eq!(running_factory.production.active_jobs, 1);
    assert!(running_factory.production.current_blocker_kind.is_none());

    world.step().expect("complete resumed recipe");
    let completed_factory = world
        .state()
        .factories
        .get("factory.blocked_resume")
        .expect("factory state after completion");
    assert_eq!(
        completed_factory.production.status,
        crate::FactoryProductionStatus::Idle
    );
    assert_eq!(completed_factory.production.active_jobs, 0);
    assert_eq!(completed_factory.production.completed_jobs, 1);
}

#[test]
fn schedule_recipe_does_not_fallback_to_world_when_site_input_ledger_is_empty() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register builder");

    world
        .set_material_balance("steel_plate", 10)
        .expect("seed build steel");
    world
        .set_material_balance("circuit_board", 2)
        .expect("seed build circuits");
    let spec = factory_spec("factory.ledger-boundary", 1, 1);
    prepare_module_test_factory_build(&mut world, "builder-a", "site-ledger-boundary", &spec);
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-ledger-boundary".to_string(),
        spec,
    });
    world.step().expect("start build");
    world.step().expect("finish build");

    let site_ledger = MaterialLedgerId::site("site-ledger-boundary");
    assert_eq!(
        world.ledger_material_balance(&site_ledger, "iron_ingot"),
        0,
        "the factory input ledger starts without the recipe material"
    );
    world
        .set_material_balance("iron_ingot", 2)
        .expect("seed only the global world ledger");
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 5)
        .expect("seed builder electricity");

    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.ledger-boundary".to_string(),
        recipe_id: "recipe.ledger-boundary".to_string(),
        plan: RecipeExecutionPlan::accepted(
            1,
            vec![MaterialStack::new("iron_ingot", 2)],
            vec![MaterialStack::new("motor_mk1", 1)],
            Vec::new(),
            1,
            1,
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    let journal_start = world.journal().events.len();
    world.step().expect("site-bound schedule rejection");

    assert_eq!(
        world.pending_recipe_jobs_len(),
        0,
        "a missing site input must not create a pending job from world stock"
    );
    assert_eq!(
        world.ledger_material_balance(&site_ledger, "iron_ingot"),
        0,
        "the empty site ledger remains empty"
    );
    assert_eq!(
        world.material_balance("iron_ingot"),
        2,
        "global stock must not be silently consumed as a site input"
    );
    assert!(
        world.journal().events[journal_start..].iter().any(|event| {
            matches!(
                &event.body,
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    reason: RejectReason::InsufficientMaterial {
                        material_kind,
                        requested,
                        available,
                    },
                    ..
                }) if material_kind == "iron_ingot" && *requested == 2 && *available == 0
            )
        }),
        "site-bound scheduling must reject with the site ledger availability"
    );
    assert!(
        !world.journal().events[journal_start..]
            .iter()
            .any(|event| matches!(
                event.body,
                WorldEventBody::Domain(DomainEvent::RecipeStarted { .. })
            )),
        "site-bound scheduling must not start a job against global stock"
    );
}

#[test]
fn schedule_recipe_post_action_uses_primary_result_event_before_followup() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register agent");

    world
        .set_material_balance("steel_plate", 20)
        .expect("seed build steel");
    world
        .set_material_balance("circuit_board", 4)
        .expect("seed build circuits");
    let spec = factory_spec("factory.blocked_resume.post_action", 1, 1);
    prepare_module_test_factory_build(&mut world, "builder-a", "site-1", &spec);
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec,
    });
    world.step().expect("start build");
    world.step().expect("finish build");

    let plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 2)],
        vec![MaterialStack::new("motor_mk1", 1)],
        Vec::new(),
        1,
        1,
    );
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 5)
        .expect("seed builder electricity");
    world.set_resource_balance(ResourceKind::Electricity, 5);
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.blocked_resume.post_action".to_string(),
        recipe_id: "recipe.blocked_resume".to_string(),
        plan: plan.clone(),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("block factory production");

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 2)
        .expect("seed recovery iron");

    let observer_wasm_bytes = b"module-post-action-followup-observer";
    let observer_wasm_hash = util::sha256_hex(observer_wasm_bytes);
    world
        .register_module_artifact(observer_wasm_hash.clone(), observer_wasm_bytes)
        .unwrap();
    activate_module_manifest_for_test(
        &mut world,
        ModuleManifest {
            module_id: "m.post-action.followup-observer".to_string(),
            name: "PostActionFollowupObserver".to_string(),
            version: "0.1.0".to_string(),
            kind: ModuleKind::Pure,
            role: ModuleRole::Domain,
            wasm_hash: observer_wasm_hash.clone(),
            interface_version: "wasm-1".to_string(),
            abi_contract: ModuleAbiContract::default(),
            exports: vec!["call".to_string()],
            subscriptions: vec![ModuleSubscription {
                event_kinds: Vec::new(),
                action_kinds: vec!["action.economy.schedule_recipe".to_string()],
                stage: Some(ModuleSubscriptionStage::PostAction),
                filters: None,
            }],
            required_caps: Vec::new(),
            artifact_identity: Some(signed_test_artifact_identity(observer_wasm_hash.as_str())),
            limits: ModuleLimits {
                max_mem_bytes: 1024,
                max_gas: 10_000,
                max_call_rate: 1,
                max_output_bytes: 1024,
                max_effects: 0,
                max_emits: 0,
            },
        },
    );

    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.blocked_resume.post_action".to_string(),
        recipe_id: "recipe.blocked_resume".to_string(),
        plan,
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    let mut sandbox = CaptureContextSandbox::with_outputs(vec![ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: Vec::new(),
        tick_lifecycle: None,
        output_bytes: 0,
    }]);
    world
        .step_with_modules(&mut sandbox)
        .expect("resume schedule with post_action observer");

    assert_eq!(sandbox.requests.len(), 1);
    let observer_input: ModuleCallInput =
        serde_cbor::from_slice(&sandbox.requests[0].input).expect("decode observer input");
    let observed_event: WorldEvent = serde_cbor::from_slice(
        observer_input
            .event
            .as_deref()
            .expect("post_action result event bytes"),
    )
    .expect("decode post_action event");
    match observed_event.body {
        WorldEventBody::Domain(DomainEvent::RecipeStarted {
            factory_id,
            recipe_id,
            ..
        }) => {
            assert_eq!(factory_id, "factory.blocked_resume.post_action");
            assert_eq!(recipe_id, "recipe.blocked_resume");
        }
        other => panic!("expected RecipeStarted, got {other:?}"),
    }

    match &world.journal().events.last().expect("followup event").body {
        WorldEventBody::Domain(DomainEvent::FactoryProductionResumed {
            factory_id,
            recipe_id,
            ..
        }) => {
            assert_eq!(factory_id, "factory.blocked_resume.post_action");
            assert_eq!(recipe_id, "recipe.blocked_resume");
        }
        other => panic!("expected FactoryProductionResumed, got {other:?}"),
    }
}

#[test]
fn non_owner_schedule_recipe_with_module_rejects_before_module_plan_or_sink() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register owner");
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-b".to_string(),
        pos: pos(1, 0),
    });
    world.step().expect("register non-owner");
    world
        .set_material_balance("steel_plate", 10)
        .expect("seed steel");
    world
        .set_material_balance("circuit_board", 2)
        .expect("seed circuits");
    let spec = factory_spec("factory.recipe.module-owner", 1, 1);
    prepare_module_test_factory_build(&mut world, "builder-a", "site-1", &spec);
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec,
    });
    world.step().expect("start build");
    world.step().expect("complete build");
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 2)
        .expect("seed recipe input");
    world.set_resource_balance(ResourceKind::Electricity, 30);
    activate_pure_module(
        &mut world,
        "m4.recipe.module-owner",
        b"recipe-module-owner-guard",
    );
    world.submit_action(Action::ScheduleRecipeWithModule {
        requester_agent_id: "builder-b".to_string(),
        factory_id: "factory.recipe.module-owner".to_string(),
        recipe_id: "recipe.module-owner".to_string(),
        module_id: "m4.recipe.module-owner".to_string(),
        desired_batches: 1,
        deterministic_seed: 20260818,
    });

    let input_before =
        world.ledger_material_balance(&MaterialLedgerId::site("site-1"), "iron_ingot");
    let power_before = world.resource_balance(ResourceKind::Electricity);
    let journal_start = world.journal().events.len();
    let mut sandbox = CaptureContextSandbox::with_outputs(vec![ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "economy.recipe_execution_plan".to_string(),
            payload: serde_json::to_value(RecipeExecutionPlan::accepted(
                1,
                vec![MaterialStack::new("iron_ingot", 2)],
                vec![MaterialStack::new("gear", 1)],
                Vec::new(),
                5,
                1,
            ))
            .expect("serialize owner-guard recipe plan"),
        }],
        tick_lifecycle: None,
        output_bytes: 256,
    }]);

    world
        .step_with_modules(&mut sandbox)
        .expect("reject non-owner module schedule");

    assert!(
        sandbox.requests.is_empty(),
        "non-owner schedule must be rejected before invoking the recipe module"
    );
    assert_eq!(world.pending_recipe_jobs_len(), 0);
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::site("site-1"), "iron_ingot"),
        input_before,
        "non-owner module schedule must not sink input"
    );
    assert_eq!(
        world.resource_balance(ResourceKind::Electricity),
        power_before,
        "non-owner module schedule must not sink power"
    );
    assert!(world.journal().events[journal_start..].iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                reason: RejectReason::RuleDenied { .. },
                ..
            })
        )
    }));
}

#[path = "economy_module_origin_tests.rs"]
mod origin_tests;
