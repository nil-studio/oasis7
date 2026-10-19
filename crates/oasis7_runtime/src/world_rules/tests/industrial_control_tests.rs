//! Claim-backed control integration; production helpers live in the parent fixture.
use super::*;
fn delivered() -> (WorldState, String) {
    let mut state = industrial_fixture();
    let id = reserve(&mut state);
    produce(&mut state, &id);
    apply(
        &mut state,
        &event(
            "owner",
            "deliver-control",
            WorldRuleCommandV1::DeliverAgentOrder {
                order_id: id.clone(),
            },
        ),
        24,
    );
    let target = order(&state, &id).target_agent_id.clone();
    (state, target)
}

#[test]
fn manufactured_actor_cannot_reuse_unbound_first_claim_to_expand_without_an_order() {
    let (mut state, child) = delivered();
    state
        .main_token_balances
        .get_mut("owner")
        .unwrap()
        .liquid_balance -= 2_000;
    for actor in [child.as_str(), "primary"] {
        state
            .main_token_balances
            .entry(actor.to_owned())
            .or_default()
            .liquid_balance = 1_000;
    }
    // A legitimate unbound first-claim supplies a complete canonical event;
    // replacing only its actor must not open that route to a manufactured child.
    let mut legitimate = World::new_with_state(state.clone());
    legitimate.submit_action(Action::ClaimAgent {
        claimer_agent_id: "primary".into(),
        target_agent_id: "recipient".into(),
    });
    legitimate.step().unwrap();
    let mut raw = legitimate
        .journal()
        .events
        .iter()
        .find_map(|event| match &event.body {
            crate::WorldEventBody::Domain(event @ DomainEvent::AgentClaimed { .. }) => {
                Some(event.clone())
            }
            _ => None,
        })
        .expect("legitimate first claim must produce a canonical claim event");
    if let DomainEvent::AgentClaimed {
        claimer_agent_id, ..
    } = &mut raw
    {
        *claimer_agent_id = child.clone();
    }
    let mut world = World::new_with_state(state);
    let before = world.snapshot();
    let error = world
        .append_event_for_test(crate::WorldEventBody::Domain(raw), None)
        .unwrap_err();
    assert!(format!("{error:?}").contains("manufactured_actor_cannot_acquire_agent"));
    assert_eq!(world.snapshot(), before);

    let claims = world.state().agent_claims.clone();
    let balances = world.state().main_token_balances.clone();
    let supply = world.state().main_token_supply.clone();
    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: child,
        target_agent_id: "recipient".into(),
    });
    world.step().unwrap();
    assert!(
        world
            .journal()
            .events
            .iter()
            .any(|event| matches!(&event.body,
                crate::WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. })
                    if format!("{reason:?}").contains("manufactured_actor_cannot_acquire_agent")
            ))
    );
    assert_eq!(world.state().agent_claims, claims);
    assert_eq!(world.state().main_token_balances, balances);
    assert_eq!(world.state().main_token_supply, supply);
}

#[test]
fn delivery_freezes_source_and_terminal_claim_cannot_keep_or_invent_owner() {
    let (mut state, target) = delivered();
    let source = state.agent_claims[&target].control_source.clone().unwrap();
    assert_eq!(source.root_agent_id, "owner");
    assert_eq!(source.player_id, "player");
    assert_eq!(source.acquired_event_digest.len(), 64);
    assert_eq!(
        state.resolved_agent_owner(&target).unwrap().player_id,
        "player"
    );
    state.agent_claims.remove(&target);
    assert!(state.resolved_agent_owner(&target).is_none());
    let mut fake = state.starter_oc_claims["owner"].clone();
    fake.agent_id = target.clone();
    state.starter_oc_claims.insert(target.clone(), fake);
    assert!(state.resolved_agent_owner(&target).is_none());
}
#[test]
fn primary_rebind_does_not_transfer_manufactured_agent_to_new_player() {
    let (mut state, target) = delivered();
    state.starter_oc_claims.get_mut("owner").unwrap().player_id = "new-player".into();
    assert!(state.resolved_agent_owner(&target).is_none());
    state.starter_oc_claims.get_mut("owner").unwrap().player_id = "player".into();
    state.starter_oc_claims.get_mut("owner").unwrap().claimed_at += 1;
    assert!(state.resolved_agent_owner(&target).is_none());
}
#[test]
fn current_identity_and_native_source_change_after_root_generation_advance() {
    let (state, target) = delivered();
    let mut world = World::new_with_state(state);
    world
        .bind_cognition_runtime("control-world", "main", 0, None, "pending", 0)
        .unwrap();
    world
        .install_capability_agent_identity("owner", "player", 1)
        .unwrap();
    let old = world.current_agent_identity(&target).unwrap();
    assert_eq!(old.generation, 1);
    let before_rotations = world.snapshot();
    let owner = world.current_agent_owner(&target).unwrap();
    let grant = crate::AgentDelegationGrantV1 {
        grant_id: "old-grant".into(),
        source_id: owner.source_id,
        issuer_id: "player".into(),
        owner_id: "player".into(),
        organization_id: None,
        agent_id: target.clone(),
        object_id: target.clone(),
        action_kinds: vec!["MoveAgent".into()],
        revision: 1,
        period_id: "period".into(),
        valid_from_tick: 0,
        valid_until_tick: 100,
        limit_units: 100,
        resource_kind: "electricity".into(),
        revoked: false,
    };
    world
        .install_test_native_delegation_grant(grant.clone())
        .unwrap();
    world
        .install_capability_agent_identity("owner", "player", u64::MAX - 1)
        .unwrap();
    assert_eq!(world.current_agent_identity(&target).unwrap().generation, 2);
    world
        .install_capability_agent_identity("owner", "player", u64::MAX)
        .unwrap();
    assert_eq!(world.current_agent_identity(&target).unwrap().generation, 3);
    let replayed_world = World::from_snapshot(before_rotations, world.journal().clone()).unwrap();
    assert_eq!(
        replayed_world.capability_revocation_state(),
        world.capability_revocation_state()
    );
    assert_eq!(
        replayed_world.current_agent_identity(&target),
        world.current_agent_identity(&target)
    );
    let restored_world = World::from_snapshot(world.snapshot(), world.journal().clone()).unwrap();
    assert_eq!(
        restored_world.current_agent_identity(&target),
        world.current_agent_identity(&target)
    );
    assert!(
        world
            .install_capability_agent_identity(&target, "player", 100)
            .is_err()
    );
    assert!(world.install_test_native_delegation_grant(grant).is_err());
    let restored: WorldState =
        serde_json::from_slice(&serde_json::to_vec(world.state()).unwrap()).unwrap();
    assert_eq!(
        restored.resolved_agent_owner(&target),
        world.state().resolved_agent_owner(&target)
    );
}
#[test]
fn raw_starter_support_cannot_replace_claim_backed_control() {
    let (mut state, target) = delivered();
    let e = DomainEvent::StarterOcClaimed {
        agent_id: target,
        player_id: "new-player".into(),
        public_key: Some("new-key".into()),
        amount: 100,
        claimed_at: 25,
        source_treasury_bucket_id: None,
    };
    let before = state.clone();
    assert!(state.apply_domain_event(&e, 25).is_err());
    assert_eq!(state, before);
}
#[test]
fn legitimate_successor_claim_creates_fresh_source_instead_of_reviving_old_one() {
    let (mut state, target) = delivered();
    let previous = state
        .agent_claims
        .remove(&target)
        .unwrap()
        .control_source
        .unwrap();
    state.starter_oc_claims.get_mut("owner").unwrap().player_id = "successor-player".into();
    let mut world = World::new_with_state(state);
    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "owner".into(),
        target_agent_id: target.clone(),
    });
    world.step().unwrap();
    assert!(
        world.state().agent_claims.contains_key(&target),
        "{:?}",
        world.journal().events
    );
    let source = world.state().agent_claims[&target]
        .control_source
        .as_ref()
        .unwrap();
    assert_eq!(source.player_id, "successor-player");
    assert_ne!(source.acquired_event_digest, previous.acquired_event_digest);
    assert_eq!(
        world
            .state()
            .resolved_agent_owner(&target)
            .unwrap()
            .player_id,
        "successor-player"
    );
}

#[test]
fn historical_registered_namespace_is_not_reinterpreted_as_manufactured_control() {
    let mut state = WorldState::default();
    let historical = DomainEvent::AgentRegistered {
        registration_contract_version: 0,
        agent_id: "industrial:historical".into(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    };
    assert!(
        !serde_json::to_string(&historical)
            .unwrap()
            .contains("registration_contract_version")
    );
    state.apply_domain_event(&historical, 1).unwrap();
    assert!(!state.is_manufactured_agent("industrial:historical"));
    let before = serde_json::to_value(&state).unwrap();
    let current = DomainEvent::AgentRegistered {
        registration_contract_version: 1,
        agent_id: "industrial:forged".into(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    };
    assert!(state.apply_domain_event(&current, 2).is_err());
    assert_eq!(before, serde_json::to_value(&state).unwrap());
}
#[test]
fn primary_binding_without_economic_claim_cannot_use_plain_claim_as_free_extra_supply() {
    let state = industrial_fixture();
    let mut world = crate::World::new_with_state(state);
    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "owner".into(),
        target_agent_id: "recipient".into(),
    });
    world.step().unwrap();
    assert!(!world.state().agent_claims.contains_key("recipient"));
    assert!(world.journal().events.iter().any(|e| matches!(
        &e.body,
        crate::WorldEventBody::Domain(DomainEvent::ActionRejected { .. })
    )));
}

fn terminal_release_for(state: &WorldState, target: &str) -> DomainEvent {
    let claim = &state.agent_claims[target];
    assert_eq!(claim.claim_bond_locked_restricted_amount, 0);
    DomainEvent::AgentClaimReleased {
        claimer_agent_id: claim.claim_owner_id.clone(),
        target_agent_id: target.into(),
        released_at_epoch: claim.upkeep_paid_through_epoch,
        refunded_bond_amount: claim.locked_bond_amount,
        refunded_bond_restricted_amount: 0,
        refunded_bond_liquid_amount: claim.locked_bond_amount,
        refunded_bond_restricted_sink: Default::default(),
        refunded_bond_restricted_sink_bucket_id: String::new(),
    }
}
#[test]
fn identical_same_tick_successor_acquisitions_use_persistent_sequence_and_never_revive_grants() {
    let (mut state, target) = delivered();
    state.time = 24;
    let mut world = World::new_with_state(state);
    world
        .append_event_for_test(
            crate::WorldEventBody::Domain(terminal_release_for(world.state(), &target)),
            None,
        )
        .unwrap();
    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "owner".into(),
        target_agent_id: target.clone(),
    });
    world.step().unwrap();
    let acquire = world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|e| match &e.body {
            crate::WorldEventBody::Domain(
                d @ DomainEvent::AgentClaimed {
                    target_agent_id, ..
                },
            ) if target_agent_id == &target => Some(d.clone()),
            _ => None,
        })
        .unwrap();
    let first = world.state().agent_claims[&target]
        .control_source
        .clone()
        .unwrap();
    assert_eq!(first.acquisition_sequence, 2);
    world
        .bind_cognition_runtime("sequence-world", "main", 0, None, "pending", 0)
        .unwrap();
    world
        .install_capability_agent_identity("owner", "player", 1)
        .unwrap();
    let old_identity = world.current_agent_identity(&target).unwrap();
    let old_source = world.current_agent_owner(&target).unwrap().source_id;
    let grant = crate::AgentDelegationGrantV1 {
        grant_id: "sequence-old".into(),
        source_id: old_source,
        issuer_id: "player".into(),
        owner_id: "player".into(),
        organization_id: None,
        agent_id: target.clone(),
        object_id: target.clone(),
        action_kinds: vec!["MoveAgent".into()],
        revision: 1,
        period_id: "period".into(),
        valid_from_tick: 0,
        valid_until_tick: 100,
        limit_units: 100,
        resource_kind: "electricity".into(),
        revoked: false,
    };
    world
        .install_test_native_delegation_grant(grant.clone())
        .unwrap();
    let now = world.state().time;
    world
        .append_event_for_test(
            crate::WorldEventBody::Domain(terminal_release_for(world.state(), &target)),
            None,
        )
        .unwrap();
    assert!(!world.state().agent_claims.contains_key(&target));
    assert_eq!(world.state().agent_claim_acquisition_sequences[&target], 2);
    let terminal: WorldState =
        serde_json::from_slice(&serde_json::to_vec(world.state()).unwrap()).unwrap();
    assert_eq!(terminal.agent_claim_acquisition_sequences[&target], 2);
    let before = world.state().clone();
    let prepared =
        crate::world::agent_claim_economic_publication::PreparedAgentClaimEconomic::prepare(
            &before, &acquire, now,
        )
        .unwrap();
    let projection = serde_json::to_value(
        WorldStateProjection::borrowed(&before).with_agent_claim_economic_overlay(&prepared),
    )
    .unwrap();
    let mut overflow = before.clone();
    overflow
        .agent_claim_acquisition_sequences
        .insert(target.clone(), u64::MAX);
    let original = overflow.clone();
    assert!(
        crate::world::agent_claim_economic_publication::PreparedAgentClaimEconomic::prepare(
            &overflow, &acquire, now
        )
        .is_err()
    );
    assert_eq!(overflow, original);
    world
        .append_event_for_test(crate::WorldEventBody::Domain(acquire), None)
        .unwrap();
    assert_eq!(world.state().time, now);
    assert_eq!(projection, serde_json::to_value(world.state()).unwrap());
    let second = world.state().agent_claims[&target]
        .control_source
        .as_ref()
        .unwrap();
    assert_eq!(second.acquisition_sequence, 3);
    assert_ne!(first.acquired_event_digest, second.acquired_event_digest);
    assert_eq!(world.state().agent_claim_acquisition_sequences[&target], 3);
    assert!(world.current_agent_identity(&target).unwrap().generation > old_identity.generation);
    assert!(world.install_test_native_delegation_grant(grant).is_err());
    let restored: WorldState =
        serde_json::from_slice(&serde_json::to_vec(world.state()).unwrap()).unwrap();
    assert_eq!(
        restored.resolved_agent_owner(&target),
        world.state().resolved_agent_owner(&target)
    );
}
#[test]
fn historical_claim_events_do_not_create_acquisition_tombstones_or_change_empty_projection_shape() {
    let mut state = industrial_fixture();
    state.agent_claims.clear();
    state.agent_claim_acquisition_sequences.clear();
    state.starter_oc_claims.clear();
    let mut world = World::new_with_state(state);
    assert!(
        !serde_json::to_value(world.state())
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("agent_claim_acquisition_sequences")
    );
    world.append_legacy_claim_fixture("owner", "recipient");
    assert!(world.state().agent_claim_acquisition_sequences.is_empty());
    assert!(
        !serde_json::to_value(world.state())
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("agent_claim_acquisition_sequences")
    );
}

fn rehash_test_authorization(snapshot: &mut crate::Snapshot) {
    let mut revocation = snapshot.capability_revocation_state.clone();
    revocation.world_service_results.clear();
    snapshot.capability_authorization_root = oasis7_wasm_abi::canonical_hash(&serde_json::json!({
        "grants":snapshot.capability_grants_v2, "revocation":revocation,
        "invocation_contexts":snapshot.capability_invocation_contexts,
        "budget_accounts":snapshot.capability_budget_accounts,
        "nonce_records":snapshot.capability_nonce_records,
        "receipts":snapshot.capability_authorization_receipts,
        "effect_receipt_links":snapshot.capability_effect_receipt_links,
    }))
    .unwrap();
}
#[test]
fn manufactured_identity_counter_overflow_rejects_root_rotation_atomically() {
    let (state, target) = delivered();
    let mut world = World::new_with_state(state);
    world
        .bind_cognition_runtime("counter-world", "main", 0, None, "pending", 0)
        .unwrap();
    world
        .install_capability_agent_identity("owner", "player", 1)
        .unwrap();
    let mut snapshot = world.snapshot();
    snapshot
        .capability_revocation_state
        .manufactured_identity_acquisition_sequences
        .insert(target.clone(), 1);
    snapshot
        .capability_revocation_state
        .agent_identities
        .insert(
            target.clone(),
            crate::capability_authorization::CapabilityAgentIdentity {
                owner_binding: "player".into(),
                generation: u64::MAX,
            },
        );
    rehash_test_authorization(&mut snapshot);
    let mut world = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
    assert_eq!(
        world.current_agent_identity(&target).unwrap().generation,
        u64::MAX
    );
    let before = world.snapshot();
    assert!(
        world
            .install_capability_agent_identity("owner", "player", 2)
            .is_err()
    );
    assert_eq!(world.snapshot(), before);
    // Terminal status may release the claim, but another acquisition cannot
    // install a target whose next identity is no longer representable.
    world
        .append_event_for_test(
            crate::WorldEventBody::Domain(terminal_release_for(world.state(), &target)),
            None,
        )
        .unwrap();
    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "owner".into(),
        target_agent_id: target.clone(),
    });
    world.step().unwrap();
    assert!(!world.state().agent_claims.contains_key(&target));
    assert_eq!(world.state().agent_claim_acquisition_sequences[&target], 1);
    let mut candidate = World::new_with_state(world.state().clone());
    candidate.submit_action(Action::ClaimAgent {
        claimer_agent_id: "owner".into(),
        target_agent_id: target.clone(),
    });
    candidate.step().unwrap();
    let raw = candidate
        .journal()
        .events
        .iter()
        .find_map(|e| match &e.body {
            crate::WorldEventBody::Domain(event @ DomainEvent::AgentClaimed { .. }) => {
                Some(event.clone())
            }
            _ => None,
        })
        .unwrap();
    let before_raw = world.snapshot();
    assert!(
        world
            .append_event_for_test(crate::WorldEventBody::Domain(raw), None)
            .is_err()
    );
    assert_eq!(world.snapshot(), before_raw);
}

#[test]
fn manufactured_identity_cannot_move_to_another_world_manifest() {
    let (mut state, target) = delivered();
    let binding = crate::util::hash_json(&(
        "oasis7-world-rule-world-v1",
        "identity-world-a",
        "identity-chain",
        "identity-genesis",
    ))
    .unwrap();
    state
        .world_rules
        .as_mut()
        .unwrap()
        .agent_orders
        .values_mut()
        .find(|o| o.target_agent_id == target)
        .unwrap()
        .quote
        .world_binding = Some(binding);
    let mut world = World::new_with_state(state);
    world
        .install_capability_agent_identity("owner", "player", 1)
        .unwrap();
    let with_world = |world: &World, world_id: &str| {
        let snapshot = world.snapshot_with_chain_resource_context(
            crate::ChainResourceDerivationContext {
                world_id,
                chain_id: "identity-chain",
                genesis_ref: Some("identity-genesis"),
                created_at_height: 0,
                manifest_height: 0,
                commit_block_hash: None,
                tick: world.state().time,
            },
            "a".repeat(64),
            "b".repeat(64),
        );
        World::from_snapshot(snapshot, world.journal().clone()).unwrap()
    };
    let original = with_world(&world, "identity-world-a");
    assert_eq!(
        original.current_agent_identity(&target).unwrap().generation,
        1
    );
    let mut moved = with_world(&original, "identity-world-b");
    assert!(moved.current_agent_identity(&target).is_none());
    assert!(moved.current_agent_owner(&target).is_none());
    let binding_b = crate::util::hash_json(&(
        "oasis7-world-rule-world-v1",
        "identity-world-b",
        "identity-chain",
        "identity-genesis",
    ))
    .unwrap();
    let raw = |actor: &str, request: &str, command| DomainEvent::WorldRuleCommitted {
        action_id: 99,
        actor_id: actor.into(),
        request_id: request.into(),
        command,
        governance_epoch: 0,
        world_binding: Some(binding_b.clone()),
    };
    let before = moved.snapshot();
    let foreign_actor = raw(
        &target,
        "foreign-actor",
        WorldRuleCommandV1::ConfirmAgentOrder {
            factory_id: "assembly".into(),
            quoted_at: moved.state().time,
            quote_digest: "unused".into(),
        },
    );
    let error = moved
        .append_event_for_test(crate::WorldEventBody::Domain(foreign_actor), None)
        .unwrap_err();
    assert!(format!("{error:?}").contains("manufactured_actor_current_authority_unavailable"));
    assert_eq!(moved.snapshot(), before);
    let source = original.state().resolved_agent_owner(&target).unwrap();
    let registration = oasis7_agent_api::world_action::EmergencyServiceRegistrationV1 {
        obligation_id: "copied-foreign-source".into(),
        beneficiary_id: target.clone(),
        controller_id: source.player_id,
        region_id: "region".into(),
        location_id: "assembly-location".into(),
        registered_at: moved.state().time,
        electricity_per_interval: 10,
        public_critical: false,
        recipe_ids: ["recipe.assembler.gear".into()].into(),
        source_ref: source.source_id,
    };
    let certificate = serde_json::from_value(serde_json::json!({"proposal_id":0,"manifest_hash":"unused","consensus_height":moved.state().time,"signatures":{}})).unwrap();
    let copied_source = raw(
        "owner",
        "foreign-beneficiary",
        WorldRuleCommandV1::MinimumService {
            command: oasis7_agent_api::world_action::MinimumServiceCommandV1::Register {
                registration,
                region_budget_lineage: "lineage".into(),
                certificate,
            },
        },
    );
    let error = moved
        .append_event_for_test(crate::WorldEventBody::Domain(copied_source), None)
        .unwrap_err();
    assert!(
        format!("{error:?}").contains("minimum_service_beneficiary_current_authority_unavailable")
    );
    assert_eq!(moved.snapshot(), before);
}

#[test]
fn manufactured_checkpoints_compose_multiple_root_rotations_in_one_prepared_batch() {
    let (state, target) = delivered();
    let mut world = World::new_with_state(state);
    world
        .install_capability_agent_identity("owner", "player", 1)
        .unwrap();
    let snapshot = world.snapshot();
    let events = [2, u64::MAX]
        .into_iter()
        .map(
            |generation| crate::CapabilityAuthorizationEvent::AgentIdentityInstalled {
                agent_id: "owner".into(),
                identity: crate::capability_authorization::CapabilityAgentIdentity {
                    owner_binding: "player".into(),
                    generation,
                },
            },
        )
        .collect();
    world
        .append_capability_identity_batch_for_test(events)
        .unwrap();
    assert_eq!(world.current_agent_identity(&target).unwrap().generation, 3);
    let replayed = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
    assert_eq!(
        replayed.capability_revocation_state(),
        world.capability_revocation_state()
    );
    assert_eq!(
        replayed.current_agent_identity(&target),
        world.current_agent_identity(&target)
    );
}
