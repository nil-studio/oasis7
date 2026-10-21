use super::*;
use oasis7_agent_api::world_action::{
    EmergencyEvidenceV1 as Evidence, EmergencyObservationV1 as Observation,
    EmergencyServiceRegistrationV1 as Service, EmergencySupplyCommandV1 as C,
};
fn event(actor: &str, request: &str, command: WorldRuleCommandV1) -> DomainEvent {
    let emergency = matches!(&command, WorldRuleCommandV1::EmergencySupply { .. });
    let mut event = super::event(actor, request, command);
    if emergency && let DomainEvent::WorldRuleCommitted { world_binding, .. } = &mut event {
        *world_binding = Some(adapter_tests::supply_test_binding());
    }
    event
}
fn command(c: C) -> WorldRuleCommandV1 {
    WorldRuleCommandV1::EmergencySupply { command: c }
}
fn services(state: &mut WorldState) {
    state
        .world_rules
        .get_or_insert_with(Default::default)
        .emergency_supply
        .world_id = Some(adapter_tests::supply_test_binding());
    for (actor, place) in [
        ("owner", "north"),
        ("primary", "north"),
        ("recipient", "south"),
    ] {
        let service = Service {
            obligation_id: format!("minimum-{actor}"),
            beneficiary_id: actor.into(),
            controller_id: actor.into(),
            region_id: "region".into(),
            location_id: place.into(),
            registered_at: 0,
            electricity_per_interval: 10,
            public_critical: false,
            recipe_ids: ["essential".into()].into(),
            source_ref: "authority:minimum".into(),
        };
        state
            .world_rules
            .get_or_insert_with(Default::default)
            .emergency_supply
            .services
            .insert(service.obligation_id.clone(), service);
    }
}
fn evidence() -> Evidence {
    Evidence {
        region_id: "region".into(),
        region_budget_lineage: "original-region".into(),
        source_ref: "authority:root".into(),
        observations: [600, 660, 720]
            .into_iter()
            .map(|window_start| Observation {
                window_start,
                world_root: format!("canonical-root-{window_start}"),
                obligation_ids: [
                    "minimum-owner".into(),
                    "minimum-primary".into(),
                    "minimum-recipient".into(),
                ]
                .into(),
                normal_coverage: [
                    ("minimum-owner".into(), 0),
                    ("minimum-primary".into(), 0),
                    ("minimum-recipient".into(), 0),
                ]
                .into(),
            })
            .collect(),
    }
}
#[test]
fn supply_policy_rejects_materials_and_unverified_legacy_electricity_without_debit() {
    for material in ["alloy_plate", "Electricity"] {
        let state = fixture();
        let before = state.clone();
        let c = WorldRuleCommandV1::AuthorizeSupply {
            proposal_key: "approval".into(),
            crisis_id: "ordinary-crisis".into(),
            material: material.into(),
            beneficiaries: ["recipient".into()].into(),
            budget: 10,
            expires_at: 200,
        };
        let failure =
            PreparedWorldRule::prepare(&state, &event("owner", "auth", c), 10).unwrap_err();
        assert!(
            format!("{failure:?}").contains(if material == "Electricity" {
                "unsupported_resource_path"
            } else {
                "scope_mismatch"
            })
        );
        assert_eq!(state, before);
    }
}
#[test]
fn supply_requires_three_complete_windows_and_mature_complete_region() {
    let mut state = fixture();
    services(&mut state);
    let policy = &state.world_rules.as_ref().unwrap().emergency_supply;
    let e = evidence();
    assert!(policy.validate_evidence(&e, 780).is_ok());
    let mut invalid = e.clone();
    invalid.observations.pop();
    assert!(policy.validate_evidence(&invalid, 780).is_err());
    let mut invalid = e.clone();
    invalid.observations[0]
        .obligation_ids
        .remove("minimum-owner");
    assert!(policy.validate_evidence(&invalid, 780).is_err());
    let mut invalid = e.clone();
    for w in &mut invalid.observations {
        w.normal_coverage.insert("minimum-recipient".into(), 100);
    }
    assert!(policy.validate_evidence(&invalid, 780).is_err());
    assert!(policy.validate_evidence(&e, 841).is_err());
    state
        .world_rules
        .as_mut()
        .unwrap()
        .emergency_supply
        .services
        .get_mut("minimum-recipient")
        .unwrap()
        .registered_at = 1;
    assert!(
        state
            .world_rules
            .as_ref()
            .unwrap()
            .emergency_supply
            .validate_evidence(&e, 780)
            .is_err()
    );
}
#[test]
fn supply_largest_fraction_precedes_lexical_identity_and_aggregation_is_stable() {
    let result =
        emergency_proportional_allocation(&[("a".into(), 1), ("z".into(), 2)].into(), 1).unwrap();
    assert_eq!(result["a"], 0);
    assert_eq!(result["z"], 1);
    let result =
        emergency_proportional_allocation(&[("a".into(), 1), ("z".into(), 1)].into(), 1).unwrap();
    assert_eq!(result["a"], 1);
    let result = emergency_proportional_allocation(
        &[("a".into(), i64::MAX), ("z".into(), i64::MAX)].into(),
        i64::MAX,
    )
    .unwrap();
    assert_eq!(
        result.values().map(|n| i128::from(*n)).sum::<i128>(),
        i128::from(i64::MAX)
    );
    assert!(emergency_proportional_allocation(&[("bad".into(), -1)].into(), 10).is_err());
}
#[test]
fn supply_ordinary_approval_cannot_certify_source_evidence_or_electrical_delivery() {
    let mut state = fixture();
    services(&mut state);
    let mut registration = state
        .world_rules
        .as_ref()
        .unwrap()
        .emergency_supply
        .services["minimum-owner"]
        .clone();
    registration.obligation_id = "new-minimum".into();
    registration.registered_at = 780;
    for (name, c, reason) in [
        (
            "registration",
            C::RegisterService {
                proposal_key: "registration".into(),
                registration,
            },
            "need_source_unverifiable",
        ),
        (
            "proof",
            C::PublishEvidence {
                proposal_key: "proof".into(),
                evidence: evidence(),
            },
            "invalid_evidence",
        ),
    ] {
        let intent = command(c);
        approve(&mut state, "owner", name, &intent);
        let before = state.clone();
        let err =
            PreparedWorldRule::prepare(&state, &event("owner", name, intent), 780).unwrap_err();
        assert!(format!("{err:?}").contains(reason));
        assert_eq!(state, before);
    }
    let before = state.clone();
    let delivery = event(
        "owner",
        "deliver",
        command(C::Deliver {
            batch_id: "unverified-local-grid".into(),
            need_id: "arbitrary-need".into(),
            quantity: 9,
        }),
    );
    let err = PreparedWorldRule::prepare(&state, &delivery, 840).unwrap_err();
    assert!(format!("{err:?}").contains("window_closed"));
    assert_eq!(state, before);
    assert!(
        state
            .world_rules
            .as_ref()
            .unwrap()
            .emergency_supply
            .restricted_power
            .is_empty()
    );
}
#[test]
fn supply_restricted_consumption_kernel_cannot_spend_outside_recipe_owner_or_interval() {
    // A kernel fixture proves restricted accounting only. It is deliberately not
    // an electrical adapter or authority-producer acceptance test.
    let mut state = fixture();
    services(&mut state);
    let rules = &mut state.world_rules.as_mut().unwrap().emergency_supply;
    let key = format!("need:{}", blake3::hash(b"15:minimum-primary:900").to_hex());
    rules.restricted_power.insert(
        key.clone(),
        EmergencyRestrictedPowerV1 {
            beneficiary_id: "primary".into(),
            obligation_id: "minimum-primary".into(),
            interval_start: 900,
            remaining: 5,
            delivered: 5,
        },
    );
    assert!(
        rules
            .prepare_consumption("recipient", "minimum-primary", "essential", 900, 1)
            .is_err()
    );
    assert!(
        rules
            .prepare_consumption("primary", "minimum-primary", "expansion", 900, 1)
            .is_err()
    );
    assert!(
        rules
            .prepare_consumption("primary", "minimum-primary", "essential", 899, 1)
            .is_err()
    );
    let next = rules
        .prepare_consumption("primary", "minimum-primary", "essential", 900, 5)
        .unwrap();
    assert_eq!(next.restricted_power[&key].remaining, 0);
    assert!(
        next.prepare_consumption("primary", "minimum-primary", "essential", 900, 1)
            .is_err()
    );
    let restored: WorldState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    assert_eq!(restored, state);
}

#[test]
fn supply_capacity_overflow_persists_for_entire_window_and_blocks_prior_requests() {
    let mut state = adapter_tests::registered_fixture();
    let rules = &mut state.world_rules.as_mut().unwrap().emergency_supply;
    rules.evidence.insert("evidence".into(), evidence());
    let observation_key = format!("observation:{}", blake3::hash(b"6:region:720").to_hex());
    let need_key = format!("need:{}", blake3::hash(b"15:minimum-primary:900").to_hex());
    rules
        .observation_interval_coverage
        .insert(observation_key, [(need_key, 0)].into());
    let mut requested = std::collections::BTreeSet::new();
    for index in 0..512 {
        let obligation = format!("obligation-{index}");
        let mut service = rules.services["minimum-primary"].clone();
        service.obligation_id = obligation.clone();
        rules.services.insert(obligation.clone(), service);
        for interval in [840, 900, 960, 1020, 1080, 1140, 1200, 1260] {
            let id = format!("existing-{index}-{interval}");
            requested.insert(id.clone());
            rules.needs.insert(
                id,
                EmergencyNeedLedgerV1 {
                    obligation_id: obligation.clone(),
                    controller_id: "primary".into(),
                    location_id: "north".into(),
                    beneficiary_id: "primary".into(),
                    interval_start: interval,
                    minimum: 10,
                    normal_coverage: 0,
                    fulfilled: 0,
                    reserved: 0,
                    public_critical: false,
                },
            );
        }
    }
    rules.authorizations.insert(
        "authorization".into(),
        EmergencyAuthorizationV1 {
            id: "authorization".into(),
            evidence_id: "evidence".into(),
            provider_id: "owner".into(),
            provider_controller_id: "owner".into(),
            reviewer_id: "recipient".into(),
            reviewer_controller_id: "player-recipient".into(),
            reviewer_owner_source_id: "starter_claim:recipient:1".into(),
            region_id: "region".into(),
            starts_at: 780,
            expires_at: 1380,
            remaining: 9,
            active: true,
            requested,
        },
    );
    let overflow = event(
        "primary",
        "over-limit",
        command(C::Request {
            authorization_id: "authorization".into(),
            needs: vec![oasis7_agent_api::world_action::EmergencyNeedV1 {
                obligation_id: "minimum-primary".into(),
                interval_start: 900,
            }],
        }),
    );
    apply(&mut state, &overflow, 800);
    let rules = &state.world_rules.as_ref().unwrap().emergency_supply;
    assert!(rules.overflowed_windows.contains(&("region".into(), 780)));
    assert_eq!(rules.authorizations["authorization"].requested.len(), 4096);
    assert!(rules.needs.values().all(|n| n.reserved == 0));
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event(
                "owner",
                "freeze-overflow",
                command(C::Freeze {
                    authorization_id: "authorization".into()
                })
            ),
            840
        )
        .is_err()
    );
    let restored: WorldState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    assert!(
        restored
            .world_rules
            .as_ref()
            .unwrap()
            .emergency_supply
            .overflowed_windows
            .contains(&("region".into(), 780))
    );
}

#[test]
fn supply_independent_review_cancels_only_unsettled_reservation_and_keeps_history() {
    let mut state = adapter_tests::registered_fixture();
    let rules = &mut state.world_rules.as_mut().unwrap().emergency_supply;
    let need = EmergencyNeedLedgerV1 {
        obligation_id: "minimum-primary".into(),
        controller_id: "player-primary".into(),
        location_id: "north".into(),
        beneficiary_id: "primary".into(),
        interval_start: 900,
        minimum: 10,
        normal_coverage: 0,
        fulfilled: 4,
        reserved: 5,
        public_critical: false,
    };
    rules.needs.insert("need".into(), need.clone());
    rules.authorizations.insert(
        "authorization".into(),
        EmergencyAuthorizationV1 {
            id: "authorization".into(),
            evidence_id: "proof".into(),
            provider_id: "owner".into(),
            provider_controller_id: "player-owner".into(),
            reviewer_id: "recipient".into(),
            reviewer_controller_id: "player-recipient".into(),
            reviewer_owner_source_id: "starter_claim:recipient:1".into(),
            region_id: "region".into(),
            starts_at: 780,
            expires_at: 1380,
            remaining: 5,
            active: true,
            requested: ["need".into()].into(),
        },
    );
    rules.batches.insert(
        "batch".into(),
        EmergencyBatchV1 {
            authorization_id: "authorization".into(),
            frozen_at: 840,
            frozen_pool: 9,
            evidence_id: "proof".into(),
            demands: [("need".into(), 9)].into(),
            frozen_ledger: [("need".into(), need)].into(),
            allocation: [("need".into(), 5)].into(),
            allocated: true,
            committed_at: Some(841),
        },
    );
    let open = command(C::OpenReview {
        batch_id: "batch".into(),
        need_id: "need".into(),
        reason_ref: "canonical-result-dispute".into(),
    });
    assert!(
        PreparedWorldRule::prepare(&state, &event("primary", "late", open.clone()), 962).is_err()
    );
    apply(&mut state, &event("primary", "review", open), 850);
    let id = request_key("primary", "review");
    assert_eq!(
        state.world_rules.as_ref().unwrap().emergency_supply.reviews[&id].deadline,
        970
    );
    let decide = command(C::DecideReview {
        proposal_key: "decision".into(),
        review_id: id.clone(),
        cancel_unsettled: true,
    });
    approve(&mut state, "owner", "decision", &decide);
    assert!(
        PreparedWorldRule::prepare(&state, &event("owner", "self-review", decide.clone()), 971)
            .is_err()
    );
    approve(&mut state, "recipient", "decision", &decide);
    apply(
        &mut state,
        &event("recipient", "decision", decide.clone()),
        971,
    );
    let rules = &state.world_rules.as_ref().unwrap().emergency_supply;
    assert_eq!(rules.needs["need"].fulfilled, 4);
    assert_eq!(rules.needs["need"].reserved, 0);
    assert_eq!(rules.batches["batch"].allocation["need"], 0);
    assert_eq!(rules.reviews[&id].decision, Some(true));
    assert!(
        PreparedWorldRule::prepare(&state, &event("recipient", "decide-again", decide), 972)
            .is_err()
    );
}

#[path = "supply_adapter_tests.rs"]
mod adapter_tests;

pub fn emergency_authorize_fixture() -> (World, Action) {
    adapter_tests::authorized_preparation_fixture()
}
#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
pub fn essential_recipe_fixture() -> (World, Action) {
    adapter_tests::essential_preparation_fixture()
}
