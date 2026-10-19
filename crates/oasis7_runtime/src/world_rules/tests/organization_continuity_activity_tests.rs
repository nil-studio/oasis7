//! Registered activity evidence, observation coverage, and externality boundaries.
use super::*;
use crate::WorldEventBody;

fn registered_activity_world() -> (World, String) {
    let world_binding = crate::util::hash_json(&(
        "oasis7-world-rule-world-v1",
        "organization-activity-test-world",
        "organization-activity-test-chain",
        "organization-activity-test-genesis",
    ))
    .unwrap();
    let mut state = configured_organization_fixture();
    state.time = 10;
    let contract = crate::EconomicContractState {
        contract_id: "contract.activity".into(),
        creator_agent_id: "owner".into(),
        counterparty_agent_id: "recipient".into(),
        fulfillment_kind: crate::EconomicContractFulfillmentKind::AtomicExchange,
        settlement_kind: oasis7_agent_api::world_action::ResourceKind::Electricity,
        settlement_amount: 7,
        reputation_stake: 2,
        expires_at: 100,
        description: "organization activity proof".into(),
        status: crate::EconomicContractStatus::Accepted,
        accepted_at: Some(3),
        settled_at: None,
        settlement_success: None,
        transfer_amount: 0,
        tax_amount: 0,
        settlement_notes: None,
    };
    state
        .economic_contracts
        .insert(contract.contract_id.clone(), contract.clone());

    let enable = outer(enable_command(&state));
    approve(&mut state, "owner", "enable-org", &enable);
    let register = outer(ContinuityCommand::RegisterActivityContract {
        organization_id: "org".into(),
        contract_id: contract.contract_id.clone(),
        expected_terms_hash: crate::world_rules::economic_contract_terms_hash(&contract).unwrap(),
        governance_proposal_key: "activity-source".into(),
    });
    approve(&mut state, "owner", "activity-source", &register);

    let unbound_world = World::new_with_state(state);
    let snapshot = unbound_world.snapshot_with_chain_resource_context(
        crate::ChainResourceDerivationContext {
            world_id: "organization-activity-test-world",
            chain_id: "organization-activity-test-chain",
            genesis_ref: Some("organization-activity-test-genesis"),
            created_at_height: 0,
            manifest_height: 0,
            commit_block_hash: None,
            tick: unbound_world.state().time,
        },
        "a".repeat(64),
        "b".repeat(64),
    );
    let mut world = World::from_snapshot(snapshot, unbound_world.journal().clone()).unwrap();
    assert_eq!(
        world.world_rule_binding().as_deref(),
        Some(world_binding.as_str())
    );
    world
        .set_agent_resource_balance(
            "owner",
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            100,
        )
        .unwrap();
    let WorldRuleCommandV1::OrganizationContinuity { command: enable } = enable else {
        unreachable!();
    };
    let WorldRuleCommandV1::OrganizationContinuity { command: register } = register else {
        unreachable!();
    };
    world.submit_action(Action::WorldRule {
        actor_id: "owner".into(),
        request_id: "activity-enable".into(),
        command: WorldRuleCommandV1::OrganizationContinuity { command: enable },
    });
    world.submit_action(Action::WorldRule {
        actor_id: "owner".into(),
        request_id: "activity-source".into(),
        command: WorldRuleCommandV1::OrganizationContinuity { command: register },
    });
    world.step().unwrap();
    assert!(
        world.state().world_rules.is_some(),
        "organization actions were rejected: {:?}",
        world
            .journal()
            .events
            .iter()
            .map(|event| &event.body)
            .collect::<Vec<_>>()
    );
    (world, world_binding)
}

#[test]
fn founder_read_model_lists_only_external_atomic_exchange_registration_candidates() {
    let mut state = configured_organization_fixture();
    let command = outer(enable_command(&state));
    approve(&mut state, "owner", "enable-org", &command);
    apply(
        &mut state,
        &event_with_binding("owner", "enable", command, WORLD_BINDING),
        10,
    );
    let contract = crate::EconomicContractState {
        contract_id: "contract.external".into(),
        creator_agent_id: "owner".into(),
        counterparty_agent_id: "recipient".into(),
        fulfillment_kind: crate::EconomicContractFulfillmentKind::AtomicExchange,
        settlement_kind: oasis7_agent_api::world_action::ResourceKind::Data,
        settlement_amount: 7,
        reputation_stake: 2,
        expires_at: 100,
        description: "external".into(),
        status: crate::EconomicContractStatus::Accepted,
        accepted_at: Some(3),
        settled_at: None,
        settlement_success: None,
        transfer_amount: 0,
        tax_amount: 0,
        settlement_notes: None,
    };
    let expected_terms_hash = crate::world_rules::economic_contract_terms_hash(&contract).unwrap();
    state
        .economic_contracts
        .insert(contract.contract_id.clone(), contract);

    let owner_status = state
        .world_rule_status_with_binding(Some("owner"), Some(WORLD_BINDING))
        .unwrap();
    let candidates = owner_status["organization_activity_contract_candidates"]
        .as_array()
        .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0]["organization_id"], "org");
    assert_eq!(candidates[0]["contract_id"], "contract.external");
    assert_eq!(candidates[0]["expected_terms_hash"], expected_terms_hash);
    assert_eq!(candidates[0]["expires_at"], 100);

    let member_status = state
        .world_rule_status_with_binding(Some("primary"), Some(WORLD_BINDING))
        .unwrap();
    assert!(
        member_status["organization_activity_contract_candidates"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let public_status = state
        .world_rule_status_with_binding(None, Some(WORLD_BINDING))
        .unwrap();
    assert!(
        public_status["organization_activity_contract_candidates"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let internal_contract = crate::EconomicContractState {
        contract_id: "contract.internal".into(),
        counterparty_agent_id: "primary".into(),
        ..state.economic_contracts["contract.external"].clone()
    };
    state
        .economic_contracts
        .insert(internal_contract.contract_id.clone(), internal_contract);
    let owner_status = state
        .world_rule_status_with_binding(Some("owner"), Some(WORLD_BINDING))
        .unwrap();
    assert_eq!(
        owner_status["organization_activity_contract_candidates"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "an organization member cannot be offered as an external counterparty"
    );
}

#[test]
fn settled_registered_exchange_is_closed_after_gameplay_and_raw_underpayment_is_not_activity() {
    use oasis7_agent_api::world_action::Action;

    let (mut world, _world_binding) = registered_activity_world();
    world.submit_action(Action::SettleEconomicContract {
        operator_agent_id: "owner".into(),
        contract_id: "contract.activity".into(),
        success: true,
        notes: "completed".into(),
    });
    world.step().unwrap();

    let continuity = &world
        .state()
        .world_rules
        .as_ref()
        .unwrap()
        .organization_continuity;
    let registration = &continuity.registrations["org"];
    let settled_at = world.state().economic_contracts["contract.activity"]
        .settled_at
        .unwrap();
    let activity =
        &continuity.activity_receipts["org"]["economic-contract-settlement:contract.activity"];
    assert_eq!(activity.world_tick, settled_at);
    assert_eq!(
        activity.kind,
        crate::world_rules::OrganizationActivityKindV1::ExternalAtomicExchangeSettlement
    );
    assert_eq!(
        registration.activity_close_cursor_world_tick,
        world.state().time
    );
    assert_eq!(
        registration.activity_evidence_complete_through_world_tick,
        world.state().time
    );
    assert_eq!(registration.activity_evidence_paused_reason, None);
    let settlement_id = world
        .journal()
        .events
        .iter()
        .find(|event| {
            matches!(
                &event.body,
                WorldEventBody::Domain(DomainEvent::EconomicContractSettled {
                    contract_id,
                    ..
                }) if contract_id == "contract.activity"
            )
        })
        .unwrap()
        .id;
    let close_id = world
        .journal()
        .events
        .iter()
        .find(|event| {
            matches!(
                &event.body,
                WorldEventBody::Domain(DomainEvent::WorldRuleCommitted {
                    action_id: 0,
                    command: WorldRuleCommandV1::OrganizationContinuity {
                        command: ContinuityCommand::CloseActivityTick {
                            through_world_tick,
                            ..
                        }
                    },
                    ..
                }) if *through_world_tick == world.state().time
            )
        })
        .unwrap()
        .id;
    assert!(
        settlement_id < close_id,
        "activity close must follow settlement"
    );

    let (mut underpaid, underpaid_binding) = registered_activity_world();
    let foreign = DomainEvent::EconomicContractSettled {
        operator_agent_id: "owner".into(),
        contract_id: "contract.activity".into(),
        world_binding: Some("foreign-world-binding".into()),
        success: true,
        transfer_amount: 1,
        tax_amount: 0,
        notes: "forged foreign receipt".into(),
        creator_reputation_delta: 0,
        counterparty_reputation_delta: 0,
    };
    assert!(
        underpaid
            .append_event_for_test(WorldEventBody::Domain(foreign), None)
            .is_err()
    );
    let partial = DomainEvent::EconomicContractSettled {
        operator_agent_id: "owner".into(),
        contract_id: "contract.activity".into(),
        world_binding: Some(underpaid_binding),
        success: true,
        transfer_amount: 1,
        tax_amount: 0,
        notes: "forged underpayment".into(),
        creator_reputation_delta: 0,
        counterparty_reputation_delta: 0,
    };
    underpaid
        .append_event_for_test(WorldEventBody::Domain(partial), None)
        .unwrap();
    underpaid.step().unwrap();
    let continuity = &underpaid
        .state()
        .world_rules
        .as_ref()
        .unwrap()
        .organization_continuity;
    assert!(
        !continuity.activity_receipts.get("org").is_some_and(
            |receipts| receipts.contains_key("economic-contract-settlement:contract.activity")
        )
    );
    assert_eq!(
        continuity.registrations["org"]
            .activity_evidence_paused_reason
            .as_deref(),
        Some("activity_source_integrity_failed")
    );
}

#[test]
fn counterparty_joining_after_activity_registration_invalidates_externality() {
    use oasis7_agent_api::world_action::Action;

    let (mut world, _) = registered_activity_world();
    world.submit_action(Action::JoinAlliance {
        operator_agent_id: "owner".into(),
        alliance_id: "org".into(),
        member_agent_id: "recipient".into(),
    });
    world.step().unwrap();
    assert!(
        world.state().alliances["org"]
            .members
            .iter()
            .any(|member| member == "recipient")
    );

    world.submit_action(Action::SettleEconomicContract {
        operator_agent_id: "owner".into(),
        contract_id: "contract.activity".into(),
        success: true,
        notes: "completed after joining".into(),
    });
    world.step().unwrap();

    assert_eq!(
        world.state().economic_contracts["contract.activity"].settlement_success,
        Some(true),
        "the unrelated economic settlement still completes"
    );
    let continuity = &world
        .state()
        .world_rules
        .as_ref()
        .unwrap()
        .organization_continuity;
    assert!(
        !continuity
            .activity_receipts
            .get("org")
            .is_some_and(|receipts| {
                receipts.contains_key("economic-contract-settlement:contract.activity")
            })
    );
    assert_eq!(
        continuity.registrations["org"]
            .activity_evidence_paused_reason
            .as_deref(),
        Some("activity_source_integrity_failed")
    );
}

#[test]
fn incomplete_activity_coverage_blocks_dormancy_and_is_publicly_visible() {
    let mut state = organization_fixture();
    let registration = OrganizationRegistrationV1 {
        organization_id: "org".into(),
        founder_id: "owner".into(),
        policy_version: ORGANIZATION_CONTINUITY_POLICY_VERSION_V1,
        enabled_at_world_tick: 1,
        last_accounted_world_tick: 50_000,
        activity_evidence_complete_through_world_tick: 1,
        activity_evidence_paused_reason: Some("activity_evidence_unavailable".into()),
        availability_paused_reason: None,
        world_binding: WORLD_BINDING.into(),
        charter_hash: "charter".into(),
        registration_receipt: "registration".into(),
        manifest_root: "manifest".into(),
        manifest_asset_refs: BTreeSet::new(),
        designated_successors: Vec::new(),
        neutral_maintainers: Vec::new(),
        appointment_authorities: BTreeSet::from(["owner".into()]),
        remedy_adjudicators: BTreeSet::new(),
        residual_share_bps: BTreeMap::new(),
        controller_bindings: BTreeMap::new(),
        membership_controller_bindings: BTreeMap::new(),
        activity_sources: BTreeMap::new(),
        activity_observation_started_at_world_tick: None,
        activity_close_cursor_world_tick: 0,
        activity_close_hash: None,
        activity_last_closed_event_id: None,
        activity_last_closed_event_hash: None,
    };
    let continuity = OrganizationContinuityStateV1 {
        registrations: BTreeMap::from([("org".into(), registration)]),
        availability: BTreeMap::from([(
            "org".into(),
            vec![OrganizationAvailabilityIntervalV1 {
                from_world_tick: 2,
                through_world_tick: 50_000,
                world_writes_available: true,
                notice_available: true,
                remedy_available: true,
                evidence_status: OrganizationAvailabilityEvidenceStatusV1::Certified,
                world_binding: WORLD_BINDING.into(),
                registration_receipt: "registration".into(),
                certificate_hash: "availability".into(),
            }],
        )]),
        ..Default::default()
    };
    state.world_rules = Some(crate::world_rules::WorldRulesStateV1 {
        organization_continuity: continuity,
        ..Default::default()
    });

    let continuity = &state.world_rules.as_ref().unwrap().organization_continuity;
    assert_eq!(
        continuity.for_reader(None).registrations["org"]
            .activity_evidence_paused_reason
            .as_deref(),
        Some("activity_evidence_unavailable")
    );
    assert!(
        continuity
            .lifecycle_commands(&state, 50_000)
            .unwrap()
            .is_empty()
    );

    let open = outer(ContinuityCommand::OpenDormancyCase {
        organization_id: "org".into(),
        expected_manifest_root: "manifest".into(),
        trigger_activity_receipt: None,
    });
    let error = PreparedWorldRule::prepare(
        &state,
        &event_with_binding(
            "owner",
            "open-without-activity-watermark",
            open,
            WORLD_BINDING,
        ),
        50_000,
    )
    .unwrap_err();
    assert!(format!("{error:?}").contains("organization_activity_evidence_unavailable"));
}

#[test]
fn inactivity_baseline_at_unknown_tick_starts_only_from_later_certified_time() {
    let mut state = configured_organization_fixture();
    let enable = outer(enable_command(&state));
    approve(&mut state, "owner", "enable-org", &enable);
    apply(
        &mut state,
        &event_with_binding("owner", "enable", enable, WORLD_BINDING),
        10,
    );

    let now = 12 + crate::world_rules::ORGANIZATION_INACTIVITY_TICKS_V1;
    let contract = crate::EconomicContractState {
        contract_id: "contract.unknown-baseline".into(),
        creator_agent_id: "owner".into(),
        counterparty_agent_id: "recipient".into(),
        fulfillment_kind: crate::EconomicContractFulfillmentKind::AtomicExchange,
        settlement_kind: oasis7_agent_api::world_action::ResourceKind::Data,
        settlement_amount: 7,
        reputation_stake: 2,
        expires_at: now + 1,
        description: "unknown baseline test".into(),
        status: crate::EconomicContractStatus::Accepted,
        accepted_at: Some(11),
        settled_at: None,
        settlement_success: None,
        transfer_amount: 0,
        tax_amount: 0,
        settlement_notes: None,
    };
    state
        .economic_contracts
        .insert(contract.contract_id.clone(), contract.clone());
    let rules = state.world_rules.as_mut().unwrap();
    let continuity = &mut rules.organization_continuity;
    let registration = continuity.registrations.get_mut("org").unwrap();
    registration.activity_sources.insert(
        contract.contract_id.clone(),
        crate::world_rules::OrganizationActivityContractSourceV1 {
            contract_id: contract.contract_id.clone(),
            terms_hash: crate::world_rules::economic_contract_terms_hash(&contract).unwrap(),
            creator_binding: registration.controller_bindings["owner"].clone(),
            counterparty_binding: registration.controller_bindings["recipient"].clone(),
            registered_at_world_tick: 12,
            registration_receipt: "source-registration".into(),
            world_binding: WORLD_BINDING.into(),
        },
    );
    registration.activity_observation_started_at_world_tick = Some(12);
    registration.activity_close_cursor_world_tick = now;
    registration.activity_evidence_complete_through_world_tick = now;
    registration.activity_evidence_paused_reason = None;
    registration.last_accounted_world_tick = now;
    continuity.availability.insert(
        "org".into(),
        vec![
            crate::world_rules::OrganizationAvailabilityIntervalV1 {
                from_world_tick: 11,
                through_world_tick: 12,
                world_writes_available: false,
                notice_available: false,
                remedy_available: false,
                evidence_status:
                    crate::world_rules::OrganizationAvailabilityEvidenceStatusV1::Unknown,
                world_binding: WORLD_BINDING.into(),
                registration_receipt: registration.registration_receipt.clone(),
                certificate_hash: "unknown".into(),
            },
            crate::world_rules::OrganizationAvailabilityIntervalV1 {
                from_world_tick: 13,
                through_world_tick: now,
                world_writes_available: true,
                notice_available: true,
                remedy_available: true,
                evidence_status:
                    crate::world_rules::OrganizationAvailabilityEvidenceStatusV1::Certified,
                world_binding: WORLD_BINDING.into(),
                registration_receipt: registration.registration_receipt.clone(),
                certificate_hash: "certified".into(),
            },
        ],
    );

    let continuity = &state.world_rules.as_ref().unwrap().organization_continuity;
    let work = continuity.lifecycle_commands(&state, now).unwrap();
    let (actor, request_id, command) = work
        .into_iter()
        .find(|(_, _, command)| matches!(command, ContinuityCommand::OpenDormancyCase { .. }))
        .expect("the full certified window after the unknown baseline should become actionable");
    assert_eq!(actor, "owner");
    let prepared = PreparedWorldRule::prepare(
        &state,
        &event_with_binding(&actor, &request_id, outer(command), WORLD_BINDING),
        now,
    );
    assert!(
        prepared.is_ok(),
        "fresh certified time must satisfy the same baseline gate"
    );
    assert_eq!(
        continuity.continuity_clock_at("org", 12),
        0,
        "unknown ticks contribute no continuity time"
    );
}
