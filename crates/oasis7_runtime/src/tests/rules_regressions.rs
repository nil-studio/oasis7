#[test]
fn transfer_requires_rule_module() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-2".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    world.submit_action(Action::TransferResource {
        from_agent_id: "agent-1".to_string(),
        to_agent_id: "agent-2".to_string(),
        kind: ResourceKind::Electricity,
        amount: 5,
    });
    world.step().unwrap();

    let last = world.journal().events.last().unwrap();
    match &last.body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => {
            assert!(matches!(reason, RejectReason::RuleDenied { .. }));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn body_action_requires_body_module() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    let view = BodyKernelView {
        mass_kg: 120,
        radius_cm: 80,
        thrust_limit: 200,
        cross_section_cm2: 4000,
    };

    world.submit_action(Action::BodyAction {
        agent_id: "agent-1".to_string(),
        kind: "boot".to_string(),
        payload: serde_json::to_value(view).unwrap(),
    });
    world.step().unwrap();

    let last = world.journal().events.last().unwrap();
    match &last.body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => {
            assert!(matches!(reason, RejectReason::RuleDenied { .. }));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn rule_module_order_is_deterministic() {
    let mut world = World::new();
    install_rule_modules(&mut world, "action.move_agent", &["b.rule", "a.rule"]);

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    let action_id = world.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: pos(1, 0),
    });

    let mut outputs = BTreeMap::new();
    outputs.insert(
        "a.rule".to_string(),
        rule_decision_output(rule_decision_with_notes(
            action_id,
            RuleVerdict::Allow,
            None,
            &["a"],
        )),
    );
    outputs.insert(
        "b.rule".to_string(),
        rule_decision_output(rule_decision_with_notes(
            action_id,
            RuleVerdict::Allow,
            None,
            &["b"],
        )),
    );
    let mut sandbox = MapSandbox::new(outputs);
    world.step_with_modules(&mut sandbox).unwrap();

    let records: Vec<_> = world
        .journal()
        .events
        .iter()
        .filter_map(|event| match &event.body {
            WorldEventBody::RuleDecisionRecorded(record) if record.action_id == action_id => {
                Some(record)
            }
            _ => None,
        })
        .collect();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].module_id, "a.rule");
    assert_eq!(records[1].module_id, "b.rule");
}

#[test]
fn rule_conflicting_overrides_rejects_action() {
    let mut world = World::new();
    install_rule_modules(&mut world, "action.move_agent", &["a.rule", "b.rule"]);

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    let action_id = world.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: pos(1, 0),
    });

    let override_a = Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: pos(2, 0),
    };
    let override_b = Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: pos(3, 0),
    };

    let mut outputs = BTreeMap::new();
    outputs.insert(
        "a.rule".to_string(),
        rule_decision_output(rule_decision_with_notes(
            action_id,
            RuleVerdict::Modify,
            Some(override_a),
            &["a"],
        )),
    );
    outputs.insert(
        "b.rule".to_string(),
        rule_decision_output(rule_decision_with_notes(
            action_id,
            RuleVerdict::Modify,
            Some(override_b),
            &["b"],
        )),
    );
    let mut sandbox = MapSandbox::new(outputs);
    world.step_with_modules(&mut sandbox).unwrap();

    let last = world.journal().events.last().unwrap();
    match &last.body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => match reason {
            RejectReason::RuleDenied { notes } => {
                assert!(notes
                    .iter()
                    .any(|note| note.contains("conflicting override")));
            }
            other => panic!("unexpected reject reason: {other:?}"),
        },
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn rule_deny_overrides_modify() {
    let mut world = World::new();
    install_rule_modules(&mut world, "action.move_agent", &["a.rule", "b.rule"]);

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    let action_id = world.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: pos(1, 0),
    });

    let override_action = Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: pos(2, 0),
    };

    let mut outputs = BTreeMap::new();
    outputs.insert(
        "a.rule".to_string(),
        rule_decision_output(rule_decision_with_notes(
            action_id,
            RuleVerdict::Modify,
            Some(override_action),
            &["modify"],
        )),
    );
    outputs.insert(
        "b.rule".to_string(),
        rule_decision_output(rule_decision_with_notes(
            action_id,
            RuleVerdict::Deny,
            None,
            &["deny"],
        )),
    );
    let mut sandbox = MapSandbox::new(outputs);
    world.step_with_modules(&mut sandbox).unwrap();

    let last = world.journal().events.last().unwrap();
    match &last.body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => match reason {
            RejectReason::RuleDenied { notes } => {
                assert!(notes.iter().any(|note| note.contains("deny")));
            }
            other => panic!("unexpected reject reason: {other:?}"),
        },
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn rule_same_override_is_applied() {
    let mut world = World::new();
    install_rule_modules(&mut world, "action.move_agent", &["a.rule", "b.rule"]);

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    let action_id = world.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: pos(1, 0),
    });

    let override_action = Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: pos(4, 0),
    };

    let mut outputs = BTreeMap::new();
    outputs.insert(
        "a.rule".to_string(),
        rule_decision_output(rule_decision_with_notes(
            action_id,
            RuleVerdict::Modify,
            Some(override_action.clone()),
            &["a"],
        )),
    );
    outputs.insert(
        "b.rule".to_string(),
        rule_decision_output(rule_decision_with_notes(
            action_id,
            RuleVerdict::Modify,
            Some(override_action),
            &["b"],
        )),
    );
    let mut sandbox = MapSandbox::new(outputs);
    world.step_with_modules(&mut sandbox).unwrap();

    let agent = world.state().agents.get("agent-1").unwrap();
    assert_eq!(agent.state.pos, pos(4, 0));
}
