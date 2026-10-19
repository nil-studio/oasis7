use super::*;

#[test]
fn industrial_order_atomic_delivery_and_replay() {
    let mut state = fixture();
    let confirm = event(
        "owner",
        "order",
        WorldRuleCommandV1::ConfirmAgentOrder {
            factory_id: "assembly".into(),
        },
    );
    apply(&mut state, &confirm, 10);
    let key = request_key("owner", "order");
    let order = state.world_rules.as_ref().unwrap().agent_orders[&key].clone();
    assert!(!state.agents.contains_key(&order.target_agent_id));
    assert_eq!(
        state.material_ledgers[&MaterialLedgerId::agent("owner")]["alloy_plate"],
        108
    );
    let delivery = event(
        "owner",
        "deliver",
        WorldRuleCommandV1::DeliverAgentOrder {
            order_id: key.clone(),
        },
    );
    assert!(PreparedWorldRule::prepare(&state, &delivery, 129).is_err());
    let mut unfunded = state.clone();
    unfunded
        .main_token_balances
        .insert("owner".into(), MainTokenAccountBalance::default());
    assert!(PreparedWorldRule::prepare(&unfunded, &delivery, 130).is_err());
    apply(&mut state, &delivery, 130);
    assert_eq!(
        state.agent_claims[&order.target_agent_id].claim_owner_id,
        "owner"
    );
    let balance = state.main_token_balances["owner"].liquid_balance;
    apply(&mut state, &delivery, 140);
    assert_eq!(state.main_token_balances["owner"].liquid_balance, balance);
    let restored: WorldState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    assert_eq!(state, restored);
    let another = event(
        "owner",
        "deliver-again",
        WorldRuleCommandV1::DeliverAgentOrder { order_id: key },
    );
    assert!(PreparedWorldRule::prepare(&restored, &another, 140).is_err());
}

#[test]
fn industrial_resource_capacity_and_identity_fail_closed() {
    let mut state = fixture();
    state
        .material_ledgers
        .get_mut(&MaterialLedgerId::agent("owner"))
        .unwrap()
        .clear();
    let confirm = event(
        "owner",
        "order",
        WorldRuleCommandV1::ConfirmAgentOrder {
            factory_id: "assembly".into(),
        },
    );
    let before = state.clone();
    assert!(PreparedWorldRule::prepare(&state, &confirm, 10).is_err());
    assert_eq!(state, before);
    let mut state = fixture();
    apply(&mut state, &confirm, 10);
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event(
                "owner",
                "other",
                WorldRuleCommandV1::ConfirmAgentOrder {
                    factory_id: "assembly".into()
                }
            ),
            11
        )
        .is_err()
    );
    let target = state
        .world_rules
        .as_ref()
        .unwrap()
        .agent_orders
        .values()
        .next()
        .unwrap()
        .target_agent_id
        .clone();
    assert!(
        state
            .apply_domain_event(
                &DomainEvent::AgentRegistered {
                    agent_id: target,
                    pos: GeoPos::new(0, 0, 0)
                },
                11
            )
            .is_err()
    );
}

#[test]
fn public_action_path_delivers_only_at_authoritative_tick() {
    let mut state = fixture();
    state.time = 10;
    let mut world = World::new_with_state(state);
    world.submit_action(Action::WorldRule {
        actor_id: "owner".into(),
        request_id: "order".into(),
        command: WorldRuleCommandV1::ConfirmAgentOrder {
            factory_id: "assembly".into(),
        },
    });
    world.step().unwrap();
    let order = world
        .state()
        .world_rules
        .as_ref()
        .unwrap()
        .agent_orders
        .values()
        .next()
        .unwrap()
        .clone();
    assert!(!world.state().agents.contains_key(&order.target_agent_id));
    for _ in 0..120 {
        world.step().unwrap();
    }
    assert!(world.state().agents.contains_key(&order.target_agent_id));
    assert_eq!(
        world.state().agent_claims[&order.target_agent_id].claim_owner_id,
        "owner"
    );
    world.verify_tick_consensus_chain().unwrap();
}

#[test]
fn industrial_delivery_uses_canonical_restricted_funding_if_primary_was_released() {
    let mut state = fixture();
    apply(
        &mut state,
        &event(
            "owner",
            "order",
            WorldRuleCommandV1::ConfirmAgentOrder {
                factory_id: "assembly".into(),
            },
        ),
        10,
    );
    // Simulate the already-settled release before delivery; this fixture is
    // about the delivery funding split, not the separately tested release.
    state.agent_claims.clear();
    state
        .main_token_balances
        .get_mut("owner")
        .unwrap()
        .restricted_starter_claim_balance = 100;
    state.main_token_supply.total_supply += 100;
    state.main_token_supply.circulating_supply += 100;
    let order = request_key("owner", "order");
    let target = state.world_rules.as_ref().unwrap().agent_orders[&order]
        .target_agent_id
        .clone();
    apply(
        &mut state,
        &event(
            "owner",
            "delivery",
            WorldRuleCommandV1::DeliverAgentOrder { order_id: order },
        ),
        130,
    );
    assert_eq!(
        state.agent_claims[&target].upfront_restricted_spent_amount,
        100
    );
}

#[test]
fn extra_registered_identity_cannot_bypass_industrial_supply() {
    let mut world = World::new_with_state(fixture());
    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "owner".into(),
        target_agent_id: "recipient".into(),
    });
    world.step().unwrap();
    assert!(!world.state().agent_claims.contains_key("recipient"));
    assert!(world.journal().events.iter().any(|e|matches!(&e.body,crate::WorldEventBody::Domain(DomainEvent::ActionRejected{reason:crate::RejectReason::RuleDenied{notes},..}) if notes.iter().any(|s|s.contains("industrial_delivery")))));
}

#[test]
fn maximum_length_request_keeps_delivered_identity_within_bound() {
    let mut state = fixture();
    let request = "r".repeat(128);
    apply(
        &mut state,
        &event(
            "owner",
            &request,
            WorldRuleCommandV1::ConfirmAgentOrder {
                factory_id: "assembly".into(),
            },
        ),
        10,
    );
    assert!(
        state
            .world_rules
            .as_ref()
            .unwrap()
            .agent_orders
            .values()
            .next()
            .unwrap()
            .target_agent_id
            .len()
            <= 128
    );
}
