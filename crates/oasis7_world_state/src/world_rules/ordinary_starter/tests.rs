use super::*;
use crate::agent_state::AgentState;
use crate::state::{WorldStateProjection, industry_transition::PreparedIndustryEvent};
use oasis7_wasm_abi::{FactoryProfileV1, MaterialStack, ProductProfileV1};

const WORLD: &str = "world-genesis-binding";
const SUPPLIER: &str = "public-supplier";

fn fixture(count: usize) -> (WorldState, OrdinaryStarterOfferV1) {
    let mut state = WorldState::default();
    for actor in [SUPPLIER, "alice", "bob"] {
        state.agents.insert(
            actor.into(),
            AgentCell::new(AgentState::new(actor, GeoPos::new(0, 0, 0)), 0),
        );
    }
    state.product_profiles.insert(
        "iron_ingot".into(),
        ProductProfileV1 {
            product_id: "iron_ingot".into(),
            role_tag: "industrial".into(),
            maintenance_sink: vec![MaterialStack::new("lubricant", 2)],
            tradable: true,
            unlock_stage: String::new(),
        },
    );
    state.location_anchors.insert(
        "yard".into(),
        crate::LocationAnchorV1 {
            location_id: "yard".into(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        },
    );
    let mut slots = Vec::new();
    for index in 0..count {
        let factory = format!("smelter-{index}");
        let site = format!("site-{index}");
        let spec = builtin_factory_spec(ORDINARY_STARTER_FACTORY_KIND, &factory).unwrap();
        state.factory_profiles.insert(
            factory.clone(),
            FactoryProfileV1 {
                factory_id: factory.clone(),
                tier: spec.tier,
                recipe_slots: spec.recipe_slots,
                tags: spec.tags,
            },
        );
        state.factory_construction_power_profiles.insert(
            factory.clone(),
            crate::FactoryConstructionPowerProfileV1 {
                factory_id: factory.clone(),
                factory_kind: ORDINARY_STARTER_FACTORY_KIND.into(),
                electricity_amount: 10,
                authority_revision: 1,
                active: true,
                ..Default::default()
            },
        );
        state.factory_site_authorities.insert(
            site.clone(),
            crate::FactorySiteAuthorityV1 {
                site_id: site.clone(),
                location_id: "yard".into(),
                owner_agent_id: SUPPLIER.into(),
                authorized_agent_ids: vec![SUPPLIER.into()],
                chunk_ready: true,
                active: true,
                authority_revision: 1,
                registered_at: 0,
            },
        );
        slots.push(OrdinaryStarterSlotInputV1 {
            slot_id: format!("slot-{index}"),
            site_id: site,
            factory_id: factory,
        });
    }
    let (materials, power, _) = state.ordinary_starter_package().unwrap();
    state.material_ledgers.insert(
        MaterialLedgerId::agent(SUPPLIER),
        materials
            .into_iter()
            .map(|(kind, amount)| (kind, amount * count as i64))
            .collect(),
    );
    state
        .agents
        .get_mut(SUPPLIER)
        .unwrap()
        .state
        .resources
        .set(ResourceKind::Electricity, (power + 10) * count as i64)
        .unwrap();
    let offer = state
        .ordinary_starter_offer("starter-v1", SUPPLIER, WORLD, &slots, 0)
        .unwrap();
    (state, offer)
}

fn event(
    actor: &str,
    request: &str,
    command: OrdinaryStarterCommandV1,
    system: bool,
) -> DomainEvent {
    DomainEvent::WorldRuleCommitted {
        action_id: if system { 0 } else { 100 },
        actor_id: actor.into(),
        request_id: request.into(),
        command: WorldRuleCommandV1::OrdinaryStarter {
            command: Box::new(command),
        },
        governance_epoch: 0,
        world_binding: Some(WORLD.into()),
    }
}

fn publish(state: &mut WorldState, offer: &OrdinaryStarterOfferV1) {
    apply(
        state,
        &event(
            SUPPLIER,
            "publish",
            OrdinaryStarterCommandV1::Publish {
                offer: offer.clone(),
            },
            true,
        ),
        0,
    );
}

fn claim(
    offer: &OrdinaryStarterOfferV1,
    actor: &str,
    owner: &str,
    slot: usize,
    request: &str,
) -> DomainEvent {
    event(
        actor,
        request,
        OrdinaryStarterCommandV1::Claim {
            offer_id: offer.offer_id.clone(),
            offer_revision: offer.revision,
            slot_id: format!("slot-{slot}"),
            offer_digest: ordinary_starter_offer_digest(offer).unwrap(),
            owner_binding: owner.into(),
            owner_generation: 1,
        },
        false,
    )
}

fn apply(state: &mut WorldState, event: &DomainEvent, now: u64) {
    let prepared = PreparedWorldRule::prepare(state, event, now).unwrap();
    let projected = serde_json::to_value(
        WorldStateProjection::borrowed(state).with_world_rule_overlay(&prepared),
    )
    .unwrap();
    state.apply_domain_event(event, now).unwrap();
    state.route_domain_event(event);
    let actual = serde_json::to_value(&*state).unwrap();
    let differences = projected
        .as_object()
        .unwrap()
        .keys()
        .filter(|key| projected.get(*key) != actual.get(*key))
        .collect::<Vec<_>>();
    assert!(
        differences.is_empty(),
        "projection differs in {differences:?}"
    );
}

fn reject_unchanged(state: &mut WorldState, event: &DomainEvent, reason: &str) {
    let before = state.clone();
    let error = state.apply_domain_event(event, 1).unwrap_err();
    assert!(format!("{error:?}").contains(reason), "{error:?}");
    assert_eq!(state, &before);
}

#[test]
fn ordinary_starter_canonical_package_includes_both_full_maintenance_obligations() {
    let (state, offer) = fixture(2);
    assert_eq!(
        offer.materials,
        BTreeMap::from([
            ("structural_frame".into(), 12),
            ("heat_coil".into(), 4),
            ("refractory_brick".into(), 6),
            ("iron_ore".into(), 8),
            ("carbon_fuel".into(), 2),
            ("lubricant".into(), 12),
        ])
    );
    assert_eq!(offer.recipe_electricity, 16);
    assert_eq!(offer.slots["slot-0"].construction_electricity, 10);
    let mut insufficient = state.clone();
    insufficient
        .material_ledgers
        .get_mut(&MaterialLedgerId::agent(SUPPLIER))
        .unwrap()
        .insert("lubricant".into(), 23);
    assert!(validate_offer(&insufficient, &offer, 0).is_err());
    let mut overflow = state;
    overflow
        .product_profiles
        .get_mut("iron_ingot")
        .unwrap()
        .maintenance_sink[0]
        .amount = i64::MAX;
    assert!(overflow.ordinary_starter_package().is_err());
}

#[test]
fn ordinary_starter_claim_debits_finite_source_and_atomically_assigns_real_site() {
    let (mut state, offer) = fixture(2);
    publish(&mut state, &offer);
    let supply_before = state.material_ledgers[&MaterialLedgerId::agent(SUPPLIER)].clone();
    apply(
        &mut state,
        &claim(&offer, "alice", "owner-alice", 0, "claim"),
        1,
    );
    for (kind, amount) in &offer.materials {
        assert_eq!(
            state.material_ledgers[&MaterialLedgerId::agent(SUPPLIER)][kind],
            supply_before[kind] - amount
        );
        assert_eq!(
            state.material_ledgers[&MaterialLedgerId::agent("alice")][kind],
            *amount
        );
    }
    assert_eq!(
        state.agents[SUPPLIER]
            .state
            .resources
            .get(ResourceKind::Electricity),
        26
    );
    assert_eq!(
        state.agents["alice"]
            .state
            .resources
            .get(ResourceKind::Electricity),
        26
    );
    assert_eq!(
        state.factory_site_authorities["site-0"].owner_agent_id,
        "alice"
    );
    assert_eq!(
        state.factory_site_authorities["site-0"].authorized_agent_ids,
        ["alice"]
    );
    assert_eq!(
        state.factory_site_authorities["site-0"].authority_revision,
        2
    );
    assert_eq!(
        state.agent_location_authorities["alice"].location_id,
        "yard"
    );
    assert!(state.factories.is_empty());
    assert!(
        state
            .industry_progress
            .starter_industrial_milestone
            .is_none()
    );
    let rules = state.world_rules.as_ref().unwrap();
    let record =
        &rules.ordinary_starter.claims[&ordinary_starter_owner_key(WORLD, "owner-alice").unwrap()];
    assert!(record.first_production.is_none());
    assert_eq!(record.receipt.receipt_key, request_key("alice", "claim"));
    assert!(
        rules
            .for_reader(Some("alice"))
            .ordinary_starter
            .claims
            .is_empty()
    );
    assert!(
        rules
            .for_reader(Some("bob"))
            .ordinary_starter
            .claims
            .is_empty()
    );
    assert!(rules.for_reader(None).ordinary_starter.claims.is_empty());
    let public = ordinary_starter_public_offers(&state, Some(WORLD), 1);
    assert_eq!(public["starter-v1"].available_slots, ["slot-1"]);
    assert_eq!(public["starter-v1"].supplier_electricity, 26);
}

#[test]
fn ordinary_starter_retry_is_stable_and_new_request_or_generation_cannot_claim_twice() {
    let (mut state, offer) = fixture(2);
    publish(&mut state, &offer);
    let first = claim(&offer, "alice", "owner-alice", 0, "same-request");
    apply(&mut state, &first, 1);
    let balances = state.material_ledgers.clone();
    let receipt =
        state.world_rules.as_ref().unwrap().requests[&request_key("alice", "same-request")].clone();
    apply(&mut state, &first, 2);
    assert_eq!(state.material_ledgers, balances);
    assert_eq!(
        state.world_rules.as_ref().unwrap().requests[&request_key("alice", "same-request")],
        receipt
    );
    reject_unchanged(
        &mut state,
        &claim(&offer, "alice", "owner-alice", 1, "same-request"),
        "request_content_conflict",
    );
    let mut rotated = claim(&offer, "bob", "owner-alice", 1, "new-request");
    if let DomainEvent::WorldRuleCommitted {
        command: WorldRuleCommandV1::OrdinaryStarter { command },
        ..
    } = &mut rotated
        && let OrdinaryStarterCommandV1::Claim {
            owner_generation, ..
        } = command.as_mut()
    {
        *owner_generation = 2;
    }
    reject_unchanged(&mut state, &rotated, "owner_already_claimed");
    let restored: WorldState =
        serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
    assert_eq!(restored, state);
}

#[test]
fn ordinary_starter_last_slot_is_exclusive_and_never_refilled_by_request() {
    let (mut state, offer) = fixture(1);
    publish(&mut state, &offer);
    apply(
        &mut state,
        &claim(&offer, "alice", "owner-alice", 0, "a"),
        1,
    );
    reject_unchanged(
        &mut state,
        &claim(&offer, "bob", "owner-bob", 0, "b"),
        "slot_claimed",
    );
    assert!(
        ordinary_starter_public_offers(&state, Some(WORLD), 1)["starter-v1"]
            .available_slots
            .is_empty()
    );
    assert_eq!(
        state.agents[SUPPLIER]
            .state
            .resources
            .get(ResourceKind::Electricity),
        0
    );
    reject_unchanged(
        &mut state,
        &event(
            "bob",
            "publish-by-player",
            OrdinaryStarterCommandV1::Publish { offer },
            false,
        ),
        "publication_authority_required",
    );
}

#[test]
fn ordinary_starter_rejects_stale_or_insufficient_claim_without_partial_effects() {
    let (mut base, offer) = fixture(1);
    publish(&mut base, &offer);
    let request = claim(&offer, "alice", "owner-alice", 0, "claim");
    for case in [
        "source_material",
        "source_power",
        "recipient_material",
        "recipient_power",
        "anchor",
        "profile",
        "construction_power",
        "maintenance",
    ] {
        let mut state = base.clone();
        match case {
            "source_material" => {
                state
                    .material_ledgers
                    .get_mut(&MaterialLedgerId::agent(SUPPLIER))
                    .unwrap()
                    .clear();
            }
            "source_power" => {
                state
                    .agents
                    .get_mut(SUPPLIER)
                    .unwrap()
                    .state
                    .resources
                    .set(ResourceKind::Electricity, 0)
                    .unwrap();
            }
            "recipient_material" => {
                state.material_ledgers.insert(
                    MaterialLedgerId::agent("alice"),
                    BTreeMap::from([("iron_ore".into(), i64::MAX)]),
                );
            }
            "recipient_power" => {
                state
                    .agents
                    .get_mut("alice")
                    .unwrap()
                    .state
                    .resources
                    .set(ResourceKind::Electricity, i64::MAX)
                    .unwrap();
            }
            "anchor" => {
                state.location_anchors.get_mut("yard").unwrap().active = false;
            }
            "profile" => {
                state.factory_profiles.get_mut("smelter-0").unwrap().tier += 1;
            }
            "construction_power" => {
                state
                    .factory_construction_power_profiles
                    .get_mut("smelter-0")
                    .unwrap()
                    .authority_revision += 1;
            }
            "maintenance" => {
                state
                    .product_profiles
                    .get_mut("iron_ingot")
                    .unwrap()
                    .maintenance_sink[0]
                    .amount += 1;
            }
            _ => unreachable!(),
        }
        let before = state.clone();
        assert!(state.apply_domain_event(&request, 1).is_err(), "{case}");
        assert_eq!(state, before, "{case}");
    }
    let mut wrong = request;
    if let DomainEvent::WorldRuleCommitted { world_binding, .. } = &mut wrong {
        *world_binding = Some("other-genesis".into());
    }
    reject_unchanged(&mut base, &wrong, "offer_binding_changed");
}

fn completion_fixture() -> (WorldState, DomainEvent) {
    completion_fixture_for_batches(1)
}

fn completion_fixture_for_batches(batches: u32) -> (WorldState, DomainEvent) {
    let (mut state, offer) = fixture(2);
    publish(&mut state, &offer);
    apply(
        &mut state,
        &claim(&offer, "alice", "owner-alice", 0, "a"),
        1,
    );
    apply(&mut state, &claim(&offer, "bob", "owner-bob", 1, "b"), 1);
    let spec = builtin_factory_spec(ORDINARY_STARTER_FACTORY_KIND, "smelter-0").unwrap();
    state.factories.insert("smelter-0".into(), serde_json::from_value(serde_json::json!({
        "factory_id":"smelter-0", "site_id":"site-0", "builder_agent_id":"alice", "spec":spec,
        "input_ledger":MaterialLedgerId::agent("alice"), "output_ledger":MaterialLedgerId::agent("alice"),
        "built_at":1
    })).unwrap());
    let plan = builtin_recipe_plan(ORDINARY_STARTER_RECIPE_ID, batches).unwrap();
    let consume =
        crate::state::apply_domain_event_industry_helpers::recipe_consume_with_maintenance(
            &state,
            &plan.consume,
            &plan.produce,
        );
    let ready_at = 2 + u64::from(plan.duration_ticks);
    let started = DomainEvent::RecipeStarted {
        emergency_power_obligation: None,
        committed_recipe_origin: None,
        job_id: 500,
        requester_agent_id: "alice".into(),
        factory_id: "smelter-0".into(),
        recipe_id: ORDINARY_STARTER_RECIPE_ID.into(),
        accepted_batches: batches,
        consume,
        produce: plan.produce.clone(),
        byproducts: plan.byproducts.clone(),
        power_required: plan.power_required,
        power_owner_agent_id: Some("alice".into()),
        duration_ticks: plan.duration_ticks,
        consume_ledger: MaterialLedgerId::agent("alice"),
        output_ledger: MaterialLedgerId::agent("alice"),
        bottleneck_tags: vec![],
        market_quotes: vec![],
        logistics_route_ids: vec![],
        logistics_path_ids: vec![],
        ready_at,
    };
    state.apply_domain_event(&started, 2).unwrap();
    state.route_domain_event(&started);
    state.time = ready_at;
    let completed = DomainEvent::RecipeCompleted {
        committed_recipe_origin: None,
        job_id: 500,
        requester_agent_id: "alice".into(),
        factory_id: "smelter-0".into(),
        recipe_id: ORDINARY_STARTER_RECIPE_ID.into(),
        accepted_batches: batches,
        produce: plan.produce,
        byproducts: plan.byproducts,
        output_ledger: MaterialLedgerId::agent("alice"),
        bottleneck_tags: vec![],
        logistics_route_ids: vec![],
        logistics_path_ids: vec![],
    };
    (state, completed)
}

#[test]
fn ordinary_starter_personal_first_production_is_real_atomic_and_owner_isolated() {
    let (mut state, completed) = completion_fixture();
    let key = ordinary_starter_owner_key(WORLD, "owner-alice").unwrap();
    let peer = ordinary_starter_owner_key(WORLD, "owner-bob").unwrap();
    assert!(
        state.world_rules.as_ref().unwrap().ordinary_starter.claims[&key]
            .first_production
            .is_none()
    );
    let prepared = PreparedIndustryEvent::prepare(&state, &completed, state.time).unwrap();
    let mut projection = WorldStateProjection::borrowed(&state);
    projection.industry_overlay = Some(&prepared);
    let projected = serde_json::to_value(projection).unwrap();
    state.apply_domain_event(&completed, state.time).unwrap();
    state.route_domain_event(&completed);
    assert_eq!(projected, serde_json::to_value(&state).unwrap());
    let first = state.world_rules.as_ref().unwrap().ordinary_starter.claims[&key]
        .first_production
        .clone()
        .unwrap();
    assert_eq!(first.job_id, 500);
    assert_eq!(first.owner_binding, "owner-alice");
    assert_eq!(first.site_id, "site-0");
    assert_eq!(first.produce, [MaterialStack::new("iron_ingot", 3)]);
    assert_eq!(
        state.material_ledgers[&MaterialLedgerId::agent("alice")]["iron_ingot"],
        3
    );
    assert!(
        state.world_rules.as_ref().unwrap().ordinary_starter.claims[&peer]
            .first_production
            .is_none()
    );
    assert!(
        state
            .industry_progress
            .starter_industrial_milestone
            .is_none()
    );
    state
        .apply_domain_event(&completed, state.time + 1)
        .unwrap();
    assert_eq!(
        state.world_rules.as_ref().unwrap().ordinary_starter.claims[&key]
            .first_production
            .as_ref(),
        Some(&first)
    );
    assert_eq!(
        state.material_ledgers[&MaterialLedgerId::agent("alice")]["iron_ingot"],
        3
    );
    let restored: WorldState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    assert_eq!(restored.world_rules, state.world_rules);
}

#[test]
fn ordinary_starter_first_real_production_also_recognizes_multiple_paid_batches() {
    let (mut state, event) = completion_fixture_for_batches(2);
    state.apply_domain_event(&event, state.time).unwrap();
    let key = ordinary_starter_owner_key(WORLD, "owner-alice").unwrap();
    let first = state.world_rules.as_ref().unwrap().ordinary_starter.claims[&key]
        .first_production
        .as_ref()
        .unwrap();
    assert_eq!(first.accepted_batches, 2);
    assert_eq!(first.produce, [MaterialStack::new("iron_ingot", 6)]);
    assert_eq!(first.power_required, 16);
    assert_eq!(
        state.material_ledgers[&MaterialLedgerId::agent("alice")]["iron_ingot"],
        6
    );
}

#[test]
fn ordinary_starter_failed_or_mismatched_completion_never_sets_milestone() {
    let (mut state, mut completed) = completion_fixture();
    state
        .material_ledgers
        .get_mut(&MaterialLedgerId::agent("alice"))
        .unwrap()
        .insert("iron_ingot".into(), i64::MAX);
    let before = state.clone();
    assert!(state.apply_domain_event(&completed, state.time).is_err());
    assert_eq!(state, before);
    if let DomainEvent::RecipeCompleted {
        requester_agent_id, ..
    } = &mut completed
    {
        *requester_agent_id = "bob".into();
    }
    assert!(state.apply_domain_event(&completed, state.time).is_err());
    assert_eq!(state, before);
}

#[test]
fn ordinary_starter_empty_state_preserves_legacy_serialized_shape() {
    let value = serde_json::to_value(WorldRulesStateV1::default()).unwrap();
    assert!(value.get("ordinary_starter").is_none());
    let restored: WorldRulesStateV1 = serde_json::from_value(value).unwrap();
    assert_eq!(restored.ordinary_starter, OrdinaryStarterStateV1::default());
}

#[test]
fn ordinary_starter_supplier_rejects_both_raw_personal_acquisition_reducers() {
    let (mut state, offer) = fixture(1);
    publish(&mut state, &offer);
    assert!(state.is_ordinary_starter_supplier(SUPPLIER));
    let starter = DomainEvent::StarterOcClaimed {
        agent_id: SUPPLIER.into(),
        player_id: "attacker".into(),
        public_key: Some("attacker-key".into()),
        amount: 1,
        claimed_at: 1,
        source_treasury_bucket_id: None,
    };
    reject_unchanged(&mut state, &starter, "ordinary_starter_supplier_reserved");
    for acquisition_contract_version in [0, 1] {
        let economic = DomainEvent::AgentClaimed {
            acquisition_contract_version,
            claimer_agent_id: "alice".into(),
            target_agent_id: SUPPLIER.into(),
            reputation_tier: 0,
            slot_index: 1,
            activation_fee_amount: 100,
            activation_fee_burn_amount: 50,
            activation_fee_treasury_amount: 50,
            claim_bond_amount: 200,
            upfront_restricted_spent_amount: 0,
            upfront_liquid_spent_amount: 325,
            auto_issued_restricted_amount: 0,
            auto_issued_restricted_source_treasury_bucket_id: None,
            claim_bond_locked_restricted_amount: 0,
            claim_bond_locked_liquid_amount: 200,
            upkeep_per_epoch: 25,
            claimed_at_epoch: 0,
            upkeep_paid_through_epoch: 0,
            release_cooldown_epochs: 3,
            grace_epochs: 2,
            idle_warning_epochs: 7,
            forced_idle_reclaim_epochs: 10,
            forced_reclaim_penalty_bps: 2_000,
        };
        reject_unchanged(&mut state, &economic, "ordinary_starter_supplier_reserved");
    }
}

#[test]
fn ordinary_starter_cannot_publish_an_already_personally_owned_supplier() {
    let (mut state, offer) = fixture(1);
    state.starter_oc_claims.insert(
        SUPPLIER.into(),
        crate::gameplay_state::StarterOcClaimState {
            agent_id: SUPPLIER.into(),
            player_id: "prior-owner".into(),
            public_key: Some("prior-key".into()),
            ..Default::default()
        },
    );
    reject_unchanged(
        &mut state,
        &event(
            SUPPLIER,
            "publish",
            OrdinaryStarterCommandV1::Publish { offer },
            true,
        ),
        "ordinary_starter_offer_invalid",
    );
}
