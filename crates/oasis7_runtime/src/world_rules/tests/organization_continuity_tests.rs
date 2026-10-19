use super::*;
use crate::{WorldEventBody, world_rules::OrganizationContinuityStateV1};
use ed25519_dalek::{Signer, SigningKey};
use oasis7_agent_api::world_action::{
    GovernanceFinalityCertificate, OrganizationContinuityCommandV1 as ContinuityCommand,
    WorldRuleCommandV1,
};
use std::collections::{BTreeMap, BTreeSet};

const WORLD_BINDING: &str = "world/genesis/chain:canonical";

fn configured_organization_fixture() -> WorldState {
    let mut state = organization_fixture();
    for (agent_id, player_id) in [
        ("owner", "player-owner"),
        ("primary", "player-primary"),
        ("recipient", "player-recipient"),
    ] {
        state.starter_oc_claims.insert(
            agent_id.into(),
            crate::gameplay_state::StarterOcClaimState {
                agent_id: agent_id.into(),
                player_id: player_id.into(),
                public_key: Some(format!("key-{agent_id}")),
                claimed_at: 1,
                ..Default::default()
            },
        );
    }
    let charter = crate::world_rules::OrganizationContinuityCharterV1 {
        schema: "organization-continuity-v1".into(),
        designated_successors: vec!["primary".into()],
        neutral_maintainers: vec!["recipient".into()],
        appointment_authorities: BTreeSet::from(["owner".into()]),
        remedy_adjudicators: BTreeSet::from(["recipient".into()]),
        activity_source_policy: crate::world_rules::ORGANIZATION_ACTIVITY_SOURCE_POLICY_V1.into(),
        residual_share_bps: BTreeMap::from([("owner".into(), 5_000), ("primary".into(), 5_000)]),
    };
    state.alliances.get_mut("org").unwrap().charter = serde_json::to_string(&charter).unwrap();
    state
}

fn enable_command(state: &WorldState) -> ContinuityCommand {
    let alliance = &state.alliances["org"];
    let charter: crate::world_rules::OrganizationContinuityCharterV1 =
        serde_json::from_str(&alliance.charter).unwrap();
    ContinuityCommand::Enable {
        organization_id: "org".into(),
        expected_charter_hash: crate::util::sha256_hex(alliance.charter.as_bytes()),
        governance_proposal_key: "enable-org".into(),
        designated_successors: charter.designated_successors,
        neutral_maintainers: charter.neutral_maintainers,
        appointment_authorities: charter.appointment_authorities,
        remedy_adjudicators: charter.remedy_adjudicators,
        residual_share_bps: charter.residual_share_bps,
    }
}

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

fn outer(command: ContinuityCommand) -> WorldRuleCommandV1 {
    WorldRuleCommandV1::OrganizationContinuity { command }
}

fn install_finality_registry(state: &mut WorldState) -> BTreeMap<String, SigningKey> {
    let keys = BTreeMap::from([
        ("finality-a".to_owned(), SigningKey::from_bytes(&[11; 32])),
        ("finality-b".to_owned(), SigningKey::from_bytes(&[22; 32])),
    ]);
    let bindings = keys
        .iter()
        .map(|(node, key)| (node.clone(), hex::encode(key.verifying_key().to_bytes())))
        .collect::<BTreeMap<_, _>>();
    state.node_identity_bindings.extend(bindings.clone());
    state.governance_finality_signer_registry = Some(crate::GovernanceFinalitySignerRegistry {
        slot_id: "org-continuity-test".into(),
        threshold: 2,
        threshold_bps: 10_000,
        signer_bindings: bindings,
        validator_stakes: BTreeMap::from([("finality-a".into(), 1), ("finality-b".into(), 1)]),
    });
    keys
}

fn availability_certificate(
    state: &WorldState,
    signing_keys: &BTreeMap<String, SigningKey>,
    organization_id: &str,
    world_binding: &str,
    from: u64,
    through: u64,
    world_writes: bool,
    notice: bool,
    remedy: bool,
) -> GovernanceFinalityCertificate {
    let registration = &state
        .world_rules
        .as_ref()
        .unwrap()
        .organization_continuity
        .registrations[organization_id];
    let binding = (
        "oasis7.organization-continuity.availability.v1",
        world_binding,
        &registration.registration_receipt,
        organization_id,
        &registration.charter_hash,
        &registration.world_binding,
        from,
        through,
        world_writes,
        notice,
        remedy,
    );
    let manifest_hash = crate::util::hash_json(&binding).unwrap();
    let proposal_id = u64::from_str_radix(&manifest_hash[..16], 16).unwrap();
    let signer_ids = signing_keys.keys().cloned().collect::<Vec<_>>();
    let validator_set_hash = crate::util::sha256_hex(signer_ids.join("|").as_bytes());
    let stake_root = crate::util::sha256_hex(b"finality-a:1|finality-b:1");
    let mut certificate = GovernanceFinalityCertificate {
        proposal_id,
        manifest_hash,
        consensus_height: through,
        epoch_id: 7,
        validator_set_hash,
        stake_root,
        threshold_bps: 10_000,
        min_unique_signers: 2,
        threshold: 2,
        signatures: BTreeMap::new(),
    };
    for (node_id, key) in signing_keys {
        let payload = GovernanceFinalityCertificate::signing_payload_v1(
            certificate.proposal_id,
            &certificate.manifest_hash,
            certificate.consensus_height,
            certificate.epoch_id,
            &certificate.validator_set_hash,
            &certificate.stake_root,
            certificate.threshold_bps,
            certificate.effective_min_unique_signers(),
            node_id,
        );
        certificate.signatures.insert(
            node_id.clone(),
            format!(
                "{}{}",
                GovernanceFinalityCertificate::SIGNATURE_PREFIX_ED25519_V1,
                hex::encode(key.sign(&payload).to_bytes())
            ),
        );
    }
    certificate
}

fn event_with_binding(
    actor: &str,
    request: &str,
    command: WorldRuleCommandV1,
    world_binding: &str,
) -> DomainEvent {
    event_with_binding_and_action_id(1_000_001, actor, request, command, world_binding)
}

fn event_with_binding_and_action_id(
    action_id: u64,
    actor: &str,
    request: &str,
    command: WorldRuleCommandV1,
    world_binding: &str,
) -> DomainEvent {
    DomainEvent::WorldRuleCommitted {
        action_id,
        actor_id: actor.into(),
        request_id: request.into(),
        command,
        governance_epoch: 7,
        world_binding: Some(world_binding.into()),
    }
}

#[test]
fn legacy_alliances_are_not_auto_enabled_and_unknown_charters_fail_closed() {
    let mut state = organization_fixture();
    let continuity = OrganizationContinuityStateV1::default();
    assert!(
        continuity
            .lifecycle_commands(&state, 50_000)
            .unwrap()
            .is_empty()
    );

    let command = outer(ContinuityCommand::OpenDormancyCase {
        organization_id: "org".into(),
        expected_manifest_root: "empty".into(),
        trigger_activity_receipt: None,
    });
    assert!(PreparedWorldRule::prepare(&state, &event("owner", "open", command), 50_000).is_err());

    let enable = outer(ContinuityCommand::Enable {
        organization_id: "org".into(),
        expected_charter_hash: crate::util::sha256_hex(state.alliances["org"].charter.as_bytes()),
        governance_proposal_key: "enable-org".into(),
        designated_successors: Vec::new(),
        neutral_maintainers: Vec::new(),
        appointment_authorities: BTreeSet::new(),
        remedy_adjudicators: BTreeSet::new(),
        residual_share_bps: BTreeMap::new(),
    });
    approve(&mut state, "owner", "enable-org", &enable);
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event_with_binding("owner", "enable", enable, WORLD_BINDING),
            1
        )
        .is_err()
    );
    assert!(
        state
            .world_rules
            .as_ref()
            .is_none_or(|rules| rules.organization_continuity.registrations.is_empty())
    );
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

#[test]
fn enable_requires_the_exact_typed_charter_founder_and_passed_governance() {
    let mut state = configured_organization_fixture();
    let command = outer(enable_command(&state));
    approve(&mut state, "owner", "enable-org", &command);
    apply(
        &mut state,
        &event_with_binding("owner", "enable", command, WORLD_BINDING),
        10,
    );

    let registration = &state
        .world_rules
        .as_ref()
        .unwrap()
        .organization_continuity
        .registrations["org"];
    assert_eq!(registration.enabled_at_world_tick, 10);
    assert_eq!(registration.policy_version, 1);
    assert_eq!(registration.world_binding, WORLD_BINDING);
    assert!(registration.manifest_asset_refs.is_empty());
    assert!(registration.manifest_root.len() == 64);
}

#[test]
fn material_estate_deposit_requires_registration_and_extends_the_committed_manifest() {
    let mut state = configured_organization_fixture();
    let deposit = outer_legacy_deposit();
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event_with_binding(
                "owner",
                "deposit-before-enable",
                deposit.clone(),
                WORLD_BINDING
            ),
            9
        )
        .is_err()
    );

    let enable = outer(enable_command(&state));
    approve(&mut state, "owner", "enable-org", &enable);
    apply(
        &mut state,
        &event_with_binding("owner", "enable", enable, WORLD_BINDING),
        10,
    );
    let registration_root = state
        .world_rules
        .as_ref()
        .unwrap()
        .organization_continuity
        .registrations["org"]
        .manifest_root
        .clone();

    apply(
        &mut state,
        &event_with_binding("owner", "deposit", deposit, WORLD_BINDING),
        11,
    );
    let rules = state.world_rules.as_ref().unwrap();
    let deposit_id = request_key("owner", "deposit");
    let registration = &rules.organization_continuity.registrations["org"];
    assert_ne!(registration.manifest_root, registration_root);
    assert!(registration.manifest_asset_refs.contains(&deposit_id));
    let asset = &rules.organization_continuity.assets[&deposit_id];
    assert_eq!(asset.source_receipt, deposit_id);
    assert_eq!(asset.contributor_id, "owner");
    assert_eq!(
        state.material_ledgers[&MaterialLedgerId::agent("owner")]["alloy_plate"],
        113
    );
}

#[test]
fn reader_projection_preserves_public_gates_and_hides_evidence_and_balances() {
    use crate::world_rules::{
        OrganizationCaseOriginV1, OrganizationContinuityCaseV1, OrganizationDispositionStageV1,
        OrganizationManifestAssetV1, OrganizationRemedyRequestV1, OrganizationSuccessorCandidateV1,
    };

    let candidate = OrganizationSuccessorCandidateV1 {
        candidate_id: "candidate".into(),
        controller_root_id: "candidate-root".into(),
        controller_player_id: "candidate-player".into(),
        controller_source_id: "candidate-source".into(),
        controller_generation: 1,
        scope_refs: BTreeSet::from(["asset".into()]),
        budget_ref: "agent:candidate".into(),
        budget_snapshot: BTreeMap::from([("alloy_plate".into(), 77)]),
        qualification_receipts: BTreeSet::from(["job:1".into(), "job:2".into()]),
        conflict_disclosures: BTreeSet::from(["disclosure".into()]),
        acceptance_receipt: "acceptance".into(),
        submitted_at_clock: 10,
        submission_order: 1,
        appointed: false,
    };
    let remedy = OrganizationRemedyRequestV1 {
        request_id: "appeal".into(),
        requester_id: "recipient".into(),
        kind: oasis7_agent_api::world_action::OrganizationRemedyKindV1::Appeal,
        scope_refs: BTreeSet::from(["asset".into()]),
        fact_ref: "fact".into(),
        evidence_refs: vec!["private-evidence".into()],
        admitted_at_clock: 5,
        deadline_clock: 20,
        outcome: None,
        supported_scope_refs: BTreeSet::new(),
        adjudication_ref: None,
        resolved_by: None,
    };
    let case = OrganizationContinuityCaseV1 {
        case_id: "case".into(),
        organization_id: "org".into(),
        registration_receipt: "registration".into(),
        policy_version: 1,
        origin: OrganizationCaseOriginV1::Dormancy,
        revision: 8,
        phase: crate::world_rules::OrganizationContinuityPhaseV1::Protection,
        disposition_stage: OrganizationDispositionStageV1::Frozen,
        manifest_root: "a".repeat(64),
        manifest_asset_refs: BTreeSet::from(["asset".into()]),
        recipients: BTreeSet::from(["owner".into(), "recipient".into()]),
        trigger_activity_receipt: Some("activity".into()),
        opened_at_world_tick: 10,
        opened_at_clock: 1,
        notice_receipt: Some("notice".into()),
        notified_at_clock: Some(2),
        notified_at_world_tick: Some(12),
        protection_started_at_clock: Some(2),
        protection_ends_at_clock: Some(20),
        candidate_window_starts_at_clock: Some(15),
        disposition_started_at_clock: None,
        candidates: BTreeMap::from([("candidate".into(), candidate)]),
        remedies: BTreeMap::from([("appeal".into(), remedy)]),
        quarantined_scope_refs: BTreeSet::new(),
        handoff_candidate_id: None,
        handoff_scope_refs: BTreeSet::new(),
        handoff_expires_at_clock: None,
        stage_receipts: vec!["case".into()],
    };
    let registration = crate::world_rules::OrganizationRegistrationV1 {
        organization_id: "org".into(),
        founder_id: "owner".into(),
        policy_version: 1,
        enabled_at_world_tick: 1,
        last_accounted_world_tick: 1,
        activity_evidence_complete_through_world_tick: 1,
        activity_evidence_paused_reason: Some("activity_evidence_unavailable".into()),
        availability_paused_reason: Some("availability_unknown".into()),
        world_binding: WORLD_BINDING.into(),
        charter_hash: "b".repeat(64),
        registration_receipt: "registration".into(),
        manifest_root: "a".repeat(64),
        manifest_asset_refs: BTreeSet::from(["asset".into()]),
        designated_successors: vec!["candidate".into()],
        neutral_maintainers: vec![],
        appointment_authorities: BTreeSet::from(["owner".into()]),
        remedy_adjudicators: BTreeSet::from(["owner".into()]),
        residual_share_bps: BTreeMap::from([("owner".into(), 10_000)]),
        controller_bindings: BTreeMap::new(),
        membership_controller_bindings: BTreeMap::new(),
        activity_sources: BTreeMap::new(),
        activity_observation_started_at_world_tick: None,
        activity_close_cursor_world_tick: 1,
        activity_close_hash: None,
        activity_last_closed_event_id: None,
        activity_last_closed_event_hash: None,
    };
    let asset = OrganizationManifestAssetV1 {
        asset_ref: "asset".into(),
        source_receipt: "deposit".into(),
        organization_id: "org".into(),
        contributor_id: "recipient".into(),
        material: "alloy_plate".into(),
        amount: 77,
        held_in_trust: true,
        version: 1,
        disposed: false,
        effect_receipt: None,
    };
    let continuity = OrganizationContinuityStateV1 {
        registrations: BTreeMap::from([("org".into(), registration)]),
        assets: BTreeMap::from([("asset".into(), asset)]),
        cases: BTreeMap::from([("case".into(), case)]),
        ..Default::default()
    };
    let public = continuity.for_reader(None);
    let public_case = &public.cases["case"];
    assert_eq!(
        public_case.phase,
        crate::world_rules::OrganizationContinuityPhaseV1::Protection
    );
    assert_eq!(public_case.protection_ends_at_clock, Some(20));
    assert!(public.assets.is_empty());
    assert!(
        public_case.candidates["candidate"]
            .budget_snapshot
            .is_empty()
    );
    assert!(public_case.remedies["appeal"].evidence_refs.is_empty());
    assert_eq!(public_case.remedies["appeal"].outcome, None);
    assert_eq!(
        public.registrations["org"]
            .availability_paused_reason
            .as_deref(),
        Some("availability_unknown")
    );

    let owner = continuity.for_reader(Some("owner"));
    assert_eq!(owner.assets["asset"].amount, 77);
    assert_eq!(
        owner.cases["case"].remedies["appeal"].evidence_refs,
        vec!["private-evidence"]
    );
}

fn outer_legacy_deposit() -> WorldRuleCommandV1 {
    WorldRuleCommandV1::DepositOrganizationEstate {
        organization_id: "org".into(),
        material: "alloy_plate".into(),
        amount: 7,
        held_in_trust: true,
    }
}

#[test]
fn availability_uses_world_bound_threshold_signatures_and_pauses_clock() {
    let mut state = configured_organization_fixture();
    let command = outer(enable_command(&state));
    approve(&mut state, "owner", "enable-org", &command);
    apply(
        &mut state,
        &event_with_binding("owner", "enable", command, WORLD_BINDING),
        10,
    );
    let signers = install_finality_registry(&mut state);
    let world_binding = WORLD_BINDING;

    let available = ContinuityCommand::RecordAvailability {
        organization_id: "org".into(),
        from_world_tick: 11,
        through_world_tick: 11,
        world_writes_available: true,
        notice_available: true,
        remedy_available: true,
        certificate: availability_certificate(
            &state,
            &signers,
            "org",
            world_binding,
            11,
            11,
            true,
            true,
            true,
        ),
    };
    apply(
        &mut state,
        &event_with_binding("owner", "availability-11", outer(available), world_binding),
        11,
    );
    assert_eq!(
        state
            .world_rules
            .as_ref()
            .unwrap()
            .organization_continuity
            .continuity_clock_at("org", 11),
        1
    );

    let paused = ContinuityCommand::RecordAvailability {
        organization_id: "org".into(),
        from_world_tick: 12,
        through_world_tick: 20,
        world_writes_available: false,
        notice_available: true,
        remedy_available: true,
        certificate: availability_certificate(
            &state,
            &signers,
            "org",
            world_binding,
            12,
            20,
            false,
            true,
            true,
        ),
    };
    apply(
        &mut state,
        &event_with_binding("owner", "availability-12-20", outer(paused), world_binding),
        20,
    );
    assert_eq!(
        state
            .world_rules
            .as_ref()
            .unwrap()
            .organization_continuity
            .continuity_clock_at("org", 20),
        1
    );

    let mut wrong_world = availability_certificate(
        &state,
        &signers,
        "org",
        "other-world",
        21,
        21,
        true,
        true,
        true,
    );
    wrong_world.proposal_id = 99;
    let wrong = outer(ContinuityCommand::RecordAvailability {
        organization_id: "org".into(),
        from_world_tick: 21,
        through_world_tick: 21,
        world_writes_available: true,
        notice_available: true,
        remedy_available: true,
        certificate: wrong_world,
    });
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event_with_binding("owner", "wrong-world", wrong, world_binding),
            21
        )
        .is_err()
    );
}

#[test]
fn unknown_availability_is_finalized_before_late_certificates_can_backfill_it() {
    let mut state = configured_organization_fixture();
    let command = outer(enable_command(&state));
    approve(&mut state, "owner", "enable-org", &command);
    apply(
        &mut state,
        &event_with_binding("owner", "enable", command, WORLD_BINDING),
        10,
    );
    let signers = install_finality_registry(&mut state);

    let first_interval = ContinuityCommand::RecordAvailability {
        organization_id: "org".into(),
        from_world_tick: 11,
        through_world_tick: 11,
        world_writes_available: true,
        notice_available: true,
        remedy_available: true,
        certificate: availability_certificate(
            &state,
            &signers,
            "org",
            WORLD_BINDING,
            11,
            11,
            true,
            true,
            true,
        ),
    };
    apply(
        &mut state,
        &event_with_binding(
            "owner",
            "availability-11",
            outer(first_interval),
            WORLD_BINDING,
        ),
        12,
    );

    let continuity = &state.world_rules.as_ref().unwrap().organization_continuity;
    let lifecycle = continuity.lifecycle_commands(&state, 12).unwrap();
    assert_eq!(lifecycle.len(), 1);
    let (actor, request_id, command) = lifecycle.into_iter().next().unwrap();
    assert_eq!(actor, "owner");
    assert!(matches!(
        &command,
        ContinuityCommand::FinalizeAvailabilityUnknown {
            organization_id,
            through_world_tick: 12
        } if organization_id == "org"
    ));
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event_with_binding(
                "owner",
                "client-finalize",
                outer(command.clone()),
                WORLD_BINDING,
            ),
            12,
        )
        .is_err()
    );
    apply(
        &mut state,
        &event_with_binding_and_action_id(0, &actor, &request_id, outer(command), WORLD_BINDING),
        12,
    );

    let continuity = &state.world_rules.as_ref().unwrap().organization_continuity;
    let registration = &continuity.registrations["org"];
    assert_eq!(registration.last_accounted_world_tick, 12);
    assert_eq!(
        registration.availability_paused_reason.as_deref(),
        Some("availability_unknown")
    );
    assert_eq!(continuity.availability["org"][1].from_world_tick, 12);
    assert_eq!(continuity.availability["org"][1].through_world_tick, 12);
    assert_eq!(
        continuity.availability["org"][1].evidence_status,
        crate::world_rules::OrganizationAvailabilityEvidenceStatusV1::Unknown
    );
    assert_eq!(continuity.continuity_clock_at("org", 12), 1);

    let late_certificate = availability_certificate(
        &state,
        &signers,
        "org",
        WORLD_BINDING,
        12,
        13,
        true,
        true,
        true,
    );
    let late = outer(ContinuityCommand::RecordAvailability {
        organization_id: "org".into(),
        from_world_tick: 12,
        through_world_tick: 13,
        world_writes_available: true,
        notice_available: true,
        remedy_available: true,
        certificate: late_certificate,
    });
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event_with_binding("owner", "late-availability", late, WORLD_BINDING),
            13,
        )
        .is_err()
    );

    let current_interval = ContinuityCommand::RecordAvailability {
        organization_id: "org".into(),
        from_world_tick: 13,
        through_world_tick: 13,
        world_writes_available: true,
        notice_available: true,
        remedy_available: true,
        certificate: availability_certificate(
            &state,
            &signers,
            "org",
            WORLD_BINDING,
            13,
            13,
            true,
            true,
            true,
        ),
    };
    apply(
        &mut state,
        &event_with_binding(
            "owner",
            "availability-13",
            outer(current_interval),
            WORLD_BINDING,
        ),
        13,
    );
    let continuity = &state.world_rules.as_ref().unwrap().organization_continuity;
    assert_eq!(continuity.continuity_clock_at("org", 13), 2);
    assert_eq!(continuity.availability["org"][1].through_world_tick, 12);
}

#[test]
fn replayed_availability_interval_and_activity_self_report_are_rejected() {
    let mut continuity = OrganizationContinuityStateV1::default();
    assert!(
        !continuity
            .record_activity_from_event(
                "legacy-org",
                "fake-delivery",
                1,
                crate::world_rules::OrganizationActivityKindV1::FulfilledDelivery,
            )
            .unwrap()
    );
}

#[test]
fn shared_authority_verifier_rejects_duplicate_keys_and_invalid_stakes() {
    fn signed_certificate(
        manifest_hash: &str,
        signing_keys: &BTreeMap<String, SigningKey>,
    ) -> GovernanceFinalityCertificate {
        let signers = signing_keys.keys().cloned().collect::<Vec<_>>();
        let validator_set_hash = crate::util::sha256_hex(signers.join("|").as_bytes());
        let stake_root = crate::util::sha256_hex(b"finality-a:1|finality-b:1");
        let mut certificate = GovernanceFinalityCertificate {
            proposal_id: 9,
            manifest_hash: manifest_hash.into(),
            consensus_height: 9,
            epoch_id: 7,
            validator_set_hash,
            stake_root,
            threshold_bps: 10_000,
            min_unique_signers: 2,
            threshold: 2,
            signatures: BTreeMap::new(),
        };
        for (node_id, key) in signing_keys {
            let payload = GovernanceFinalityCertificate::signing_payload_v1(
                certificate.proposal_id,
                &certificate.manifest_hash,
                certificate.consensus_height,
                certificate.epoch_id,
                &certificate.validator_set_hash,
                &certificate.stake_root,
                certificate.threshold_bps,
                certificate.effective_min_unique_signers(),
                node_id,
            );
            certificate.signatures.insert(
                node_id.clone(),
                format!(
                    "{}{}",
                    GovernanceFinalityCertificate::SIGNATURE_PREFIX_ED25519_V1,
                    hex::encode(key.sign(&payload).to_bytes())
                ),
            );
        }
        certificate
    }

    let mut state = organization_fixture();
    let keys = install_finality_registry(&mut state);
    let certificate = signed_certificate("fixed-manifest", &keys);
    assert!(
        crate::world_rules::verify_world_rule_authority(&state, "fixed-manifest", 9, &certificate,)
            .is_ok()
    );

    let repeated_key = hex::encode_upper(keys["finality-a"].verifying_key().to_bytes());
    let registry = state.governance_finality_signer_registry.as_mut().unwrap();
    registry
        .signer_bindings
        .insert("finality-b".into(), repeated_key.clone());
    state
        .node_identity_bindings
        .insert("finality-b".into(), repeated_key);
    let duplicate_key_certificate = signed_certificate(
        "fixed-manifest",
        &BTreeMap::from([
            ("finality-a".into(), keys["finality-a"].clone()),
            ("finality-b".into(), keys["finality-a"].clone()),
        ]),
    );
    assert!(
        crate::world_rules::verify_world_rule_authority(
            &state,
            "fixed-manifest",
            9,
            &duplicate_key_certificate,
        )
        .is_err()
    );

    let mut state = organization_fixture();
    let keys = install_finality_registry(&mut state);
    let certificate = signed_certificate("fixed-manifest", &keys);
    state
        .governance_finality_signer_registry
        .as_mut()
        .unwrap()
        .validator_stakes
        .remove("finality-b");
    assert!(
        crate::world_rules::verify_world_rule_authority(&state, "fixed-manifest", 9, &certificate,)
            .is_err()
    );

    let mut state = organization_fixture();
    let keys = install_finality_registry(&mut state);
    let mut certificate = signed_certificate("fixed-manifest", &keys);
    certificate.threshold_bps = 0;
    assert!(
        crate::world_rules::verify_world_rule_authority(&state, "fixed-manifest", 9, &certificate,)
            .is_err()
    );
}

#[path = "organization_continuity_transfer_tests.rs"]
mod transfer_tests;
