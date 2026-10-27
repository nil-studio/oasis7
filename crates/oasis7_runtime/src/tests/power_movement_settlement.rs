// Real WASM/bootstrap and governed gameplay admission share this fixture.
fn movement_world() -> (World, WasmExecutor) {
    let mut world = World::new();
    super::rules::install_m1_move_rule(&mut world);
    world
        .install_m1_power_bootstrap_modules("bootstrap")
        .expect("power bootstrap");
    let mut sandbox = power_module_sandbox();
    world.submit_action(Action::RegisterAgent {
        agent_id: "a".into(),
        pos: pos(0, 0),
    });
    world
        .step_with_modules(&mut sandbox)
        .expect("register through modules");
    world
        .set_agent_resource_balance("a", ResourceKind::Electricity, 1000)
        .unwrap();
    world
        .set_agent_resource_balance("a", ResourceKind::Data, 1000)
        .unwrap();
    (world, sandbox)
}

fn battery(world: &World) -> (serde_json::Value, i64) {
    let value: serde_json::Value =
        serde_cbor::from_slice(&world.state().module_states[M1_STORAGE_POWER_MODULE_ID]).unwrap();
    (
        value["agents"]["a"]["pos"].clone(),
        value["agents"]["a"]["level"].as_i64().unwrap(),
    )
}

#[test]
fn rejected_movement_keeps_origin_then_allows_alternate_route() {
    let (mut world, mut sandbox) = movement_world();
    super::gameplay_protocol::authorize_policy_update(&mut world, "a", "movement-policy");
    world.submit_action(Action::UpdateGameplayPolicy {
        operator_agent_id: "a".into(),
        electricity_tax_bps: 0,
        data_tax_bps: 0,
        power_trade_fee_bps: 0,
        max_open_contracts_per_agent: 4,
        blocked_agents: vec![],
        forbidden_location_ids: vec!["100000:0:0".into()],
    });
    world.step().expect("governed forbidden location");
    let start_time = world.state().time;
    let electricity = world
        .state()
        .resources
        .get(&ResourceKind::Electricity)
        .copied();
    let journal_start = world.journal().events.len();
    let agent_electricity = world
        .agent_resource_balance("a", ResourceKind::Electricity)
        .unwrap();
    let agent_data = world
        .agent_resource_balance("a", ResourceKind::Data)
        .unwrap();
    let rejected_id = world.submit_action(Action::MoveAgent {
        agent_id: "a".into(),
        to: pos(100_000, 0),
    });
    world
        .step_with_modules(&mut sandbox)
        .expect("domain rejection");
    assert_eq!(world.state().agents["a"].state.pos, pos(0, 0));
    assert_eq!(
        battery(&world),
        (serde_json::to_value(pos(0, 0)).unwrap(), 7)
    );
    assert_eq!(world.state().time, start_time + 1);
    assert_eq!(
        world
            .state()
            .resources
            .get(&ResourceKind::Electricity)
            .copied(),
        electricity,
        "world rule distance debit must not survive a rejected move"
    );
    assert!(world.journal().events.iter().any(|event| matches!(&event.body,
        WorldEventBody::Domain(DomainEvent::ActionRejected { action_id, .. }) if *action_id == rejected_id)));
    let (invocation_electricity, invocation_data) = world.journal().events[journal_start..]
        .iter()
        .fold((0, 0), |(electricity, data), event| match &event.body {
            WorldEventBody::ModuleRuntimeCharged(charge) if charge.payer_agent_id == "a" => (
                electricity + charge.electricity_fee_amount,
                data + charge.compute_fee_amount,
            ),
            _ => (electricity, data),
        });
    assert!(invocation_electricity > 0 && invocation_data > 0);
    assert_eq!(
        world
            .agent_resource_balance("a", ResourceKind::Electricity)
            .unwrap(),
        agent_electricity - invocation_electricity
    );
    assert_eq!(
        world
            .agent_resource_balance("a", ResourceKind::Data)
            .unwrap(),
        agent_data - invocation_data
    );
    world.submit_action(Action::MoveAgent {
        agent_id: "a".into(),
        to: pos(-100_000, 0),
    });
    world
        .step_with_modules(&mut sandbox)
        .expect("legal alternative");
    assert_eq!(world.state().agents["a"].state.pos, pos(-100_000, 0));
    assert_eq!(
        battery(&world),
        (serde_json::to_value(pos(-100_000, 0)).unwrap(), 5)
    );
    assert_eq!(
        world
            .state()
            .resources
            .get(&ResourceKind::Electricity)
            .copied(),
        electricity.map(|amount| amount - 1)
    );
    let snapshot = world.snapshot();
    let restored = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
    assert_eq!(battery(&restored), battery(&world));
}

#[test]
fn storage_activation_rejects_unmigrated_historical_state_without_reset() {
    let (world, _) = movement_world();
    let mut snapshot = world.snapshot();
    let legacy = serde_json::json!({"agents": {"a": {"pos": pos(100_000, 0), "level": 4}}});
    snapshot.state.module_states.insert(
        M1_STORAGE_POWER_MODULE_ID.into(),
        serde_cbor::to_vec(&legacy).unwrap(),
    );
    let mut restored = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
    let before = restored.snapshot();
    let error = restored
        .install_m1_power_bootstrap_modules("bootstrap")
        .expect_err("requires explicit migration");
    assert!(format!("{error:?}").contains("explicit migration"));
    assert_eq!(restored.snapshot(), before);
}

#[test]
fn insufficient_world_movement_resources_preserve_battery_harvest_and_origin() {
    let (mut world, mut sandbox) = movement_world();
    world.set_resource_balance(ResourceKind::Electricity, 0);
    let id = world.submit_action(Action::MoveAgent {
        agent_id: "a".into(),
        to: pos(100_000, 0),
    });
    world.step_with_modules(&mut sandbox).unwrap();
    assert_eq!(
        battery(&world),
        (serde_json::to_value(pos(0, 0)).unwrap(), 7)
    );
    assert_eq!(world.state().agents["a"].state.pos, pos(0, 0));
    assert!(world.journal().events.iter().any(|event| matches!(&event.body,
        WorldEventBody::Domain(DomainEvent::ActionRejected { action_id, reason: RejectReason::InsufficientResources { .. } }) if *action_id == id)));
    world.set_resource_balance(ResourceKind::Electricity, 10);
    world.submit_action(Action::MoveAgent {
        agent_id: "a".into(),
        to: pos(-100_000, 0),
    });
    world.step_with_modules(&mut sandbox).unwrap();
    assert_eq!(
        battery(&world),
        (serde_json::to_value(pos(-100_000, 0)).unwrap(), 5)
    );
}

struct MovementDirectiveSandbox {
    inner: WasmExecutor,
    deny: bool,
    target: Option<i64>,
    non_move: bool,
}

impl ModuleSandbox for MovementDirectiveSandbox {
    fn call(
        &mut self,
        request: &oasis7_wasm_abi::ModuleCallRequest,
    ) -> Result<oasis7_wasm_abi::ModuleOutput, oasis7_wasm_abi::ModuleCallFailure> {
        if request.module_id != "movement.test.rule" {
            return self.inner.call(request);
        }
        let input: oasis7_wasm_abi::ModuleCallInput =
            serde_cbor::from_slice(&request.input).unwrap();
        let envelope: ActionEnvelope =
            serde_cbor::from_slice(input.action.as_deref().unwrap()).unwrap();
        Ok(oasis7_wasm_abi::ModuleOutput {
            new_state: None,
            effects: vec![],
            tick_lifecycle: None,
            output_bytes: 0,
            emits: vec![oasis7_wasm_abi::ModuleEmit {
                kind: "rule.decision".into(),
                payload: serde_json::to_value(RuleDecision {
                    action_id: envelope.id,
                    verdict: if self.deny {
                        RuleVerdict::Deny
                    } else if self.target.is_some() || self.non_move {
                        RuleVerdict::Modify
                    } else {
                        RuleVerdict::Allow
                    },
                    override_action: if self.non_move {
                        Some(Action::RegisterAgent {
                            agent_id: "b".into(),
                            pos: pos(0, 0),
                        })
                    } else {
                        self.target.map(|target| Action::MoveAgent {
                            agent_id: "a".into(),
                            to: pos(target, 0),
                        })
                    },
                    cost: ResourceDelta::default(),
                    notes: vec!["isolated movement rule".into()],
                })
                .unwrap(),
            }],
        })
    }
}

#[test]
fn another_rule_deny_preserves_harvest_and_modified_move_prices_final_target_once() {
    for (deny, target, expected_level, expected_x) in [
        (true, None, 7, 0),
        (false, Some(200_000), 1, 200_000),
        (false, Some(300_000), 7, 0),
    ] {
        let (mut world, inner) = movement_world();
        super::rules::install_rule_modules(
            &mut world,
            "action.move_agent",
            &["movement.test.rule"],
        );
        let mut sandbox = MovementDirectiveSandbox {
            inner,
            deny,
            target,
            non_move: false,
        };
        let initial = world.resource_balance(ResourceKind::Electricity);
        world.submit_action(Action::MoveAgent {
            agent_id: "a".into(),
            to: pos(100_000, 0),
        });
        world.step_with_modules(&mut sandbox).unwrap();
        assert_eq!(
            battery(&world),
            (
                serde_json::to_value(pos(expected_x, 0)).unwrap(),
                expected_level
            )
        );
        assert_eq!(world.state().agents["a"].state.pos, pos(expected_x, 0));
        assert_eq!(
            world.resource_balance(ResourceKind::Electricity),
            initial - if expected_x == 200_000 { 2 } else { 0 }
        );
    }
}

#[test]
fn zero_distance_has_no_settlement_but_preserves_one_harvest() {
    let (mut world, mut sandbox) = movement_world();
    let initial = world.resource_balance(ResourceKind::Electricity);
    world.submit_action(Action::MoveAgent {
        agent_id: "a".into(),
        to: pos(0, 0),
    });
    world.step_with_modules(&mut sandbox).unwrap();
    assert_eq!(
        battery(&world),
        (serde_json::to_value(pos(0, 0)).unwrap(), 7)
    );
    assert_eq!(world.resource_balance(ResourceKind::Electricity), initial);
}

#[test]
fn nonmovement_override_keeps_its_existing_domain_contract_without_distance_debit() {
    let (mut world, inner) = movement_world();
    super::rules::install_rule_modules(&mut world, "action.move_agent", &["movement.test.rule"]);
    let mut sandbox = MovementDirectiveSandbox {
        inner,
        deny: false,
        target: None,
        non_move: true,
    };
    let initial = world.resource_balance(ResourceKind::Electricity);
    world.submit_action(Action::MoveAgent {
        agent_id: "a".into(),
        to: pos(100_000, 0),
    });
    world.step_with_modules(&mut sandbox).unwrap();
    assert!(world.state().agents.contains_key("b"));
    assert_eq!(
        battery(&world),
        (serde_json::to_value(pos(0, 0)).unwrap(), 6)
    );
    assert_eq!(world.resource_balance(ResourceKind::Electricity), initial);
}

#[test]
fn adding_only_format_marker_does_not_repair_historical_position_drift() {
    let (world, _) = movement_world();
    let mut snapshot = world.snapshot();
    let drifted = serde_json::json!({"last_settled_event_id": null,
        "agents": {"a": {"pos": pos(100_000, 0), "level": 4}}});
    snapshot.state.module_states.insert(
        M1_STORAGE_POWER_MODULE_ID.into(),
        serde_cbor::to_vec(&drifted).unwrap(),
    );
    let mut restored = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
    let before = restored.snapshot();
    assert!(
        restored
            .install_m1_power_bootstrap_modules("bootstrap")
            .is_err()
    );
    assert_eq!(restored.snapshot(), before);
}
