use super::*;
use crate::{
    Journal, ProductValidationAttemptV1, ProductValidationDeliveryCursor,
    RuntimeCommittedTickContext, WorldError, WorldTime,
};
use oasis7_wasm_abi::{FactoryBuildDecision, ModuleStateUpdate};

fn append_product_validation_event(
    journal: &mut Journal,
    time: WorldTime,
    id: &mut u64,
    body: DomainEvent,
) -> u64 {
    let event_id = *id;
    journal.append(WorldEvent {
        id: event_id,
        time,
        caused_by: None,
        body: WorldEventBody::Domain(body),
    });
    *id = (*id).saturating_add(1);
    event_id
}

#[test]
fn product_validation_delivery_cursor_does_not_shadow_same_name_reducer_state() {
    let world = World::new();
    let mut journal = world.journal().clone();
    journal.append(WorldEvent {
        id: 1,
        time: 0,
        caused_by: None,
        body: WorldEventBody::ModuleStateUpdated(ModuleStateUpdate {
            module_id: "__oasis7.product_validation_delivery.v1".to_string(),
            trace_id: "reducer-state".to_string(),
            state: vec![0xA5, 0x5A],
        }),
    });
    journal.append(WorldEvent {
        id: 2,
        time: 0,
        caused_by: None,
        body: WorldEventBody::ProductValidationDeliveryCursorUpdated(
            ProductValidationDeliveryCursor {
                routed_through_event_id: 1,
                event_id_era: 0,
            },
        ),
    });
    let recovered = World::from_snapshot(world.snapshot(), journal).expect("replay cursor");
    assert_eq!(
        recovered
            .state()
            .module_states
            .get("__oasis7.product_validation_delivery.v1"),
        Some(&vec![0xA5, 0x5A])
    );
    assert_eq!(
        recovered
            .state()
            .product_validation_delivery_cursor
            .routed_through_event_id,
        1
    );
}

#[test]
fn product_validation_delivery_cursor_stays_bounded_over_long_run() {
    let mut cursor = ProductValidationDeliveryCursor::default();
    for event_id in 1..=1_000_000 {
        cursor.routed_through_event_id = event_id;
    }
    let encoded = serde_cbor::to_vec(&cursor).expect("encode bounded cursor");
    assert!(
        encoded.len() <= 48,
        "cursor grew to {} bytes",
        encoded.len()
    );
    assert_eq!(cursor.routed_through_event_id, 1_000_000);
}

#[test]
fn product_validation_delivery_cursor_routes_after_event_id_rollover() {
    let mut cursor = ProductValidationDeliveryCursor {
        routed_through_event_id: u64::MAX,
        event_id_era: 3,
    };
    assert!(cursor.has_routed(3, u64::MAX));
    assert!(!cursor.has_routed(4, 1));

    cursor.advance_to(4, 1);
    assert_eq!(cursor.event_id_era, 4);
    assert_eq!(cursor.routed_through_event_id, 1);
    assert!(cursor.has_routed(4, 1));
}

#[test]
fn product_validation_delivery_cursor_decodes_legacy_id_only_snapshot() {
    let cursor: ProductValidationDeliveryCursor =
        serde_json::from_value(serde_json::json!({ "routed_through_event_id": 7 }))
            .expect("legacy cursor should decode");
    assert_eq!(cursor.event_id_era, 0);
    assert_eq!(cursor.routed_through_event_id, 7);
}

#[test]
fn product_validation_delivery_cursor_replays_event_id_rollover_era() {
    let world = World::new();
    let mut snapshot = world.snapshot();
    snapshot.last_event_id = u64::MAX - 1;
    snapshot.event_id_era = 3;
    let mut journal = Journal::new();
    journal.append(WorldEvent {
        id: u64::MAX,
        time: 0,
        caused_by: None,
        body: WorldEventBody::ProductValidationDeliveryCursorUpdated(
            ProductValidationDeliveryCursor {
                routed_through_event_id: u64::MAX,
                event_id_era: 3,
            },
        ),
    });
    journal.append(WorldEvent {
        id: 1,
        time: 0,
        caused_by: None,
        body: WorldEventBody::ProductValidationDeliveryCursorUpdated(
            ProductValidationDeliveryCursor {
                routed_through_event_id: 1,
                event_id_era: 4,
            },
        ),
    });

    let recovered = World::from_snapshot(snapshot, journal).expect("replay rollover cursor");
    assert_eq!(
        recovered
            .state()
            .product_validation_delivery_cursor
            .event_id_era,
        4
    );
    assert_eq!(
        recovered
            .state()
            .product_validation_delivery_cursor
            .routed_through_event_id,
        1
    );
    let recovered_snapshot = recovered.snapshot();
    assert_eq!(recovered_snapshot.event_id_era, 4);
    assert_eq!(recovered_snapshot.last_event_id, 1);
}

#[test]
fn product_validation_delivery_cursor_replay_keeps_monotonic_high_water_mark() {
    let world = World::new();
    let mut snapshot = world.snapshot();
    snapshot.state.product_validation_delivery_cursor = ProductValidationDeliveryCursor {
        routed_through_event_id: 17,
        event_id_era: 4,
    };
    let mut journal = Journal::new();
    journal.append(WorldEvent {
        id: 1,
        time: 0,
        caused_by: None,
        body: WorldEventBody::ProductValidationDeliveryCursorUpdated(
            ProductValidationDeliveryCursor {
                routed_through_event_id: u64::MAX,
                event_id_era: 3,
            },
        ),
    });

    let recovered = World::from_snapshot(snapshot, journal).expect("replay stale cursor");
    assert_eq!(
        recovered.state().product_validation_delivery_cursor,
        ProductValidationDeliveryCursor {
            routed_through_event_id: 17,
            event_id_era: 4,
        }
    );
}

#[test]
fn module_factory_rejects_world_invalid_submission_before_module_evaluation() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register builder");
    let factory_id = "factory.module.admission";
    let module_id = "m4.factory.admission";
    let spec = factory_spec(factory_id, 1, 1);
    prepare_module_test_factory_build(&mut world, "builder-a", "site-1", &spec);
    activate_pure_module(&mut world, module_id, b"factory-admission-module");
    bind_factory_build_module(&mut world, factory_id, module_id);
    world.submit_action(Action::BuildFactoryWithModule {
        builder_agent_id: "builder-a".to_string(),
        site_id: "unknown-site".to_string(),
        module_id: module_id.to_string(),
        spec,
    });

    let steel_before =
        world.ledger_material_balance(&MaterialLedgerId::agent("builder-a"), "steel_plate");
    let power_before = world
        .agent_resource_balance("builder-a", ResourceKind::Electricity)
        .expect("builder power");
    let mut sandbox = CaptureContextSandbox::with_outputs(Vec::new());
    world
        .step_with_modules(&mut sandbox)
        .expect("world-invalid module build should be a structured rejection");

    assert!(
        sandbox.requests.is_empty(),
        "invalid build must not invoke module"
    );
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::agent("builder-a"), "steel_plate"),
        steel_before
    );
    assert_eq!(
        world
            .agent_resource_balance("builder-a", ResourceKind::Electricity)
            .expect("builder power after rejection"),
        power_before
    );
    assert_eq!(world.pending_factory_builds_len(), 0);
    assert!(!world.has_factory(factory_id));
    assert!(!world.journal().events.iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::FactoryBuildStarted { .. })
                | WorldEventBody::ModuleRuntimeCharged(_)
                | WorldEventBody::ModuleEmitted(_)
        )
    }));
}

#[test]
fn module_factory_revalidates_resolved_spec_before_commit() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register builder");
    let factory_id = "factory.module.resolved-spec";
    let module_id = "m4.factory.resolved-spec";
    let spec = factory_spec(factory_id, 1, 1);
    prepare_module_test_factory_build(&mut world, "builder-a", "site-1", &spec);
    activate_pure_module(&mut world, module_id, b"factory-resolved-spec-module");
    bind_factory_build_module(&mut world, factory_id, module_id);
    world.submit_action(Action::BuildFactoryWithModule {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        module_id: module_id.to_string(),
        spec,
    });
    let output = ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "economy.factory_build_decision".to_string(),
            payload: serde_json::to_value(FactoryBuildDecision::accepted(
                vec![MaterialStack::new("", 1)],
                1,
            ))
            .expect("serialize invalid resolved factory decision"),
        }],
        tick_lifecycle: None,
        output_bytes: 256,
    };
    let steel_before =
        world.ledger_material_balance(&MaterialLedgerId::agent("builder-a"), "steel_plate");
    let power_before = world
        .agent_resource_balance("builder-a", ResourceKind::Electricity)
        .expect("builder power");
    let mut sandbox = CaptureContextSandbox::with_outputs(vec![output]);
    world
        .step_with_modules(&mut sandbox)
        .expect("invalid resolved module spec should be a structured rejection");

    assert_eq!(
        sandbox.requests.len(),
        1,
        "valid admission reaches module once"
    );
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::agent("builder-a"), "steel_plate"),
        steel_before
    );
    assert_eq!(
        world
            .agent_resource_balance("builder-a", ResourceKind::Electricity)
            .expect("builder power after rejection"),
        power_before
    );
    assert_eq!(world.pending_factory_builds_len(), 0);
    assert!(!world.has_factory(factory_id));
    assert!(!world.journal().events.iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::FactoryBuildStarted { .. })
        )
    }));
    assert!(world.journal().events.iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::ActionRejected { .. })
        )
    }));
}

#[test]
fn rejected_resolved_factory_spec_discards_all_module_side_effects() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register builder");
    let factory_id = "factory.module.staged-side-effects";
    let module_id = "m4.factory.staged-side-effects";
    let wasm = b"factory-staged-side-effects-module";
    let wasm_hash = crate::util::sha256_hex(wasm);
    let spec = factory_spec(factory_id, 1, 1);
    prepare_module_test_factory_build(&mut world, "builder-a", "site-1", &spec);
    world
        .register_module_artifact(wasm_hash.clone(), wasm)
        .expect("register reducer artifact");
    activate_module_manifest_for_test(
        &mut world,
        ModuleManifest {
            module_id: module_id.to_string(),
            name: "FactoryStagedSideEffects".to_string(),
            version: "0.1.0".to_string(),
            kind: ModuleKind::Reducer,
            role: ModuleRole::Domain,
            wasm_hash: wasm_hash.clone(),
            interface_version: "wasm-1".to_string(),
            abi_contract: ModuleAbiContract::default(),
            exports: vec!["call".to_string(), "reduce".to_string()],
            subscriptions: Vec::new(),
            required_caps: Vec::new(),
            artifact_identity: Some(signed_test_artifact_identity(wasm_hash.as_str())),
            limits: ModuleLimits {
                max_mem_bytes: 1024 * 1024,
                max_gas: 1_000_000,
                max_call_rate: 1024,
                max_output_bytes: 1024 * 1024,
                max_effects: 0,
                max_emits: 8,
            },
        },
    );
    bind_factory_build_module(&mut world, factory_id, module_id);
    world.submit_action(Action::BuildFactoryWithModule {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        module_id: module_id.to_string(),
        spec,
    });
    let journal_start = world.journal().events.len();
    let output = ModuleOutput {
        new_state: Some(vec![0xCA, 0xFE]),
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "economy.factory_build_decision".to_string(),
            payload: serde_json::to_value(FactoryBuildDecision::accepted(
                vec![MaterialStack::new("", 1)],
                1,
            ))
            .expect("serialize invalid resolved factory decision"),
        }],
        tick_lifecycle: None,
        output_bytes: 256,
    };
    let mut sandbox = CaptureContextSandbox::with_outputs(vec![output]);
    world
        .step_with_modules(&mut sandbox)
        .expect("invalid resolved spec should be a structured rejection");

    assert_eq!(sandbox.requests.len(), 1);
    assert_eq!(world.pending_factory_builds_len(), 0);
    assert!(!world.has_factory(factory_id));
    assert!(!world.state().module_states.contains_key(module_id));
    assert!(
        !world.journal().events[journal_start..].iter().any(|event| {
            matches!(
                &event.body,
                WorldEventBody::ModuleRuntimeCharged(_)
                    | WorldEventBody::ModuleStateUpdated(_)
                    | WorldEventBody::ModuleEmitted(_)
                    | WorldEventBody::EffectQueued(_)
            )
        })
    );
    assert!(world.journal().events[journal_start..].iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::ActionRejected { .. })
        )
    }));
}

#[test]
fn product_validation_recovery_replays_existing_delivery_before_next_output_intent() {
    let mut world = logistics_drone_module_recipe_world("factory.recipe.delivery-checkpoint");
    let observer_wasm = b"product-validation-delivery-observer";
    let observer_hash = crate::util::sha256_hex(observer_wasm);
    world
        .register_module_artifact(observer_hash.clone(), observer_wasm)
        .expect("register observer artifact");
    activate_module_manifest_for_test(
        &mut world,
        ModuleManifest {
            module_id: "m4.product.validation.observer".to_string(),
            name: "ProductValidationDeliveryObserver".to_string(),
            version: "0.1.0".to_string(),
            kind: ModuleKind::Pure,
            role: ModuleRole::Domain,
            wasm_hash: observer_hash.clone(),
            interface_version: "wasm-1".to_string(),
            abi_contract: ModuleAbiContract::default(),
            exports: vec!["call".to_string()],
            subscriptions: vec![ModuleSubscription {
                event_kinds: vec![
                    "domain.economy.product_validation_recorded".to_string(),
                    "domain.economy.product_validated".to_string(),
                ],
                action_kinds: Vec::new(),
                stage: Some(ModuleSubscriptionStage::PostEvent),
                filters: None,
            }],
            required_caps: Vec::new(),
            artifact_identity: Some(signed_test_artifact_identity(observer_hash.as_str())),
            limits: ModuleLimits {
                max_mem_bytes: 1024 * 1024,
                max_gas: 1_000_000,
                max_call_rate: 1024,
                max_output_bytes: 1024 * 1024,
                max_effects: 0,
                max_emits: 0,
            },
        },
    );
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.recipe.delivery-checkpoint".to_string(),
        recipe_id: "recipe.assembler.logistics_drone".to_string(),
        plan: RecipeExecutionPlan::accepted(
            1,
            vec![
                MaterialStack::new("motor_mk1", 2),
                MaterialStack::new("control_chip", 1),
                MaterialStack::new("chassis_plate", 1),
            ],
            vec![
                MaterialStack::new("logistics_drone", 1),
                MaterialStack::new("logistics_drone", 1),
            ],
            Vec::new(),
            10,
            1,
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("start multi-output recipe");
    let pending = world
        .state()
        .pending_recipe_jobs
        .values()
        .next()
        .expect("pending multi-output recipe")
        .clone();
    let first_stack = pending.produce[0].clone();
    let second_stack = pending.produce[1].clone();
    let first_receipt = ProductValidationReceiptV1 {
        job_id: pending.job_id,
        validation_index: Some(0),
        requester_agent_id: pending.requester_agent_id.clone(),
        module_id: "m4.product.logistics_drone".to_string(),
        stack: first_stack.clone(),
        decision: ProductValidationDecision::accepted(
            "logistics_drone",
            32,
            true,
            vec!["fleet_grade".to_string()],
        ),
        failure_detail: None,
    };
    let mut journal = world.journal().clone();
    let mut next_id = journal.events.last().map_or(1, |event| event.id + 1);
    let first_recorded_id = next_id;
    journal.append(WorldEvent {
        id: next_id,
        time: world.state().time,
        caused_by: None,
        body: WorldEventBody::Domain(DomainEvent::ProductValidationRecorded {
            receipt: first_receipt.clone(),
        }),
    });
    next_id += 1;
    let first_validated_id = next_id;
    journal.append(WorldEvent {
        id: next_id,
        time: world.state().time,
        caused_by: None,
        body: WorldEventBody::Domain(DomainEvent::ProductValidated {
            requester_agent_id: first_receipt.requester_agent_id.clone(),
            module_id: first_receipt.module_id.clone(),
            stack: first_stack,
            stack_limit: first_receipt.decision.stack_limit,
            tradable: first_receipt.decision.tradable,
            quality_levels: first_receipt.decision.quality_levels.clone(),
            notes: first_receipt.decision.notes.clone(),
        }),
    });
    next_id += 1;
    journal.append(WorldEvent {
        id: next_id,
        time: world.state().time,
        caused_by: None,
        body: WorldEventBody::Domain(DomainEvent::ProductValidationAttemptStarted {
            attempt: ProductValidationAttemptV1 {
                job_id: pending.job_id,
                validation_index: Some(1),
                requester_agent_id: pending.requester_agent_id.clone(),
                module_id: "m4.product.logistics_drone".to_string(),
                stack: second_stack,
            },
        }),
    });
    world =
        World::from_snapshot(world.snapshot(), journal).expect("recover next-output checkpoint");
    world
        .register_module_artifact(observer_hash, observer_wasm)
        .expect("restore observer artifact after recovery");

    let product_validated_before = world
        .journal()
        .events
        .iter()
        .filter(|event| {
            matches!(
                &event.body,
                WorldEventBody::Domain(DomainEvent::ProductValidated { .. })
            )
        })
        .count();
    let mut sandbox = CaptureContextSandbox::with_outputs(Vec::new());
    world
        .step_with_modules(&mut sandbox)
        .expect("recover and route first output before second intent");

    let observed_ids: Vec<_> = sandbox
        .requests
        .iter()
        .filter(|request| request.module_id == "m4.product.validation.observer")
        .map(|request| {
            let input: ModuleCallInput =
                serde_cbor::from_slice(&request.input).expect("decode observer input");
            serde_cbor::from_slice::<WorldEvent>(
                input.event.as_deref().expect("observer event payload"),
            )
            .expect("decode observer event")
            .id
        })
        .collect();
    assert!(
        observed_ids.starts_with(&[first_recorded_id, first_validated_id]),
        "recovery must replay the original journal event IDs in order before routing newly produced events: {observed_ids:?}"
    );
    assert_eq!(
        observed_ids
            .iter()
            .filter(|event_id| **event_id == first_recorded_id)
            .count(),
        1,
        "the journaled receipt must be replayed exactly once"
    );
    assert_eq!(
        observed_ids
            .iter()
            .filter(|event_id| **event_id == first_validated_id)
            .count(),
        1,
        "the journaled validation delivery must be replayed exactly once"
    );
    assert_eq!(
        sandbox
            .requests
            .iter()
            .filter(|request| request.module_id == "m4.product.logistics_drone")
            .count(),
        0,
        "receipt recovery must not reinvoke the validator module"
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
        product_validated_before,
        "replaying a journaled delivery must not append a duplicate"
    );
}

#[test]
fn product_validation_attempt_is_emitted_to_subscribers_before_decision() {
    let mut world = logistics_drone_module_recipe_world("factory.recipe.attempt-delivery");
    let observer_wasm = b"product-validation-attempt-observer";
    let observer_hash = crate::util::sha256_hex(observer_wasm);
    world
        .register_module_artifact(observer_hash.clone(), observer_wasm)
        .expect("register observer artifact");
    activate_module_manifest_for_test(
        &mut world,
        ModuleManifest {
            module_id: "m4.product.validation.attempt-observer".to_string(),
            name: "ProductValidationAttemptObserver".to_string(),
            version: "0.1.0".to_string(),
            kind: ModuleKind::Pure,
            role: ModuleRole::Domain,
            wasm_hash: observer_hash.clone(),
            interface_version: "wasm-1".to_string(),
            abi_contract: ModuleAbiContract::default(),
            exports: vec!["call".to_string()],
            subscriptions: vec![ModuleSubscription {
                event_kinds: vec!["domain.economy.product_validation_attempt_started".to_string()],
                action_kinds: Vec::new(),
                stage: Some(ModuleSubscriptionStage::PostEvent),
                filters: None,
            }],
            required_caps: Vec::new(),
            artifact_identity: Some(signed_test_artifact_identity(observer_hash.as_str())),
            limits: ModuleLimits {
                max_mem_bytes: 1024 * 1024,
                max_gas: 1_000_000,
                max_call_rate: 1024,
                max_output_bytes: 1024 * 1024,
                max_effects: 0,
                max_emits: 0,
            },
        },
    );
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.recipe.attempt-delivery".to_string(),
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
    let mut sandbox = CaptureContextSandbox::with_outputs(vec![
        ModuleOutput {
            new_state: None,
            effects: Vec::new(),
            emits: Vec::new(),
            tick_lifecycle: None,
            output_bytes: 0,
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
                .expect("serialize product decision"),
            }],
            tick_lifecycle: None,
            output_bytes: 256,
        },
    ]);
    for _ in 0..4 {
        world
            .step_with_modules(&mut sandbox)
            .expect("advance recipe validation");
        if world.pending_recipe_jobs_len() == 0 {
            break;
        }
    }
    let observer_request = sandbox
        .requests
        .iter()
        .find(|request| request.module_id == "m4.product.validation.attempt-observer")
        .expect("attempt event must be delivered to subscriber");
    let input: ModuleCallInput =
        serde_cbor::from_slice(&observer_request.input).expect("decode observer input");
    let event: WorldEvent =
        serde_cbor::from_slice(input.event.as_deref().expect("observer event payload"))
            .expect("decode observer event");
    assert!(matches!(
        event.body,
        WorldEventBody::Domain(DomainEvent::ProductValidationAttemptStarted { .. })
    ));
}

#[test]
fn product_validation_checkpoint_routes_batch_before_persist_and_recovery_dedupes() {
    let mut world = logistics_drone_module_recipe_world("factory.recipe.delivery-atomicity");
    let observer_wasm = b"product-validation-checkpoint-observer";
    let observer_hash = crate::util::sha256_hex(observer_wasm);
    world
        .register_module_artifact(observer_hash.clone(), observer_wasm)
        .expect("register observer artifact");
    activate_module_manifest_for_test(
        &mut world,
        ModuleManifest {
            module_id: "m4.product.validation.checkpoint-observer".to_string(),
            name: "ProductValidationCheckpointObserver".to_string(),
            version: "0.1.0".to_string(),
            kind: ModuleKind::Pure,
            role: ModuleRole::Domain,
            wasm_hash: observer_hash.clone(),
            interface_version: "wasm-1".to_string(),
            abi_contract: ModuleAbiContract::default(),
            exports: vec!["call".to_string()],
            subscriptions: vec![ModuleSubscription {
                event_kinds: vec![
                    "domain.economy.product_validation_attempt_started".to_string(),
                    "domain.economy.product_validation_recorded".to_string(),
                    "domain.economy.product_validated".to_string(),
                ],
                action_kinds: Vec::new(),
                stage: Some(ModuleSubscriptionStage::PostEvent),
                filters: None,
            }],
            required_caps: Vec::new(),
            artifact_identity: Some(signed_test_artifact_identity(observer_hash.as_str())),
            limits: ModuleLimits {
                max_mem_bytes: 1024 * 1024,
                max_gas: 1_000_000,
                max_call_rate: 1024,
                max_output_bytes: 1024 * 1024,
                max_effects: 0,
                max_emits: 0,
            },
        },
    );
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.recipe.delivery-atomicity".to_string(),
        recipe_id: "recipe.assembler.logistics_drone".to_string(),
        plan: RecipeExecutionPlan::accepted(
            1,
            vec![
                MaterialStack::new("motor_mk1", 2),
                MaterialStack::new("control_chip", 1),
                MaterialStack::new("chassis_plate", 1),
            ],
            vec![
                MaterialStack::new("logistics_drone", 1),
                MaterialStack::new("logistics_drone", 1),
            ],
            Vec::new(),
            10,
            1,
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("start multi-output recipe");
    let pending = world
        .state()
        .pending_recipe_jobs
        .values()
        .next()
        .expect("pending multi-output recipe")
        .clone();
    let first_receipt = ProductValidationReceiptV1 {
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
    };
    let mut journal = world.journal().clone();
    let mut next_id = journal.events.last().map_or(1, |event| event.id + 1);
    let first_recorded_id = append_product_validation_event(
        &mut journal,
        world.state().time,
        &mut next_id,
        DomainEvent::ProductValidationRecorded {
            receipt: first_receipt.clone(),
        },
    );
    let first_validated_id = append_product_validation_event(
        &mut journal,
        world.state().time,
        &mut next_id,
        DomainEvent::ProductValidated {
            requester_agent_id: first_receipt.requester_agent_id.clone(),
            module_id: first_receipt.module_id.clone(),
            stack: first_receipt.stack.clone(),
            stack_limit: first_receipt.decision.stack_limit,
            tradable: first_receipt.decision.tradable,
            quality_levels: first_receipt.decision.quality_levels.clone(),
            notes: first_receipt.decision.notes.clone(),
        },
    );
    world = World::from_snapshot(world.snapshot(), journal).expect("recover first output");
    world
        .register_module_artifact(observer_hash.clone(), observer_wasm)
        .expect("restore observer artifact");

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
    let error = world
        .step_with_modules_for_committed_context_with_product_validation_checkpoint(
            &mut sandbox,
            &context,
            &mut |staged| {
                checkpoint = Some(staged.clone());
                Err(WorldError::DistributedValidationFailed {
                    reason: "simulated crash immediately after next-output intent checkpoint"
                        .to_string(),
                })
            },
        )
        .expect_err("simulated checkpoint crash");
    assert!(matches!(
        error,
        WorldError::DistributedValidationFailed { .. }
    ));
    let checkpoint = checkpoint.expect("checkpoint must capture staged batch");
    let delivered_ids: Vec<_> = sandbox
        .requests
        .iter()
        .filter(|request| request.module_id == "m4.product.validation.checkpoint-observer")
        .map(|request| {
            let input: ModuleCallInput =
                serde_cbor::from_slice(&request.input).expect("decode observer input");
            serde_cbor::from_slice::<WorldEvent>(
                input.event.as_deref().expect("observer event payload"),
            )
            .expect("decode observer event")
            .id
        })
        .collect();
    let second_attempt_id = checkpoint
        .journal()
        .events
        .iter()
        .find_map(|event| {
            matches!(
                &event.body,
                WorldEventBody::Domain(DomainEvent::ProductValidationAttemptStarted {
                    attempt,
                }) if attempt.validation_index == Some(1)
            )
            .then_some(event.id)
        })
        .expect("checkpoint must contain next-output attempt");
    assert_eq!(
        checkpoint
            .state()
            .product_validation_delivery_cursor
            .routed_through_event_id,
        second_attempt_id,
        "checkpoint must persist the completed validation delivery cursor"
    );
    assert_eq!(
        delivered_ids,
        vec![first_recorded_id, first_validated_id, second_attempt_id],
        "all earlier batch events and the new attempt must route before persistence"
    );

    let mut recovered = World::from_snapshot(checkpoint.snapshot(), checkpoint.journal().clone())
        .expect("recover immediately after checkpoint");
    recovered
        .register_module_artifact(observer_hash, observer_wasm)
        .expect("restore observer artifact for retry");
    let mut retry_sandbox = CaptureContextSandbox::with_outputs(Vec::new());
    recovered
        .step_with_modules(&mut retry_sandbox)
        .expect("prior attempt must fail closed after recovery");
    assert!(
        recovered
            .state()
            .product_validation_delivery_cursor
            .routed_through_event_id
            >= second_attempt_id,
        "recovery must not regress the durable validation delivery cursor"
    );
    let duplicate_ids: Vec<_> = retry_sandbox
        .requests
        .iter()
        .filter(|request| request.module_id == "m4.product.validation.checkpoint-observer")
        .filter_map(|request| {
            let input: ModuleCallInput = serde_cbor::from_slice(&request.input).ok()?;
            let event: WorldEvent = serde_cbor::from_slice(input.event.as_deref()?).ok()?;
            Some(event.id)
        })
        .filter(|id| *id == first_recorded_id || *id == first_validated_id)
        .collect();
    assert!(
        duplicate_ids.is_empty(),
        "events delivered before checkpoint must not be replayed after recovery"
    );
}

#[test]
fn product_validation_receipt_with_existing_delivery_is_not_replayed() {
    let mut world = logistics_drone_module_recipe_world("factory.recipe.delivery-replay");
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.recipe.delivery-replay".to_string(),
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
    let receipt = ProductValidationReceiptV1 {
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
    };
    let mut journal = world.journal().clone();
    let next_id = journal.events.last().map_or(1, |event| event.id + 1);
    journal.append(WorldEvent {
        id: next_id,
        time: world.state().time,
        caused_by: None,
        body: WorldEventBody::Domain(DomainEvent::ProductValidationRecorded {
            receipt: receipt.clone(),
        }),
    });
    journal.append(WorldEvent {
        id: next_id + 1,
        time: world.state().time,
        caused_by: None,
        body: WorldEventBody::Domain(DomainEvent::ProductValidated {
            requester_agent_id: receipt.requester_agent_id.clone(),
            module_id: receipt.module_id.clone(),
            stack: receipt.stack.clone(),
            stack_limit: receipt.decision.stack_limit,
            tradable: receipt.decision.tradable,
            quality_levels: receipt.decision.quality_levels.clone(),
            notes: receipt.decision.notes.clone(),
        }),
    });
    world = World::from_snapshot(world.snapshot(), journal).expect("recover delivered receipt");
    let before = world
        .journal()
        .events
        .iter()
        .filter(|event| {
            matches!(
                &event.body,
                WorldEventBody::Domain(DomainEvent::ProductValidated { .. })
            )
        })
        .count();
    let mut sandbox = CaptureContextSandbox::with_outputs(Vec::new());
    world
        .step_with_modules(&mut sandbox)
        .expect("settle receipt");
    assert!(sandbox.requests.is_empty());
    assert_eq!(
        world
            .journal()
            .events
            .iter()
            .filter(|event| matches!(
                &event.body,
                WorldEventBody::Domain(DomainEvent::ProductValidated { .. })
            ))
            .count(),
        before,
        "an already delivered validation must not be replayed"
    );
}
