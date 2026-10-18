use super::kernel_wasm_sandbox_bridge::DynamicMicroDepotSandbox;
use super::*;
use oasis7_wasm_abi::ModuleLimits;
use std::sync::{Arc, Mutex};

fn install_fixture(data_units: i64) -> (WorldKernel, KernelSnapshot) {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-install".to_string(),
        name: "Install Hub".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.step().expect("register install location");
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-owner".to_string(),
        location_id: "loc-install".to_string(),
    });
    kernel.step().expect("register install owner");
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "agent-owner".to_string(),
        },
        ResourceKind::Data,
        data_units,
    );
    let base = kernel.kernel_snapshot();
    (kernel, base)
}

fn install_action(facility_id: &str, supported_resource_kinds: Vec<&str>) -> Action {
    Action::InstallMicroDepot {
        installer_agent_id: "agent-owner".to_string(),
        facility_id: facility_id.to_string(),
        location_id: "loc-install".to_string(),
        owner_claim_id: "claim-install".to_string(),
        regional_blocker_receipt_id: "repair-logistics:loc-install:1".to_string(),
        module_id: "regional.micro_depot".to_string(),
        module_version: "0.2.0".to_string(),
        wasm_hash: "hash-install".to_string(),
        entrypoint: "evaluate_quote".to_string(),
        service_radius_cm: 250_000,
        supported_resource_kinds: supported_resource_kinds
            .into_iter()
            .map(str::to_string)
            .collect(),
    }
}

fn owner_data(kernel: &WorldKernel) -> i64 {
    kernel
        .model()
        .agents
        .get("agent-owner")
        .expect("install owner")
        .resources
        .get(ResourceKind::Data)
}

fn install(kernel: &mut WorldKernel, facility_id: &str) -> WorldEventKind {
    kernel.submit_action(install_action(facility_id, vec!["data"]));
    kernel.step().expect("resolve install").kind
}

fn reclaim(kernel: &mut WorldKernel, facility_id: &str) -> WorldEventKind {
    kernel.submit_action(Action::ReclaimMicroDepot {
        agent_id: "agent-owner".to_string(),
        facility_id: facility_id.to_string(),
    });
    kernel.step().expect("resolve reclaim").kind
}

fn configure_two_unit_service(kernel: &mut WorldKernel) {
    let sandbox = Arc::new(Mutex::new(DynamicMicroDepotSandbox {
        resource_debits: Some(vec![MicroDepotProposalResourceDebit {
            resource_kind: ResourceKind::Data,
            units: 2,
        }]),
        ..DynamicMicroDepotSandbox::default()
    }));
    kernel.set_micro_depot_wasm_module_evaluator(
        "regional.micro_depot",
        "hash-install",
        "evaluate_quote",
        vec![0x00, 0x61, 0x73, 0x6d],
        ModuleLimits::unbounded(),
        sandbox,
    );
}

fn service_repair(kernel: &mut WorldKernel, facility_id: &str) -> WorldEventKind {
    kernel.submit_action(Action::ServiceMicroDepotRepair {
        agent_id: "agent-owner".to_string(),
        facility_id: facility_id.to_string(),
        target_id: "loc-install".to_string(),
        base_cost_class: MicroDepotPressureClass::Medium,
        base_risk_class: MicroDepotPressureClass::Low,
        blocker_type: Some("supply_missing".to_string()),
    });
    kernel.step().expect("resolve repair service").kind
}

fn evaluate_repair_quote(facility_id: &str) -> MicroDepotQuoteRequest {
    MicroDepotQuoteRequest {
        agent_id: "agent-owner".to_string(),
        facility_id: facility_id.to_string(),
        target_id: "loc-install".to_string(),
        action_kind: MicroDepotActionKind::Repair,
        base_cost_class: MicroDepotPressureClass::Medium,
        base_risk_class: MicroDepotPressureClass::Low,
        blocker_type: Some("supply_missing".to_string()),
    }
}

#[test]
fn micro_depot_evaluate_quote_is_non_mutating_and_keeps_service_confirmation_available() {
    let (mut kernel, _) = install_fixture(10);
    assert!(matches!(
        install(&mut kernel, "depot-quote-action"),
        WorldEventKind::MicroDepotInstalled { .. }
    ));
    configure_two_unit_service(&mut kernel);

    let model_before = kernel.model().clone();
    let journal_before = kernel.kernel_journal_snapshot();
    let quote = kernel
        .micro_depot_quote(&evaluate_repair_quote("depot-quote-action"))
        .expect("active stocked depot quote");

    assert_eq!(quote.proposal.decision, MicroDepotDecision::Applicable);
    assert_eq!(
        quote.proposal.resource_debits,
        vec![MicroDepotProposalResourceDebit {
            resource_kind: ResourceKind::Data,
            units: 2,
        }]
    );
    assert_eq!(
        kernel.model(),
        &model_before,
        "quote must not debit or reserve"
    );
    assert_eq!(
        kernel.kernel_journal_snapshot(),
        journal_before,
        "quote must not append a canonical world event"
    );

    let snapshot = kernel
        .micro_depot_player_facility_snapshots()
        .pop()
        .expect("active stocked depot snapshot");
    assert_eq!(
        snapshot.available_actions,
        vec![
            "evaluate_micro_depot_quote".to_string(),
            "service_micro_depot_repair".to_string(),
            "service_micro_depot_logistics".to_string(),
            "suspend_micro_depot".to_string(),
        ],
        "evaluation is a preview; repair/logistics remain the confirmation actions"
    );
}

#[test]
fn micro_depot_install_requires_ten_data_and_commissions_eight_inventory_sixteen_throughput() {
    let (mut insufficient, _) = install_fixture(9);
    let before = insufficient.model().clone();
    assert!(matches!(
        install(&mut insufficient, "depot-nine"),
        WorldEventKind::ActionRejected { .. }
    ));
    assert_eq!(
        insufficient.model(),
        &before,
        "failed install must be atomic"
    );

    let (mut kernel, base) = install_fixture(10);
    let event = install(&mut kernel, "depot-ten");
    match event {
        WorldEventKind::MicroDepotInstalled {
            install_cost_resources,
            measured_supply_schema_version,
            supported_resource_kinds,
            commissioning_sink_resources,
            commissioned_inventory_by_kind,
            initial_throughput_limit_units_per_epoch,
            initial_throughput_remaining_units,
            ..
        } => {
            assert_eq!(
                install_cost_resources,
                vec![MicroDepotResourceDebit {
                    kind: ResourceKind::Data,
                    amount: 10,
                }]
            );
            assert_eq!(measured_supply_schema_version, 2);
            assert_eq!(supported_resource_kinds, vec!["data"]);
            assert_eq!(
                commissioning_sink_resources,
                vec![MicroDepotResourceDebit {
                    kind: ResourceKind::Data,
                    amount: 2,
                }]
            );
            assert_eq!(commissioned_inventory_by_kind.get("data"), Some(&8));
            assert_eq!(initial_throughput_limit_units_per_epoch, 16);
            assert_eq!(initial_throughput_remaining_units, 16);
        }
        other => panic!("unexpected install event: {other:?}"),
    }
    assert_eq!(owner_data(&kernel), 0);
    let depot = kernel
        .model()
        .regional_infrastructure
        .get("depot-ten")
        .unwrap();
    assert_eq!(depot.available_units_by_kind.get("data"), Some(&8));
    assert_eq!(depot.throughput_limit_units_per_epoch, 16);
    assert_eq!(depot.throughput_remaining_units, 16);

    let replayed = WorldKernel::replay_from_kernel_snapshot(base, kernel.kernel_journal_snapshot())
        .expect("replay commissioned install");
    assert_eq!(replayed.model(), kernel.model());
}

#[test]
fn micro_depot_v2_replay_rejects_malformed_commissioning_provenance() {
    let (mut kernel, base) = install_fixture(10);
    assert!(matches!(
        install(&mut kernel, "depot-bad-provenance"),
        WorldEventKind::MicroDepotInstalled { .. }
    ));
    let mut journal = kernel.kernel_journal_snapshot();
    let WorldEventKind::MicroDepotInstalled {
        commissioned_inventory_by_kind,
        ..
    } = &mut journal.events.last_mut().unwrap().kind
    else {
        panic!("expected install event");
    };
    commissioned_inventory_by_kind.insert("data".to_string(), 9);

    let error = WorldKernel::replay_from_kernel_snapshot(base, journal)
        .expect_err("malformed commissioning evidence must fail closed");
    assert!(format!("{error:?}").contains("invalid commissioning provenance"));
}

#[test]
fn micro_depot_reclaim_destroys_supply_and_reinstall_is_fresh_non_subsidized_cycle() {
    let (mut short_owner, _) = install_fixture(19);
    assert!(matches!(
        install(&mut short_owner, "depot-short"),
        WorldEventKind::MicroDepotInstalled { .. }
    ));
    assert!(matches!(
        reclaim(&mut short_owner, "depot-short"),
        WorldEventKind::MicroDepotReclaimed { .. }
    ));
    assert_eq!(owner_data(&short_owner), 9);
    let before_failed_reinstall = short_owner.model().clone();
    assert!(matches!(
        install(&mut short_owner, "depot-short-reinstall"),
        WorldEventKind::ActionRejected { .. }
    ));
    assert_eq!(short_owner.model(), &before_failed_reinstall);

    let (mut kernel, base) = install_fixture(30);

    for cycle in 0..3 {
        let facility_id = format!("depot-cycle-{cycle}");
        assert!(matches!(
            install(&mut kernel, &facility_id),
            WorldEventKind::MicroDepotInstalled { .. }
        ));
        let facility = kernel
            .model()
            .regional_infrastructure
            .get(&facility_id)
            .unwrap();
        assert_eq!(facility.available_units_by_kind.get("data"), Some(&8));
        assert_eq!(facility.throughput_remaining_units, 16);
        assert!(matches!(
            reclaim(&mut kernel, &facility_id),
            WorldEventKind::MicroDepotReclaimed { .. }
        ));
        assert!(
            !kernel
                .model()
                .regional_infrastructure
                .contains_key(&facility_id)
        );
        assert_eq!(owner_data(&kernel), 30 - (cycle + 1) * 10);
    }

    assert_eq!(owner_data(&kernel), 0, "reclaim must refund zero");
    assert!(matches!(
        install(&mut kernel, "depot-no-subsidy"),
        WorldEventKind::ActionRejected { .. }
    ));
    assert!(kernel.model().regional_infrastructure.is_empty());

    let replayed = WorldKernel::replay_from_kernel_snapshot(base, kernel.kernel_journal_snapshot())
        .expect("replay repeated install/reclaim cycles");
    assert_eq!(replayed.model(), kernel.model());
}

#[test]
fn micro_depot_reachable_exhaustion_rejects_upkeep_then_reclaims_and_reinstalls_fresh() {
    let (mut kernel, _) = install_fixture(20);
    assert!(matches!(
        install(&mut kernel, "depot-lifecycle"),
        WorldEventKind::MicroDepotInstalled { .. }
    ));
    assert_eq!(
        owner_data(&kernel),
        10,
        "owner retains enough funds for upkeep"
    );
    let initial_snapshot = kernel
        .micro_depot_player_facility_snapshots()
        .pop()
        .expect("commissioned depot snapshot");
    assert_eq!(initial_snapshot.owner_claim_id, "claim-install");
    assert_eq!(initial_snapshot.status, "active");
    assert_eq!(
        initial_snapshot.available_units_by_kind.get("data"),
        Some(&8)
    );
    assert_eq!(initial_snapshot.throughput_remaining_units, 16);
    configure_two_unit_service(&mut kernel);

    for expected in [(6, 14), (4, 12), (2, 10), (0, 8)] {
        assert!(matches!(
            service_repair(&mut kernel, "depot-lifecycle"),
            WorldEventKind::MicroDepotServiceApplied { .. }
        ));
        let post_service_snapshot = kernel
            .micro_depot_player_facility_snapshots()
            .pop()
            .expect("post-service depot snapshot");
        assert_eq!(
            post_service_snapshot.available_units_by_kind.get("data"),
            Some(&expected.0)
        );
        assert_eq!(post_service_snapshot.throughput_remaining_units, expected.1);
    }
    let exhausted = kernel
        .model()
        .regional_infrastructure
        .get("depot-lifecycle")
        .expect("exhausted depot remains")
        .clone();
    assert_eq!(exhausted.available_units_by_kind.get("data"), Some(&0));
    assert_eq!(exhausted.throughput_remaining_units, 8);

    let depleted_snapshot = kernel
        .micro_depot_player_facility_snapshots()
        .pop()
        .expect("depleted depot snapshot");
    assert_eq!(depleted_snapshot.owner_claim_id, "claim-install");
    assert_eq!(depleted_snapshot.status, "depleted");
    assert_eq!(
        depleted_snapshot.available_units_by_kind.get("data"),
        Some(&0)
    );
    assert_eq!(depleted_snapshot.throughput_remaining_units, 8);

    let owner_before_service_rejection = kernel
        .model()
        .agents
        .get("agent-owner")
        .expect("owner before rejected fifth service")
        .clone();
    let journal_before_service_rejection = kernel.kernel_journal_snapshot();
    let rejected_service = service_repair(&mut kernel, "depot-lifecycle");
    let rejection_json =
        serde_json::to_string(&rejected_service).expect("serialize fifth service rejection");
    assert!(matches!(
        rejected_service,
        WorldEventKind::ActionRejected { .. }
    ));
    assert!(rejection_json.contains("DEPOT_INVENTORY_INSUFFICIENT"));
    assert!(rejection_json.contains("available_units=0"));
    assert!(!rejection_json.contains("DEPOT_THROUGHPUT_EXHAUSTED"));
    assert_eq!(
        kernel
            .model()
            .agents
            .get("agent-owner")
            .expect("owner after rejected fifth service"),
        &owner_before_service_rejection,
        "rejected fifth service must preserve owner business state"
    );
    assert_eq!(
        kernel
            .model()
            .regional_infrastructure
            .get("depot-lifecycle")
            .expect("depot after rejected fifth service"),
        &exhausted,
        "rejected fifth service must preserve facility business state"
    );
    let journal_after_service_rejection = kernel.kernel_journal_snapshot();
    assert_eq!(
        &journal_after_service_rejection.events[..journal_before_service_rejection.events.len()],
        journal_before_service_rejection.events.as_slice(),
        "rejected fifth service must not rewrite canonical history"
    );
    assert_eq!(
        journal_after_service_rejection.events.len(),
        journal_before_service_rejection.events.len() + 1
    );
    assert!(matches!(
        journal_after_service_rejection
            .events
            .last()
            .map(|event| &event.kind),
        Some(WorldEventKind::ActionRejected { .. })
    ));

    let owner_before_upkeep = owner_data(&kernel);
    let journal_before_upkeep = kernel.kernel_journal_snapshot();
    kernel.submit_action(Action::PayMicroDepotUpkeep {
        agent_id: "agent-owner".to_string(),
        facility_id: "depot-lifecycle".to_string(),
    });
    let upkeep = kernel.step().expect("resolve exhausted upkeep");
    assert!(matches!(upkeep.kind, WorldEventKind::ActionRejected { .. }));
    assert_eq!(owner_data(&kernel), owner_before_upkeep);
    assert_eq!(
        kernel
            .model()
            .regional_infrastructure
            .get("depot-lifecycle")
            .expect("rejected upkeep preserves depot"),
        &exhausted
    );
    let journal_after_upkeep = kernel.kernel_journal_snapshot();
    assert_eq!(
        &journal_after_upkeep.events[..journal_before_upkeep.events.len()],
        journal_before_upkeep.events.as_slice(),
        "rejected upkeep must not rewrite canonical history"
    );
    assert_eq!(
        journal_after_upkeep.events.len(),
        journal_before_upkeep.events.len() + 1
    );
    assert!(matches!(
        journal_after_upkeep.events.last().map(|event| &event.kind),
        Some(WorldEventKind::ActionRejected { .. })
    ));

    let snapshot = kernel
        .micro_depot_player_facility_snapshots()
        .pop()
        .expect("exhausted depot snapshot");
    assert!(
        !snapshot
            .available_actions
            .contains(&"pay_micro_depot_upkeep".to_string())
    );
    assert_eq!(
        snapshot.available_actions,
        vec!["reclaim_micro_depot".to_string()]
    );

    let owner_before_reclaim = owner_data(&kernel);
    assert!(matches!(
        reclaim(&mut kernel, "depot-lifecycle"),
        WorldEventKind::MicroDepotReclaimed { .. }
    ));
    assert_eq!(
        owner_data(&kernel),
        owner_before_reclaim,
        "reclaim refunds zero"
    );

    assert!(matches!(
        install(&mut kernel, "depot-lifecycle"),
        WorldEventKind::MicroDepotInstalled { .. }
    ));
    assert_eq!(owner_data(&kernel), 0, "reinstall charges a fresh ten data");
    let reinstalled = kernel
        .model()
        .regional_infrastructure
        .get("depot-lifecycle")
        .expect("reinstalled depot");
    assert_eq!(reinstalled.available_units_by_kind.get("data"), Some(&8));
    assert_eq!(reinstalled.throughput_remaining_units, 16);
}

#[test]
fn micro_depot_v2_install_accepts_exactly_canonical_data_kind_without_mutating_rejections() {
    let cases = [
        (vec!["data"], true),
        (vec![], false),
        (vec!["hardware"], false),
        (vec!["data", "hardware"], false),
        (vec!["data", "data"], false),
        (vec!["Data"], false),
    ];

    for (index, (kinds, accepted)) in cases.into_iter().enumerate() {
        let (mut kernel, _) = install_fixture(10);
        let before = kernel.model().clone();
        kernel.submit_action(install_action(
            &format!("depot-kind-{index}"),
            kinds.clone(),
        ));
        let event = kernel.step().expect("resolve supported-kind case").kind;
        if accepted {
            assert!(
                matches!(event, WorldEventKind::MicroDepotInstalled { .. }),
                "{kinds:?}"
            );
        } else {
            assert!(
                matches!(event, WorldEventKind::ActionRejected { .. }),
                "{kinds:?}"
            );
            assert_eq!(kernel.model(), &before, "rejected {kinds:?} mutated state");
        }
    }
}

#[test]
fn micro_depot_install_quote_is_non_mutating_and_makes_the_finite_commission_visible() {
    let (mut kernel, _) = install_fixture(10);
    configure_two_unit_service(&mut kernel);
    let model_before = kernel.model().clone();
    let journal_before = kernel.kernel_journal_snapshot();

    let quote = kernel
        .micro_depot_install_quote(&install_action("depot-quote", vec!["data"]))
        .expect("valid prospective install quote");
    let quote = serde_json::to_value(quote).expect("serialize install quote");

    assert_eq!(quote["deployable"], true);
    assert_eq!(
        quote["install_cost_resources"],
        serde_json::json!([{
            "kind": "data",
            "amount": 10
        }])
    );
    assert_eq!(
        quote["commissioning_sink_resources"],
        serde_json::json!([{
            "kind": "data",
            "amount": 2
        }])
    );
    assert_eq!(
        quote["commissioned_inventory_by_kind"],
        serde_json::json!({"data": 8})
    );
    assert_eq!(
        quote["upkeep_per_epoch_resources"],
        serde_json::json!([{
            "kind": "data",
            "amount": 1
        }])
    );
    assert_eq!(quote["projected_commissioning_service_uses"], 4);
    assert_eq!(quote["break_even_uses"], 5);
    assert_eq!(
        quote["forecast_status"], "insufficient",
        "the current runtime has no canonical regional-pressure forecast horizon"
    );
    let forecast_basis = quote["forecast_basis"]
        .as_str()
        .expect("forecast insufficiency must be auditable");
    let forecast_basis_lower = forecast_basis.to_ascii_lowercase();
    for required_term in [
        "canonical",
        "regional",
        "pressure",
        "forecast",
        "unavailable",
    ] {
        assert!(
            forecast_basis_lower.contains(required_term),
            "forecast basis {forecast_basis:?} must identify missing canonical regional-pressure forecast evidence"
        );
    }
    assert!(
        quote
            .get("expected_future_blockers_covered")
            .is_none_or(serde_json::Value::is_null),
        "missing forecast evidence must not invent future-blocker coverage"
    );
    assert_eq!(
        quote["deployable"], true,
        "forecast insufficiency preserves player choice when install rules pass"
    );
    assert_eq!(quote["low_use_warning"], true);
    let recommendation_reason = quote["recommendation_reason"]
        .as_str()
        .expect("recommendation is player-readable")
        .to_ascii_lowercase();
    assert!(
        recommendation_reason.contains("not recommended"),
        "insufficient forecast must be explicitly non-recommending: {recommendation_reason:?}"
    );
    assert_eq!(
        quote["expected_blocker_effect"]["blocker_change"],
        "repair_parts_gap_reduced"
    );
    assert!(quote["remaining_blockers"].is_array());
    assert!(quote["reasons"].as_array().is_some_and(Vec::is_empty));

    assert_eq!(
        kernel.model(),
        &model_before,
        "quoting must not reserve or debit"
    );
    assert_eq!(
        kernel.kernel_journal_snapshot(),
        journal_before,
        "quoting must not emit canonical history"
    );
}

#[test]
fn micro_depot_install_quote_reports_funded_post_install_upkeep_horizon_without_mutation() {
    let (mut kernel, _) = install_fixture(12);
    configure_two_unit_service(&mut kernel);
    let model_before = kernel.model().clone();
    let journal_before = kernel.kernel_journal_snapshot();

    let quote = kernel
        .micro_depot_install_quote(&install_action("depot-upkeep-horizon", vec!["data"]))
        .expect("funded prospective install quote");
    let quote = serde_json::to_value(quote).expect("serialize install quote");

    assert_eq!(quote["deployable"], true);
    assert_eq!(
        quote["upkeep_horizon_epochs"],
        serde_json::json!(2),
        "after reserving the ten-data install cost, two liquid Data cover two one-Data upkeep epochs"
    );
    assert_eq!(
        kernel.model(),
        &model_before,
        "quoting funded upkeep horizon must not reserve or debit"
    );
    assert_eq!(
        kernel.kernel_journal_snapshot(),
        journal_before,
        "quoting funded upkeep horizon must not emit canonical history"
    );
}

#[test]
fn micro_depot_install_quote_returns_non_mutating_structured_reasons_for_non_deployable_cases() {
    let cases = [
        (9, vec!["data"], "insufficient install resources"),
        (10, vec!["Data"], "invalid supported resource kinds"),
    ];

    for (index, (data_units, kinds, case_name)) in cases.into_iter().enumerate() {
        let (mut kernel, _) = install_fixture(data_units);
        configure_two_unit_service(&mut kernel);
        let model_before = kernel.model().clone();
        let journal_before = kernel.kernel_journal_snapshot();

        let quote = kernel
            .micro_depot_install_quote(&install_action(
                &format!("depot-quote-rejected-{index}"),
                kinds,
            ))
            .expect(case_name);
        let quote = serde_json::to_value(quote).expect("serialize rejected install quote");

        assert_eq!(quote["deployable"], false, "{case_name}");
        assert!(
            quote["reasons"]
                .as_array()
                .is_some_and(|reasons| !reasons.is_empty()),
            "{case_name} must be player-readable rather than a failed quote call"
        );
        assert_eq!(kernel.model(), &model_before, "{case_name} mutated model");
        assert_eq!(
            kernel.kernel_journal_snapshot(),
            journal_before,
            "{case_name} emitted history"
        );
    }

    let (mut kernel, _) = install_fixture(10);
    let zero_debit_sandbox = Arc::new(Mutex::new(DynamicMicroDepotSandbox {
        resource_debits: Some(vec![MicroDepotProposalResourceDebit {
            resource_kind: ResourceKind::Data,
            units: 0,
        }]),
        ..DynamicMicroDepotSandbox::default()
    }));
    kernel.set_micro_depot_wasm_module_evaluator(
        "regional.micro_depot",
        "hash-install",
        "evaluate_quote",
        vec![0x00, 0x61, 0x73, 0x6d],
        ModuleLimits::unbounded(),
        zero_debit_sandbox,
    );
    let model_before = kernel.model().clone();
    let journal_before = kernel.kernel_journal_snapshot();
    let quote = kernel
        .micro_depot_install_quote(&install_action("depot-zero-benefit", vec!["data"]))
        .expect("zero-benefit quote remains structured");
    let quote = serde_json::to_value(quote).expect("serialize zero-benefit quote");

    assert_eq!(quote["deployable"], false);
    assert_eq!(quote["projected_commissioning_service_uses"], 0);
    assert!(quote["low_use_warning"].as_bool().unwrap_or(false));
    assert!(
        quote["reasons"]
            .as_array()
            .is_some_and(|reasons| !reasons.is_empty()),
        "zero or inapplicable prospective debit must explain why it cannot support deployment"
    );
    assert_eq!(kernel.model(), &model_before);
    assert_eq!(kernel.kernel_journal_snapshot(), journal_before);
}
