#[test]
fn economic_contract_settlement_overflow_keeps_state_atomic() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b"]);
    authorize_policy_update(&mut world, "a", "proposal.policy.atomicity");
    world
        .set_agent_resource_balance("a", ResourceKind::Data, 100)
        .expect("seed creator data");
    world
        .set_agent_resource_balance("b", ResourceKind::Data, i64::MAX)
        .expect("seed counterparty data at boundary");

    world.submit_action(Action::UpdateGameplayPolicy {
        operator_agent_id: "a".to_string(),
        electricity_tax_bps: 0,
        data_tax_bps: 0,
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
        contract_id: "contract.atomic.overflow".to_string(),
        counterparty_agent_id: "b".to_string(),
        fulfillment_kind: EconomicContractFulfillmentKind::AtomicExchange,
        settlement_kind: ResourceKind::Data,
        settlement_amount: 1,
        reputation_stake: 5,
        expires_at,
        description: "overflow settlement".to_string(),
    });
    world.step().expect("open economic contract");
    world.submit_action(Action::AcceptEconomicContract {
        accepter_agent_id: "b".to_string(),
        contract_id: "contract.atomic.overflow".to_string(),
    });
    world.step().expect("accept economic contract");
    let events_before = world.journal().len();

    world.submit_action(Action::SettleEconomicContract {
        operator_agent_id: "a".to_string(),
        contract_id: "contract.atomic.overflow".to_string(),
        success: true,
        notes: "attempt overflow settle".to_string(),
    });
    let err = world.step().expect_err("overflow settlement must fail");
    assert!(
        matches!(err, WorldError::ResourceBalanceInvalid { .. }),
        "unexpected error: {err:?}"
    );

    let contract = world
        .state()
        .economic_contracts
        .get("contract.atomic.overflow")
        .expect("contract should still exist");
    assert_eq!(contract.status, EconomicContractStatus::Accepted);
    assert_eq!(contract.settled_at, None);
    assert_eq!(contract.settlement_success, None);
    assert_eq!(contract.transfer_amount, 0);
    assert_eq!(contract.tax_amount, 0);
    assert_eq!(contract.settlement_notes, None);
    assert_eq!(
        world
            .state()
            .agents
            .get("a")
            .expect("creator agent")
            .state
            .resources
            .get(ResourceKind::Data),
        100
    );
    assert_eq!(
        world
            .state()
            .agents
            .get("b")
            .expect("counterparty agent")
            .state
            .resources
            .get(ResourceKind::Data),
        i64::MAX
    );
    assert_eq!(
        world.state().resources.get(&ResourceKind::Data).copied(),
        None
    );
    assert_eq!(world.state().reputation_scores.get("a"), None);
    assert_eq!(world.state().reputation_scores.get("b"), None);
    assert_eq!(world.journal().len(), events_before);
}

#[test]
fn economic_contract_service_open_is_denied_without_state_effect() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b"]);
    let action = Action::OpenEconomicContract {
        creator_agent_id: "a".to_string(),
        contract_id: "contract.service.unavailable".to_string(),
        counterparty_agent_id: "b".to_string(),
        fulfillment_kind: EconomicContractFulfillmentKind::AtomicExchange,
        settlement_kind: ResourceKind::Electricity,
        settlement_amount: 10,
        reputation_stake: 1,
        expires_at: world.state().time.saturating_add(10),
        description: "unsupported ongoing maintenance".to_string(),
    };
    let mut value = serde_json::to_value(action).expect("serialize contract action");
    value["data"]["fulfillment_kind"] = serde_json::json!("service");
    let service_action: Action = serde_json::from_value(value).expect("deserialize service action");
    let events_before = world.journal().len();

    world.submit_action(service_action);
    world
        .step()
        .expect("service opening should be rejected as a world event");

    assert_latest_rule_denied_contains(
        &world,
        "service contracts unavailable: collateral/evidence/remedy not implemented",
    );
    assert!(
        !world
            .state()
            .economic_contracts
            .contains_key("contract.service.unavailable"),
        "denied service opening must not persist a contract"
    );
    assert_eq!(world.journal().len(), events_before.saturating_add(1));
}

#[test]
fn economic_contract_atomic_failure_is_denied_without_settlement_effect() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b"]);
    world
        .set_agent_resource_balance("a", ResourceKind::Electricity, 20)
        .expect("seed creator electricity");

    world.submit_action(Action::OpenEconomicContract {
        creator_agent_id: "a".to_string(),
        contract_id: "contract.atomic.failure".to_string(),
        counterparty_agent_id: "b".to_string(),
        fulfillment_kind: EconomicContractFulfillmentKind::AtomicExchange,
        settlement_kind: ResourceKind::Electricity,
        settlement_amount: 10,
        reputation_stake: 3,
        expires_at: world.state().time.saturating_add(10),
        description: "atomic exchange cannot be breach-settled".to_string(),
    });
    world.step().expect("open atomic contract");
    world.submit_action(Action::AcceptEconomicContract {
        accepter_agent_id: "b".to_string(),
        contract_id: "contract.atomic.failure".to_string(),
    });
    world.step().expect("accept atomic contract");
    let events_before = world.journal().len();

    world.submit_action(Action::SettleEconomicContract {
        operator_agent_id: "a".to_string(),
        contract_id: "contract.atomic.failure".to_string(),
        success: false,
        notes: "reported breach".to_string(),
    });
    world
        .step()
        .expect("failed atomic settlement should be rejected as a world event");

    assert_latest_rule_denied_contains(&world, "atomic exchange settlement requires success=true");
    let contract = world
        .state()
        .economic_contracts
        .get("contract.atomic.failure")
        .expect("accepted atomic contract remains available");
    assert_eq!(contract.status, EconomicContractStatus::Accepted);
    assert_eq!(contract.settlement_success, None);
    assert_eq!(contract.settled_at, None);
    assert_eq!(world.state().reputation_scores.get("a"), None);
    assert_eq!(world.state().reputation_scores.get("b"), None);
    assert_eq!(
        world
            .agent_resource_balance("a", ResourceKind::Electricity)
            .expect("creator electricity"),
        20
    );
    assert_eq!(
        world
            .agent_resource_balance("b", ResourceKind::Electricity)
            .expect("counterparty electricity"),
        0
    );
    assert_eq!(world.journal().len(), events_before.saturating_add(1));
}

#[test]
fn economic_contract_success_reputation_reward_respects_stake_and_cap() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b", "c"]);
    authorize_policy_update(&mut world, "a", "proposal.policy.reputation");
    world
        .set_agent_resource_balance("a", ResourceKind::Data, 2_000)
        .expect("seed creator data");

    world.submit_action(Action::UpdateGameplayPolicy {
        operator_agent_id: "a".to_string(),
        electricity_tax_bps: 0,
        data_tax_bps: 0,
        power_trade_fee_bps: 0,
        max_open_contracts_per_agent: 8,
        blocked_agents: Vec::new(),
        forbidden_location_ids: Vec::new(),
    });
    world.step().expect("update gameplay policy");
    world.submit_action(Action::GrantDataAccess {
        owner_agent_id: "a".to_string(),
        grantee_agent_id: "b".to_string(),
    });
    world.step().expect("grant data access b");
    world.submit_action(Action::GrantDataAccess {
        owner_agent_id: "a".to_string(),
        grantee_agent_id: "c".to_string(),
    });
    world.step().expect("grant data access c");

    let expires_at = world.state().time.saturating_add(10);
    world.submit_action(Action::OpenEconomicContract {
        creator_agent_id: "a".to_string(),
        contract_id: "contract.stake.bound".to_string(),
        counterparty_agent_id: "b".to_string(),
        fulfillment_kind: EconomicContractFulfillmentKind::AtomicExchange,
        settlement_kind: ResourceKind::Data,
        settlement_amount: 200,
        reputation_stake: 5,
        expires_at,
        description: "stake bound contract".to_string(),
    });
    world.step().expect("open stake-bound contract");
    world.submit_action(Action::AcceptEconomicContract {
        accepter_agent_id: "b".to_string(),
        contract_id: "contract.stake.bound".to_string(),
    });
    world.step().expect("accept stake-bound contract");
    world.submit_action(Action::SettleEconomicContract {
        operator_agent_id: "a".to_string(),
        contract_id: "contract.stake.bound".to_string(),
        success: true,
        notes: "settle stake bound".to_string(),
    });
    world.step().expect("settle stake-bound contract");

    let second_expires_at = world.state().time.saturating_add(10);
    world.submit_action(Action::OpenEconomicContract {
        creator_agent_id: "a".to_string(),
        contract_id: "contract.cap.bound".to_string(),
        counterparty_agent_id: "c".to_string(),
        fulfillment_kind: EconomicContractFulfillmentKind::AtomicExchange,
        settlement_kind: ResourceKind::Data,
        settlement_amount: 500,
        reputation_stake: 80,
        expires_at: second_expires_at,
        description: "cap bound contract".to_string(),
    });
    world.step().expect("open cap-bound contract");
    world.submit_action(Action::AcceptEconomicContract {
        accepter_agent_id: "c".to_string(),
        contract_id: "contract.cap.bound".to_string(),
    });
    world.step().expect("accept cap-bound contract");
    world.submit_action(Action::SettleEconomicContract {
        operator_agent_id: "a".to_string(),
        contract_id: "contract.cap.bound".to_string(),
        success: true,
        notes: "settle cap bound".to_string(),
    });
    world.step().expect("settle cap-bound contract");

    assert_eq!(world.state().reputation_scores.get("a"), Some(&17));
    assert_eq!(world.state().reputation_scores.get("b"), Some(&5));
    assert_eq!(world.state().reputation_scores.get("c"), Some(&12));
}

fn advance_ticks_with_rejected_observation(world: &mut World, agent_id: &str, ticks: u64) {
    for _ in 0..ticks {
        world.submit_action(Action::QueryObservation {
            agent_id: agent_id.to_string(),
        });
        world.step().expect("advance tick");
    }
}

#[test]
fn economic_contract_pair_cooldown_rejects_repeated_success_settlement() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b"]);
    world
        .set_agent_resource_balance("a", ResourceKind::Electricity, 1_000)
        .expect("seed creator electricity");

    let expires_at = world.state().time.saturating_add(20);
    world.submit_action(Action::OpenEconomicContract {
        creator_agent_id: "a".to_string(),
        contract_id: "contract.cooldown.1".to_string(),
        counterparty_agent_id: "b".to_string(),
        fulfillment_kind: EconomicContractFulfillmentKind::AtomicExchange,
        settlement_kind: ResourceKind::Electricity,
        settlement_amount: 20,
        reputation_stake: 5,
        expires_at,
        description: "cooldown-first".to_string(),
    });
    world.step().expect("open first contract");
    world.submit_action(Action::AcceptEconomicContract {
        accepter_agent_id: "b".to_string(),
        contract_id: "contract.cooldown.1".to_string(),
    });
    world.step().expect("accept first contract");
    world.submit_action(Action::SettleEconomicContract {
        operator_agent_id: "a".to_string(),
        contract_id: "contract.cooldown.1".to_string(),
        success: true,
        notes: "settle first".to_string(),
    });
    world.step().expect("settle first contract");
    let first_settlement_tick = world.state().time;

    let second_expires_at = world.state().time.saturating_add(20);
    world.submit_action(Action::OpenEconomicContract {
        creator_agent_id: "a".to_string(),
        contract_id: "contract.cooldown.2".to_string(),
        counterparty_agent_id: "b".to_string(),
        fulfillment_kind: EconomicContractFulfillmentKind::AtomicExchange,
        settlement_kind: ResourceKind::Electricity,
        settlement_amount: 20,
        reputation_stake: 5,
        expires_at: second_expires_at,
        description: "cooldown-second".to_string(),
    });
    world.step().expect("open second contract");
    world.submit_action(Action::AcceptEconomicContract {
        accepter_agent_id: "b".to_string(),
        contract_id: "contract.cooldown.2".to_string(),
    });
    world.step().expect("accept second contract");
    world.submit_action(Action::SettleEconomicContract {
        operator_agent_id: "a".to_string(),
        contract_id: "contract.cooldown.2".to_string(),
        success: true,
        notes: "settle second too early".to_string(),
    });
    world
        .step()
        .expect("second settlement should be rejected by cooldown");
    assert_latest_rule_denied_contains(&world, "pair cooldown active");

    let second_contract = world
        .state()
        .economic_contracts
        .get("contract.cooldown.2")
        .expect("second contract exists");
    assert_eq!(second_contract.status, EconomicContractStatus::Accepted);

    let target_tick = first_settlement_tick.saturating_add(5);
    if world.state().time < target_tick {
        let wait_ticks = target_tick.saturating_sub(world.state().time);
        advance_ticks_with_rejected_observation(&mut world, "a", wait_ticks);
    }
    world.submit_action(Action::SettleEconomicContract {
        operator_agent_id: "a".to_string(),
        contract_id: "contract.cooldown.2".to_string(),
        success: true,
        notes: "settle second after cooldown".to_string(),
    });
    world.step().expect("settle second contract after cooldown");
    let second_contract = world
        .state()
        .economic_contracts
        .get("contract.cooldown.2")
        .expect("second contract settled");
    assert_eq!(second_contract.status, EconomicContractStatus::Settled);
}

#[test]
fn economic_contract_reputation_window_cap_decays_reward_to_zero_then_recovers() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b", "c", "d", "e"]);
    world
        .set_agent_resource_balance("a", ResourceKind::Electricity, 5_000)
        .expect("seed creator electricity");

    let settle_success = |world: &mut World, contract_id: &str, counterparty_agent_id: &str| {
        let expires_at = world.state().time.saturating_add(20);
        world.submit_action(Action::OpenEconomicContract {
            creator_agent_id: "a".to_string(),
            contract_id: contract_id.to_string(),
            counterparty_agent_id: counterparty_agent_id.to_string(),
            fulfillment_kind: EconomicContractFulfillmentKind::AtomicExchange,
            settlement_kind: ResourceKind::Electricity,
            settlement_amount: 200,
            reputation_stake: 20,
            expires_at,
            description: format!("window-cap-{contract_id}"),
        });
        world.step().expect("open contract");
        world.submit_action(Action::AcceptEconomicContract {
            accepter_agent_id: counterparty_agent_id.to_string(),
            contract_id: contract_id.to_string(),
        });
        world.step().expect("accept contract");
        world.submit_action(Action::SettleEconomicContract {
            operator_agent_id: "a".to_string(),
            contract_id: contract_id.to_string(),
            success: true,
            notes: format!("settle {contract_id}"),
        });
        world.step().expect("settle contract");
    };

    settle_success(&mut world, "contract.window.1", "b");
    settle_success(&mut world, "contract.window.2", "c");
    settle_success(&mut world, "contract.window.3", "d");

    let third_settle_event = world.journal().events.last().expect("third settle event");
    match &third_settle_event.body {
        WorldEventBody::Domain(DomainEvent::EconomicContractSettled {
            creator_reputation_delta,
            counterparty_reputation_delta,
            ..
        }) => {
            assert_eq!(*creator_reputation_delta, 0);
            assert_eq!(*counterparty_reputation_delta, 12);
        }
        other => panic!("expected EconomicContractSettled, got {other:?}"),
    }

    assert_eq!(world.state().reputation_scores.get("a"), Some(&24));
    assert_eq!(world.state().reputation_scores.get("b"), Some(&12));
    assert_eq!(world.state().reputation_scores.get("c"), Some(&12));
    assert_eq!(world.state().reputation_scores.get("d"), Some(&12));

    advance_ticks_with_rejected_observation(&mut world, "a", 20);
    settle_success(&mut world, "contract.window.4", "e");

    let fourth_contract = world
        .state()
        .economic_contracts
        .get("contract.window.4")
        .expect("fourth contract exists");
    assert_eq!(fourth_contract.status, EconomicContractStatus::Settled);
    assert_eq!(world.state().reputation_scores.get("a"), Some(&36));
    assert_eq!(world.state().reputation_scores.get("e"), Some(&12));
}

#[test]
fn economic_contract_respects_policy_quota_and_block_list() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b", "c"]);
    authorize_policy_update(&mut world, "a", "proposal.policy.quota");

    world.submit_action(Action::UpdateGameplayPolicy {
        operator_agent_id: "a".to_string(),
        electricity_tax_bps: 0,
        data_tax_bps: 0,
        power_trade_fee_bps: 0,
        max_open_contracts_per_agent: 1,
        blocked_agents: vec!["b".to_string()],
        forbidden_location_ids: Vec::new(),
    });
    world.step().expect("update gameplay policy");

    let expires_at = world.state().time.saturating_add(8);
    world.submit_action(Action::OpenEconomicContract {
        creator_agent_id: "a".to_string(),
        contract_id: "contract.ok".to_string(),
        counterparty_agent_id: "c".to_string(),
        fulfillment_kind: EconomicContractFulfillmentKind::AtomicExchange,
        settlement_kind: ResourceKind::Electricity,
        settlement_amount: 10,
        reputation_stake: 4,
        expires_at,
        description: "power shipment".to_string(),
    });
    world.step().expect("open first contract");

    world.submit_action(Action::OpenEconomicContract {
        creator_agent_id: "a".to_string(),
        contract_id: "contract.quota".to_string(),
        counterparty_agent_id: "c".to_string(),
        fulfillment_kind: EconomicContractFulfillmentKind::AtomicExchange,
        settlement_kind: ResourceKind::Electricity,
        settlement_amount: 10,
        reputation_stake: 4,
        expires_at,
        description: "second contract".to_string(),
    });
    world.step().expect("reject quota overflow");
    assert_latest_rule_denied_contains(&world, "quota exceeded for creator");

    world.submit_action(Action::OpenEconomicContract {
        creator_agent_id: "a".to_string(),
        contract_id: "contract.blocked".to_string(),
        counterparty_agent_id: "b".to_string(),
        fulfillment_kind: EconomicContractFulfillmentKind::AtomicExchange,
        settlement_kind: ResourceKind::Electricity,
        settlement_amount: 10,
        reputation_stake: 4,
        expires_at,
        description: "blocked counterparty".to_string(),
    });
    world.step().expect("reject blocked counterparty");
    assert_latest_rule_denied_contains(&world, "blocked by gameplay policy");
}

#[test]
fn economic_contract_expires_and_penalizes_reputation() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b"]);
    let expires_at = world.state().time.saturating_add(2);

    world.submit_action(Action::OpenEconomicContract {
        creator_agent_id: "a".to_string(),
        contract_id: "contract.expire".to_string(),
        counterparty_agent_id: "b".to_string(),
        fulfillment_kind: EconomicContractFulfillmentKind::AtomicExchange,
        settlement_kind: ResourceKind::Electricity,
        settlement_amount: 5,
        reputation_stake: 6,
        expires_at,
        description: "expiring contract".to_string(),
    });
    world.step().expect("open contract");

    let snapshot_before_prepare = world.snapshot();
    let journal_before_prepare = world.journal().clone();
    let prepared = world.prepared_economic_contract_expiry_events_for_test(expires_at);
    assert_eq!(world.snapshot(), snapshot_before_prepare);
    assert_eq!(world.journal(), &journal_before_prepare);
    assert!(matches!(
        prepared.as_slice(),
        [DomainEvent::EconomicContractExpired {
            contract_id,
            creator_reputation_delta: -6,
            counterparty_reputation_delta: 0,
            ..
        }] if contract_id == "contract.expire"
    ));

    while world.state().time <= expires_at {
        world.submit_action(Action::QueryObservation {
            agent_id: "a".to_string(),
        });
        world.step().expect("advance tick for expiry");
    }

    let contract = world
        .state()
        .economic_contracts
        .get("contract.expire")
        .expect("expired contract");
    assert_eq!(contract.status, EconomicContractStatus::Expired);
    assert_eq!(world.state().reputation_scores.get("a"), Some(&-6));
    assert_eq!(world.state().reputation_scores.get("b"), None);
    let has_expired_event = world.journal().events.iter().any(|event| {
        matches!(
            event.body,
            WorldEventBody::Domain(DomainEvent::EconomicContractExpired { .. })
        )
    });
    assert!(has_expired_event, "expected EconomicContractExpired event");
}

#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
#[test]
fn step_with_modules_uses_gameplay_tick_modules_without_fallback() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b"]);
    world
        .install_m5_gameplay_bootstrap_modules("bootstrap")
        .expect("install gameplay modules");

    open_governance_proposal(&mut world, "proposal.module_only", 1, 1, 5_000);

    let mut sandbox = GameplayDirectiveSandbox::empty();
    world
        .step_with_modules(&mut sandbox)
        .expect("module-driven gameplay tick");

    let proposal = world
        .state()
        .governance_proposals
        .get("proposal.module_only")
        .expect("proposal should still exist");
    assert_eq!(proposal.status, GovernanceProposalStatus::Open);
}

#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
#[test]
fn step_with_modules_applies_gameplay_directive_emits_to_domain_events() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b"]);
    world
        .install_m5_gameplay_bootstrap_modules("bootstrap")
        .expect("install gameplay modules");

    open_governance_proposal(&mut world, "proposal.directive", 8, 1, 5_000);

    let payload = serde_json::json!({
        "directives": [
            {
                "type": "governance_finalize",
                "proposal_key": "proposal.directive",
                "winning_option": "approve",
                "winning_weight": 3,
                "total_weight": 3,
                "passed": true
            }
        ]
    });
    let mut sandbox = GameplayDirectiveSandbox::with_governance_directive(payload);
    world
        .step_with_modules(&mut sandbox)
        .expect("module-driven directive tick");

    let proposal = world
        .state()
        .governance_proposals
        .get("proposal.directive")
        .expect("proposal finalized by directive");
    assert_eq!(proposal.status, GovernanceProposalStatus::Passed);
    assert_eq!(proposal.winning_option.as_deref(), Some("approve"));
    assert_eq!(proposal.winning_weight, 3);
    assert_eq!(proposal.total_weight_at_finalize, 3);
}
