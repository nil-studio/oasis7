use super::*;
use crate::world_rules::{AgentIndustrialPhaseV1 as P, manufacturing_capacity_guard};
use oasis7_agent_api::world_action::ResourceKind;

fn industrial_fixture() -> WorldState {
    let mut state = fixture();
    state.agent_claims.clear();
    state.starter_oc_claims.insert(
        "owner".into(),
        crate::gameplay_state::StarterOcClaimState {
            agent_id: "owner".into(),
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
            owner_agent_id: "owner".into(),
            active: true,
            chunk_ready: true,
            authority_revision: 1,
            ..Default::default()
        },
    );
    state
        .material_ledgers
        .get_mut(&MaterialLedgerId::agent("owner"))
        .unwrap()
        .insert("module_rack".into(), 10);
    state
        .agents
        .get_mut("owner")
        .unwrap()
        .state
        .resources
        .set(ResourceKind::Electricity, 1000)
        .unwrap();
    state
}
fn confirm(state: &WorldState, at: u64) -> WorldRuleCommandV1 {
    let q = state
        .agent_manufacturing_quote("owner", "assembly", at)
        .unwrap();
    WorldRuleCommandV1::ConfirmAgentOrder {
        factory_id: "assembly".into(),
        quoted_at: at,
        quote_digest: q.quote_digest,
    }
}
fn reserve(state: &mut WorldState) -> String {
    let command = confirm(state, 10);
    apply(state, &event("owner", "order", command), 10);
    request_key("owner", "order")
}
fn advance(state: &mut WorldState, id: &str, now: u64) {
    apply(
        state,
        &event(
            "owner",
            &format!("advance-{now}"),
            WorldRuleCommandV1::AdvanceAgentOrder {
                order_id: id.into(),
            },
        ),
        now,
    );
}
fn produce(state: &mut WorldState, id: &str) {
    for now in 11..=23 {
        advance(state, id, now);
    }
}
fn order<'a>(state: &'a WorldState, id: &str) -> &'a AgentIndustrialOrderV1 {
    &state.world_rules.as_ref().unwrap().agent_orders[id]
}

#[test]
fn manufacturing_quote_is_read_only_complete_and_uses_liquid_slot_two() {
    let mut state = industrial_fixture();
    state
        .agents
        .get_mut("owner")
        .unwrap()
        .state
        .resources
        .set(ResourceKind::Electricity, 48)
        .unwrap();
    let before = state.clone();
    let q = state
        .agent_manufacturing_quote("owner", "assembly", 10)
        .unwrap();
    assert_eq!(state, before);
    assert_eq!(q.claim_cost.slot_index, 2);
    assert_eq!(q.upfront_liquid, 488);
    assert_eq!(q.electricity, 48);
    assert_eq!(q.electricity_after_reservation, 0);
    assert_eq!(q.inputs["alloy_plate"], 8);
    assert_eq!(q.inputs["control_chip"], 4);
    assert_eq!(q.inputs["module_rack"], 1);
    assert_eq!(q.expires_at, 15);
    state
        .main_token_balances
        .get_mut("owner")
        .unwrap()
        .liquid_balance = 487;
    state
        .main_token_balances
        .get_mut("owner")
        .unwrap()
        .restricted_starter_claim_balance = 10000;
    assert!(
        state
            .agent_manufacturing_quote("owner", "assembly", 10)
            .is_err()
    );
}
#[test]
fn quote_expiry_and_dependency_drift_do_not_reserve_assets() {
    let state = industrial_fixture();
    let command = confirm(&state, 10);
    let e = event("owner", "order", command);
    assert!(PreparedWorldRule::prepare(&state, &e, 14).is_ok());
    assert!(PreparedWorldRule::prepare(&state, &e, 15).is_err());
    assert!(PreparedWorldRule::prepare(&state, &e, 9).is_err());
    let mut drifted = state.clone();
    drifted
        .main_token_balances
        .get_mut("owner")
        .unwrap()
        .liquid_balance -= 1;
    assert!(PreparedWorldRule::prepare(&drifted, &e, 11).is_err());
    assert_eq!(state.world_rules, None);
    let mut unrelated = state.clone();
    unrelated.time = 12;
    unrelated.reputation_scores.insert("unrelated".into(), 99);
    assert!(PreparedWorldRule::prepare(&unrelated, &e, 12).is_ok());
}
#[test]
fn confirmation_reserves_inputs_electricity_liquid_without_identity_or_claim() {
    let mut state = industrial_fixture();
    let before = state.clone();
    let id = reserve(&mut state);
    assert_eq!(order(&state, &id).phase, P::Reserved);
    assert_eq!(order(&state, &id).completed_work_ticks, 0);
    assert_eq!(
        state.main_token_balances["owner"].liquid_balance,
        before.main_token_balances["owner"].liquid_balance - 488
    );
    assert_eq!(
        state.agents["owner"]
            .state
            .resources
            .get(ResourceKind::Electricity),
        952
    );
    assert_eq!(
        state.material_ledgers[&MaterialLedgerId::agent("owner")]["alloy_plate"],
        112
    );
    assert!(
        !state
            .agents
            .contains_key(&order(&state, &id).target_agent_id)
    );
    assert!(state.agent_claims.is_empty());
    assert!(
        state
            .agent_manufacturing_quote("owner", "assembly", 11)
            .is_err()
    );
}
#[test]
fn cancellation_before_start_refunds_exact_original_assets_once() {
    let mut state = industrial_fixture();
    let before = state.clone();
    let id = reserve(&mut state);
    let e = event(
        "owner",
        "cancel",
        WorldRuleCommandV1::CancelAgentOrder {
            order_id: id.clone(),
        },
    );
    apply(&mut state, &e, 11);
    assert_eq!(order(&state, &id).phase, P::Cancelled);
    assert_eq!(state.main_token_balances, before.main_token_balances);
    assert_eq!(state.material_ledgers, before.material_ledgers);
    assert_eq!(
        state.agents["owner"].state.resources,
        before.agents["owner"].state.resources
    );
    apply(&mut state, &e, 12);
    assert_eq!(state.main_token_balances, before.main_token_balances);
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event(
                "owner",
                "cancel-again",
                WorldRuleCommandV1::CancelAgentOrder { order_id: id }
            ),
            13
        )
        .is_err()
    );
}
#[test]
fn start_deadline_endpoint_cancels_without_consumption() {
    let mut state = industrial_fixture();
    let before = state.clone();
    let id = reserve(&mut state);
    advance(&mut state, &id, 20);
    assert_eq!(order(&state, &id).phase, P::Cancelled);
    assert_eq!(state.material_ledgers, before.material_ledgers);
    assert_eq!(state.main_token_balances, before.main_token_balances);
    let mut last = industrial_fixture();
    let id = reserve(&mut last);
    advance(&mut last, &id, 19);
    assert_eq!(order(&last, &id).phase, P::Producing);
}
#[test]
fn twelve_legal_ticks_produce_lot_and_delivery_settles_frozen_claim_once() {
    let mut state = industrial_fixture();
    let id = reserve(&mut state);
    let target = order(&state, &id).target_agent_id.clone();
    let funds = state.main_token_balances["owner"].liquid_balance;
    produce(&mut state, &id);
    assert_eq!(order(&state, &id).phase, P::AwaitingDelivery);
    assert_eq!(order(&state, &id).completed_work_ticks, 12);
    assert!(!state.agents.contains_key(&target));
    let e = event(
        "owner",
        "deliver",
        WorldRuleCommandV1::DeliverAgentOrder {
            order_id: id.clone(),
        },
    );
    apply(&mut state, &e, 24);
    assert_eq!(order(&state, &id).phase, P::Delivered);
    assert_eq!(state.agent_claims[&target].slot_index, 2);
    assert_eq!(
        state.agent_claims[&target].upfront_restricted_spent_amount,
        0
    );
    assert_eq!(state.main_token_balances["owner"].liquid_balance, funds);
    assert_eq!(state.agents[&target].state.resources, Default::default());
    let restored: WorldState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    assert_eq!(state, restored);
    apply(&mut state, &e, 25);
    assert_eq!(state.main_token_balances["owner"].liquid_balance, funds);
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event(
                "owner",
                "second-deliver",
                WorldRuleCommandV1::DeliverAgentOrder { order_id: id }
            ),
            25
        )
        .is_err()
    );
}
#[test]
fn progress_requires_legal_distinct_ticks_and_preserves_first_block_deadline() {
    let mut state = industrial_fixture();
    let id = reserve(&mut state);
    advance(&mut state, &id, 11);
    advance(&mut state, &id, 12);
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event(
                "owner",
                "duplicate-progress",
                WorldRuleCommandV1::AdvanceAgentOrder {
                    order_id: id.clone()
                }
            ),
            12
        )
        .is_err()
    );
    state.factories.get_mut("assembly").unwrap().durability_ppm = 0;
    advance(&mut state, &id, 13);
    advance(&mut state, &id, 14);
    assert_eq!(order(&state, &id).blocked_at, Some(13));
    assert_eq!(order(&state, &id).completed_work_ticks, 1);
    state.factories.get_mut("assembly").unwrap().durability_ppm = 1_000_000;
    advance(&mut state, &id, 15);
    assert_eq!(order(&state, &id).completed_work_ticks, 2);
    assert_eq!(order(&state, &id).blocked_at, Some(13));
    advance(&mut state, &id, 37);
    assert_eq!(order(&state, &id).phase, P::Disposed);
}
#[test]
fn post_start_disposal_only_returns_claim_funds_and_keeps_input_loss() {
    let mut state = industrial_fixture();
    let before = state.clone();
    let id = reserve(&mut state);
    advance(&mut state, &id, 11);
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event(
                "owner",
                "cancel",
                WorldRuleCommandV1::CancelAgentOrder {
                    order_id: id.clone()
                }
            ),
            12
        )
        .is_err()
    );
    apply(
        &mut state,
        &event(
            "owner",
            "dispose",
            WorldRuleCommandV1::DisposeAgentOrder {
                order_id: id.clone(),
            },
        ),
        12,
    );
    assert_eq!(order(&state, &id).phase, P::Disposed);
    assert_eq!(state.main_token_balances, before.main_token_balances);
    assert_eq!(
        state.material_ledgers[&MaterialLedgerId::agent("owner")]["alloy_plate"],
        112
    );
    assert_eq!(
        state.agents["owner"]
            .state
            .resources
            .get(ResourceKind::Electricity),
        952
    );
}
#[test]
fn delivery_expiry_endpoint_disposes_lot_and_unfreezes_funds() {
    let mut state = industrial_fixture();
    let id = reserve(&mut state);
    produce(&mut state, &id);
    let e = event(
        "owner",
        "deliver",
        WorldRuleCommandV1::DeliverAgentOrder {
            order_id: id.clone(),
        },
    );
    assert!(PreparedWorldRule::prepare(&state, &e, 46).is_ok());
    assert!(PreparedWorldRule::prepare(&state, &e, 47).is_err());
    advance(&mut state, &id, 47);
    assert_eq!(order(&state, &id).phase, P::Disposed);
}
#[test]
fn primary_binding_cap_and_shared_capacity_remain_after_delivery() {
    let mut state = industrial_fixture();
    state.reputation_scores.insert("owner".into(), 10);
    let id = reserve(&mut state);
    assert!(manufacturing_capacity_guard(&state, "owner", Some("recipient"), false).is_err());
    produce(&mut state, &id);
    apply(
        &mut state,
        &event(
            "owner",
            "deliver",
            WorldRuleCommandV1::DeliverAgentOrder { order_id: id },
        ),
        24,
    );
    assert!(manufacturing_capacity_guard(&state, "owner", Some("recipient"), false).is_err());
    assert!(manufacturing_capacity_guard(&state, "owner", Some("owner"), false).is_ok());
    state.starter_oc_claims.clear();
    assert!(
        state
            .agent_manufacturing_quote("owner", "assembly", 25)
            .is_err()
    );
}
#[test]
fn frozen_slot_price_survives_other_claim_release_and_no_restricted_subsidy() {
    let mut state = industrial_fixture();
    state.agent_claims = fixture().agent_claims;
    let id = reserve(&mut state);
    assert_eq!(order(&state, &id).quote.upfront_liquid, 650);
    state.agent_claims.clear();
    produce(&mut state, &id);
    apply(
        &mut state,
        &event(
            "owner",
            "deliver",
            WorldRuleCommandV1::DeliverAgentOrder {
                order_id: id.clone(),
            },
        ),
        24,
    );
    let target = &order(&state, &id).target_agent_id;
    assert_eq!(state.agent_claims[target].slot_index, 3);
    assert_eq!(
        state.agent_claims[target].upfront_restricted_spent_amount,
        0
    );
}
#[test]
fn manufacturing_quote_single_use_and_maximum_request_identity() {
    let mut state = industrial_fixture();
    let command = confirm(&state, 10);
    let request = "r".repeat(128);
    apply(&mut state, &event("owner", &request, command.clone()), 10);
    let id = request_key("owner", &request);
    assert!(order(&state, &id).target_agent_id.len() <= 128);
    apply(
        &mut state,
        &event(
            "owner",
            "cancel",
            WorldRuleCommandV1::CancelAgentOrder { order_id: id },
        ),
        11,
    );
    assert!(PreparedWorldRule::prepare(&state, &event("owner", "different", command), 12).is_err());
}
#[test]
fn raw_identity_registration_cannot_forge_manufactured_target() {
    let mut state = industrial_fixture();
    let id = reserve(&mut state);
    let target = order(&state, &id).target_agent_id.clone();
    assert!(
        state
            .apply_domain_event(
                &DomainEvent::AgentRegistered {
                    registration_contract_version: 1,
                    agent_id: target,
                    pos: GeoPos::new(0, 0, 0)
                },
                11
            )
            .is_err()
    );
}

#[test]
fn missing_material_power_factory_and_source_fail_without_partial_reservations() {
    let original = industrial_fixture();
    let command = confirm(&original, 10);
    for change in 0..5 {
        let mut state = original.clone();
        match change {
            0 => {
                state
                    .material_ledgers
                    .get_mut(&MaterialLedgerId::agent("owner"))
                    .unwrap()
                    .remove("module_rack");
            }
            1 => {
                state
                    .agents
                    .get_mut("owner")
                    .unwrap()
                    .state
                    .resources
                    .set(ResourceKind::Electricity, 47)
                    .unwrap();
            }
            2 => {
                state
                    .factories
                    .get_mut("assembly")
                    .unwrap()
                    .production
                    .active_jobs = 2;
            }
            3 => {
                state
                    .location_anchors
                    .get_mut("assembly-location")
                    .unwrap()
                    .active = false;
            }
            _ => {
                state.starter_oc_claims.clear();
            }
        }
        let before = state.clone();
        assert!(
            PreparedWorldRule::prepare(&state, &event("owner", "order", command.clone()), 10)
                .is_err()
        );
        assert_eq!(state, before);
    }
    let mut ordinary = original.clone();
    ordinary
        .factories
        .get_mut("assembly")
        .unwrap()
        .production
        .active_jobs = 1;
    assert!(
        ordinary
            .agent_manufacturing_quote("owner", "assembly", 10)
            .is_ok()
    );
}
#[test]
fn root_owner_rebinding_blocks_same_order_instead_of_changing_beneficiary() {
    let mut state = industrial_fixture();
    let id = reserve(&mut state);
    advance(&mut state, &id, 11);
    state.starter_oc_claims.get_mut("owner").unwrap().player_id = "replacement-player".into();
    advance(&mut state, &id, 12);
    assert_eq!(order(&state, &id).phase, P::ProductionBlocked);
    assert_eq!(order(&state, &id).completed_work_ticks, 0);
    state.starter_oc_claims.get_mut("owner").unwrap().player_id = "player".into();
    advance(&mut state, &id, 13);
    assert_eq!(order(&state, &id).completed_work_ticks, 1);
}
#[test]
fn hard_production_deadline_is_not_extended_by_long_gap_or_repair() {
    let mut state = industrial_fixture();
    let id = reserve(&mut state);
    advance(&mut state, &id, 11);
    advance(&mut state, &id, 46);
    assert_eq!(order(&state, &id).completed_work_ticks, 1);
    advance(&mut state, &id, 47);
    assert_eq!(order(&state, &id).phase, P::Disposed);
}
#[test]
fn production_tick_gap_does_not_count_elapsed_time_and_delivery_rechecks_cap() {
    let mut state = industrial_fixture();
    let id = reserve(&mut state);
    advance(&mut state, &id, 11);
    advance(&mut state, &id, 20);
    assert_eq!(order(&state, &id).completed_work_ticks, 1);
    for tick in 21..=31 {
        advance(&mut state, &id, tick);
    }
    state.reputation_scores.insert("owner".into(), 0);
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event(
                "owner",
                "deliver",
                WorldRuleCommandV1::DeliverAgentOrder {
                    order_id: id.clone()
                }
            ),
            32
        )
        .is_err()
    );
    assert_eq!(order(&state, &id).phase, P::AwaitingDelivery);
    assert!(state.agent_claims.is_empty());
    state.reputation_scores.insert("owner".into(), 10);
    apply(
        &mut state,
        &event(
            "owner",
            "deliver-recovered",
            WorldRuleCommandV1::DeliverAgentOrder { order_id: id },
        ),
        33,
    );
}

#[test]
fn historical_over_cap_only_blocks_expansion_not_existing_target_claims() {
    let mut state = industrial_fixture();
    state.agent_claims = fixture().agent_claims;
    state.reputation_scores.insert("owner".into(), 0);
    assert!(manufacturing_capacity_guard(&state, "owner", Some("primary"), false).is_ok());
    assert!(manufacturing_capacity_guard(&state, "owner", Some("recipient"), false).is_err());
    assert!(manufacturing_capacity_guard(&state, "owner", None, true).is_err());
}

#[test]
fn inconsistent_recovered_phase_is_rejected_without_panicking_or_refunding() {
    let mut state = industrial_fixture();
    let id = reserve(&mut state);
    state
        .world_rules
        .as_mut()
        .unwrap()
        .agent_orders
        .get_mut(&id)
        .unwrap()
        .phase = P::Producing;
    let before = state.clone();
    let e = event(
        "owner",
        "recover",
        WorldRuleCommandV1::AdvanceAgentOrder { order_id: id },
    );
    assert!(PreparedWorldRule::prepare(&state, &e, 11).is_err());
    assert_eq!(state, before);
}

#[path = "industrial_control_tests.rs"]
mod industrial_control_tests;

#[test]
fn same_player_new_root_source_cannot_inherit_reserved_order() {
    let mut state = industrial_fixture();
    let id = reserve(&mut state);
    advance(&mut state, &id, 11);
    state.starter_oc_claims.get_mut("owner").unwrap().claimed_at += 1;
    advance(&mut state, &id, 12);
    assert_eq!(order(&state, &id).phase, P::ProductionBlocked);
    assert_eq!(order(&state, &id).completed_work_ticks, 0);
}

#[test]
fn new_root_source_cannot_cancel_or_dispose_original_orders() {
    for started in [false, true] {
        let mut state = industrial_fixture();
        let id = reserve(&mut state);
        if started {
            advance(&mut state, &id, 11);
        }
        state.starter_oc_claims.get_mut("owner").unwrap().claimed_at += 1;
        let command = if started {
            WorldRuleCommandV1::DisposeAgentOrder { order_id: id }
        } else {
            WorldRuleCommandV1::CancelAgentOrder { order_id: id }
        };
        let before = state.clone();
        assert!(
            PreparedWorldRule::prepare(&state, &event("owner", "new-root-terminal", command), 12)
                .is_err()
        );
        assert_eq!(state, before);
    }
}

#[test]
fn original_source_frozen_refund_survives_terminal_snapshot_and_releases_once_after_recovery() {
    for started in [false, true] {
        let mut state = industrial_fixture();
        let before = state.clone();
        let id = reserve(&mut state);
        if started {
            advance(&mut state, &id, 11);
        }
        let reserved = state.clone();
        let original_root = state.starter_oc_claims["owner"].clone();
        state.starter_oc_claims.get_mut("owner").unwrap().player_id = "replacement-player".into();
        advance(&mut state, &id, if started { 47 } else { 20 });
        assert_eq!(
            order(&state, &id).phase,
            if started { P::Disposed } else { P::Cancelled }
        );
        assert!(order(&state, &id).refund_pending);
        assert_eq!(order(&state, &id).refunded_at, None);
        assert_eq!(state.main_token_balances, reserved.main_token_balances);
        assert_eq!(state.material_ledgers, reserved.material_ledgers);
        assert_eq!(
            state.agents["owner"].state.resources,
            reserved.agents["owner"].state.resources
        );
        assert_eq!(
            crate::world_rules::manufacturing_reserved_slots(&state, "assembly"),
            0
        );
        assert!(crate::world_rules::manufacturing_reserved_targets(&state, "owner").is_empty());
        assert!(
            order(&state, &id)
                .blocker(&state, 50)
                .unwrap()
                .contains("restore_original_owner_source")
        );
        let mut restored: WorldState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        let request = event(
            "owner",
            "refund-recovery",
            WorldRuleCommandV1::AdvanceAgentOrder {
                order_id: id.clone(),
            },
        );
        let frozen = restored.clone();
        assert!(PreparedWorldRule::prepare(&restored, &request, 50).is_err());
        assert_eq!(restored, frozen);
        restored
            .starter_oc_claims
            .insert("owner".into(), original_root);
        apply(&mut restored, &request, 51);
        assert!(!order(&restored, &id).refund_pending);
        assert_eq!(order(&restored, &id).refunded_at, Some(51));
        assert_eq!(restored.main_token_balances, before.main_token_balances);
        if started {
            assert_eq!(restored.material_ledgers, reserved.material_ledgers);
            assert_eq!(
                restored.agents["owner"].state.resources,
                reserved.agents["owner"].state.resources
            );
        } else {
            assert_eq!(restored.material_ledgers, before.material_ledgers);
            assert_eq!(
                restored.agents["owner"].state.resources,
                before.agents["owner"].state.resources
            );
        }
        let paid = restored.clone();
        apply(&mut restored, &request, 52);
        assert_eq!(restored.main_token_balances, paid.main_token_balances);
        assert_eq!(restored.material_ledgers, paid.material_ledgers);
        assert!(
            PreparedWorldRule::prepare(
                &restored,
                &event(
                    "owner",
                    "refund-twice",
                    WorldRuleCommandV1::AdvanceAgentOrder { order_id: id }
                ),
                53
            )
            .is_err()
        );
    }
}
#[test]
fn blocked_original_source_timeout_retains_refund_and_overflow_cannot_partially_release() {
    let mut state = industrial_fixture();
    let id = reserve(&mut state);
    state.gameplay_policy.blocked_agents.push("owner".into());
    let mut timeout = event(
        "owner",
        "blocked-timeout",
        WorldRuleCommandV1::AdvanceAgentOrder {
            order_id: id.clone(),
        },
    );
    if let DomainEvent::WorldRuleCommitted { action_id, .. } = &mut timeout {
        *action_id = 0;
    }
    apply(&mut state, &timeout, 20);
    assert!(order(&state, &id).refund_pending);
    state.gameplay_policy.blocked_agents.clear();
    state
        .main_token_balances
        .get_mut("owner")
        .unwrap()
        .liquid_balance = u64::MAX;
    let before = state.clone();
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event(
                "owner",
                "overflow-refund",
                WorldRuleCommandV1::AdvanceAgentOrder {
                    order_id: id.clone()
                }
            ),
            21
        )
        .is_err()
    );
    assert_eq!(state, before);
    state
        .main_token_balances
        .get_mut("owner")
        .unwrap()
        .liquid_balance = 0;
    advance(&mut state, &id, 22);
    assert!(!order(&state, &id).refund_pending);
    assert_eq!(state.main_token_balances["owner"].liquid_balance, 488);
}
#[test]
fn inconsistent_terminal_refund_flags_fail_closed_and_legacy_terminal_shape_is_readable() {
    let mut state = industrial_fixture();
    let id = reserve(&mut state);
    apply(
        &mut state,
        &event(
            "owner",
            "cancel-shape",
            WorldRuleCommandV1::CancelAgentOrder {
                order_id: id.clone(),
            },
        ),
        11,
    );
    let mut value = serde_json::to_value(order(&state, &id)).unwrap();
    value.as_object_mut().unwrap().remove("refunded_at");
    value.as_object_mut().unwrap().remove("refund_pending");
    let legacy: AgentIndustrialOrderV1 = serde_json::from_value(value).unwrap();
    assert!(!legacy.refund_pending);
    assert_eq!(legacy.refunded_at, None);
    let o = state
        .world_rules
        .as_mut()
        .unwrap()
        .agent_orders
        .get_mut(&id)
        .unwrap();
    o.refund_pending = true;
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event(
                "owner",
                "corrupt-refund",
                WorldRuleCommandV1::AdvanceAgentOrder { order_id: id }
            ),
            12
        )
        .is_err()
    );
}

#[path = "industrial_world_binding_tests.rs"]
mod industrial_world_binding_tests;
