use super::*;
use crate::{FactoryProductionState, FactoryState, MainTokenSupplyState, WorldState};
use oasis7_agent_api::world_action::{GeoPos, ResourceKind, WorldRuleCommandV1};
use std::collections::BTreeMap;

fn manufacturing_base() -> WorldState {
    let mut world = World::new();
    for id in [AGENT_ID, "primary", "recipient"] {
        world.submit_action(Action::RegisterAgent {
            agent_id: id.into(),
            pos: GeoPos::new(0, 0, 0),
        });
    }
    world.step().unwrap();
    world.set_agent_reputation_score(AGENT_ID, 25).unwrap();
    world.set_main_token_supply(MainTokenSupplyState {
        total_supply: 100_000,
        circulating_supply: 100_000,
        ..Default::default()
    });
    world
        .set_main_token_account_balance_with_restricted(AGENT_ID, 100_000, 0, 0)
        .unwrap();
    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: AGENT_ID.into(),
        target_agent_id: "primary".into(),
    });
    world.step().unwrap();
    let mut state = world.state().clone();
    state.factories.insert(
        "assembly".into(),
        FactoryState {
            factory_id: "assembly".into(),
            site_id: "site".into(),
            builder_agent_id: AGENT_ID.into(),
            spec: crate::builtin_factory_spec("factory.assembler.mk1", "assembly").unwrap(),
            input_ledger: MaterialLedgerId::agent(AGENT_ID),
            output_ledger: MaterialLedgerId::agent(AGENT_ID),
            durability_ppm: 1_000_000,
            production: FactoryProductionState::default(),
            site_authority_revision: None,
            site_location_id: None,
            location_anchor_revision: None,
            construction_power_profile_key: None,
            construction_power_profile_revision: None,
            built_at: 0,
        },
    );
    state.material_ledgers.insert(
        MaterialLedgerId::agent(AGENT_ID),
        BTreeMap::from([
            ("alloy_plate".into(), 120),
            ("control_chip".into(), 20),
            ("motor_mk1".into(), 40),
            ("sensor_pack".into(), 20),
        ]),
    );
    state
}
fn industrial_fixture() -> WorldState {
    let mut state = manufacturing_base();
    state.agent_claims.clear();
    state.starter_oc_claims.insert(
        AGENT_ID.into(),
        crate::gameplay_state::StarterOcClaimState {
            agent_id: AGENT_ID.into(),
            player_id: "player".into(),
            public_key: Some("owner-key".into()),
            ..Default::default()
        },
    );
    state
        .factories
        .get_mut("assembly")
        .unwrap()
        .site_location_id = Some("assembly-location".into());
    state
        .factories
        .get_mut("assembly")
        .unwrap()
        .site_authority_revision = Some(1);
    state
        .factories
        .get_mut("assembly")
        .unwrap()
        .location_anchor_revision = Some(1);
    state.location_anchors.insert(
        "assembly-location".into(),
        crate::LocationAnchorV1 {
            location_id: "assembly-location".into(),
            active: true,
            authority_revision: 1,
            ..Default::default()
        },
    );
    state.factory_site_authorities.insert(
        "site".into(),
        crate::FactorySiteAuthorityV1 {
            site_id: "site".into(),
            location_id: "assembly-location".into(),
            owner_agent_id: AGENT_ID.into(),
            active: true,
            chunk_ready: true,
            authority_revision: 1,
            ..Default::default()
        },
    );
    state
        .material_ledgers
        .get_mut(&MaterialLedgerId::agent(AGENT_ID))
        .unwrap()
        .insert("module_rack".into(), 10);
    state
        .agents
        .get_mut(AGENT_ID)
        .unwrap()
        .state
        .resources
        .set(ResourceKind::Electricity, 1000)
        .unwrap();
    state
}

fn manufacturing_action(world: &World) -> Action {
    // Quote directly from the same binding used by the formal World producer.
    let q = world
        .state()
        .agent_manufacturing_quote_with_binding(
            AGENT_ID,
            "assembly",
            world.state().time,
            world.world_rule_binding().as_deref(),
        )
        .unwrap();
    Action::WorldRule {
        actor_id: AGENT_ID.into(),
        request_id: "manufacture".into(),
        command: WorldRuleCommandV1::ConfirmAgentOrder {
            factory_id: "assembly".into(),
            quoted_at: q.quoted_at,
            quote_digest: q.quote_digest,
        },
    }
}

#[test]
fn world_rule_manufacturing_delegation_charges_reserved_electricity_once() {
    for limit in [47, 48] {
        let mut world = World::new_with_state(industrial_fixture());
        let mut allowance = grant();
        allowance.object_id = "assembly".into();
        allowance.action_kinds = vec!["WorldRule".into()];
        allowance.limit_units = limit;
        world.install_agent_delegation_grant(allowance).unwrap();
        let action = manufacturing_action(&world);
        if limit == 47 {
            let before = serde_json::to_value(world.snapshot()).unwrap();
            assert!(
                format!(
                    "{:?}",
                    world
                        .bind_agent_causal_decision(
                            "under-budget",
                            AGENT_ID,
                            &action,
                            Default::default()
                        )
                        .unwrap_err()
                )
                .contains("budget_exhausted")
            );
            assert_eq!(serde_json::to_value(world.snapshot()).unwrap(), before);
            continue;
        }
        let prepared = prepare(
            &mut world,
            "manufacture",
            action.clone(),
            48,
            Default::default(),
        );
        world
            .finalize_cognition_commit(&prepared.commit_id)
            .unwrap();
        assert_eq!(
            world.agent_delegation_authorizations(AGENT_ID).unwrap()[0].spent_units,
            48
        );
        assert_eq!(
            world
                .state()
                .world_rules
                .as_ref()
                .unwrap()
                .agent_orders
                .len(),
            1
        );
        let retry = prepare(&mut world, "business-retry", action, 0, Default::default());
        world.finalize_cognition_commit(&retry.commit_id).unwrap();
        assert_eq!(
            world.agent_delegation_authorizations(AGENT_ID).unwrap()[0].spent_units,
            48
        );
        assert_eq!(
            world
                .state()
                .world_rules
                .as_ref()
                .unwrap()
                .agent_orders
                .len(),
            1
        );
    }
}

#[test]
fn manufacturing_quote_survives_next_tick_depreciation_but_not_unhealthy_factory() {
    for unhealthy in [false, true] {
        let mut state = industrial_fixture();
        assert!(state.factories["assembly"].spec.maintenance_per_tick > 0);
        if unhealthy {
            state.factories.get_mut("assembly").unwrap().durability_ppm = 1;
        }
        let mut world = World::new_with_state(state);
        let action = manufacturing_action(&world);
        let before = world.state().factories["assembly"].durability_ppm;
        let action_id = world.submit_action(action);
        world.step().unwrap();
        assert!(world.state().factories["assembly"].durability_ppm < before);
        let committed = world.journal().events.iter().any(|event| matches!(
            &event.body, WorldEventBody::Domain(DomainEvent::WorldRuleCommitted { action_id: id, .. }) if *id == action_id
        ));
        assert_eq!(committed, !unhealthy);
    }
}

#[cfg(feature = "wasmtime")]
#[test]
fn real_builtin_modules_accept_manufacturing_world_rule_and_keep_next_tick_quote_valid() {
    let mut world = World::new_with_state(industrial_fixture());
    world.install_m1_agent_default_modules("bootstrap").unwrap();
    world
        .install_m5_gameplay_bootstrap_modules("bootstrap")
        .unwrap();
    let action = manufacturing_action(&world);
    let action_id = world.submit_action(action);
    let mut sandbox =
        oasis7_wasm_executor::WasmExecutor::new(crate::tests::test_wasm_executor_config()).unwrap();
    world.step_with_modules(&mut sandbox).unwrap();
    assert!(world.journal().events.iter().any(|event| matches!(
        &event.body, WorldEventBody::Domain(DomainEvent::WorldRuleCommitted { action_id: id, .. }) if *id == action_id
    )));
    assert!(!world.journal().events.iter().any(|event| matches!(
        &event.body, WorldEventBody::Domain(DomainEvent::ActionRejected { action_id: id, .. }) if *id == action_id
    )));
}

fn prepare_for_actor(
    world: &mut World,
    actor: &str,
    nonce: &str,
    action: Action,
    units: u64,
    context: AgentDecisionCausalContextV1,
) -> WorldCommitRecordV1 {
    let bound_world_id = world.chain_resource_manifest().world_id.clone();
    let runtime_world_id = if bound_world_id == "unbound" {
        crate::tests::agent_cognition_runtime_hardening::WORLD_ID
    } else {
        bound_world_id.as_str()
    };
    let finality_epoch = world
        .latest_tick_consensus_record()
        .map_or(0, |record| record.block.header.epoch);
    world
        .bind_cognition_runtime(runtime_world_id, "main", finality_epoch, None, "pending", 0)
        .unwrap();
    let mut envelope = envelope(world);
    envelope.agent_id = actor.into();
    envelope.world_id = runtime_world_id.into();
    let manifest = world.current_manifest_hash().unwrap();
    envelope.runtime_manifest_hash = cognition_digest_v1("oasis7.runtime.manifest.v1", &manifest);
    envelope.base_world_hash = crate::cognition::world_state_binding_digest_v1(
        &envelope.world_id,
        &envelope.branch_id,
        envelope.finality_epoch,
        envelope.finality_block_hash.as_deref(),
        &envelope.finality_status,
        world.state().time,
        &world.current_state_root_hash().unwrap(),
        envelope.reorg_epoch,
        &manifest,
    );
    envelope.capability_snapshot_hash = oasis7_agent_api::h_v1(
        "oasis7.runtime.manifest.v1",
        &world.capability_authorization_root().to_string(),
    )
    .to_string();
    envelope.authority_context_hash = oasis7_agent_api::h_v1(
        "oasis7.runtime.authority-context.v1",
        &world.capability_authorization_root().to_string(),
    )
    .to_string();
    envelope.action = serde_json::to_value(&action).unwrap();
    envelope.agent_turn_id = format!("turn.{nonce}");
    envelope.decision_request_id = format!("request.{nonce}");
    envelope.decision_kind = "act".into();
    envelope.decision_digest = envelope.derive_decision_digest();
    envelope.envelope_digest = envelope.derive_envelope_digest();
    envelope.provider_invocation_key = envelope.derive_provider_invocation_key();
    envelope.envelope_idempotency_key = envelope.derive_envelope_idempotency_key();
    world
        .bind_agent_causal_decision(&envelope.decision_request_id, actor, &action, context)
        .unwrap();
    assert_eq!(
        world.cognition()["agent_delegation"]["decisions"][&envelope.decision_request_id]["quoted_units"],
        units
    );
    world
        .prepare_cognition_envelope(
            envelope.clone(),
            Some(response_artifact_for_envelope(&envelope)),
        )
        .expect("valid fixture cognition envelope")
}

#[test]
fn emergency_authorization_native_budget_tracks_exact_validated_donation_once() {
    for limit in [8, 9] {
        let (mut world, action) = crate::world_rules::emergency_authorize_fixture();
        let actor = action.actor_id().unwrap().to_owned();
        let owner = world.current_agent_owner(&actor).unwrap();
        let Action::WorldRule { command, .. } = &action else {
            unreachable!()
        };
        let mut allowance = grant();
        allowance.agent_id = actor.clone();
        allowance.owner_id = owner.player_id.clone();
        allowance.issuer_id = owner.player_id;
        allowance.source_id = owner.source_id;
        allowance.object_id = command.authority_scope();
        allowance.action_kinds = vec!["WorldRule".into()];
        allowance.valid_until_tick = 2000;
        allowance.limit_units = limit;
        world.install_agent_delegation_grant(allowance).unwrap();
        if limit == 8 {
            let before = serde_json::to_value(world.snapshot()).unwrap();
            assert!(
                format!(
                    "{:?}",
                    world
                        .bind_agent_causal_decision(
                            "donate-short",
                            &actor,
                            &action,
                            Default::default()
                        )
                        .unwrap_err()
                )
                .contains("budget_exhausted")
            );
            assert_eq!(serde_json::to_value(world.snapshot()).unwrap(), before);
            continue;
        }
        let electricity_before = world.state().agents[&actor]
            .state
            .resources
            .get(ResourceKind::Electricity);
        let prepared = prepare_for_actor(
            &mut world,
            &actor,
            "donate",
            action.clone(),
            9,
            Default::default(),
        );
        world
            .finalize_cognition_commit(&prepared.commit_id)
            .unwrap();
        assert_eq!(
            world.agent_delegation_authorizations(&actor).unwrap()[0].spent_units,
            9
        );
        let electricity = world.state().agents[&actor]
            .state
            .resources
            .get(ResourceKind::Electricity);
        assert_eq!(electricity, electricity_before - 9);
        let authorizations = &world
            .state()
            .world_rules
            .as_ref()
            .unwrap()
            .emergency_supply
            .authorizations;
        assert_eq!(authorizations.len(), 1);
        let authorization = authorizations.values().next().unwrap();
        assert_eq!(authorization.provider_id, actor);
        assert_eq!(authorization.remaining, 9);

        world
            .bind_agent_causal_decision("donate-retry", &actor, &action, Default::default())
            .unwrap();
        assert_eq!(
            world.cognition()["agent_delegation"]["decisions"]["donate-retry"]["quoted_units"],
            0
        );
        assert_eq!(
            world.agent_delegation_authorizations(&actor).unwrap()[0].spent_units,
            9
        );
        assert_eq!(
            world.state().agents[&actor]
                .state
                .resources
                .get(ResourceKind::Electricity),
            electricity
        );
    }
}

#[cfg(feature = "wasmtime")]
#[test]
fn real_builtin_modules_accept_restricted_essential_recipe() {
    let (mut world, action) = crate::world_rules::essential_recipe_fixture();
    world.install_m1_agent_default_modules("bootstrap").unwrap();
    world
        .install_m5_gameplay_bootstrap_modules("bootstrap")
        .unwrap();
    let action_id = world.submit_action(action);
    let mut sandbox =
        oasis7_wasm_executor::WasmExecutor::new(crate::tests::test_wasm_executor_config()).unwrap();
    world.step_with_modules(&mut sandbox).unwrap();
    assert!(world.journal().events.iter().any(|event| event.caused_by
        == Some(CausedBy::Action(action_id))
        && matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::RecipeStarted {
                emergency_power_obligation: Some(_),
                ..
            })
        )));
    assert!(!world.journal().events.iter().any(|event| matches!(
        &event.body, WorldEventBody::Domain(DomainEvent::ActionRejected { action_id: id, .. }) if *id == action_id
    )));
}

#[test]
fn manufacturing_workload_uses_existing_factory_maintenance_load_factor() {
    let mut world = World::new_with_state(industrial_fixture());
    let before = world.state().factories["assembly"].durability_ppm;
    world.submit_action(manufacturing_action(&world));
    world.step().unwrap();
    let idle = world.state().factories["assembly"].durability_ppm;
    assert_eq!(before - idle, 1000);
    world.step().unwrap();
    let working = world.state().factories["assembly"].durability_ppm;
    assert_eq!(idle - working, 1500);
    let mut expiry_snapshot = world.snapshot();
    let order = expiry_snapshot
        .state
        .world_rules
        .as_ref()
        .unwrap()
        .agent_orders
        .values()
        .next()
        .unwrap();
    expiry_snapshot.state.time = order.production_deadline().unwrap() - 1;
    let mut expired = World::from_snapshot(expiry_snapshot, world.journal().clone()).unwrap();
    expired.step().unwrap();
    assert_eq!(
        working - expired.state().factories["assembly"].durability_ppm,
        1000
    );
    assert!(
        expired
            .state()
            .world_rules
            .as_ref()
            .unwrap()
            .agent_orders
            .values()
            .all(|order| order.phase == crate::world_rules::AgentIndustrialPhaseV1::Disposed)
    );
    let mut snapshot = world.snapshot();
    snapshot
        .state
        .factory_site_authorities
        .get_mut("site")
        .unwrap()
        .active = false;
    world = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
    world.step().unwrap();
    assert_eq!(
        working - world.state().factories["assembly"].durability_ppm,
        1000
    );
}
