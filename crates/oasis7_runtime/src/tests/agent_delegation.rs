use super::super::*;
use super::agent_cognition_runtime_hardening::{
    AGENT_ID, envelope, response_artifact_for_envelope,
};
use crate::cognition_recovery::cognition_digest_v1;
use crate::gameplay_state::StarterOcClaimState;
use serde_json::json;

#[path = "agent_delegation/dissent.rs"]
mod dissent;
#[path = "agent_delegation/factory.rs"]
mod factory;

fn fixture() -> World {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: AGENT_ID.into(),
        pos: super::pos(0, 0),
    });
    world.step().unwrap();
    let mut state = world.state().clone();
    state.starter_oc_claims.insert(
        AGENT_ID.into(),
        StarterOcClaimState {
            agent_id: AGENT_ID.into(),
            player_id: "player".into(),
            public_key: None,
            amount: 1,
            claimed_at: 0,
            source_treasury_bucket_id: None,
        },
    );
    let mut world = World::new_with_state(state);
    world
        .bind_cognition_runtime(
            super::agent_cognition_runtime_hardening::WORLD_ID,
            "main",
            0,
            None,
            "pending",
            0,
        )
        .unwrap();
    world
}

fn grant() -> AgentDelegationGrantV1 {
    AgentDelegationGrantV1 {
        grant_id: "grant".into(),
        source_id: format!("starter_claim:{AGENT_ID}:0"),
        issuer_id: "player".into(),
        owner_id: "player".into(),
        organization_id: None,
        agent_id: AGENT_ID.into(),
        object_id: AGENT_ID.into(),
        action_kinds: vec!["MoveAgent".into()],
        revision: 1,
        period_id: "period".into(),
        valid_from_tick: 0,
        valid_until_tick: 100,
        limit_units: 3,
        resource_kind: "electricity".into(),
        revoked: false,
    }
}

fn movement(x: i64) -> Action {
    Action::MoveAgent {
        agent_id: AGENT_ID.into(),
        to: super::pos(x, 0),
    }
}

fn accepted(world: &mut World) -> AgentIntentV2 {
    world
        .record_agent_chat_intent("player", AGENT_ID, 1, "move")
        .unwrap();
    world
        .state()
        .agents
        .get(AGENT_ID)
        .unwrap()
        .intent
        .clone()
        .unwrap()
}

fn override_control(intent: &AgentIntentV2, id: &str) -> AgentOwnerControlV1 {
    AgentOwnerControlV1 {
        control_id: id.into(),
        agent_id: AGENT_ID.into(),
        intent_id: intent.intent_id.clone(),
        request_digest: intent.request_digest.clone(),
        grant_id: Some("grant".into()),
        expected_grant_revision: Some(1),
        kind: AgentOwnerControlKindV1::Override,
    }
}

fn recipe_fixture() -> World {
    use super::economy_factory_lifecycle::{factory_spec, install_factory_authority};
    let mut world = fixture();
    let spec = factory_spec("factory", 1, 1, 0);
    for stack in &spec.build_cost {
        world
            .set_ledger_material_balance(
                MaterialLedgerId::agent(AGENT_ID),
                &stack.kind,
                stack.amount,
            )
            .unwrap();
    }
    world
        .set_agent_resource_balance(
            AGENT_ID,
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            20,
        )
        .unwrap();
    install_factory_authority(&mut world, AGENT_ID, "site", "factory", 0);
    world.submit_action(Action::BuildFactory {
        builder_agent_id: AGENT_ID.into(),
        site_id: "site".into(),
        spec,
    });
    world.step().unwrap();
    world.step().unwrap();
    let input = world.state().factories["factory"].input_ledger.clone();
    world
        .set_ledger_material_balance(input.clone(), "iron_ore", 8)
        .unwrap();
    world
        .set_ledger_material_balance(input, "carbon_fuel", 2)
        .unwrap();
    world
}

fn recipe() -> Action {
    Action::ScheduleRecipe {
        requester_agent_id: AGENT_ID.into(),
        factory_id: "factory".into(),
        recipe_id: "recipe.smelter.iron_ingot".into(),
        plan: builtin_recipe_plan("recipe.smelter.iron_ingot", 1).unwrap(),
        logistics_route_ids: vec![],
        logistics_path_ids: vec![],
    }
}

fn prepare(
    world: &mut World,
    nonce: &str,
    action: Action,
    units: u64,
    context: AgentDecisionCausalContextV1,
) -> WorldCommitRecordV1 {
    world
        .bind_cognition_runtime(
            super::agent_cognition_runtime_hardening::WORLD_ID,
            "main",
            0,
            None,
            "pending",
            0,
        )
        .unwrap();
    let mut envelope = envelope(world);
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
        .bind_agent_causal_decision(&envelope.decision_request_id, AGENT_ID, &action, context)
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
        .unwrap()
}

#[test]
fn agent_delegation_rejects_forged_builtin_recipe_plans() {
    for field in [
        "produce",
        "consume",
        "byproducts",
        "power",
        "duration",
        "recipe_id",
        "batches",
        "reject_reason",
    ] {
        let mut world = fixture();
        let mut allowance = grant();
        allowance.object_id = "factory".into();
        allowance.action_kinds = vec!["ScheduleRecipe".into()];
        allowance.limit_units = 1000;
        world.install_agent_delegation_grant(allowance).unwrap();
        let mut plan = builtin_recipe_plan("recipe.smelter.iron_ingot", 1).unwrap();
        let mut recipe_id = "recipe.smelter.iron_ingot";
        match field {
            "produce" => plan.produce[0].amount += 1,
            "consume" => plan.consume.clear(),
            "byproducts" => plan.byproducts.clear(),
            "power" => plan.power_required = 0,
            "duration" => plan.duration_ticks += 1,
            "recipe_id" => recipe_id = "unknown",
            "batches" => plan.accepted_batches = 2,
            "reject_reason" => plan.reject_reason = Some("forged".into()),
            _ => unreachable!(),
        }
        let quoted_units = u64::try_from(plan.power_required).unwrap();
        let action = Action::ScheduleRecipe {
            requester_agent_id: AGENT_ID.into(),
            factory_id: "factory".into(),
            recipe_id: recipe_id.into(),
            plan,
            logistics_route_ids: vec![],
            logistics_path_ids: vec![],
        };
        let before = serde_json::to_value(world.snapshot()).unwrap();
        let decision = AgentDelegationDecisionV1 {
            decision_request_id: format!("forged.{field}"),
            agent_id: AGENT_ID.into(),
            action_digest: cognition_digest_v1(
                "oasis7.cognition.action.v1",
                &serde_json::to_value(&action).unwrap(),
            ),
            grant_id: Some("grant".into()),
            grant_revision: Some(1),
            quote_id: Some(format!("quote.{field}")),
            quoted_units,
            quote_valid_until_tick: 100,
            context: Default::default(),
        };
        assert!(
            world
                .bind_agent_delegation_decision(decision, &action)
                .is_err(),
            "accepted forged {field}"
        );
        assert_eq!(serde_json::to_value(world.snapshot()).unwrap(), before);
    }
}

#[test]
fn agent_delegation_atomic_commit_retry_and_shared_budget() {
    let mut world = recipe_fixture();
    let mut allowance = grant();
    allowance.object_id = "factory".into();
    allowance.action_kinds = vec!["ScheduleRecipe".into()];
    allowance.limit_units = 12;
    world.install_agent_delegation_grant(allowance).unwrap();
    let prepared = prepare(
        &mut world,
        "one",
        recipe(),
        8,
        AgentDecisionCausalContextV1 {
            expected_consequence: json!({"x_cm":10}),
            alternative: json!({"wait":true}),
            ..Default::default()
        },
    );
    let committed = world
        .finalize_cognition_commit(&prepared.commit_id)
        .unwrap();
    let receipts = world.agent_causal_receipts(AGENT_ID).unwrap();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].receipt_id, committed.receipt_id);
    assert_eq!(receipts[0].disposition, "applied");
    assert!(!receipts[0].domain_event_refs.is_empty());
    assert_eq!(
        receipts[0].authorization.as_ref().unwrap().remaining_units,
        4
    );
    assert_eq!(
        world
            .finalize_cognition_commit(&prepared.commit_id)
            .unwrap(),
        committed
    );
    assert_eq!(
        world.agent_delegation_authorizations(AGENT_ID).unwrap()[0].spent_units,
        8
    );
    let mut next = envelope(&world);
    next.action = serde_json::to_value(recipe()).unwrap();
    let decision = AgentDelegationDecisionV1 {
        decision_request_id: "request.two".into(),
        agent_id: AGENT_ID.into(),
        action_digest: cognition_digest_v1("oasis7.cognition.action.v1", &next.action),
        grant_id: Some("grant".into()),
        grant_revision: Some(1),
        quote_id: Some("quote.two".into()),
        quoted_units: 8,
        quote_valid_until_tick: 100,
        context: Default::default(),
    };
    assert!(
        format!(
            "{:?}",
            world
                .bind_agent_delegation_decision(decision, &recipe())
                .unwrap_err()
        )
        .contains("budget_exhausted")
    );
    assert_eq!(world.agent_causal_receipts(AGENT_ID).unwrap(), receipts);
}

#[test]
fn agent_delegation_pending_rechecks_revoke_expiry_owner_and_direct_bypass() {
    for boundary in ["revoked", "expired", "owner"] {
        let mut world = fixture();
        world.install_agent_delegation_grant(grant()).unwrap();
        let prepared = prepare(&mut world, boundary, movement(10), 0, Default::default());
        match boundary {
            "revoked" => {
                let mut revoked = grant();
                revoked.revoked = true;
                revoked.revision = 2;
                world.install_agent_delegation_grant(revoked).unwrap();
            }
            "expired" => {
                let mut snapshot = world.snapshot();
                snapshot.cognition["agent_delegation"]["decisions"]
                    [format!("request.{boundary}")]["quote_valid_until_tick"] = json!(0);
                world = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
            }
            _ => {
                let mut snapshot = world.snapshot();
                snapshot
                    .state
                    .starter_oc_claims
                    .get_mut(AGENT_ID)
                    .unwrap()
                    .player_id = "new-owner".into();
                world = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
            }
        }
        let before = world.clone();
        assert!(
            world
                .finalize_cognition_commit(&prepared.commit_id)
                .is_err()
        );
        assert_eq!(
            world.current_state_root_hash().unwrap(),
            before.current_state_root_hash().unwrap()
        );
        assert_eq!(
            world.agent_delegation_authorizations(AGENT_ID).unwrap()[0].spent_units,
            0
        );
        assert!(world.agent_causal_receipts(AGENT_ID).unwrap().is_empty());
    }
    let mut world = fixture();
    world.install_agent_delegation_grant(grant()).unwrap();
    let action_id = world.submit_action(movement(10));
    world.step().unwrap();
    assert!(world.journal().events.iter().any(|e| matches!(&e.body, WorldEventBody::Domain(DomainEvent::ActionRejected { action_id: id, .. }) if *id == action_id)));
    assert_eq!(
        world.agent_delegation_authorizations(AGENT_ID).unwrap()[0].spent_units,
        0
    );
}

#[test]
fn agent_delegation_source_scope_dissent_override_and_legacy() {
    let mut world = fixture();
    assert!(world.agent_causal_receipts(AGENT_ID).unwrap().is_empty());
    let mut org = grant();
    org.organization_id = Some("alliance".into());
    assert!(
        format!(
            "{:?}",
            world.install_agent_delegation_grant(org).unwrap_err()
        )
        .contains("organization_source_unsupported")
    );
    world.install_agent_delegation_grant(grant()).unwrap();
    let action = movement(10);
    let mut decision = AgentDelegationDecisionV1 {
        decision_request_id: "request".into(),
        agent_id: AGENT_ID.into(),
        action_digest: cognition_digest_v1(
            "oasis7.cognition.action.v1",
            &serde_json::to_value(&action).unwrap(),
        ),
        grant_id: Some("grant".into()),
        grant_revision: Some(1),
        quote_id: Some("quote".into()),
        quoted_units: 0,
        quote_valid_until_tick: 100,
        context: AgentDecisionCausalContextV1 {
            dissent: Some("prefer wait".into()),
            ..Default::default()
        },
    };
    world
        .bind_agent_delegation_decision(decision.clone(), &action)
        .expect("valid delegation is sufficient despite advisory dissent");
    decision.context.override_actor = Some("other".into());
    assert!(
        format!(
            "{:?}",
            world
                .bind_agent_delegation_decision(decision.clone(), &action)
                .unwrap_err()
        )
        .contains("override_owner_mismatch")
    );
    decision.context.override_actor = Some("player".into());
    assert!(
        format!(
            "{:?}",
            world
                .bind_agent_delegation_decision(decision, &action)
                .unwrap_err()
        )
        .contains("override_context_untrusted")
    );
}

#[test]
fn agent_delegation_override_cannot_cross_hard_boundary_and_restart_keeps_receipt() {
    let mut world = fixture();
    let mut snapshot = world.snapshot();
    snapshot.state.gameplay_policy.forbidden_location_ids = vec!["10:0:0".into()];
    world = World::new_with_state(snapshot.state);
    world.install_agent_delegation_grant(grant()).unwrap();
    let intent = accepted(&mut world);
    let control = override_control(&intent, "control.hard-boundary");
    world
        .bind_agent_owner_control("player", control.clone())
        .unwrap();
    let prepared = prepare(
        &mut world,
        "hard-boundary",
        movement(10),
        0,
        AgentDecisionCausalContextV1 {
            dissent: Some("unsafe".into()),
            intent_id: Some(intent.intent_id),
            correction_refs: vec!["correction".into()],
            ..Default::default()
        },
    );
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .unwrap();
    let receipts = world.agent_causal_receipts(AGENT_ID).unwrap();
    assert_eq!(receipts[0].disposition, "not_applied");
    assert!(receipts[0].hard_boundary.is_some());
    assert_eq!(receipts[0].correction_refs, vec!["correction"]);
    assert_eq!(receipts[0].owner_control_refs, vec![control.control_id]);
    let mut restored = World::from_snapshot(world.snapshot(), world.journal().clone()).unwrap();
    assert_eq!(restored.agent_causal_receipts(AGENT_ID).unwrap(), receipts);
    restored
        .finalize_cognition_commit(&prepared.commit_id)
        .unwrap();
    assert_eq!(restored.agent_causal_receipts(AGENT_ID).unwrap(), receipts);
}

#[test]
fn agent_delegation_provider_cannot_omit_grant_or_forge_zero_quote() {
    let mut world = recipe_fixture();
    let mut allowance = grant();
    allowance.object_id = "factory".into();
    allowance.action_kinds = vec!["ScheduleRecipe".into()];
    allowance.limit_units = 12;
    world.install_agent_delegation_grant(allowance).unwrap();
    let action = recipe();
    let mut decision = AgentDelegationDecisionV1 {
        decision_request_id: "request.forged".into(),
        agent_id: AGENT_ID.into(),
        action_digest: cognition_digest_v1(
            "oasis7.cognition.action.v1",
            &serde_json::to_value(&action).unwrap(),
        ),
        grant_id: None,
        grant_revision: None,
        quote_id: None,
        quoted_units: 0,
        quote_valid_until_tick: 100,
        context: Default::default(),
    };
    assert!(
        format!(
            "{:?}",
            world
                .bind_agent_delegation_decision(decision.clone(), &action)
                .unwrap_err()
        )
        .contains("grant_binding_required")
    );
    decision.grant_id = Some("grant".into());
    decision.grant_revision = Some(1);
    decision.quote_id = Some("forged".into());
    assert!(
        format!(
            "{:?}",
            world
                .bind_agent_delegation_decision(decision, &action)
                .unwrap_err()
        )
        .contains("quote_units_mismatch")
    );
    world
        .bind_agent_causal_decision("request.real", AGENT_ID, &action, Default::default())
        .unwrap();
    assert_eq!(
        world.cognition()["agent_delegation"]["decisions"]["request.real"]["quoted_units"],
        8
    );
}

#[test]
fn agent_delegation_concurrent_prepared_actions_cannot_duplicate_allowance() {
    let mut world = recipe_fixture();
    let mut allowance = grant();
    allowance.object_id = "factory".into();
    allowance.action_kinds = vec!["ScheduleRecipe".into()];
    allowance.limit_units = 12;
    world
        .install_agent_delegation_grant(allowance.clone())
        .unwrap();
    let first = prepare(
        &mut world,
        "concurrent-one",
        recipe(),
        8,
        Default::default(),
    );
    let second = prepare(
        &mut world,
        "concurrent-two",
        recipe(),
        8,
        Default::default(),
    );
    world.finalize_cognition_commit(&first.commit_id).unwrap();
    assert!(world.finalize_cognition_commit(&second.commit_id).is_err());
    assert_eq!(
        world.agent_delegation_authorizations(AGENT_ID).unwrap()[0].spent_units,
        8
    );
    assert_eq!(world.agent_causal_receipts(AGENT_ID).unwrap().len(), 1);
    allowance.revision = 2;
    allowance.limit_units = 10;
    assert!(world.install_agent_delegation_grant(allowance).is_err());
    let restored = World::from_snapshot(world.snapshot(), world.journal().clone()).unwrap();
    assert_eq!(
        restored.agent_delegation_authorizations(AGENT_ID).unwrap()[0].remaining_units,
        4
    );
}

#[test]
fn agent_delegation_transfer_binds_exact_asset_source_destination_and_domain_receipt() {
    let mut world = fixture();
    world
        .set_ledger_material_balance(MaterialLedgerId::agent(AGENT_ID), "iron", 10)
        .unwrap();
    let action = Action::TransferMaterial {
        requester_agent_id: AGENT_ID.into(),
        from_ledger: MaterialLedgerId::agent(AGENT_ID),
        to_ledger: MaterialLedgerId::agent("recipient"),
        kind: "iron".into(),
        amount: 2,
        distance_km: 0,
        priority: None,
        route_id: None,
        route_ids: vec![],
        auto_reroute: false,
    };
    let mut allowance = grant();
    allowance.object_id = format!("agent:{AGENT_ID}->agent:recipient:iron");
    allowance.action_kinds = vec!["TransferMaterial".into()];
    world.install_agent_delegation_grant(allowance).unwrap();
    let prepared = prepare(
        &mut world,
        "transfer",
        action.clone(),
        0,
        Default::default(),
    );
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .unwrap();
    let receipt = world
        .agent_causal_receipts(AGENT_ID)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(receipt.disposition, "applied");
    assert_eq!(receipt.action_kind, "TransferMaterial");
    assert!(!receipt.domain_event_refs.is_empty());
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::agent(AGENT_ID), "iron"),
        8
    );
    let mut wrong = action;
    if let Action::TransferMaterial { to_ledger, .. } = &mut wrong {
        *to_ledger = MaterialLedgerId::agent("other");
    }
    assert!(
        format!(
            "{:?}",
            world
                .bind_agent_causal_decision("wrong", AGENT_ID, &wrong, Default::default())
                .unwrap_err()
        )
        .contains("grant_scope_mismatch")
    );
}

#[test]
fn agent_delegation_pending_revocation_publishes_exact_replan_terminal_without_effect() {
    let mut world = fixture();
    world.install_agent_delegation_grant(grant()).unwrap();
    let intent = accepted(&mut world);
    let prepared = prepare(
        &mut world,
        "pending-revoke",
        movement(10),
        0,
        AgentDecisionCausalContextV1 {
            intent_id: Some(intent.intent_id.clone()),
            ..Default::default()
        },
    );
    let before = world.journal().len();
    let mut revoked = grant();
    revoked.revision = 2;
    revoked.revoked = true;
    world.install_agent_delegation_grant(revoked).unwrap();
    let denied_error = world
        .finalize_cognition_commit(&prepared.commit_id)
        .unwrap_err();
    assert!(
        format!("{denied_error:?}").contains("agent_delegation:"),
        "{denied_error:?}"
    );
    let terminal = world
        .state()
        .agent_intent_ledger
        .get(&intent.intent_id)
        .unwrap();
    assert_eq!(terminal.status, "rejected");
    assert!(
        terminal
            .reason_code
            .as_deref()
            .unwrap()
            .contains("replan_required")
    );
    assert!(
        world
            .journal()
            .events
            .iter()
            .skip(before)
            .all(|event| !matches!(
                event.body,
                WorldEventBody::Domain(DomainEvent::AgentMoved { .. })
                    | WorldEventBody::ReceiptAppended(_)
            ))
    );
    assert!(world.agent_causal_receipts(AGENT_ID).unwrap().is_empty());
    let restored = World::from_snapshot(world.snapshot(), world.journal().clone()).unwrap();
    assert_eq!(
        restored.state().agent_intent_ledger[&intent.intent_id].status,
        "rejected"
    );
}

#[test]
fn agent_delegation_owner_override_is_authenticated_one_shot_and_interrupt_is_effect_free() {
    let mut world = fixture();
    world.install_agent_delegation_grant(grant()).unwrap();
    let intent = accepted(&mut world);
    let context = AgentDecisionCausalContextV1 {
        intent_id: Some(intent.intent_id.clone()),
        dissent: Some("prefer wait".into()),
        ..Default::default()
    };
    world
        .bind_agent_causal_decision("objection", AGENT_ID, &movement(10), context.clone())
        .expect("advisory dissent does not block an authorized intent");
    assert_eq!(
        world.state().agent_intent_ledger[&intent.intent_id].status,
        "accepted"
    );
    let control = override_control(&intent, "owner-override");
    assert!(
        world
            .bind_agent_owner_control("other", control.clone())
            .is_err()
    );
    assert!(
        world
            .agent_owner_control_replay("player", "absent")
            .unwrap()
            .is_none()
    );
    let acknowledged = world
        .bind_agent_owner_control("player", control.clone())
        .unwrap();
    assert_eq!(
        world
            .agent_owner_control_replay("player", "owner-override")
            .unwrap(),
        Some((control.clone(), acknowledged.clone()))
    );
    let prepared = prepare(
        &mut world,
        "after-override",
        movement(10),
        0,
        context.clone(),
    );
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .unwrap();
    let receipt = world
        .agent_causal_receipts(AGENT_ID)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(receipt.override_actor.as_deref(), Some("player"));
    assert_eq!(receipt.dissent.as_deref(), Some("prefer wait"));
    assert_eq!(receipt.owner_control_refs, vec!["owner-override"]);
    assert_eq!(
        world.bind_agent_owner_control("player", control).unwrap(),
        acknowledged
    );
    let mut reused_override: AgentDelegationDecisionV1 = serde_json::from_value(
        world.cognition()["agent_delegation"]["decisions"]["request.after-override"].clone(),
    )
    .unwrap();
    reused_override.decision_request_id = "reused-override".into();
    assert!(
        world
            .bind_agent_delegation_decision(reused_override, &movement(10))
            .is_err()
    );
    world
        .bind_agent_causal_decision("after-consumed", AGENT_ID, &movement(20), context)
        .expect("the grant still permits a new decision without reusing the override");
    let rebound = &world.cognition()["agent_delegation"]["decisions"]["after-consumed"]["context"];
    assert!(rebound["override_actor"].is_null());
    assert_eq!(rebound["owner_control_refs"], json!([]));
    let interrupt = AgentOwnerControlV1 {
        control_id: "owner-interrupt".into(),
        agent_id: AGENT_ID.into(),
        intent_id: intent.intent_id.clone(),
        request_digest: intent.request_digest,
        grant_id: None,
        expected_grant_revision: None,
        kind: AgentOwnerControlKindV1::Interrupt,
    };
    let count = world.agent_causal_receipts(AGENT_ID).unwrap().len();
    let terminal = world
        .bind_agent_owner_control("player", interrupt.clone())
        .unwrap();
    assert_eq!(terminal.status, "cancelled");
    assert_eq!(
        world.bind_agent_owner_control("player", interrupt).unwrap(),
        terminal
    );
    assert_eq!(world.agent_causal_receipts(AGENT_ID).unwrap().len(), count);
}

#[test]
fn agent_delegation_provider_artifact_cannot_replace_authority_namespace() {
    let mut world = fixture();
    world.install_agent_delegation_grant(grant()).unwrap();
    let before = world.agent_delegation_authorizations(AGENT_ID).unwrap();
    let mut artifact = response_artifact_for_envelope(&envelope(&world));
    artifact["agent_delegation"] = json!({"grants":{"forged":{"limit_units":u64::MAX}},"spent":{}});
    let _ = world.prepare_cognition_envelope(envelope(&world), Some(artifact));
    assert_eq!(
        world.agent_delegation_authorizations(AGENT_ID).unwrap(),
        before
    );
    assert!(
        world
            .agent_owner_control_replay("player", "forged")
            .unwrap()
            .is_none()
    );
}

#[test]
fn agent_delegation_unsupported_scope_cannot_install_or_publish_effects() {
    let mut world = fixture();
    for kind in ["ScheduleRecipeWithModule", "UnknownAction"] {
        let mut unsupported = grant();
        unsupported.action_kinds = vec![kind.into()];
        assert!(
            format!(
                "{:?}",
                world
                    .install_agent_delegation_grant(unsupported)
                    .unwrap_err()
            )
            .contains("grant_invalid")
        );
    }
    world.install_agent_delegation_grant(grant()).unwrap();
    let module = Action::ScheduleRecipeWithModule {
        requester_agent_id: AGENT_ID.into(),
        factory_id: "factory".into(),
        recipe_id: "recipe".into(),
        module_id: "module".into(),
        desired_batches: 1,
        deterministic_seed: 1,
    };
    assert!(
        world
            .bind_agent_causal_decision("unsupported", AGENT_ID, &module, Default::default())
            .is_err()
    );
    let foreign = Action::TransferMaterial {
        requester_agent_id: AGENT_ID.into(),
        from_ledger: MaterialLedgerId::site("foreign-site"),
        to_ledger: MaterialLedgerId::agent(AGENT_ID),
        kind: "iron".into(),
        amount: 1,
        distance_km: 0,
        priority: None,
        route_id: None,
        route_ids: vec![],
        auto_reroute: false,
    };
    assert!(
        world
            .bind_agent_causal_decision("foreign", AGENT_ID, &foreign, Default::default())
            .is_err()
    );
    assert!(world.agent_causal_receipts(AGENT_ID).unwrap().is_empty());
    assert_eq!(
        world.agent_delegation_authorizations(AGENT_ID).unwrap()[0].spent_units,
        0
    );
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::agent(AGENT_ID), "iron"),
        0
    );
}

#[test]
fn builtin_recipe_quote_is_read_only_and_matches_native_production() {
    let mut world = recipe_fixture();
    let mut allowance = grant();
    allowance.object_id = "factory".into();
    allowance.action_kinds = vec!["ScheduleRecipe".into()];
    allowance.limit_units = 12;
    world.install_agent_delegation_grant(allowance).unwrap();
    let before = serde_json::to_value(world.snapshot()).unwrap();
    let journal = world.journal().clone();
    let quote = world
        .builtin_recipe_quote(AGENT_ID, "factory", "recipe.smelter.iron_ingot", 1)
        .unwrap();
    assert_eq!(
        world
            .builtin_recipe_quote(AGENT_ID, "factory", "recipe.smelter.iron_ingot", 1)
            .unwrap(),
        quote
    );
    assert_eq!(serde_json::to_value(world.snapshot()).unwrap(), before);
    assert_eq!(world.journal(), &journal);
    assert!(quote.rejection.is_none(), "{:?}", quote.rejection);
    assert_eq!(quote.authorization.cost_units, 8);
    assert_eq!(quote.electricity_after, Some(quote.electricity_before - 8));
    let preview = quote.preview.unwrap();
    assert_eq!(
        preview.produce,
        vec![oasis7_wasm_abi::MaterialStack::new("iron_ingot", 3)]
    );
    assert_eq!(
        preview.byproducts,
        vec![oasis7_wasm_abi::MaterialStack::new("slag", 1)]
    );
    let prepared = prepare(
        &mut world,
        "quoted-production",
        quote.action,
        8,
        Default::default(),
    );
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .unwrap();
    assert_eq!(
        world.agent_delegation_authorizations(AGENT_ID).unwrap()[0].spent_units,
        8
    );
}

#[test]
fn builtin_recipe_quote_rejects_authority_and_reports_domain_shortage_without_mutation() {
    let mut world = recipe_fixture();
    assert!(
        world
            .builtin_recipe_quote(AGENT_ID, "factory", "recipe.smelter.iron_ingot", 1)
            .is_err()
    );
    let mut allowance = grant();
    allowance.object_id = "factory".into();
    allowance.action_kinds = vec!["ScheduleRecipe".into()];
    allowance.limit_units = 12;
    world.install_agent_delegation_grant(allowance).unwrap();
    let input = world.state().factories["factory"].input_ledger.clone();
    world
        .set_ledger_material_balance(input, "iron_ore", 0)
        .unwrap();
    let before = serde_json::to_value(world.snapshot()).unwrap();
    let quote = world
        .builtin_recipe_quote(AGENT_ID, "factory", "recipe.smelter.iron_ingot", 1)
        .unwrap();
    assert!(quote.rejection.is_some());
    assert!(quote.preview.is_none());
    assert!(quote.electricity_after.is_none());
    assert_eq!(serde_json::to_value(world.snapshot()).unwrap(), before);
    assert!(
        world
            .builtin_recipe_quote(AGENT_ID, "factory", "iron_ingot", 1)
            .is_err()
    );
    assert!(
        world
            .builtin_recipe_quote(AGENT_ID, "factory", "recipe.smelter.iron_ingot", 0)
            .is_err()
    );
}

#[test]
fn trusted_custom_recipe_plan_keeps_legacy_execution_and_receipt_recovery() {
    let mut world = recipe_fixture();
    let action = Action::ScheduleRecipe {
        requester_agent_id: AGENT_ID.into(),
        factory_id: "factory".into(),
        recipe_id: "trusted.custom".into(),
        plan: oasis7_wasm_abi::RecipeExecutionPlan::accepted(
            1,
            vec![],
            vec![oasis7_wasm_abi::MaterialStack::new("custom_product", 1)],
            vec![],
            2,
            1,
        ),
        logistics_route_ids: vec![],
        logistics_path_ids: vec![],
    };
    let prepared = prepare(&mut world, "trusted-custom", action, 0, Default::default());
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .unwrap();
    let receipts = world.agent_causal_receipts(AGENT_ID).unwrap();
    assert_eq!(receipts[0].disposition, "applied");
    assert!(receipts[0].authorization.is_none());
    let mut restored = World::from_snapshot(world.snapshot(), world.journal().clone()).unwrap();
    restored
        .finalize_cognition_commit(&prepared.commit_id)
        .unwrap();
    assert_eq!(restored.agent_causal_receipts(AGENT_ID).unwrap(), receipts);
}

#[path = "agent_delegation/signed_owner.rs"]
mod signed_owner;

#[path = "agent_delegation/provider_commit_boundary.rs"]
mod provider_commit_boundary;
