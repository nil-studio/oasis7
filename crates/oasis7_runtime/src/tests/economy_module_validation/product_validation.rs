use super::*;
use crate::{CausedBy, ProductValidationReceiptV1, RuntimeCommittedTickContext};

#[path = "product_validation_delivery.rs"]
mod delivery;

#[test]
fn schedule_recipe_with_module_auto_validates_outputs_before_commit() {
    let mut world = logistics_drone_module_recipe_world("factory.recipe.auto_validate");

    world.submit_action(Action::ScheduleRecipeWithModule {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.recipe.auto_validate".to_string(),
        recipe_id: "recipe.assembler.logistics_drone".to_string(),
        module_id: "m4.recipe.logistics_drone".to_string(),
        desired_batches: 1,
        deterministic_seed: 20260214,
    });

    let output = ModuleOutput {
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
                payload: serde_json::to_value(ProductValidationDecision::accepted(
                    "logistics_drone",
                    32,
                    true,
                    vec!["fleet_grade".to_string()],
                ))
                .expect("serialize product validation decision"),
            },
        ],
        tick_lifecycle: None,
        output_bytes: 512,
    };
    let mut sandbox = FixedSandbox::succeed(output);
    world
        .step_with_modules(&mut sandbox)
        .expect("start recipe with module");
    assert_eq!(world.pending_recipe_jobs_len(), 1);

    for _ in 0..4 {
        if world.pending_recipe_jobs_len() == 0 {
            break;
        }
        world
            .step_with_modules(&mut sandbox)
            .expect("advance module recipe toward validated completion");
    }
    assert_eq!(world.pending_recipe_jobs_len(), 0);
    let site_ledger = MaterialLedgerId::site("site-1");
    assert_eq!(
        world.ledger_material_balance(&site_ledger, "logistics_drone"),
        1
    );
    assert_eq!(
        world.ledger_material_balance(&site_ledger, "assembly_scrap"),
        1
    );

    let has_product_validated = world.journal().events.iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::ProductValidated {
                module_id,
                stack,
                ..
            }) if module_id == "m4.product.logistics_drone" && stack.kind == "logistics_drone"
        )
    });
    assert!(has_product_validated);
}

#[test]
fn product_validation_receipt_reuses_decision_after_crash_window_without_module_call() {
    let mut world = logistics_drone_module_recipe_world("factory.recipe.validation-retry");
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.recipe.validation-retry".to_string(),
        recipe_id: "recipe.assembler.logistics_drone".to_string(),
        plan: RecipeExecutionPlan::accepted(
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
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("start recipe");
    let pending = world
        .state()
        .pending_recipe_jobs
        .values()
        .next()
        .expect("pending recipe");
    let validation = DomainEvent::ProductValidationRecorded {
        receipt: ProductValidationReceiptV1 {
            job_id: pending.job_id,
            validation_index: Some(0),
            requester_agent_id: pending.requester_agent_id.clone(),
            module_id: "m4.product.logistics_drone".to_string(),
            stack: pending.produce[0].clone(),
            decision: ProductValidationDecision::accepted(
                "logistics_drone",
                32,
                true,
                vec!["fleet_grade".to_string()],
            ),
            failure_detail: None,
        },
    };
    // Simulate a crash after the validation event committed but before the
    // due-job loop could append RecipeCompleted.
    let mut journal = world.journal().clone();
    let event_id = journal
        .events
        .last()
        .map_or(1, |event| event.id.saturating_add(1));
    journal.append(WorldEvent {
        id: event_id,
        time: world.state().time,
        caused_by: None,
        body: WorldEventBody::Domain(validation),
    });
    world = World::from_snapshot(world.snapshot(), journal)
        .expect("recover after committed validation receipt");

    let validation_records_before = world
        .journal()
        .events
        .iter()
        .filter(|event| {
            matches!(
                &event.body,
                WorldEventBody::Domain(DomainEvent::ProductValidationRecorded { .. })
            )
        })
        .count();
    let mut sandbox = CaptureContextSandbox::with_outputs(Vec::new());
    world
        .step_with_modules(&mut sandbox)
        .expect("retry due recipe from persisted receipt");
    assert!(
        !sandbox
            .requests
            .iter()
            .any(|request| request.module_id == "m4.product.logistics_drone")
    );
    assert_eq!(world.pending_recipe_jobs_len(), 0);
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::site("site-1"), "logistics_drone"),
        1
    );
    assert_eq!(
        world
            .journal()
            .events
            .iter()
            .filter(|event| {
                matches!(
                    &event.body,
                    WorldEventBody::Domain(DomainEvent::ProductValidationRecorded { .. })
                )
            })
            .count(),
        validation_records_before,
        "recovery must not append a second validation receipt"
    );
    assert_eq!(
        world
            .journal()
            .events
            .iter()
            .filter(|event| {
                matches!(
                    &event.body,
                    WorldEventBody::Domain(DomainEvent::ProductValidated { .. })
                )
            })
            .count(),
        1,
        "recovery must append the missing accepted validation delivery once"
    );
    assert!(
        world.state().agents["builder-a"].mailbox.iter().any(
            |event| matches!(event, DomainEvent::ProductValidated { stack, .. } if stack.kind == "logistics_drone")
        ),
        "accepted validation must be delivered after receipt-only recovery"
    );
}

#[test]
fn product_validation_receipt_with_invalid_identity_does_not_update_preview() {
    let mut world = logistics_drone_module_recipe_world("factory.recipe.validation-preview");
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.recipe.validation-preview".to_string(),
        recipe_id: "recipe.assembler.logistics_drone".to_string(),
        plan: RecipeExecutionPlan::accepted(
            1,
            vec![
                MaterialStack::new("motor_mk1", 2),
                MaterialStack::new("control_chip", 1),
                MaterialStack::new("chassis_plate", 1),
            ],
            vec![MaterialStack::new("logistics_drone", 1)],
            Vec::new(),
            10,
            1,
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("start recipe");
    let pending = world
        .state()
        .pending_recipe_jobs
        .values()
        .next()
        .expect("pending recipe")
        .clone();
    let mut journal = world.journal().clone();
    journal.append(WorldEvent {
        id: journal.events.last().map_or(1, |event| event.id + 1),
        time: world.state().time,
        caused_by: None,
        body: WorldEventBody::Domain(DomainEvent::ProductValidationRecorded {
            receipt: ProductValidationReceiptV1 {
                job_id: pending.job_id,
                validation_index: Some(0),
                requester_agent_id: pending.requester_agent_id,
                module_id: "m4.product.logistics_drone".to_string(),
                stack: pending.produce[0].clone(),
                decision: ProductValidationDecision::accepted(
                    "wrong_product",
                    0,
                    true,
                    vec!["fleet_grade".to_string()],
                ),
                failure_detail: None,
            },
        }),
    });
    world = World::from_snapshot(world.snapshot(), journal)
        .expect("recover invalid validation receipt");
    assert!(
        world.state().latest_product_validation.is_none(),
        "invalid product identity or stack limit must not update latest preview"
    );
}

#[test]
fn rejected_product_validation_receipt_reuses_decision_after_crash_window() {
    let mut world = logistics_drone_module_recipe_world("factory.recipe.validation-reject-retry");
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.recipe.validation-reject-retry".to_string(),
        recipe_id: "recipe.assembler.logistics_drone".to_string(),
        plan: RecipeExecutionPlan::accepted(
            1,
            vec![
                MaterialStack::new("motor_mk1", 2),
                MaterialStack::new("control_chip", 1),
                MaterialStack::new("chassis_plate", 1),
            ],
            vec![MaterialStack::new("logistics_drone", 1)],
            Vec::new(),
            10,
            1,
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("start recipe");
    let pending = world
        .state()
        .pending_recipe_jobs
        .values()
        .next()
        .expect("pending recipe")
        .clone();
    let validation = DomainEvent::ProductValidationRecorded {
        receipt: ProductValidationReceiptV1 {
            job_id: pending.job_id,
            validation_index: Some(0),
            requester_agent_id: pending.requester_agent_id.clone(),
            module_id: "m4.product.logistics_drone".to_string(),
            stack: pending.produce[0].clone(),
            decision: ProductValidationDecision::rejected(
                "logistics_drone",
                0,
                true,
                vec!["fleet_grade".to_string()],
                vec!["stack exceeds limit".to_string()],
            ),
            failure_detail: None,
        },
    };
    let mut journal = world.journal().clone();
    let event_id = journal
        .events
        .last()
        .map_or(1, |event| event.id.saturating_add(1));
    journal.append(WorldEvent {
        id: event_id,
        time: world.state().time,
        caused_by: None,
        body: WorldEventBody::Domain(validation),
    });
    world = World::from_snapshot(world.snapshot(), journal)
        .expect("recover after committed rejected validation receipt");
    let mut sandbox = CaptureContextSandbox::with_outputs(Vec::new());
    world
        .step_with_modules(&mut sandbox)
        .expect("retry rejected due recipe from receipt");
    assert!(
        !sandbox
            .requests
            .iter()
            .any(|request| request.module_id == "m4.product.logistics_drone")
    );
    assert_eq!(world.pending_recipe_jobs_len(), 0);
    assert!(
        world
            .state()
            .factory_production_failure_dispositions
            .contains_key(&pending.job_id)
    );
}

#[test]
fn schedule_recipe_with_module_blocks_commit_when_product_validation_fails() {
    let mut world = logistics_drone_module_recipe_world("factory.recipe.auto_reject");

    world.submit_action(Action::ScheduleRecipeWithModule {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.recipe.auto_reject".to_string(),
        recipe_id: "recipe.assembler.logistics_drone".to_string(),
        module_id: "m4.recipe.logistics_drone".to_string(),
        desired_batches: 1,
        deterministic_seed: 20260214,
    });

    let factory_completion_count_before = world
        .state()
        .factories
        .get("factory.recipe.auto_reject")
        .expect("factory state before rejected settlement")
        .production
        .completed_jobs;
    let stable_line_repeat_count_before = world
        .state()
        .factories
        .get("factory.recipe.auto_reject")
        .expect("factory state before rejected settlement")
        .production
        .same_recipe_repeat_count;
    let completed_recipe_jobs_before = world.state().industry_progress.completed_recipe_jobs;
    let industry_stage_before = world.state().industry_progress.stage;

    let output = ModuleOutput {
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
    };
    let mut sandbox = FixedSandbox::succeed(output);
    world
        .step_with_modules(&mut sandbox)
        .expect("start recipe with module");
    assert_eq!(world.pending_recipe_jobs_len(), 1);
    let pending = world
        .state()
        .pending_recipe_jobs
        .values()
        .next()
        .expect("pending recipe commitment")
        .clone();

    for _ in 0..4 {
        if world.pending_recipe_jobs_len() == 0 {
            break;
        }
        world
            .step_with_modules(&mut sandbox)
            .expect("advance module recipe toward rejection settlement");
    }
    assert_eq!(world.pending_recipe_jobs_len(), 0);
    let site_ledger = MaterialLedgerId::site("site-1");
    assert_eq!(
        world.ledger_material_balance(&site_ledger, "logistics_drone"),
        0
    );
    assert_eq!(
        world.ledger_material_balance(&site_ledger, "assembly_scrap"),
        0
    );

    let factory = world
        .state()
        .factories
        .get("factory.recipe.auto_reject")
        .expect("factory state after rejected settlement");
    assert_eq!(
        factory.production.completed_jobs, factory_completion_count_before,
        "rejected product validation must not complete a factory job"
    );
    assert_eq!(
        factory.production.same_recipe_repeat_count, stable_line_repeat_count_before,
        "rejected product validation must not advance stable-line progress"
    );
    assert!(
        factory.production.last_completed_recipe_id.is_none(),
        "rejected product validation must not record a completed recipe"
    );
    assert!(
        factory
            .production
            .last_completed_canonical_snapshot
            .is_none(),
        "rejected product validation must not retain stable-line identity"
    );
    assert_eq!(
        world.state().industry_progress.completed_recipe_jobs,
        completed_recipe_jobs_before,
        "rejected product validation must not advance recipe completion progress"
    );
    assert_eq!(
        world.state().industry_progress.stage,
        industry_stage_before,
        "rejected product validation must leave the industry stage unchanged"
    );

    assert_eq!(
        world.state().factory_production_failure_dispositions.len(),
        1,
        "a product-validation rejection must persist exactly one disposition"
    );
    let disposition = world
        .state()
        .factory_production_failure_dispositions
        .get(&pending.job_id)
        .expect("product-validation failure disposition");
    assert_eq!(disposition.action_id, pending.job_id);
    assert_eq!(disposition.requester_agent_id, pending.requester_agent_id);
    assert_eq!(disposition.factory_id, pending.factory_id);
    assert_eq!(disposition.recipe_id, pending.recipe_id);
    assert_eq!(disposition.blocker_kind, "product_validation");
    assert_eq!(
        disposition.blocker_detail,
        "product validation rejected for logistics_drone before production settlement"
    );
    assert_eq!(disposition.disposition_kind, "consumed_lost");
    assert_eq!(disposition.consumed_inputs, pending.consume);
    assert_eq!(disposition.lost_inputs, pending.consume);
    assert_eq!(disposition.consumed_power, pending.power_required);
    assert_eq!(disposition.lost_power, pending.power_required);
    assert_eq!(
        disposition.next_action,
        "inspect_product_validation_and_reschedule"
    );
    assert_eq!(disposition.next_recheck, None);

    let has_rejected = world.journal().events.iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                reason: RejectReason::RuleDenied { notes },
                ..
            }) if notes.iter().any(|note| note.contains("stack exceeds limit"))
        )
    });
    assert!(has_rejected);
}

#[test]
fn product_validation_module_failure_settles_correlated_blocker_once() {
    let mut world = logistics_drone_module_recipe_world("factory.recipe.validation-failure");
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.recipe.validation-failure".to_string(),
        recipe_id: "recipe.assembler.logistics_drone".to_string(),
        plan: RecipeExecutionPlan::accepted(
            1,
            vec![
                MaterialStack::new("motor_mk1", 2),
                MaterialStack::new("control_chip", 1),
                MaterialStack::new("chassis_plate", 1),
            ],
            vec![MaterialStack::new("logistics_drone", 1)],
            Vec::new(),
            10,
            1,
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("start recipe before module failure");
    let job_id = world
        .state()
        .pending_recipe_jobs
        .keys()
        .next()
        .copied()
        .expect("pending recipe job");

    // Empty module output is an invalid product-validation response. The
    // due-job loop must turn it into a receipt, rejection, and terminal
    // disposition instead of rolling back the whole tick.
    let mut sandbox = CaptureContextSandbox::with_outputs(Vec::new());
    world
        .step_with_modules(&mut sandbox)
        .expect("invalid product output becomes durable rejection");
    assert_eq!(
        sandbox.requests.len(),
        1,
        "one validator call before failure"
    );
    assert_eq!(world.pending_recipe_jobs_len(), 0);
    let receipt = world
        .state()
        .product_validation_receipts
        .get(&job_id)
        .and_then(|receipts| receipts.first())
        .expect("failed validator receipt");
    assert!(receipt.failure_detail.is_some());
    assert_eq!(
        world
            .state()
            .factory_production_failure_dispositions
            .get(&job_id)
            .map(|disposition| disposition.blocker_kind.as_str()),
        Some("product_validation")
    );
    let attempts = world
        .state()
        .product_validation_attempts
        .get(&job_id)
        .expect("pre-call validation attempt");
    assert_eq!(attempts.len(), 1);

    let calls_after_settlement = sandbox.requests.len();
    world
        .step_with_modules(&mut sandbox)
        .expect("settled validation does not retry");
    assert_eq!(sandbox.requests.len(), calls_after_settlement);
    assert_eq!(
        world.state().factory_production_failure_dispositions.len(),
        1,
        "terminal disposition is exactly once"
    );
}

#[test]
fn product_validation_attempt_without_receipt_fails_closed_after_crash() {
    let mut world = logistics_drone_module_recipe_world("factory.recipe.validation-crash");
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.recipe.validation-crash".to_string(),
        recipe_id: "recipe.assembler.logistics_drone".to_string(),
        plan: RecipeExecutionPlan::accepted(
            1,
            vec![
                MaterialStack::new("motor_mk1", 2),
                MaterialStack::new("control_chip", 1),
                MaterialStack::new("chassis_plate", 1),
            ],
            vec![MaterialStack::new("logistics_drone", 1)],
            Vec::new(),
            10,
            1,
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("start recipe before simulated crash");
    let pending = world
        .state()
        .pending_recipe_jobs
        .values()
        .next()
        .expect("pending recipe")
        .clone();
    let mut journal = world.journal().clone();
    journal.append(WorldEvent {
        id: journal.events.last().map_or(1, |event| event.id + 1),
        time: world.state().time,
        caused_by: None,
        body: WorldEventBody::Domain(DomainEvent::ProductValidationAttemptStarted {
            attempt: crate::ProductValidationAttemptV1 {
                job_id: pending.job_id,
                validation_index: Some(0),
                requester_agent_id: pending.requester_agent_id.clone(),
                module_id: "m4.product.logistics_drone".to_string(),
                stack: pending.produce[0].clone(),
            },
        }),
    });
    world = World::from_snapshot(world.snapshot(), journal)
        .expect("recover after pre-call intent committed");
    let mut sandbox = CaptureContextSandbox::with_outputs(vec![ModuleOutput {
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
            .expect("serialize would-be retry output"),
        }],
        tick_lifecycle: None,
        output_bytes: 256,
    }]);
    world
        .step_with_modules(&mut sandbox)
        .expect("crash interval must settle fail-closed");
    assert!(
        sandbox.requests.is_empty(),
        "pre-call intent forbids retry call"
    );
    assert_eq!(world.pending_recipe_jobs_len(), 0);
    assert!(
        world
            .state()
            .product_validation_receipts
            .get(&pending.job_id)
            .is_some_and(|receipts| receipts
                .iter()
                .any(|receipt| receipt.failure_detail.is_some()))
    );
    assert!(
        world
            .state()
            .factory_production_failure_dispositions
            .contains_key(&pending.job_id)
    );
}

#[test]
fn product_validation_checkpoint_is_published_before_validator_call() {
    let mut world = logistics_drone_module_recipe_world("factory.recipe.validation-checkpoint");
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.recipe.validation-checkpoint".to_string(),
        recipe_id: "recipe.assembler.logistics_drone".to_string(),
        plan: RecipeExecutionPlan::accepted(
            1,
            vec![
                MaterialStack::new("motor_mk1", 2),
                MaterialStack::new("control_chip", 1),
                MaterialStack::new("chassis_plate", 1),
            ],
            vec![MaterialStack::new("logistics_drone", 1)],
            Vec::new(),
            10,
            1,
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("start recipe before checkpoint");

    let context = RuntimeCommittedTickContext {
        height: world.state().time.saturating_add(1),
        slot: world.state().time,
        epoch: 0,
        node_block_hash: String::new(),
        action_root: String::new(),
        authority_node_id: "test-authority".to_string(),
        committed_at_unix_ms: 0,
    };
    let mut checkpoint = None;
    let mut sandbox = CaptureContextSandbox::with_outputs(Vec::new());
    world
        .step_with_modules_for_committed_context_with_product_validation_checkpoint(
            &mut sandbox,
            &context,
            &mut |staged| {
                let attempt = staged
                    .state()
                    .product_validation_attempts
                    .values()
                    .flat_map(|attempts| attempts.iter())
                    .next()
                    .expect("checkpoint must contain pre-call validation intent");
                assert_eq!(attempt.module_id, "m4.product.logistics_drone");
                checkpoint = Some(staged.clone());
                Ok(())
            },
        )
        .expect("checkpointed product validation");

    assert_eq!(
        sandbox.requests.len(),
        1,
        "validator is called once after publish"
    );
    let checkpoint = checkpoint.expect("durable checkpoint callback");
    assert!(checkpoint.journal().events.iter().any(|event| matches!(
        &event.body,
        WorldEventBody::Domain(DomainEvent::ProductValidationAttemptStarted { .. })
    )));

    let mut recovered = World::from_snapshot(checkpoint.snapshot(), checkpoint.journal().clone())
        .expect("recover from pre-call checkpoint");
    let mut retry_sandbox = CaptureContextSandbox::with_outputs(Vec::new());
    recovered
        .step_with_modules(&mut retry_sandbox)
        .expect("recovery must fail closed after an unpublished receipt");
    assert!(retry_sandbox.requests.is_empty());
    assert!(
        recovered
            .state()
            .factory_production_failure_dispositions
            .values()
            .any(|disposition| disposition.blocker_kind == "product_validation")
    );
}

#[test]
fn product_validation_intent_recovery_preserves_predecessor_prologue() {
    let mut world = logistics_drone_module_recipe_world("factory.recipe.validation-prologue");
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.recipe.validation-prologue".to_string(),
        recipe_id: "recipe.assembler.logistics_drone".to_string(),
        plan: RecipeExecutionPlan::accepted(
            1,
            vec![
                MaterialStack::new("motor_mk1", 2),
                MaterialStack::new("control_chip", 1),
                MaterialStack::new("chassis_plate", 1),
            ],
            vec![MaterialStack::new("logistics_drone", 1)],
            Vec::new(),
            10,
            1,
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world
        .step()
        .expect("start recipe before prologue checkpoint");
    let baseline_durability = world
        .journal()
        .events
        .iter()
        .filter(|event| {
            matches!(
                &event.body,
                WorldEventBody::Domain(DomainEvent::FactoryDurabilityChanged { .. })
            )
        })
        .count();

    let context = RuntimeCommittedTickContext {
        height: world.state().time.saturating_add(1),
        slot: world.state().time,
        epoch: 0,
        node_block_hash: String::new(),
        action_root: String::new(),
        authority_node_id: "test-authority".to_string(),
        committed_at_unix_ms: 0,
    };
    let mut checkpoint = None;
    let mut sandbox = CaptureContextSandbox::with_outputs(Vec::new());
    world
        .step_with_modules_for_committed_context_with_product_validation_checkpoint(
            &mut sandbox,
            &context,
            &mut |staged| {
                checkpoint = Some(staged.clone());
                Ok(())
            },
        )
        .expect("uninterrupted product validation failure must settle");
    let checkpoint = checkpoint.expect("pre-call checkpoint");
    let checkpoint_durability = checkpoint
        .journal()
        .events
        .iter()
        .filter(|event| {
            matches!(
                &event.body,
                WorldEventBody::Domain(DomainEvent::FactoryDurabilityChanged { .. })
            )
        })
        .count();
    assert_eq!(
        checkpoint_durability.saturating_sub(baseline_durability),
        1,
        "the committed tick prologue must depreciate exactly once"
    );
    let checkpoint_durability_ppm = checkpoint
        .state()
        .factories
        .get("factory.recipe.validation-prologue")
        .expect("factory at checkpoint")
        .durability_ppm;

    // The continuation API is the uninterrupted control for a published
    // intent. A snapshot/journal round trip models the crash boundary.
    let mut uninterrupted = checkpoint.clone();
    let mut uninterrupted_sandbox = CaptureContextSandbox::with_outputs(Vec::new());
    uninterrupted
        .step_with_modules_for_committed_context_after_product_validation_intent(
            &mut uninterrupted_sandbox,
            &context,
        )
        .expect("continue published intent");

    let mut crash_recovered =
        World::from_snapshot(checkpoint.snapshot(), checkpoint.journal().clone())
            .expect("recover crash checkpoint");
    let mut retry_sandbox = CaptureContextSandbox::with_outputs(Vec::new());
    crash_recovered
        .step_with_modules_for_committed_context_after_product_validation_intent(
            &mut retry_sandbox,
            &context,
        )
        .expect("resume after crash");
    assert!(
        retry_sandbox.requests.is_empty(),
        "retry must not call validator"
    );

    let recovered_durability = crash_recovered
        .journal()
        .events
        .iter()
        .filter(|event| {
            matches!(
                &event.body,
                WorldEventBody::Domain(DomainEvent::FactoryDurabilityChanged { .. })
            )
        })
        .count();
    assert_eq!(recovered_durability, checkpoint_durability);
    assert_eq!(
        crash_recovered
            .state()
            .factories
            .get("factory.recipe.validation-prologue")
            .expect("recovered factory")
            .durability_ppm,
        checkpoint_durability_ppm,
        "recovery must not depreciate the factory a second time"
    );
    assert_eq!(
        serde_json::to_vec(crash_recovered.journal()).expect("serialize recovered journal"),
        serde_json::to_vec(uninterrupted.journal()).expect("serialize control journal"),
        "crash and uninterrupted continuation event roots must match"
    );
    assert_eq!(
        serde_json::to_vec(crash_recovered.state()).expect("serialize recovered state"),
        serde_json::to_vec(uninterrupted.state()).expect("serialize control state"),
        "crash and uninterrupted continuation execution states must match"
    );
}

#[test]
fn schedule_recipe_module_failure_is_correlated_action_rejection() {
    let mut world = logistics_drone_module_recipe_world("factory.recipe.schedule-failure");
    world.submit_action(Action::ScheduleRecipeWithModule {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.recipe.schedule-failure".to_string(),
        recipe_id: "recipe.assembler.logistics_drone".to_string(),
        module_id: "m4.recipe.logistics_drone".to_string(),
        desired_batches: 1,
        deterministic_seed: 20260902,
    });
    let journal_start = world.journal().events.len();
    let mut sandbox = CaptureContextSandbox::with_outputs(Vec::new());
    world
        .step_with_modules(&mut sandbox)
        .expect("schedule module failure becomes durable rejection");
    assert_eq!(sandbox.requests.len(), 1);
    assert_eq!(world.pending_recipe_jobs_len(), 0);
    assert!(world.journal().events[journal_start..].iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied { notes },
            }) if *action_id > 0
                && notes.iter().any(|note| note.contains("economy module evaluation failed"))
        )
    }));
}

#[test]
fn schedule_recipe_module_failure_blocks_resets_stable_line_and_recovers() {
    let factory_id = "factory.recipe.schedule-failure-recovery";
    let recipe_id = "recipe.assembler.logistics_drone";
    let plan = RecipeExecutionPlan::accepted(
        1,
        vec![
            MaterialStack::new("motor_mk1", 2),
            MaterialStack::new("control_chip", 1),
            MaterialStack::new("chassis_plate", 1),
        ],
        vec![MaterialStack::new("logistics_drone", 1)],
        Vec::new(),
        10,
        1,
    );
    let mut world = logistics_drone_module_recipe_world(factory_id);

    // Establish a non-zero stable-line candidate through the normal reducer
    // before exercising the module failure path.
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: recipe_id.to_string(),
        plan: plan.clone(),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("seed stable-line recipe");
    world.step().expect("complete stable-line recipe");
    let stable_factory = world
        .state()
        .factories
        .get(factory_id)
        .expect("factory after stable-line recipe");
    assert_eq!(stable_factory.production.same_recipe_repeat_count, 1);
    assert!(
        stable_factory
            .production
            .last_completed_canonical_snapshot
            .is_some()
    );

    world.submit_action(Action::ScheduleRecipeWithModule {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: recipe_id.to_string(),
        module_id: "m4.recipe.logistics_drone".to_string(),
        desired_batches: 1,
        deterministic_seed: 20260902,
    });
    let journal_start = world.journal().events.len();
    let mut sandbox = CaptureContextSandbox::with_outputs(Vec::new());
    world
        .step_with_modules(&mut sandbox)
        .expect("module failure becomes durable blocker");
    assert_eq!(sandbox.requests.len(), 1);
    assert_eq!(world.pending_recipe_jobs_len(), 0);

    let action_id = world.journal().events[journal_start..]
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied { notes },
            }) if notes
                .iter()
                .any(|note| note.contains("economy module evaluation failed")) =>
            {
                Some(*action_id)
            }
            _ => None,
        })
        .expect("module failure action rejection");
    let blocker_event = world.journal().events[journal_start..]
        .iter()
        .find(|event| {
            matches!(
                &event.body,
                WorldEventBody::Domain(DomainEvent::FactoryProductionBlocked {
                    action_id: blocked_action_id,
                    factory_id: blocked_factory,
                    recipe_id: blocked_recipe,
                    blocker_kind,
                    blocker_detail,
                    ..
                }) if *blocked_action_id == action_id
                    && blocked_factory == factory_id
                    && blocked_recipe == recipe_id
                    && blocker_kind == "module_failure"
                    && blocker_detail.contains("economy module evaluation failed")
            )
        })
        .expect("correlated module-failure blocker");
    assert_eq!(
        blocker_event.caused_by,
        Some(CausedBy::Action(action_id)),
        "blocker is causally correlated to the rejected action"
    );

    let blocked_factory = world
        .state()
        .factories
        .get(factory_id)
        .expect("factory after module failure blocker");
    assert_eq!(
        blocked_factory.production.status,
        FactoryProductionStatus::Blocked
    );
    assert_eq!(blocked_factory.production.active_jobs, 0);
    assert_eq!(blocked_factory.production.same_recipe_repeat_count, 0);
    assert!(
        blocked_factory
            .production
            .last_completed_recipe_id
            .is_none()
    );
    assert!(
        blocked_factory
            .production
            .last_completed_canonical_snapshot
            .is_none()
    );
    assert_eq!(
        blocked_factory.production.current_blocker_kind.as_deref(),
        Some("module_failure")
    );
    assert!(
        blocked_factory
            .production
            .current_blocker_detail
            .as_deref()
            .is_some_and(|detail| detail.contains("economy module evaluation failed"))
    );

    // Restore the input ledger and use the direct scheduler as the executable
    // recovery/recheck path. It must resume the blocked factory and create a
    // fresh pending job after the reset.
    for (kind, amount) in [("motor_mk1", 2), ("control_chip", 1), ("chassis_plate", 1)] {
        world
            .set_ledger_material_balance(MaterialLedgerId::site("site-1"), kind, amount)
            .expect("seed recovery input");
    }
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: recipe_id.to_string(),
        plan,
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    let recovery_journal_start = world.journal().events.len();
    world
        .step()
        .expect("direct schedule recovers blocked factory");
    assert!(
        world.journal().events[recovery_journal_start..]
            .iter()
            .any(|event| {
                matches!(
                    &event.body,
                    WorldEventBody::Domain(DomainEvent::FactoryProductionResumed {
                        factory_id: resumed_factory,
                        recipe_id: resumed_recipe,
                        previous_blocker_kind,
                        ..
                    }) if resumed_factory == factory_id
                        && resumed_recipe == recipe_id
                        && previous_blocker_kind.as_deref() == Some("module_failure")
                )
            })
    );
    let recovered_factory = world
        .state()
        .factories
        .get(factory_id)
        .expect("factory after recovery schedule");
    assert_eq!(
        recovered_factory.production.status,
        FactoryProductionStatus::Running
    );
    assert_eq!(recovered_factory.production.active_jobs, 1);
    assert!(recovered_factory.production.current_blocker_kind.is_none());
}

#[test]
fn product_validation_failure_disposition_survives_non_empty_snapshot_roundtrip() {
    let world = settled_product_validation_rejection_world("factory.receipt.snapshot");
    let expected = world
        .state()
        .factory_production_failure_dispositions
        .clone();
    let expected_validation_receipts = world.state().product_validation_receipts.clone();
    assert!(!expected.is_empty());
    assert!(!expected_validation_receipts.is_empty());

    let restored = World::from_snapshot(world.snapshot(), world.journal().clone())
        .expect("restore product-validation disposition snapshot");
    assert_eq!(
        restored.state().factory_production_failure_dispositions,
        expected,
        "non-empty failure disposition must survive snapshot replay"
    );
    assert_eq!(
        restored.state().product_validation_receipts,
        expected_validation_receipts,
        "product validation receipt must survive snapshot replay"
    );
}

#[test]
fn product_validation_failure_disposition_exact_replay_is_idempotent_and_byte_stable() {
    let world = settled_product_validation_rejection_world("factory.receipt.replay");
    let blocker = product_validation_blocker_event(&world);
    let mut replay = world.state().clone();
    let before = serde_json::to_vec(&replay).expect("serialize state before exact replay");

    replay
        .apply_domain_event(&blocker, replay.time)
        .expect("exact product-validation blocker replay");

    assert_eq!(
        serde_json::to_vec(&replay).expect("serialize state after exact replay"),
        before,
        "exact same-ID disposition replay must be byte-stable"
    );
}

#[test]
fn product_validation_failure_disposition_conflicting_same_id_rejects_without_mutation() {
    let world = settled_product_validation_rejection_world("factory.receipt.conflict");
    let mut blocker = product_validation_blocker_event(&world);
    if let DomainEvent::FactoryProductionBlocked { blocker_detail, .. } = &mut blocker {
        blocker_detail.push_str(" (conflict)");
    }

    let mut replay = world.state().clone();
    let before = serde_json::to_vec(&replay).expect("serialize state before conflicting replay");
    let result = replay.apply_domain_event(&blocker, replay.time);
    assert!(
        matches!(result, Err(WorldError::ResourceBalanceInvalid { .. })),
        "conflicting same-ID disposition must fail closed: {result:?}"
    );
    assert_eq!(
        serde_json::to_vec(&replay).expect("serialize state after conflicting replay"),
        before,
        "conflicting same-ID disposition must not mutate state"
    );
}
