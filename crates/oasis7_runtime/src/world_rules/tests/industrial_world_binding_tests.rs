use super::*;

fn bound_event(request: &str, command: WorldRuleCommandV1, binding: &str) -> DomainEvent {
    let mut event = event("owner", request, command);
    if let DomainEvent::WorldRuleCommitted { world_binding, .. } = &mut event {
        *world_binding = Some(binding.into());
    }
    event
}

fn bound_reservation(state: &mut WorldState) -> String {
    let quote = state
        .agent_manufacturing_quote_with_binding("owner", "assembly", 10, Some("world-a"))
        .unwrap();
    apply(
        state,
        &bound_event(
            "bound-order",
            WorldRuleCommandV1::ConfirmAgentOrder {
                factory_id: "assembly".into(),
                quoted_at: 10,
                quote_digest: quote.quote_digest,
            },
            "world-a",
        ),
        10,
    );
    request_key("owner", "bound-order")
}

#[test]
fn manufacturing_quote_commits_world_and_confirmation_cannot_rebind_it() {
    let state = industrial_fixture();
    let local = state
        .agent_manufacturing_quote("owner", "assembly", 10)
        .unwrap();
    let a = state
        .agent_manufacturing_quote_with_binding("owner", "assembly", 10, Some("world-a"))
        .unwrap();
    let b = state
        .agent_manufacturing_quote_with_binding("owner", "assembly", 10, Some("world-b"))
        .unwrap();
    assert_ne!(local.quote_digest, a.quote_digest);
    assert_ne!(a.quote_digest, b.quote_digest);
    let before = serde_json::to_value(&state).unwrap();
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &bound_event(
                "wrong-world",
                WorldRuleCommandV1::ConfirmAgentOrder {
                    factory_id: "assembly".into(),
                    quoted_at: 10,
                    quote_digest: a.quote_digest,
                },
                "world-b"
            ),
            10
        )
        .is_err()
    );
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
}

#[test]
fn every_order_effect_and_frozen_refund_remains_in_original_world() {
    let mut state = industrial_fixture();
    let id = bound_reservation(&mut state);
    for command in [
        WorldRuleCommandV1::AdvanceAgentOrder {
            order_id: id.clone(),
        },
        WorldRuleCommandV1::CancelAgentOrder {
            order_id: id.clone(),
        },
        WorldRuleCommandV1::DisposeAgentOrder {
            order_id: id.clone(),
        },
        WorldRuleCommandV1::DeliverAgentOrder {
            order_id: id.clone(),
        },
    ] {
        let before = serde_json::to_value(&state).unwrap();
        assert!(
            PreparedWorldRule::prepare(&state, &bound_event("cross-world", command, "world-b"), 11)
                .is_err()
        );
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }
    state.starter_oc_claims.get_mut("owner").unwrap().player_id = "new-player".into();
    let mut timeout = bound_event(
        "timeout",
        WorldRuleCommandV1::AdvanceAgentOrder {
            order_id: id.clone(),
        },
        "world-a",
    );
    if let DomainEvent::WorldRuleCommitted { action_id, .. } = &mut timeout {
        *action_id = 0;
    }
    apply(&mut state, &timeout, 20);
    assert!(order(&state, &id).refund_pending);
    state.starter_oc_claims.get_mut("owner").unwrap().player_id = "player".into();
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &bound_event(
                "refund-b",
                WorldRuleCommandV1::AdvanceAgentOrder {
                    order_id: id.clone()
                },
                "world-b"
            ),
            21
        )
        .is_err()
    );
    apply(
        &mut state,
        &bound_event(
            "refund-a",
            WorldRuleCommandV1::AdvanceAgentOrder {
                order_id: id.clone(),
            },
            "world-a",
        ),
        21,
    );
    assert!(!order(&state, &id).refund_pending);
}

#[test]
fn private_quote_visibility_requires_frozen_world_and_principal() {
    let mut state = industrial_fixture();
    let id = bound_reservation(&mut state);
    assert!(state.can_read_agent_industrial_order(
        order(&state, &id),
        Some("owner"),
        Some("world-a")
    ));
    assert!(!state.can_read_agent_industrial_order(
        order(&state, &id),
        Some("owner"),
        Some("world-b")
    ));
    state.starter_oc_claims.get_mut("owner").unwrap().player_id = "new-player".into();
    assert!(!state.can_read_agent_industrial_order(
        order(&state, &id),
        Some("owner"),
        Some("world-a")
    ));
}

#[test]
fn world_root_identity_conflict_blocks_delivery_then_freezes_expired_refund_until_restored() {
    let mut state = industrial_fixture();
    let id = reserve(&mut state);
    produce(&mut state, &id);
    state.time = 24;
    let target = order(&state, &id).target_agent_id.clone();
    let mut world = World::new_with_state(state);
    world
        .install_capability_agent_identity("owner", "player", 1)
        .unwrap();
    let before_conflict = world.snapshot();
    world
        .install_capability_agent_identity("owner", "other-player", 2)
        .unwrap();
    let liquid = world.state().main_token_balances["owner"].liquid_balance;
    assert!(!world.can_read_agent_industrial_order(order(world.state(), &id), Some("owner")));
    assert_eq!(
        world.world_rule_status(Some("owner")).unwrap()["manufacturing_quotes"],
        serde_json::json!({})
    );
    assert_eq!(
        world
            .industrial_order_blocker(order(world.state(), &id), 24)
            .as_deref(),
        Some("manufacturing_owner_identity_changed")
    );
    assert!(
        world
            .prepare_world_rule_event(
                &event(
                    "owner",
                    "conflicting-delivery",
                    WorldRuleCommandV1::DeliverAgentOrder {
                        order_id: id.clone()
                    }
                ),
                24
            )
            .is_err()
    );
    assert!(
        world
            .prepare_world_rule_event(
                &event(
                    "owner",
                    "conflicting-dispose",
                    WorldRuleCommandV1::DisposeAgentOrder {
                        order_id: id.clone()
                    }
                ),
                24
            )
            .is_err()
    );
    while world.state().time < 47 {
        world.step().unwrap();
    }
    assert!(!world.state().agents.contains_key(&target));
    assert_eq!(order(world.state(), &id).phase, P::Disposed);
    assert!(order(world.state(), &id).refund_pending);
    assert_eq!(
        world.state().main_token_balances["owner"].liquid_balance,
        liquid
    );
    assert!(
        !world
            .state()
            .gameplay_policy
            .blocked_agents
            .contains(&"owner".into())
    );
    let replayed = World::from_snapshot(before_conflict, world.journal().clone()).unwrap();
    assert_eq!(
        serde_json::to_value(replayed.state()).unwrap(),
        serde_json::to_value(world.state()).unwrap()
    );
    world
        .install_capability_agent_identity("owner", "player", 3)
        .unwrap();
    world.step().unwrap();
    assert!(!order(world.state(), &id).refund_pending);
    assert_eq!(
        world.state().main_token_balances["owner"].liquid_balance,
        liquid + order(world.state(), &id).quote.upfront_liquid
    );
    let refunded = world.state().main_token_balances["owner"].liquid_balance;
    world.step().unwrap();
    assert_eq!(
        world.state().main_token_balances["owner"].liquid_balance,
        refunded
    );
}

#[test]
fn world_root_same_player_generation_rotation_preserves_delivery() {
    let mut state = industrial_fixture();
    let id = reserve(&mut state);
    produce(&mut state, &id);
    state.time = 24;
    let target = order(&state, &id).target_agent_id.clone();
    let mut world = World::new_with_state(state);
    world
        .install_capability_agent_identity("owner", "player", 1)
        .unwrap();
    world
        .install_capability_agent_identity("owner", "player", 2)
        .unwrap();
    world.step().unwrap();
    assert_eq!(order(world.state(), &id).phase, P::Delivered);
    assert_eq!(
        world.current_agent_identity(&target).unwrap().owner_binding,
        "player"
    );
}
