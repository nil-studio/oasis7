use super::industrial_progression::{
    build_first_assembler_via_gameplay_action, build_first_smelter_via_gameplay_action,
    expect_player_gameplay, setup_industrial_gameplay_with_completed_jobs,
    setup_runtime_industrial_gameplay_session, smelter_schedule_action,
};
use super::*;
use crate::runtime::{Action, ProductProfileV1, RecipeProfileV1};
use crate::simulator::{PlayerGameplayGoalKind, PlayerGameplayStageStatus};
use oasis7_agent_api::world_action::ResourceKind;

mod assembler_power_boundaries;
mod starter_chain_prerequisite;

fn submit_iron_ingot_schedule(
    server: &mut ViewerRuntimeLiveServer,
    agent_id: &str,
    public_key: &str,
    private_key: &str,
    nonce: u64,
) {
    server
        .handle_gameplay_action(signed_gameplay_action_request(
            crate::viewer::GameplayActionRequest {
                action_id: crate::viewer::ACTION_SCHEDULE_SMELTER_IRON_INGOT.to_string(),
                target_agent_id: agent_id.to_string(),
                actor_agent_id: None,
                player_id: "player-a".to_string(),
                public_key: None,
                auth: None,
            },
            nonce,
            public_key,
            private_key,
        ))
        .expect("queue iron ingot schedule");
    server
        .world
        .step()
        .expect("settle rejected iron ingot schedule");
}

fn settle_one_smelter_iron_ingot_job(
    server: &mut ViewerRuntimeLiveServer,
    agent_id: &str,
    public_key: &str,
    private_key: &str,
    nonce: u64,
) {
    let completed_before = server.world.state().industry_progress.completed_recipe_jobs;
    submit_iron_ingot_schedule(server, agent_id, public_key, private_key, nonce);
    for _ in 0..12 {
        server.world.step().expect("settle starter smelter recipe");
        if server.world.state().industry_progress.completed_recipe_jobs > completed_before {
            return;
        }
    }
    panic!("expected starter smelter recipe to settle");
}

fn seed_assembler_site_materials(server: &mut ViewerRuntimeLiveServer, materials: &[(&str, i64)]) {
    let site_ledger = server
        .world
        .state()
        .factories
        .get(crate::viewer::FACTORY_ASSEMBLER_MK1)
        .expect("assembler factory")
        .site_id
        .clone();
    let site_ledger = crate::runtime::MaterialLedgerId::site(site_ledger);
    for (kind, amount) in materials {
        server
            .world
            .set_ledger_material_balance(site_ledger.clone(), *kind, *amount)
            .expect("seed assembler site material");
    }
}

fn ready_smelter_for_schedule_site_parity(seed: u8) -> (ViewerRuntimeLiveServer, String) {
    let (mut server, agent_id, public_key, private_key) =
        setup_runtime_industrial_gameplay_session(seed);
    build_first_smelter_via_gameplay_action(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        u64::from(seed),
    );
    (server, agent_id)
}

fn assert_smelter_schedule_enabled(server: &mut ViewerRuntimeLiveServer, label: &'static str) {
    let gameplay = expect_player_gameplay(server, label);
    let action =
        smelter_schedule_action(&gameplay, crate::viewer::ACTION_SCHEDULE_SMELTER_IRON_INGOT);
    assert_eq!(
        action.disabled_reason, None,
        "runtime ScheduleRecipe admission does not gate on current site status: {label}"
    );
}

#[test]
fn runtime_live_formal_bootstrap_keeps_smelter_and_assembler_builds_ready() {
    let (mut world, _) = crate::viewer::viewer_bootstrap_formal_release_runtime_world()
        .expect("formal release bootstrap");
    let agent_id = crate::viewer::runtime_live::support::FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID;

    let initial = super::super::gameplay_snapshot::build_player_gameplay_snapshot(
        world.state(),
        Some(agent_id),
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
    let smelter = initial
        .available_actions
        .iter()
        .find(|action| action.action_id == crate::viewer::ACTION_BUILD_SMELTER_MK1)
        .expect("formal bootstrap must publish the smelter build");
    assert_eq!(smelter.disabled_reason, None);

    for action_id in [
        crate::viewer::ACTION_BUILD_SMELTER_MK1,
        crate::viewer::ACTION_SCHEDULE_SMELTER_IRON_INGOT,
    ] {
        let action = crate::viewer::gameplay_actions::build_runtime_action_from_gameplay_request(
            &crate::viewer::GameplayActionRequest {
                action_id: action_id.to_string(),
                target_agent_id: agent_id.to_string(),
                actor_agent_id: None,
                player_id: "bootstrap-readiness-test".to_string(),
                public_key: None,
                auth: None,
            },
        )
        .expect("formal bootstrap industrial action");
        world.submit_action(action);
        world.step().expect("settle formal bootstrap action");
        if action_id == crate::viewer::ACTION_BUILD_SMELTER_MK1 {
            world.step().expect("complete formal smelter construction");
        }
    }
    let completed_smelter_jobs_before = world.state().industry_progress.completed_recipe_jobs;
    for _ in 0..12 {
        world
            .step()
            .expect("complete formal starter smelter recipe");
        if world.state().industry_progress.completed_recipe_jobs > completed_smelter_jobs_before {
            break;
        }
    }
    assert!(
        world.state().industry_progress.completed_recipe_jobs > completed_smelter_jobs_before,
        "formal bootstrap must settle the starter smelter recipe before assembler build"
    );

    let after_first_run = super::super::gameplay_snapshot::build_player_gameplay_snapshot(
        world.state(),
        Some(agent_id),
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
    let assembler = after_first_run
        .available_actions
        .iter()
        .find(|action| action.action_id == crate::viewer::ACTION_BUILD_ASSEMBLER_MK1)
        .expect("first smelter run must publish the assembler build");
    assert_eq!(assembler.disabled_reason, None);

    let build_assembler =
        crate::viewer::gameplay_actions::build_runtime_action_from_gameplay_request(
            &crate::viewer::GameplayActionRequest {
                action_id: crate::viewer::ACTION_BUILD_ASSEMBLER_MK1.to_string(),
                target_agent_id: agent_id.to_string(),
                actor_agent_id: None,
                player_id: "bootstrap-readiness-test".to_string(),
                public_key: None,
                auth: None,
            },
        )
        .expect("formal bootstrap assembler build action");
    world.submit_action(build_assembler);
    world.step().expect("settle formal assembler construction");
    world
        .step()
        .expect("complete formal assembler construction");
    assert!(world.has_factory(crate::viewer::FACTORY_ASSEMBLER_MK1));
    assert_eq!(
        world
            .agent_resource_balance(agent_id, ResourceKind::Electricity)
            .expect("starter agent power before first assembler recipe"),
        16,
        "bootstrap must reserve four gear batches at four electricity each"
    );

    let after_assembler = super::super::gameplay_snapshot::build_player_gameplay_snapshot(
        world.state(),
        Some(agent_id),
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
    let gear = after_assembler
        .available_actions
        .iter()
        .find(|action| action.action_id == crate::viewer::ACTION_SCHEDULE_ASSEMBLER_GEAR)
        .expect("new assembler must publish the canonical first gear recipe");
    assert_eq!(gear.disabled_reason, None);

    let gear_action = crate::viewer::gameplay_actions::build_runtime_action_from_gameplay_request(
        &crate::viewer::GameplayActionRequest {
            action_id: crate::viewer::ACTION_SCHEDULE_ASSEMBLER_GEAR.to_string(),
            target_agent_id: agent_id.to_string(),
            actor_agent_id: None,
            player_id: "bootstrap-readiness-test".to_string(),
            public_key: None,
            auth: None,
        },
    )
    .expect("formal bootstrap first assembler recipe action");
    world.submit_action(gear_action);
    world
        .step()
        .expect("settle formal bootstrap first assembler recipe");
    assert_eq!(world.pending_recipe_jobs_len(), 1);
    assert_eq!(
        world
            .agent_resource_balance(agent_id, ResourceKind::Electricity)
            .expect("starter agent power after first assembler recipe admission"),
        0
    );

    world
        .step()
        .expect("complete formal bootstrap first assembler recipe");
    let gear_ledger = crate::runtime::MaterialLedgerId::site("site-assembler");
    assert_eq!(
        world.ledger_material_balance(&gear_ledger, "gear"),
        4,
        "the canonical first assembler recipe must settle its output"
    );
}

#[test]
fn runtime_gameplay_schedule_readiness_matches_governed_recipe_and_output_unlocks() {
    let _guard = lock_test_llm_env();
    let (mut server, _agent_id) = ready_smelter_for_schedule_site_parity(78);

    server
        .world
        .upsert_recipe_profile(RecipeProfileV1 {
            recipe_id: "recipe.smelter.iron_ingot".to_string(),
            bottleneck_tags: vec!["iron_ore".to_string()],
            stage_gate: "scale-out".to_string(),
            preferred_factory_tags: vec!["smelter".to_string()],
        })
        .expect("govern iron ingot recipe stage");
    let gameplay = expect_player_gameplay(&mut server, "governed recipe stage readiness");
    let stage_reason =
        smelter_schedule_action(&gameplay, crate::viewer::ACTION_SCHEDULE_SMELTER_IRON_INGOT)
            .disabled_reason
            .as_deref()
            .expect("governed recipe stage must disable scheduling");
    assert_eq!(
        stage_reason,
        "recipe stage gate denied: recipe=recipe.smelter.iron_ingot required_stage=scale-out current_stage=bootstrap"
    );

    server
        .world
        .upsert_recipe_profile(RecipeProfileV1 {
            recipe_id: "recipe.smelter.iron_ingot".to_string(),
            bottleneck_tags: vec!["iron_ore".to_string()],
            stage_gate: "bootstrap".to_string(),
            preferred_factory_tags: vec!["assembler".to_string()],
        })
        .expect("govern iron ingot recipe factory tags");
    let gameplay = expect_player_gameplay(&mut server, "governed recipe tag readiness");
    let tag_reason =
        smelter_schedule_action(&gameplay, crate::viewer::ACTION_SCHEDULE_SMELTER_IRON_INGOT)
            .disabled_reason
            .as_deref()
            .expect("incompatible governed factory tags must disable scheduling");
    assert_eq!(
        tag_reason,
        "recipe preferred_factory_tags mismatch: recipe=recipe.smelter.iron_ingot preferred=[\"assembler\"] factory_tags=[\"smelter\", \"thermal\"]"
    );

    server
        .world
        .upsert_recipe_profile(RecipeProfileV1 {
            recipe_id: "recipe.smelter.iron_ingot".to_string(),
            bottleneck_tags: vec!["iron_ore".to_string()],
            stage_gate: "bootstrap".to_string(),
            preferred_factory_tags: vec!["smelter".to_string()],
        })
        .expect("allow governed iron ingot recipe");
    server
        .world
        .upsert_product_profile(ProductProfileV1 {
            product_id: "iron_ingot".to_string(),
            role_tag: "scale".to_string(),
            maintenance_sink: Vec::new(),
            tradable: true,
            unlock_stage: "governance".to_string(),
        })
        .expect("govern iron ingot output unlock");
    let gameplay = expect_player_gameplay(&mut server, "governed output unlock readiness");
    let output_reason =
        smelter_schedule_action(&gameplay, crate::viewer::ACTION_SCHEDULE_SMELTER_IRON_INGOT)
            .disabled_reason
            .as_deref()
            .expect("governed product unlock must disable scheduling");
    assert_eq!(
        output_reason,
        "product unlock_stage denied: product=iron_ingot required_stage=governance current_stage=bootstrap"
    );

    server
        .world
        .upsert_product_profile(ProductProfileV1 {
            product_id: "iron_ingot".to_string(),
            role_tag: "scale".to_string(),
            maintenance_sink: Vec::new(),
            tradable: true,
            unlock_stage: "bootstrap".to_string(),
        })
        .expect("allow governed iron ingot output");
    let gameplay = expect_player_gameplay(&mut server, "governed schedule readiness parity");
    assert_eq!(
        smelter_schedule_action(&gameplay, crate::viewer::ACTION_SCHEDULE_SMELTER_IRON_INGOT,)
            .disabled_reason,
        None,
        "ready governed recipe must match runtime admission"
    );
}

#[test]
fn runtime_gameplay_smelter_readiness_matches_submit_material_and_owner_power_checks() {
    let _guard = lock_test_llm_env();
    let (mut server, agent_id, public_key, private_key) =
        setup_runtime_industrial_gameplay_session(43);
    build_first_smelter_via_gameplay_action(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        43,
    );
    server
        .world
        .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Electricity, i64::MAX)
        .expect("fund agent electricity");
    server
        .world
        .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Data, i64::MAX)
        .expect("fund agent data");

    let input_ledger = server
        .world
        .state()
        .factories
        .get("factory.smelter.mk1")
        .expect("smelter factory")
        .input_ledger
        .clone();
    server
        .world
        .set_ledger_material_balance(input_ledger, "iron_ore", 0)
        .expect("drain smelter iron ore");
    server
        .world
        .set_material_balance("iron_ore", 0)
        .expect("drain world iron ore");

    let gameplay = expect_player_gameplay(&mut server, "material readiness snapshot");
    let material_reason =
        smelter_schedule_action(&gameplay, crate::viewer::ACTION_SCHEDULE_SMELTER_IRON_INGOT)
            .disabled_reason
            .as_deref()
            .expect("missing material should disable iron ingot scheduling");
    assert!(material_reason.contains("insufficient iron_ore"));
    assert!(material_reason.contains("replenish iron_ore"));

    submit_iron_ingot_schedule(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        44,
    );
    assert_eq!(
        server
            .world
            .state()
            .factories
            .get("factory.smelter.mk1")
            .and_then(|factory| factory.production.current_blocker_kind.as_deref()),
        Some("material_shortage"),
        "submit must reject the material shortage surfaced by readiness"
    );

    let input_ledger = server
        .world
        .state()
        .factories
        .get("factory.smelter.mk1")
        .expect("smelter factory")
        .input_ledger
        .clone();
    server
        .world
        .set_ledger_material_balance(input_ledger, "iron_ore", 400)
        .expect("restore smelter iron ore");
    server
        .world
        .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Electricity, 0)
        .expect("drain factory-owner electricity");

    let gameplay = expect_player_gameplay(&mut server, "owner power readiness snapshot");
    let power_reason =
        smelter_schedule_action(&gameplay, crate::viewer::ACTION_SCHEDULE_SMELTER_IRON_INGOT)
            .disabled_reason
            .as_deref()
            .expect("missing owner power should disable iron ingot scheduling");
    assert!(power_reason.contains("insufficient factory-owner electricity"));
    assert!(power_reason.contains("replenish electricity"));

    submit_iron_ingot_schedule(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        45,
    );
    assert_eq!(
        server
            .world
            .state()
            .factories
            .get("factory.smelter.mk1")
            .and_then(|factory| factory.production.current_blocker_kind.as_deref()),
        Some("power_shortage"),
        "submit must reject the owner power shortage surfaced by readiness"
    );
}

#[test]
fn runtime_gameplay_actions_keep_retired_starter_factory_rebuild_disabled() {
    let _guard = lock_test_llm_env();
    let mut server = setup_industrial_gameplay_with_completed_jobs(73, 6);
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");

    server.world.submit_action(Action::RecycleFactory {
        operator_agent_id: agent_id,
        factory_id: "factory.smelter.mk1".to_string(),
    });
    server.world.step().expect("settle starter smelter recycle");
    assert!(
        server
            .world
            .state()
            .retired_factory_ids
            .contains("factory.smelter.mk1")
    );

    let gameplay = expect_player_gameplay(&mut server, "retired factory rebuild readiness");
    let action = gameplay
        .available_actions
        .iter()
        .find(|action| action.action_id == "build_factory_smelter_mk1")
        .expect("retired smelter rebuild action");
    let disabled_reason = action
        .disabled_reason
        .as_deref()
        .expect("retired factory identity must disable rebuild");
    assert!(
        disabled_reason.contains("retired"),
        "reason={disabled_reason}"
    );
    assert!(
        disabled_reason.contains("factory.smelter.mk1"),
        "reason={disabled_reason}"
    );
}

#[test]
fn runtime_gameplay_allows_recipe_when_current_site_authority_revision_changes() {
    let _guard = lock_test_llm_env();
    let (mut server, agent_id, public_key, private_key) =
        setup_runtime_industrial_gameplay_session(74);
    build_first_smelter_via_gameplay_action(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        74,
    );
    server
        .world
        .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Electricity, i64::MAX)
        .expect("fund factory owner electricity");

    let factory = server
        .world
        .state()
        .factories
        .get("factory.smelter.mk1")
        .expect("smelter factory");
    let site_id = factory.site_id.clone();
    let construction_revision = factory
        .site_authority_revision
        .expect("construction site authority revision");
    let mut site_authority = server
        .world
        .state()
        .factory_site_authorities
        .get(site_id.as_str())
        .expect("smelter site authority")
        .clone();
    site_authority.authority_revision = site_authority.authority_revision.saturating_add(1);
    assert_ne!(site_authority.authority_revision, construction_revision);
    server
        .world
        .set_factory_site_authority(site_authority)
        .expect("advance current site authority revision");
    assert_eq!(
        server
            .world
            .state()
            .factories
            .get("factory.smelter.mk1")
            .and_then(|factory| factory.site_authority_revision),
        Some(construction_revision),
        "construction-time revision should remain historical"
    );

    let gameplay = expect_player_gameplay(&mut server, "current site revision recipe readiness");
    let action =
        smelter_schedule_action(&gameplay, crate::viewer::ACTION_SCHEDULE_SMELTER_IRON_INGOT);
    assert_eq!(
        action.disabled_reason, None,
        "current authority should govern recipe readiness even when its revision advanced"
    );
}

#[test]
fn runtime_gameplay_keeps_recipe_schedule_enabled_when_site_is_deactivated() {
    let _guard = lock_test_llm_env();
    let (mut server, _agent_id) = ready_smelter_for_schedule_site_parity(75);
    let mut site_authority = server
        .world
        .state()
        .factory_site_authorities
        .get("site-smelter")
        .expect("smelter site authority")
        .clone();
    site_authority.authority_revision = site_authority.authority_revision.saturating_add(1);
    site_authority.active = false;
    server
        .world
        .set_factory_site_authority(site_authority)
        .expect("deactivate smelter site authority");

    assert_smelter_schedule_enabled(&mut server, "recipe schedule after site deactivation");
}

#[test]
fn runtime_gameplay_keeps_recipe_schedule_enabled_when_site_access_is_revoked() {
    let _guard = lock_test_llm_env();
    let (mut server, _agent_id) = ready_smelter_for_schedule_site_parity(76);
    let mut site_authority = server
        .world
        .state()
        .factory_site_authorities
        .get("site-smelter")
        .expect("smelter site authority")
        .clone();
    site_authority.authority_revision = site_authority.authority_revision.saturating_add(1);
    site_authority.owner_agent_id = "revoked-site-owner".to_string();
    site_authority.authorized_agent_ids.clear();
    server
        .world
        .set_factory_site_authority(site_authority)
        .expect("revoke smelter site access");

    assert_smelter_schedule_enabled(&mut server, "recipe schedule after site access revocation");
}

#[test]
fn runtime_gameplay_keeps_recipe_schedule_enabled_when_site_moves() {
    let _guard = lock_test_llm_env();
    let (mut server, _agent_id) = ready_smelter_for_schedule_site_parity(77);
    let moved_location_id = "location-moved-for-recipe-schedule";
    server
        .world
        .set_location_anchor(crate::runtime::LocationAnchorV1 {
            location_id: moved_location_id.to_string(),
            active: true,
            authority_revision: 1,
            effective_at: server.world.state().time,
        })
        .expect("register moved site location anchor");
    let mut site_authority = server
        .world
        .state()
        .factory_site_authorities
        .get("site-smelter")
        .expect("smelter site authority")
        .clone();
    site_authority.authority_revision = site_authority.authority_revision.saturating_add(1);
    site_authority.location_id = moved_location_id.to_string();
    server
        .world
        .set_factory_site_authority(site_authority)
        .expect("move smelter site authority");

    assert_smelter_schedule_enabled(&mut server, "recipe schedule after site move");
}

#[test]
fn runtime_gameplay_smelter_readiness_does_not_fallback_to_global_material_ledger() {
    let _guard = lock_test_llm_env();
    let (mut server, agent_id, public_key, private_key) =
        setup_runtime_industrial_gameplay_session(46);
    build_first_smelter_via_gameplay_action(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        46,
    );
    server
        .world
        .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Electricity, i64::MAX)
        .expect("fund agent electricity");
    server
        .world
        .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Data, i64::MAX)
        .expect("fund agent data");

    let factory = server
        .world
        .state()
        .factories
        .get("factory.smelter.mk1")
        .expect("smelter factory");
    let input_ledger = factory.input_ledger.clone();
    let site_id = factory.site_id.clone();
    for kind in ["iron_ore", "carbon_fuel"] {
        server
            .world
            .set_ledger_material_balance(input_ledger.clone(), kind, 0)
            .expect("drain factory input ledger");
        server
            .world
            .set_material_balance(kind, 200)
            .expect("seed global fallback material");
    }

    let gameplay = expect_player_gameplay(&mut server, "site material readiness snapshot");
    let action =
        smelter_schedule_action(&gameplay, crate::viewer::ACTION_SCHEDULE_SMELTER_IRON_INGOT);
    let disabled_reason = action
        .disabled_reason
        .as_deref()
        .expect("missing site inputs must disable scheduling even when world has materials");
    assert!(disabled_reason.contains("insufficient iron_ore"));
    assert!(disabled_reason.contains(format!("site:{site_id}").as_str()));
    assert!(disabled_reason.contains("replenish iron_ore"));
}

#[test]
fn runtime_gameplay_action_unlocks_first_expansion_tradeoff_after_scale_out() {
    let _guard = lock_test_llm_env();
    let mut server = setup_industrial_gameplay_with_completed_jobs(41, 4);
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .expect("scale-out agent")
        .clone();
    server
        .world
        .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Electricity, i64::MAX)
        .expect("fund scale-out agent electricity");
    server
        .world
        .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Data, i64::MAX)
        .expect("fund scale-out agent data");
    let smelter_site_ledger = crate::runtime::MaterialLedgerId::site(
        server
            .world
            .state()
            .factories
            .get(crate::viewer::FACTORY_SMELTER_MK1)
            .expect("scale-out smelter")
            .site_id
            .as_str(),
    );
    server
        .world
        .set_ledger_material_balance(smelter_site_ledger.clone(), "iron_ingot", 200)
        .expect("seed scale-out alloy iron ingot");
    server
        .world
        .set_ledger_material_balance(smelter_site_ledger, "copper_wire", 200)
        .expect("seed scale-out alloy copper wire");
    let builder_ledger = crate::runtime::MaterialLedgerId::agent(agent_id.as_str());
    for (kind, amount) in [
        ("structural_frame", 8),
        ("iron_ingot", 10),
        ("copper_wire", 8),
    ] {
        server
            .world
            .set_ledger_material_balance(builder_ledger.clone(), kind, amount)
            .expect("seed scale-out assembler construction material");
    }
    server
        .world
        .set_resource_balance(ResourceKind::Electricity, 2_000);
    let gameplay = expect_player_gameplay(&mut server, "player gameplay after scale-out");
    assert_eq!(
        gameplay.goal_id,
        "post_onboarding.choose_first_expansion_tradeoff"
    );
    assert_eq!(
        gameplay.goal_kind,
        PlayerGameplayGoalKind::ChooseFirstExpansionTradeoff
    );
    assert_eq!(
        gameplay.stage_status,
        PlayerGameplayStageStatus::BranchReady
    );
    assert_eq!(gameplay.progress_percent, 92);
    assert!(
        gameplay
            .branch_hint
            .as_deref()
            .is_some_and(|hint| hint.contains("throughput expansion"))
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
    assert!(
        gameplay
            .available_actions
            .iter()
            .any(|action| action.action_id == "build_factory_assembler_mk1")
    );
    assert!((1..=3).contains(&gameplay.branch_recommendations.len()));
    assert_eq!(
        gameplay
            .branch_recommendations
            .iter()
            .map(|recommendation| recommendation.action_id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "schedule_recipe_smelter_alloy_plate",
            "build_factory_assembler_mk1",
        ]
    );
    for recommendation in &gameplay.branch_recommendations {
        assert!(!recommendation.route_label.trim().is_empty());
        assert!(!recommendation.immediate_gain.trim().is_empty());
        assert!(!recommendation.future_beat_changed.trim().is_empty());
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
        assert!(!recommendation.risk_or_lockin.trim().is_empty());
        assert!(!recommendation.next_session_hook.trim().is_empty());
        assert!(
            gameplay
                .available_actions
                .iter()
                .any(|action| action.action_id == recommendation.action_id
                    && action.disabled_reason.is_none())
        );
    }
    assert_eq!(
        gameplay.small_player_lane_id.as_deref(),
        Some("local_operator")
    );
    assert_eq!(
        gameplay.leverage_class.as_deref(),
        Some("regional_specialization_option")
    );
    assert_eq!(gameplay.same_loop_repeat_count, 4);
    assert!(!gameplay.grind_only_flag);
    assert_eq!(
        gameplay.major_power_dependency_status.as_deref(),
        Some("independent_path_available")
    );
    assert_eq!(
        gameplay.recovery_path_kind.as_deref(),
        Some("repair_rebuild_or_pivot")
    );
    assert!(
        gameplay
            .recovery_path_detail
            .as_deref()
            .is_some_and(|detail| detail.contains("repair"))
    );
}

#[test]
fn runtime_gameplay_actions_allow_assembler_build_from_authoritative_builder_ledger() {
    let _guard = lock_test_llm_env();
    let (mut server, agent_id, public_key, private_key) =
        setup_runtime_industrial_gameplay_session(35);
    build_first_smelter_via_gameplay_action(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        35,
    );
    settle_one_smelter_iron_ingot_job(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        36,
    );
    let agent_ledger = crate::runtime::MaterialLedgerId::agent(agent_id.as_str());
    server
        .world
        .set_ledger_material_balance(agent_ledger.clone(), "iron_ingot", 10)
        .expect("seed agent iron ingot");
    server
        .world
        .set_ledger_material_balance(agent_ledger.clone(), "copper_wire", 8)
        .expect("seed agent copper wire");
    server
        .world
        .set_ledger_material_balance(agent_ledger, "structural_frame", 8)
        .expect("seed agent structural frame");

    let gameplay = expect_player_gameplay(
        &mut server,
        "player gameplay after seeding assembler build materials on agent ledger",
    );
    let assembler_action = gameplay
        .available_actions
        .iter()
        .find(|action| action.action_id == "build_factory_assembler_mk1")
        .expect("assembler build action");
    assert_eq!(assembler_action.disabled_reason, None);
}

#[test]
fn runtime_gameplay_actions_allow_build_when_profile_kind_differs_from_factory_id() {
    let _guard = lock_test_llm_env();
    let (mut server, agent_id, public_key, private_key) =
        setup_runtime_industrial_gameplay_session(42);
    build_first_smelter_via_gameplay_action(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        42,
    );
    settle_one_smelter_iron_ingot_job(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        43,
    );

    let mut profile = server
        .world
        .state()
        .factory_construction_power_profiles
        .get(crate::viewer::FACTORY_ASSEMBLER_MK1)
        .cloned()
        .expect("assembler construction power profile");
    profile.factory_kind = "assembler_mk1_profile_kind".to_string();
    profile.authority_revision = profile.authority_revision.saturating_add(1);
    server
        .world
        .set_factory_construction_power_profile(profile)
        .expect("update assembler construction power profile kind");

    let agent_ledger = crate::runtime::MaterialLedgerId::agent(agent_id.as_str());
    for (kind, amount) in [
        ("structural_frame", 8),
        ("iron_ingot", 10),
        ("copper_wire", 8),
    ] {
        server
            .world
            .set_ledger_material_balance(agent_ledger.clone(), kind, amount)
            .expect("seed assembler build material");
    }

    let gameplay = expect_player_gameplay(
        &mut server,
        "player gameplay with valid exact-id profile and distinct profile kind",
    );
    let assembler_action = gameplay
        .available_actions
        .iter()
        .find(|action| action.action_id == "build_factory_assembler_mk1")
        .expect("assembler build action");
    assert_eq!(
        assembler_action.disabled_reason, None,
        "runtime-valid profile kind metadata must not disable exact factory-id build"
    );
}

#[test]
fn runtime_gameplay_actions_do_not_enable_assembler_build_from_world_ledger() {
    let _guard = lock_test_llm_env();
    let (mut server, agent_id, public_key, private_key) =
        setup_runtime_industrial_gameplay_session(37);
    build_first_smelter_via_gameplay_action(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        37,
    );
    settle_one_smelter_iron_ingot_job(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        38,
    );
    let builder_ledger = crate::runtime::MaterialLedgerId::agent(agent_id.as_str());
    let world_ledger = crate::runtime::MaterialLedgerId::world();
    for (kind, amount) in [
        ("structural_frame", 8),
        ("iron_ingot", 10),
        ("copper_wire", 8),
    ] {
        server
            .world
            .set_ledger_material_balance(builder_ledger.clone(), kind, 0)
            .expect("drain authoritative builder ledger");
        server
            .world
            .set_ledger_material_balance(world_ledger.clone(), kind, amount)
            .expect("seed world compatibility ledger");
    }

    let gameplay = expect_player_gameplay(
        &mut server,
        "player gameplay with construction materials only on world ledger",
    );
    let assembler_action = gameplay
        .available_actions
        .iter()
        .find(|action| action.action_id == "build_factory_assembler_mk1")
        .expect("assembler build action");
    let disabled_reason = assembler_action
        .disabled_reason
        .as_deref()
        .expect("world ledger must not enable builder construction");
    assert!(disabled_reason.contains("insufficient builder material"));
    assert!(disabled_reason.contains(builder_ledger.to_string().as_str()));
    assert!(disabled_reason.contains("replenish"));
}

#[test]
fn runtime_gameplay_actions_disable_factory_build_when_builder_power_is_insufficient() {
    let _guard = lock_test_llm_env();
    let (mut server, agent_id, _, _) = setup_runtime_industrial_gameplay_session(38);
    server
        .world
        .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Electricity, 0)
        .expect("drain builder construction power");

    let gameplay = expect_player_gameplay(&mut server, "builder power readiness snapshot");
    let action = gameplay
        .available_actions
        .iter()
        .find(|action| action.action_id == "build_factory_smelter_mk1")
        .expect("smelter build action");
    let disabled_reason = action
        .disabled_reason
        .as_deref()
        .expect("missing builder power must disable construction");
    assert!(disabled_reason.contains("insufficient builder electricity"));
    assert!(disabled_reason.contains("replenish electricity"));
}

#[test]
fn runtime_gameplay_actions_disable_factory_build_when_site_authority_is_inactive() {
    let _guard = lock_test_llm_env();
    let (mut server, agent_id, _, _) = setup_runtime_industrial_gameplay_session(39);
    let mut site_authority = server
        .world
        .state()
        .factory_site_authorities
        .get("site-smelter")
        .expect("smelter site authority")
        .clone();
    site_authority.authority_revision = site_authority.authority_revision.saturating_add(1);
    site_authority.active = false;
    server
        .world
        .set_factory_site_authority(site_authority)
        .expect("deactivate smelter site authority");

    let gameplay = expect_player_gameplay(&mut server, "inactive site readiness snapshot");
    let action = gameplay
        .available_actions
        .iter()
        .find(|action| action.action_id == "build_factory_smelter_mk1")
        .expect("smelter build action");
    let disabled_reason = action
        .disabled_reason
        .as_deref()
        .expect("inactive site authority must disable construction");
    assert!(disabled_reason.contains("site authority inactive_or_stale"));
    assert!(disabled_reason.contains("chunk_ready=true"));
    assert!(
        disabled_reason.contains(agent_id.as_str()) || disabled_reason.contains("site-smelter")
    );
}

#[test]
fn runtime_gameplay_actions_keep_assembler_build_disabled_when_cost_is_split_across_ledgers() {
    let _guard = lock_test_llm_env();
    let (mut server, agent_id, public_key, private_key) =
        setup_runtime_industrial_gameplay_session(36);
    build_first_smelter_via_gameplay_action(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        36,
    );
    settle_one_smelter_iron_ingot_job(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        37,
    );
    let agent_ledger = crate::runtime::MaterialLedgerId::agent(agent_id.as_str());
    server
        .world
        .set_ledger_material_balance(agent_ledger.clone(), "structural_frame", 0)
        .expect("keep structural frames outside the authoritative builder ledger");
    server
        .world
        .set_ledger_material_balance(agent_ledger.clone(), "iron_ingot", 10)
        .expect("seed agent iron ingot");
    server
        .world
        .set_ledger_material_balance(agent_ledger, "copper_wire", 8)
        .expect("seed agent copper wire");

    let gameplay = expect_player_gameplay(
        &mut server,
        "player gameplay with split assembler build materials across ledgers",
    );
    let assembler_action = gameplay
        .available_actions
        .iter()
        .find(|action| action.action_id == "build_factory_assembler_mk1")
        .expect("assembler build action");
    let disabled_reason = assembler_action
        .disabled_reason
        .as_deref()
        .expect("split ledger cost should keep assembler action disabled");
    assert!(disabled_reason.contains("insufficient builder material"));
    assert!(disabled_reason.contains("structural_frame"));
    assert!(disabled_reason.contains("replenish"));
}

#[test]
fn runtime_gameplay_actions_do_not_enable_assembler_schedule_from_world_ledger() {
    let _guard = lock_test_llm_env();
    let (mut server, agent_id, public_key, private_key) =
        setup_runtime_industrial_gameplay_session(40);
    build_first_smelter_via_gameplay_action(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        40,
    );
    settle_one_smelter_iron_ingot_job(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        41,
    );
    build_first_assembler_via_gameplay_action(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        50,
    );
    let assembler_site = server
        .world
        .state()
        .factories
        .get("factory.assembler.mk1")
        .expect("assembler factory state")
        .site_id
        .clone();
    let site_ledger = crate::runtime::MaterialLedgerId::site(assembler_site);
    let world_ledger = crate::runtime::MaterialLedgerId::world();
    server
        .world
        .set_ledger_material_balance(site_ledger.clone(), "iron_ingot", 0)
        .expect("drain authoritative assembler site ledger");
    server
        .world
        .set_ledger_material_balance(world_ledger, "iron_ingot", 8)
        .expect("seed world compatibility ledger");

    let gameplay = expect_player_gameplay(
        &mut server,
        "player gameplay with assembler inputs only on world ledger",
    );
    let gear_action = gameplay
        .available_actions
        .iter()
        .find(|action| action.action_id == "schedule_recipe_assembler_gear")
        .expect("assembler gear schedule action");
    let disabled_reason = gear_action
        .disabled_reason
        .as_deref()
        .expect("world ledger must not enable assembler scheduling");
    assert!(disabled_reason.contains("insufficient iron_ingot"));
    assert!(disabled_reason.contains(site_ledger.to_string().as_str()));
    assert!(disabled_reason.contains("replenish"));
}
