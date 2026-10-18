use super::*;
use crate::runtime::{
    Action, FactoryModuleSpec, FactoryProductionState, FactoryState, IndustryStage,
    MaterialLedgerId, MaterialStack, WorldState,
};
use crate::simulator::{
    PlayerGameplayGoalKind, PlayerGameplayRecentFeedback, PlayerGameplayStageStatus,
};
use crate::viewer::FACTORY_SMELTER_MK1;
use oasis7_agent_api::world_action::ResourceKind;

pub(super) fn setup_runtime_industrial_gameplay_session(
    signer_seed: u8,
) -> (ViewerRuntimeLiveServer, String, String, String) {
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let (public_key, private_key) = test_signer(signer_seed);
    let register_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(agent_id.as_str()),
        u64::from(signer_seed.saturating_sub(1)),
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );
    (server, agent_id, public_key, private_key)
}

pub(super) fn build_first_smelter_via_gameplay_action(
    server: &mut ViewerRuntimeLiveServer,
    agent_id: &str,
    public_key: &str,
    private_key: &str,
    nonce: u64,
) {
    let build_request = signed_gameplay_action_request(
        crate::viewer::GameplayActionRequest {
            action_id: "build_factory_smelter_mk1".to_string(),
            target_agent_id: agent_id.to_string(),
            actor_agent_id: None,
            player_id: "player-a".to_string(),
            public_key: None,
            auth: None,
        },
        nonce,
        public_key,
        private_key,
    );
    let build_ack = server
        .handle_gameplay_action(build_request)
        .expect("queue smelter build");
    assert_eq!(build_ack.action_id, "build_factory_smelter_mk1");
    for _ in 0..2 {
        server.world.step().expect("settle smelter build");
    }
    assert!(server.world.has_factory("factory.smelter.mk1"));
    let factory = server
        .world
        .state()
        .factories
        .get("factory.smelter.mk1")
        .expect("smelter factory state");
    let site_ledger = crate::runtime::MaterialLedgerId::site(factory.site_id.as_str());
    server
        .world
        .set_ledger_material_balance(site_ledger.clone(), "iron_ore", 400)
        .expect("seed iron ore for repeated recipes");
    server
        .world
        .set_ledger_material_balance(site_ledger.clone(), "carbon_fuel", 120)
        .expect("seed carbon fuel for repeated recipes");
    server
        .world
        .set_ledger_material_balance(site_ledger.clone(), "copper_ore", 60)
        .expect("seed copper ore for smelter recipes");
    server
        .world
        .set_ledger_material_balance(site_ledger, "silicate_ore", 20)
        .expect("seed silicate ore for smelter recipes");
    server
        .world
        .set_material_balance("hardware_part", 200)
        .expect("seed maintenance parts for repeated recipes");
    server.world.set_resource_balance(
        oasis7_agent_api::world_action::ResourceKind::Electricity,
        2_000,
    );
    server
        .world
        .set_agent_resource_balance(
            agent_id,
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            2_000,
        )
        .expect("seed factory-builder electricity for repeated recipes");
}

pub(super) fn build_first_assembler_via_gameplay_action(
    server: &mut ViewerRuntimeLiveServer,
    agent_id: &str,
    public_key: &str,
    private_key: &str,
    nonce: u64,
) {
    let agent_ledger = crate::runtime::MaterialLedgerId::agent(agent_id);
    server
        .world
        .set_ledger_material_balance(agent_ledger.clone(), "structural_frame", 8)
        .expect("seed agent structural frame for assembler");
    server
        .world
        .set_ledger_material_balance(agent_ledger.clone(), "iron_ingot", 10)
        .expect("seed agent iron ingot for assembler");
    server
        .world
        .set_ledger_material_balance(agent_ledger, "copper_wire", 8)
        .expect("seed agent copper wire for assembler");

    let build_request = signed_gameplay_action_request(
        crate::viewer::GameplayActionRequest {
            action_id: "build_factory_assembler_mk1".to_string(),
            target_agent_id: agent_id.to_string(),
            actor_agent_id: None,
            player_id: "player-a".to_string(),
            public_key: None,
            auth: None,
        },
        nonce,
        public_key,
        private_key,
    );
    let build_ack = server
        .handle_gameplay_action(build_request)
        .expect("queue assembler build");
    assert_eq!(build_ack.action_id, "build_factory_assembler_mk1");
    for _ in 0..2 {
        server.world.step().expect("settle assembler build");
    }
    assert!(server.world.has_factory("factory.assembler.mk1"));
}

fn block_secondary_assembler_via_missing_inputs(
    server: &mut ViewerRuntimeLiveServer,
    agent_id: &str,
    public_key: &str,
    private_key: &str,
    nonce: u64,
) {
    let recipe_request = signed_gameplay_action_request(
        crate::viewer::GameplayActionRequest {
            action_id: "schedule_recipe_assembler_control_chip".to_string(),
            target_agent_id: agent_id.to_string(),
            actor_agent_id: None,
            player_id: "player-a".to_string(),
            public_key: None,
            auth: None,
        },
        nonce,
        public_key,
        private_key,
    );
    let recipe_ack = server
        .handle_gameplay_action(recipe_request)
        .expect("queue blocked assembler recipe");
    assert_eq!(
        recipe_ack.action_id,
        "schedule_recipe_assembler_control_chip"
    );
    for _ in 0..4 {
        server
            .world
            .step()
            .expect("settle blocked assembler recipe");
        if server
            .world
            .state()
            .factories
            .get("factory.assembler.mk1")
            .and_then(|factory| factory.production.current_blocker_kind.as_ref())
            .is_some()
        {
            break;
        }
    }
    assert!(
        server
            .world
            .state()
            .factories
            .get("factory.assembler.mk1")
            .and_then(|factory| factory.production.current_blocker_kind.as_ref())
            .is_some(),
        "expected assembler blocker after missing-input recipe attempt"
    );
}

fn complete_smelter_iron_ingot_jobs(
    server: &mut ViewerRuntimeLiveServer,
    agent_id: &str,
    public_key: &str,
    private_key: &str,
    start_nonce: u64,
    jobs: u64,
) {
    for offset in 0..jobs {
        let recipe_request = signed_gameplay_action_request(
            crate::viewer::GameplayActionRequest {
                action_id: "schedule_recipe_smelter_iron_ingot".to_string(),
                target_agent_id: agent_id.to_string(),
                actor_agent_id: None,
                player_id: "player-a".to_string(),
                public_key: None,
                auth: None,
            },
            start_nonce + offset,
            public_key,
            private_key,
        );
        let recipe_ack = server
            .handle_gameplay_action(recipe_request)
            .expect("queue iron ingot recipe");
        assert_eq!(recipe_ack.action_id, "schedule_recipe_smelter_iron_ingot");

        let completed_before = server.world.state().industry_progress.completed_recipe_jobs;
        for _ in 0..12 {
            server.world.step().expect("settle recipe");
            if server.world.state().industry_progress.completed_recipe_jobs > completed_before {
                break;
            }
        }
        assert!(
            server.world.state().industry_progress.completed_recipe_jobs > completed_before,
            "expected one more completed recipe job"
        );
    }
}

pub(crate) fn setup_industrial_gameplay_with_completed_jobs(
    signer_seed: u8,
    jobs: u64,
) -> ViewerRuntimeLiveServer {
    let (mut server, agent_id, public_key, private_key) =
        setup_runtime_industrial_gameplay_session(signer_seed);
    let build_nonce = u64::from(signer_seed);
    build_first_smelter_via_gameplay_action(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        build_nonce,
    );
    complete_smelter_iron_ingot_jobs(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        build_nonce + 1,
        jobs,
    );
    server
}

pub(super) fn expect_player_gameplay(
    server: &mut ViewerRuntimeLiveServer,
    context: &'static str,
) -> crate::simulator::PlayerGameplaySnapshot {
    server
        .compat_snapshot(Some("player-a"))
        .player_gameplay
        .expect(context)
}

pub(super) fn smelter_schedule_action<'a>(
    gameplay: &'a crate::simulator::PlayerGameplaySnapshot,
    action_id: &str,
) -> &'a crate::simulator::PlayerGameplayAction {
    gameplay
        .available_actions
        .iter()
        .find(|action| action.action_id == action_id)
        .unwrap_or_else(|| panic!("missing smelter schedule action {action_id}"))
}

#[test]
fn runtime_gameplay_snapshot_gates_every_smelter_schedule_by_authoritative_resource_affordability()
{
    let _guard = lock_test_llm_env();
    let (mut server, agent_id, public_key, private_key) =
        setup_runtime_industrial_gameplay_session(29);
    build_first_smelter_via_gameplay_action(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        29,
    );
    let unlocked_smelter_actions = [
        crate::viewer::ACTION_SCHEDULE_SMELTER_IRON_INGOT,
        crate::viewer::ACTION_SCHEDULE_SMELTER_COPPER_WIRE,
        crate::viewer::ACTION_SCHEDULE_SMELTER_POLYMER_RESIN,
    ];

    for (electricity, data, expected_reason) in [
        (0, i64::MAX, Some("insufficient factory-owner electricity")),
        (i64::MAX, 0, None),
        (i64::MAX, i64::MAX, None),
    ] {
        server
            .world
            .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Electricity, electricity)
            .expect("seed electricity");
        server
            .world
            .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Data, data)
            .expect("seed data");

        let gameplay = expect_player_gameplay(&mut server, "smelter affordability snapshot");
        for action_id in unlocked_smelter_actions {
            let action = smelter_schedule_action(&gameplay, action_id);
            match expected_reason {
                Some(expected_reason) => {
                    let disabled_reason = action.disabled_reason.as_deref().unwrap_or_else(|| {
                        panic!("insufficient-resource {action_id} should be disabled")
                    });
                    assert!(disabled_reason.contains(expected_reason));
                    assert!(disabled_reason.contains("replenish"));
                }
                None => {
                    assert_eq!(action.disabled_reason, None, "{action_id}");
                    assert!(
                        action.label.contains("Data estimate only")
                            && action.label.contains("non-authoritative"),
                        "{action_id} should retain Data only as an advisory label: {}",
                        action.label
                    );
                }
            }
        }
        assert_eq!(
            server
                .world
                .state()
                .agents
                .get(agent_id.as_str())
                .expect("bound agent")
                .state
                .resources
                .get(ResourceKind::Electricity),
            electricity,
            "availability snapshots must not debit electricity"
        );
        assert_eq!(
            server
                .world
                .state()
                .agents
                .get(agent_id.as_str())
                .expect("bound agent")
                .state
                .resources
                .get(ResourceKind::Data),
            data,
            "availability snapshots must not debit data"
        );
    }

    server
        .world
        .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Electricity, 0)
        .expect("drain electricity for stage precedence");
    server
        .world
        .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Data, 0)
        .expect("drain data for stage precedence");
    let bootstrap_gameplay = expect_player_gameplay(&mut server, "bootstrap alloy snapshot");
    let bootstrap_alloy = smelter_schedule_action(
        &bootstrap_gameplay,
        crate::viewer::ACTION_SCHEDULE_SMELTER_ALLOY_PLATE,
    );
    assert_eq!(
        bootstrap_alloy.disabled_reason.as_deref(),
        Some(
            "requires industry stage scale_out (current: bootstrap); complete additional smelter runs to unlock scale_out"
        ),
        "alloy stage gate takes precedence until the recipe is unlocked"
    );

    let mut scale_out_server = setup_industrial_gameplay_with_completed_jobs(30, 3);
    let scale_out_agent_id = scale_out_server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("scale-out agent");
    assert_eq!(
        scale_out_server.world.state().industry_progress.stage,
        IndustryStage::ScaleOut
    );
    let scale_out_site_ledger = MaterialLedgerId::site(
        scale_out_server
            .world
            .state()
            .factories
            .get(FACTORY_SMELTER_MK1)
            .expect("scale-out smelter")
            .site_id
            .as_str(),
    );
    scale_out_server
        .world
        .set_ledger_material_balance(scale_out_site_ledger.clone(), "iron_ingot", 200)
        .expect("seed alloy iron ingot");
    scale_out_server
        .world
        .set_ledger_material_balance(scale_out_site_ledger, "copper_wire", 200)
        .expect("seed alloy copper wire");
    for (electricity, data, expected_reason) in [
        (0, i64::MAX, Some("insufficient factory-owner electricity")),
        (i64::MAX, 0, None),
        (i64::MAX, i64::MAX, None),
    ] {
        scale_out_server
            .world
            .set_agent_resource_balance(
                scale_out_agent_id.as_str(),
                ResourceKind::Electricity,
                electricity,
            )
            .expect("seed scale-out electricity");
        scale_out_server
            .world
            .set_agent_resource_balance(scale_out_agent_id.as_str(), ResourceKind::Data, data)
            .expect("seed scale-out data");
        let gameplay = expect_player_gameplay(&mut scale_out_server, "scale-out alloy snapshot");
        let alloy = smelter_schedule_action(
            &gameplay,
            crate::viewer::ACTION_SCHEDULE_SMELTER_ALLOY_PLATE,
        );
        match expected_reason {
            Some(expected_reason) => {
                let disabled_reason = alloy
                    .disabled_reason
                    .as_deref()
                    .expect("insufficient-resource alloy schedule should be disabled");
                assert!(disabled_reason.contains(expected_reason));
                assert!(disabled_reason.contains("replenish"));
            }
            None => {
                assert_eq!(alloy.disabled_reason, None);
                assert!(
                    alloy.label.contains("Data estimate only")
                        && alloy.label.contains("non-authoritative"),
                    "alloy schedule should retain Data only as an advisory label: {}",
                    alloy.label
                );
            }
        }
        assert_eq!(
            scale_out_server
                .world
                .state()
                .agents
                .get(scale_out_agent_id.as_str())
                .expect("scale-out agent")
                .state
                .resources
                .get(ResourceKind::Electricity),
            electricity,
            "alloy availability snapshots must not debit electricity"
        );
        assert_eq!(
            scale_out_server
                .world
                .state()
                .agents
                .get(scale_out_agent_id.as_str())
                .expect("scale-out agent")
                .state
                .resources
                .get(ResourceKind::Data),
            data,
            "alloy availability snapshots must not debit data"
        );
    }
}

fn small_player_test_factory_spec(factory_id: &str) -> FactoryModuleSpec {
    FactoryModuleSpec {
        factory_id: factory_id.to_string(),
        display_name: "Test Smelter MK1".to_string(),
        tier: 2,
        tags: vec!["smelter".to_string(), "thermal".to_string()],
        build_cost: vec![MaterialStack::new("structural_frame", 12)],
        build_time_ticks: 1,
        base_power_draw: 20,
        recipe_slots: 2,
        throughput_bps: 10_000,
        maintenance_per_tick: 1,
    }
}

#[test]
fn runtime_gameplay_action_promotes_first_output_into_resilient_production_goal() {
    let _guard = lock_test_llm_env();
    let mut server = setup_industrial_gameplay_with_completed_jobs(31, 1);
    let gameplay = expect_player_gameplay(&mut server, "player gameplay after industrial progress");
    assert_eq!(
        gameplay.goal_id,
        "post_onboarding.stabilize_first_line_after_output"
    );
    assert_eq!(
        gameplay.goal_title,
        "Harden your first output into resilient production"
    );
    assert_eq!(gameplay.progress_percent, 80);
    assert_eq!(gameplay.stage_status, PlayerGameplayStageStatus::Active);
}

#[test]
fn runtime_gameplay_snapshot_surfaces_same_loop_repeat_count_before_new_leverage() {
    let _guard = lock_test_llm_env();
    let mut server = setup_industrial_gameplay_with_completed_jobs(32, 2);
    let gameplay = expect_player_gameplay(
        &mut server,
        "player gameplay after repeated first-line output",
    );
    assert_eq!(
        gameplay.goal_id,
        "post_onboarding.stabilize_first_line_after_output"
    );
    assert_eq!(gameplay.same_loop_repeat_count, 2);
    assert_eq!(gameplay.leverage_class.as_deref(), Some("throughput_only"));
    assert!(!gameplay.grind_only_flag);
}

#[test]
fn runtime_gameplay_snapshot_marks_forced_major_power_dependency_when_no_local_recovery_path_exists()
 {
    let feedback = PlayerGameplayRecentFeedback {
        action: "step".to_string(),
        stage: "blocked".to_string(),
        effect: "no committed recovery".to_string(),
        intent_summary: Some("advance the blocked line".to_string()),
        target_agent_id: None,
        reason: Some(
            "only viable continuation requires major power sponsorship; no independent path remains"
                .to_string(),
        ),
        hint: Some("secure a major-power sponsor before continuing".to_string()),
        delta_logical_time: 0,
        delta_event_seq: 0,
    };
    let gameplay = super::super::gameplay_snapshot::build_player_gameplay_snapshot(
        &WorldState::default(),
        None,
        true,
        Some(&feedback),
        None,
        None,
        true,
        None,
        false,
        true,
        None,
    );

    assert_eq!(
        gameplay.goal_kind,
        PlayerGameplayGoalKind::RecoverCapability
    );
    assert_eq!(
        gameplay.major_power_dependency_status.as_deref(),
        Some("forced")
    );
    assert_eq!(
        gameplay.requires_major_power_sponsorship.as_deref(),
        Some("yes")
    );
    assert_eq!(gameplay.recovery_path_kind.as_deref(), Some("none"));
    assert_eq!(gameplay.repair_available, Some(false));
    assert_eq!(gameplay.rebuild_available, Some(false));
    assert_eq!(gameplay.pivot_available, Some(false));
    assert!(
        gameplay
            .recovery_path_detail
            .as_deref()
            .is_some_and(|detail| detail.contains("no repair, rebuild, or pivot path"))
    );
}

#[test]
fn runtime_gameplay_snapshot_publishes_complete_distinguishable_recovery_options() {
    let _guard = lock_test_llm_env();
    let mut server = setup_industrial_gameplay_with_completed_jobs(42, 4);
    let gameplay = expect_player_gameplay(
        &mut server,
        "recoverable player progression after first expansion tradeoff",
    );

    assert_eq!(
        gameplay.recovery_path_kind.as_deref(),
        Some("repair_rebuild_or_pivot")
    );
    assert_eq!(gameplay.repair_available, Some(true));
    assert_eq!(gameplay.rebuild_available, Some(true));
    assert_eq!(gameplay.pivot_available, Some(true));

    assert_eq!(
        gameplay.recovery_options.len(),
        3,
        "recoverable progression must publish one canonical option for each recovery path"
    );
    let expected_kinds = ["repair", "rebuild", "pivot"];
    for kind in expected_kinds {
        let option = gameplay
            .recovery_options
            .iter()
            .find(|option| option.kind == kind)
            .unwrap_or_else(|| panic!("missing canonical {kind} recovery option"));
        assert!(
            !option.estimated_time_class.trim().is_empty(),
            "{kind} must publish a qualitative estimated-time class"
        );
        assert!(
            !option.estimated_resource_class.trim().is_empty(),
            "{kind} must publish a qualitative estimated-resource class"
        );
        assert!(
            !option.risk_class.trim().is_empty(),
            "{kind} must publish a qualitative risk class"
        );
        assert!(
            !option.retained_benefit.trim().is_empty(),
            "{kind} must state the player benefit retained by this recovery path"
        );
        assert!(
            !option.recommendation_reason.trim().is_empty(),
            "{kind} must state why this recovery path is recommended"
        );
    }
}

#[test]
fn runtime_gameplay_snapshot_does_not_borrow_retired_agent_factory_for_branch_progress() {
    let mut state = WorldState::default();
    state.industry_progress.stage = IndustryStage::ScaleOut;
    state.industry_progress.completed_recipe_jobs = 4;
    state.factories.insert(
        FACTORY_SMELTER_MK1.to_string(),
        FactoryState {
            factory_id: FACTORY_SMELTER_MK1.to_string(),
            site_id: "runtime:10:20:0".to_string(),
            builder_agent_id: "retired-agent".to_string(),
            spec: small_player_test_factory_spec(FACTORY_SMELTER_MK1),
            input_ledger: MaterialLedgerId::world(),
            output_ledger: MaterialLedgerId::world(),
            durability_ppm: 1_000_000,
            production: FactoryProductionState {
                completed_jobs: 4,
                last_completed_at: Some(12),
                last_completed_recipe_id: Some("recipe.smelter.iron_ingot".to_string()),
                ..FactoryProductionState::default()
            },
            location_anchor_revision: None,
            site_authority_revision: None,
            site_location_id: None,
            construction_power_profile_key: None,
            construction_power_profile_revision: None,
            built_at: 1,
        },
    );
    let gameplay = super::super::gameplay_snapshot::build_player_gameplay_snapshot(
        &state,
        Some("missing-agent"),
        true,
        None,
        None,
        None,
        true,
        None,
        false,
        true,
        None,
    );
    assert_eq!(gameplay.stage_status, PlayerGameplayStageStatus::Active);
    assert_eq!(
        gameplay.execution_state,
        crate::simulator::PlayerGameplayExecutionState::Executing
    );
    assert!(gameplay.branch_recommendations.is_empty());
    assert_eq!(gameplay.blocker_kind, None);
    assert_eq!(
        gameplay.goal_kind,
        PlayerGameplayGoalKind::EstablishFirstCapability
    );
    assert_eq!(gameplay.branch_hint, None);
}

#[test]
fn runtime_gameplay_action_promotes_to_generic_midloop_after_governance_ready() {
    let _guard = lock_test_llm_env();
    let mut server = setup_industrial_gameplay_with_completed_jobs(51, 6);
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .expect("governance-ready agent")
        .clone();
    server
        .world
        .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Electricity, i64::MAX)
        .expect("fund governance-ready agent electricity");
    server
        .world
        .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Data, i64::MAX)
        .expect("fund governance-ready agent data");
    let smelter_site_ledger = MaterialLedgerId::site(
        server
            .world
            .state()
            .factories
            .get(FACTORY_SMELTER_MK1)
            .expect("governance-ready smelter")
            .site_id
            .as_str(),
    );
    server
        .world
        .set_ledger_material_balance(smelter_site_ledger.clone(), "iron_ingot", 200)
        .expect("seed governance alloy iron ingot");
    server
        .world
        .set_ledger_material_balance(smelter_site_ledger, "copper_wire", 200)
        .expect("seed governance alloy copper wire");
    server
        .world
        .set_resource_balance(ResourceKind::Electricity, 2_000);
    let gameplay =
        expect_player_gameplay(&mut server, "player gameplay after governance-ready output");
    assert_eq!(gameplay.goal_id, "post_onboarding.choose_midloop_path");
    assert_eq!(
        gameplay.goal_kind,
        PlayerGameplayGoalKind::ChooseMidLoopPath
    );
    assert_eq!(gameplay.progress_percent, 100);
    assert_eq!(
        gameplay.small_player_lane_id.as_deref(),
        Some("regional_specialist")
    );
    assert_eq!(
        gameplay.major_power_dependency_status.as_deref(),
        Some("independent_path_available")
    );
    let next_step_hint = gameplay.next_step_hint.to_lowercase();
    let branch_hint = gameplay
        .branch_hint
        .as_deref()
        .expect("regional-specialist snapshot must publish a branch hint")
        .to_lowercase();
    for (label, hint) in [
        ("next_step_hint", next_step_hint.as_str()),
        ("branch_hint", branch_hint.as_str()),
    ] {
        assert!(
            hint.contains("local") || hint.contains("regional"),
            "{label} must signal a local or regional route: {hint}",
        );
        assert!(
            hint.contains("specialist") || hint.contains("specialization"),
            "{label} must signal specialization: {hint}",
        );
        for prohibited_term in ["governance", "conflict"] {
            assert!(
                !hint.contains(prohibited_term),
                "{label} must not default to {prohibited_term}: {hint}",
            );
        }
    }
    assert!((1..=3).contains(&gameplay.branch_recommendations.len()));
    for recommendation in &gameplay.branch_recommendations {
        assert_eq!(
            recommendation.future_beats.len(),
            2,
            "each actionable branch recommendation must expose exactly two future beats"
        );
        assert!(
            recommendation
                .future_beats
                .iter()
                .all(|beat| !beat.trim().is_empty()),
            "future beats must be player-readable"
        );
        assert_ne!(
            recommendation.future_beats[0].trim(),
            recommendation.future_beats[1].trim(),
            "future beats must describe substantively distinct changes"
        );
        let published_claims = format!(
            "{} {} {} {} {} {}",
            recommendation.route_label,
            recommendation.immediate_gain,
            recommendation.future_beat_changed,
            recommendation.risk_or_lockin,
            recommendation.next_session_hook,
            recommendation.future_beats.join(" "),
        )
        .to_lowercase();
        for unsupported_claim in [
            "governance",
            "conflict",
            "vote",
            "voting",
            "territory",
            "territorial",
            "security",
        ] {
            assert!(
                !published_claims.contains(unsupported_claim),
                "regional-specialist recommendation must not claim unsupported {unsupported_claim} mechanics",
            );
        }
    }
    assert!(
        gameplay
            .branch_recommendations
            .iter()
            .any(|recommendation| {
                recommendation.action_id == "schedule_recipe_smelter_alloy_plate"
            })
    );
    assert!(
        gameplay
            .available_actions
            .iter()
            .any(
                |action| action.action_id == "schedule_recipe_smelter_alloy_plate"
                    && action.disabled_reason.is_none()
            )
    );
}

#[test]
fn runtime_gameplay_actions_expose_scale_out_and_governance_recipes_once_assembler_exists() {
    let _guard = lock_test_llm_env();
    let (mut server, agent_id, public_key, private_key) =
        setup_runtime_industrial_gameplay_session(52);
    let build_nonce = 52_u64;
    build_first_smelter_via_gameplay_action(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        build_nonce,
    );
    complete_smelter_iron_ingot_jobs(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        build_nonce + 1,
        6,
    );
    build_first_assembler_via_gameplay_action(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        build_nonce + 10,
    );
    let assembler_site_ledger = crate::runtime::MaterialLedgerId::site(
        server
            .world
            .state()
            .factories
            .get("factory.assembler.mk1")
            .expect("governance-ready assembler")
            .site_id
            .as_str(),
    );
    for (kind, amount) in [
        ("control_chip", 20),
        ("copper_wire", 20),
        ("sensor_pack", 10),
        ("module_rack", 4),
        ("alloy_plate", 10),
        ("hardware_part", 20),
    ] {
        server
            .world
            .set_ledger_material_balance(assembler_site_ledger.clone(), kind, amount)
            .expect("seed governance-ready assembler site ledger");
    }

    let gameplay = expect_player_gameplay(
        &mut server,
        "player gameplay after assembler build in governance stage",
    );
    assert!(
        gameplay
            .available_actions
            .iter()
            .any(
                |action| action.action_id == "schedule_recipe_assembler_sensor_pack"
                    && action.disabled_reason.is_none()
            )
    );
    assert!(
        gameplay
            .available_actions
            .iter()
            .any(
                |action| action.action_id == "schedule_recipe_assembler_module_rack"
                    && action.disabled_reason.is_none()
            )
    );
    assert!(
        gameplay
            .available_actions
            .iter()
            .any(
                |action| action.action_id == "schedule_recipe_assembler_factory_core"
                    && action.disabled_reason.is_none()
            )
    );
}

#[test]
fn runtime_gameplay_action_drops_midloop_ready_after_last_capability_is_recycled() {
    let _guard = lock_test_llm_env();
    let mut server = setup_industrial_gameplay_with_completed_jobs(61, 6);
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let gameplay = expect_player_gameplay(
        &mut server,
        "player gameplay before recycling the only output capability",
    );
    assert_eq!(gameplay.goal_id, "post_onboarding.choose_midloop_path");
    assert_eq!(
        server.world.state().industry_progress.stage,
        IndustryStage::Governance
    );

    server.world.submit_action(Action::RecycleFactory {
        operator_agent_id: agent_id,
        factory_id: "factory.smelter.mk1".to_string(),
    });
    server.world.step().expect("recycle smelter capability");

    assert!(!server.world.has_factory("factory.smelter.mk1"));
    assert_eq!(
        server.world.state().industry_progress.stage,
        IndustryStage::Bootstrap
    );

    let gameplay = expect_player_gameplay(
        &mut server,
        "player gameplay after recycling the only output capability",
    );
    assert_ne!(gameplay.goal_id, "post_onboarding.choose_midloop_path");
    assert!(gameplay.progress_percent < 100);
    assert_ne!(
        gameplay.stage_status,
        PlayerGameplayStageStatus::BranchReady
    );
}

#[test]
fn runtime_gameplay_action_keeps_primary_goal_when_secondary_factory_blocks() {
    let _guard = lock_test_llm_env();
    let (mut server, agent_id, public_key, private_key) =
        setup_runtime_industrial_gameplay_session(71);
    let build_nonce = 71_u64;
    build_first_smelter_via_gameplay_action(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        build_nonce,
    );
    complete_smelter_iron_ingot_jobs(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        build_nonce + 1,
        3,
    );
    build_first_assembler_via_gameplay_action(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        build_nonce + 10,
    );
    block_secondary_assembler_via_missing_inputs(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        build_nonce + 11,
    );

    let smelter = server
        .world
        .state()
        .factories
        .get("factory.smelter.mk1")
        .expect("smelter state");
    assert!(smelter.production.current_blocker_kind.is_none());
    let assembler = server
        .world
        .state()
        .factories
        .get("factory.assembler.mk1")
        .expect("assembler state");
    assert!(assembler.production.current_blocker_kind.is_some());

    let gameplay = expect_player_gameplay(
        &mut server,
        "player gameplay with healthy smelter and blocked secondary assembler",
    );
    assert_eq!(
        gameplay.goal_id,
        "post_onboarding.choose_first_expansion_tradeoff"
    );
    assert_eq!(
        gameplay.stage_status,
        PlayerGameplayStageStatus::BranchReady
    );
    assert_eq!(gameplay.blocker_kind, None);
}

#[test]
fn legacy_operator_observer_rejects_gameplay_model_initialization() {
    let _guard = lock_test_llm_env();
    let execution_world_dir = runtime_live_temp_dir("chain_gameplay_submit");
    crate::runtime::World::new_production_hardened()
        .save_to_dir(execution_world_dir.as_path())
        .expect("persist initial execution world");

    let chain_status = TestChainStatusServer::start(execution_world_dir.clone());
    let chain_submit = TestChainStatusServer::start(execution_world_dir.clone());
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm)
            .with_chain_status_bind(chain_status.addr.clone())
            .with_chain_execution_world_dir(execution_world_dir.clone())
            .with_chain_submit_bind(chain_submit.addr.clone())
            .with_chain_poll_interval(Duration::from_millis(50)),
    )
    .expect("runtime server");
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let (public_key, private_key) = test_signer(41);
    let register_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(agent_id.as_str()),
        40,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );
    server
        .world
        .save_to_dir(execution_world_dir.as_path())
        .expect("persist viewer baseline execution world");

    let submit_request = signed_gameplay_action_request(
        crate::viewer::GameplayActionRequest {
            action_id: "build_factory_smelter_mk1".to_string(),
            target_agent_id: agent_id.clone(),
            actor_agent_id: None,
            player_id: "player-a".to_string(),
            public_key: None,
            auth: None,
        },
        41,
        public_key.as_str(),
        private_key.as_str(),
    );
    // A directory-backed operator observer supplies no authenticated Agent
    // service context. A valid signed request cannot open a legacy model lane.
    let before_world = serde_json::to_value(server.world.snapshot()).unwrap();
    let before_events = server.world.journal().events.clone();
    let before_provider = server.test_canonical_provider_summary();
    let before_session = serde_json::to_value(&server.session_policy).unwrap();
    let before_height = server.last_chain_committed_height;
    let error = server
        .handle_gameplay_action(submit_request)
        .expect_err("operator observer must refuse gameplay model initialization");
    assert_eq!(error.code, "llm_init_failed");
    assert_eq!(
        error.message,
        format!(
            "canonical Agent observation unavailable (action_id=build_factory_smelter_mk1, target_agent_id={agent_id})"
        )
    );
    assert_eq!(
        error.action_id.as_deref(),
        Some("build_factory_smelter_mk1")
    );
    assert_eq!(error.target_agent_id.as_deref(), Some(agent_id.as_str()));
    assert_eq!(
        serde_json::to_value(server.world.snapshot()).unwrap(),
        before_world
    );
    assert_eq!(server.world.journal().events, before_events);
    assert_eq!(server.test_canonical_provider_summary(), before_provider);
    assert_eq!(
        serde_json::to_value(&server.session_policy).unwrap(),
        before_session
    );
    assert_eq!(server.last_chain_committed_height, before_height);
    assert!(!server.world.has_factory("factory.smelter.mk1"));
    assert!(chain_status.submitted_gameplay_requests().is_empty());
    assert!(chain_submit.submitted_gameplay_requests().is_empty());
}
