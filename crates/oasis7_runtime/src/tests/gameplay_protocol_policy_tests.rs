use super::*;

#[test]
fn update_gameplay_policy_requires_governance_authorization() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b"]);

    world.submit_action(Action::UpdateGameplayPolicy {
        operator_agent_id: "a".to_string(),
        electricity_tax_bps: 0,
        data_tax_bps: 1_000,
        power_trade_fee_bps: 0,
        max_open_contracts_per_agent: 4,
        blocked_agents: vec!["b".to_string()],
        forbidden_location_ids: Vec::new(),
    });
    world
        .step()
        .expect("reject unauthorized gameplay policy update");

    assert_latest_rule_denied_contains(
        &world,
        "requires passed governance proposal total_weight >=",
    );
}

#[test]
fn move_agent_is_denied_when_target_location_is_forbidden() {
    let mut world = World::new();
    register_agents(&mut world, &["a"]);
    authorize_policy_update(&mut world, "a", "proposal.policy.forbidden-location");

    world.submit_action(Action::UpdateGameplayPolicy {
        operator_agent_id: "a".to_string(),
        electricity_tax_bps: 0,
        data_tax_bps: 0,
        power_trade_fee_bps: 0,
        max_open_contracts_per_agent: 4,
        blocked_agents: Vec::new(),
        forbidden_location_ids: vec!["10:0:0".to_string()],
    });
    world.step().expect("update gameplay policy");

    world.submit_action(Action::MoveAgent {
        agent_id: "a".to_string(),
        to: pos(10, 0),
    });
    world.step().expect("forbidden move should be rejected");
    assert_latest_rule_denied_contains(&world, "forbidden_location_ids");

    world.submit_action(Action::MoveAgent {
        agent_id: "a".to_string(),
        to: pos(11, 0),
    });
    world.step().expect("non-forbidden move should pass");
    match last_domain_event(&world) {
        DomainEvent::AgentMoved { agent_id, to, .. } => {
            assert_eq!(agent_id, "a");
            assert_eq!(*to, pos(11, 0));
        }
        other => panic!("expected AgentMoved, got {other:?}"),
    }
}

#[test]
fn economic_contract_electricity_settlement_applies_power_trade_fee_and_tax() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b"]);
    authorize_policy_update(&mut world, "a", "proposal.policy.power-fee");
    world
        .set_agent_resource_balance("a", ResourceKind::Electricity, 100)
        .expect("seed creator electricity");

    world.submit_action(Action::UpdateGameplayPolicy {
        operator_agent_id: "a".to_string(),
        electricity_tax_bps: 1_000,
        data_tax_bps: 0,
        power_trade_fee_bps: 500,
        max_open_contracts_per_agent: 4,
        blocked_agents: Vec::new(),
        forbidden_location_ids: Vec::new(),
    });
    world.step().expect("update gameplay policy");

    let expires_at = world.state().time.saturating_add(10);
    world.submit_action(Action::OpenEconomicContract {
        creator_agent_id: "a".to_string(),
        contract_id: "contract.electricity.fee.1".to_string(),
        counterparty_agent_id: "b".to_string(),
        fulfillment_kind: EconomicContractFulfillmentKind::AtomicExchange,
        settlement_kind: ResourceKind::Electricity,
        settlement_amount: 20,
        reputation_stake: 4,
        expires_at,
        description: "power shipment with fee".to_string(),
    });
    world.step().expect("open economic contract");
    world.submit_action(Action::AcceptEconomicContract {
        accepter_agent_id: "b".to_string(),
        contract_id: "contract.electricity.fee.1".to_string(),
    });
    world.step().expect("accept economic contract");
    world.submit_action(Action::SettleEconomicContract {
        operator_agent_id: "a".to_string(),
        contract_id: "contract.electricity.fee.1".to_string(),
        success: true,
        notes: "delivered".to_string(),
    });
    world.step().expect("settle economic contract");

    let contract = world
        .state()
        .economic_contracts
        .get("contract.electricity.fee.1")
        .expect("settled contract");
    assert_eq!(contract.tax_amount, 3);
    assert_eq!(
        world
            .agent_resource_balance("a", ResourceKind::Electricity)
            .expect("creator electricity"),
        77
    );
    assert_eq!(
        world
            .agent_resource_balance("b", ResourceKind::Electricity)
            .expect("counterparty electricity"),
        20
    );
    assert_eq!(
        world
            .state()
            .resources
            .get(&ResourceKind::Electricity)
            .copied(),
        Some(3)
    );
}

#[test]
fn economic_contract_settlement_applies_tax_and_reputation() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b"]);
    authorize_policy_update(&mut world, "a", "proposal.policy.tax");
    world
        .set_agent_resource_balance("a", ResourceKind::Data, 100)
        .expect("seed creator data");

    world.submit_action(Action::UpdateGameplayPolicy {
        operator_agent_id: "a".to_string(),
        electricity_tax_bps: 0,
        data_tax_bps: 1_000,
        power_trade_fee_bps: 0,
        max_open_contracts_per_agent: 4,
        blocked_agents: Vec::new(),
        forbidden_location_ids: Vec::new(),
    });
    world.step().expect("update gameplay policy");
    world.submit_action(Action::GrantDataAccess {
        owner_agent_id: "a".to_string(),
        grantee_agent_id: "b".to_string(),
    });
    world.step().expect("grant data access");

    let expires_at = world.state().time.saturating_add(10);
    world.submit_action(Action::OpenEconomicContract {
        creator_agent_id: "a".to_string(),
        contract_id: "contract.data.1".to_string(),
        counterparty_agent_id: "b".to_string(),
        fulfillment_kind: EconomicContractFulfillmentKind::AtomicExchange,
        settlement_kind: ResourceKind::Data,
        settlement_amount: 30,
        reputation_stake: 8,
        expires_at,
        description: "data labeling batch".to_string(),
    });
    world.step().expect("open economic contract");

    world.submit_action(Action::AcceptEconomicContract {
        accepter_agent_id: "b".to_string(),
        contract_id: "contract.data.1".to_string(),
    });
    world.step().expect("accept economic contract");

    world.submit_action(Action::SettleEconomicContract {
        operator_agent_id: "a".to_string(),
        contract_id: "contract.data.1".to_string(),
        success: true,
        notes: "delivered on time".to_string(),
    });
    world.step().expect("settle economic contract");

    let contract = world
        .state()
        .economic_contracts
        .get("contract.data.1")
        .expect("settled contract");
    assert_eq!(contract.status, EconomicContractStatus::Settled);
    assert_eq!(contract.transfer_amount, 30);
    assert_eq!(contract.tax_amount, 3);
    assert_eq!(contract.settlement_success, Some(true));

    let creator_data = world
        .state()
        .agents
        .get("a")
        .expect("creator agent")
        .state
        .resources
        .get(ResourceKind::Data);
    let counterparty_data = world
        .state()
        .agents
        .get("b")
        .expect("counterparty agent")
        .state
        .resources
        .get(ResourceKind::Data);
    assert_eq!(creator_data, 67);
    assert_eq!(counterparty_data, 30);
    assert_eq!(
        world.state().resources.get(&ResourceKind::Data).copied(),
        Some(3)
    );
    assert_eq!(world.state().reputation_scores.get("a"), Some(&3));
    assert_eq!(world.state().reputation_scores.get("b"), Some(&3));
    let has_settled_event = world.journal().events.iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::EconomicContractSettled { contract_id, .. })
                if contract_id == "contract.data.1"
        )
    });
    assert!(has_settled_event, "expected EconomicContractSettled event");
}
