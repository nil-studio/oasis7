use super::pos;
use crate::{Action, World};
use oasis7_agent_api::world_action::ResourceKind;

fn register_agents(world: &mut World, agent_ids: &[&str]) {
    for (index, agent_id) in agent_ids.iter().enumerate() {
        world.submit_action(Action::RegisterAgent {
            agent_id: (*agent_id).to_string(),
            pos: pos(index as i64, 0),
        });
    }
    world.step().expect("register agents");
}

fn form_war_alliances(world: &mut World) {
    world.submit_action(Action::FormAlliance {
        proposer_agent_id: "a".to_string(),
        alliance_id: "alliance.red".to_string(),
        members: vec!["b".to_string()],
        charter: "charter.red".to_string(),
    });
    world.step().expect("form aggressor alliance");
    world.submit_action(Action::FormAlliance {
        proposer_agent_id: "c".to_string(),
        alliance_id: "alliance.blue".to_string(),
        members: vec!["d".to_string(), "e".to_string()],
        charter: "charter.blue".to_string(),
    });
    world.step().expect("form defender alliance");
}

fn seed_declaration_resources(world: &mut World) {
    world
        .set_agent_resource_balance("a", ResourceKind::Electricity, 120)
        .expect("seed aggressor electricity");
    world
        .set_agent_resource_balance("a", ResourceKind::Data, 120)
        .expect("seed aggressor data");
}

#[test]
fn war_declaration_quote_is_deterministic_non_mutating_and_exposes_projected_core_outcome() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b", "c", "d", "e"]);
    form_war_alliances(&mut world);

    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let quote = world
        .war_declaration_quote("a", "alliance.red", "alliance.blue", 3)
        .expect("declaring alliance can inspect a war quote before submission");
    let repeated_quote = world
        .war_declaration_quote("a", "alliance.red", "alliance.blue", 3)
        .expect("war quote remains available without submitting an action");

    assert_eq!(quote, repeated_quote);
    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(quote.actor_alliance_id, "alliance.red");
    assert_eq!(quote.target_alliance_id, "alliance.blue");
    assert_eq!(quote.action_kind, "declare_war");
    assert_eq!(quote.intensity, 3);
    assert_eq!(quote.settlement_path, "core_fallback");
    assert_eq!(quote.war_duration_ticks, 12);
    assert_eq!(quote.aggressor_score_estimate, 23);
    assert_eq!(quote.defender_score_estimate, 30);
    assert_eq!(quote.likely_winner_before_action, "alliance.blue");
    assert_eq!(quote.victory_margin_estimate, -7);
    assert!(quote.conflict_window_blocked_until > world.state().time);
    assert!(quote.settlement_risk.contains("loss"));
    assert!(!quote.expected_narrative_or_module_reward.is_empty());
}

#[test]
fn war_declaration_quote_reports_minimum_winning_intensity_from_the_active_settlement_path() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b", "c", "d", "e"]);
    form_war_alliances(&mut world);

    let quote = world
        .war_declaration_quote("a", "alliance.red", "alliance.blue", 3)
        .expect("war quote for a reachable threshold");

    // Core score: red members 2 * 10 + intensity, blue members 3 * 10. A tie wins.
    assert_eq!(quote.minimum_winning_intensity, Some(10));
    assert!(!quote.mobilization_affordable);
    assert_eq!(quote.recommended_war_action, "gather_resources");
    assert_eq!(quote.alternative_action, "gather_resources");
    assert!(
        quote
            .why_this_war_is_worth_or_risky
            .contains("mobilization")
    );
}

#[test]
fn war_declaration_quote_blocks_a_recommended_declaration_until_mobilization_is_affordable() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b", "c", "d", "e"]);
    form_war_alliances(&mut world);

    let quote = world
        .war_declaration_quote("a", "alliance.red", "alliance.blue", 10)
        .expect("quote exposes the affordability blocker");

    assert_eq!(quote.mobilization_electricity_required, 52);
    assert_eq!(quote.mobilization_electricity_current, 0);
    assert_eq!(quote.mobilization_electricity_after, -52);
    assert_eq!(quote.mobilization_data_required, 38);
    assert_eq!(quote.mobilization_data_current, 0);
    assert_eq!(quote.mobilization_data_after, -38);
    assert!(!quote.mobilization_affordable);
    assert_eq!(quote.recommended_war_action, "gather_resources");
    assert!(
        quote
            .why_this_war_is_worth_or_risky
            .contains("mobilization")
    );
}

#[test]
fn war_declaration_quote_allows_an_exactly_affordable_winning_declaration() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b", "c", "d", "e"]);
    form_war_alliances(&mut world);
    world
        .set_agent_resource_balance("a", ResourceKind::Electricity, 52)
        .expect("seed exact electricity cost");
    world
        .set_agent_resource_balance("a", ResourceKind::Data, 38)
        .expect("seed exact data cost");

    let quote = world
        .war_declaration_quote("a", "alliance.red", "alliance.blue", 10)
        .expect("quote accepts exact affordability");

    assert!(quote.mobilization_affordable);
    assert_eq!(quote.mobilization_electricity_after, 0);
    assert_eq!(quote.mobilization_data_after, 0);
    assert_eq!(quote.recommended_war_action, "declare_war");
}

#[test]
fn war_declaration_quote_fails_closed_when_the_m5_war_reducer_is_active() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b", "c", "d", "e"]);
    form_war_alliances(&mut world);
    // This quote guard depends on a genuinely activated registry entry, not
    // execution of the canonical M5 bytecode. Register a local module through
    // the existing proposal lifecycle so this unit test cannot recursively
    // build/download the full bootstrap while its parent Cargo holds locks.
    // Canonical bootstrap and execution remain covered by the full-tier tests.
    use crate::{
        GameplayContract, GameplayModuleKind, M5_GAMEPLAY_MODULE_VERSION,
        M5_GAMEPLAY_WAR_MODULE_ID, ModuleAbiContract, ModuleKind, ModuleLimits, ModuleManifest,
        ModuleRole,
    };
    let bytes =
        hex::decode("0061736d0100000001040160000003020100070a010672656475636500000a040102000b")
            .expect("local valid module exporting reduce");
    let hash = super::super::util::sha256_hex(&bytes);
    world
        .register_module_artifact(hash.clone(), &bytes)
        .expect("register local module");
    super::modules::activate_module_manifest(
        &mut world,
        ModuleManifest {
            module_id: M5_GAMEPLAY_WAR_MODULE_ID.into(),
            name: "Local war quote guard fixture".into(),
            version: M5_GAMEPLAY_MODULE_VERSION.into(),
            kind: ModuleKind::Reducer,
            role: ModuleRole::Gameplay,
            wasm_hash: hash.clone(),
            interface_version: "wasm-1".into(),
            artifact_identity: Some(super::signed_test_artifact_identity(&hash)),
            abi_contract: ModuleAbiContract {
                abi_version: Some(1),
                input_schema: Some("gameplay.input@1".into()),
                output_schema: Some("gameplay.output@1".into()),
                gameplay: Some(GameplayContract {
                    kind: GameplayModuleKind::War,
                    game_modes: vec!["sandbox".into()],
                    min_players: 2,
                    max_players: None,
                }),
                ..Default::default()
            },
            exports: vec!["reduce".into()],
            subscriptions: vec![],
            required_caps: vec![],
            limits: ModuleLimits {
                max_mem_bytes: 64 * 1024,
                max_gas: 10_000,
                max_call_rate: 1,
                max_output_bytes: 1024,
                max_effects: 0,
                max_emits: 0,
            },
        },
    );
    assert!(
        world
            .module_registry()
            .active
            .contains_key(M5_GAMEPLAY_WAR_MODULE_ID)
    );

    assert_eq!(
        world.war_declaration_quote("a", "alliance.red", "alliance.blue", 3),
        Err("war_declaration_quote_missing")
    );
}

#[test]
fn war_declaration_quote_waits_for_a_related_queued_declaration() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b", "c", "d", "e"]);
    form_war_alliances(&mut world);
    seed_declaration_resources(&mut world);
    world.submit_action(Action::DeclareWar {
        initiator_agent_id: "a".to_string(),
        war_id: "war.quote.pending".to_string(),
        aggressor_alliance_id: "alliance.red".to_string(),
        defender_alliance_id: "alliance.blue".to_string(),
        objective: "hold belt".to_string(),
        intensity: 3,
    });

    let quote = world
        .war_declaration_quote("a", "alliance.red", "alliance.blue", 3)
        .expect("queued action remains an advisory blocker");

    assert_eq!(quote.conflict_status, "pending_conflict");
    assert_eq!(quote.recommended_war_action, "wait");
    assert_eq!(quote.conflict_window_blocked_until, world.state().time);
    assert!(
        quote
            .why_this_war_is_worth_or_risky
            .contains("queued DeclareWar")
    );
}

#[test]
fn war_declaration_quote_does_not_promise_a_retry_tick_for_an_invalid_queued_declaration() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b", "c", "d", "e"]);
    form_war_alliances(&mut world);
    world.submit_action(Action::DeclareWar {
        initiator_agent_id: "a".to_string(),
        war_id: "war.quote.pending.invalid".to_string(),
        aggressor_alliance_id: "alliance.red".to_string(),
        defender_alliance_id: "alliance.blue".to_string(),
        objective: String::new(),
        intensity: 1,
    });

    let quote = world
        .war_declaration_quote("a", "alliance.red", "alliance.blue", 10)
        .expect("queued declaration is an advisory blocker before admission");

    assert_eq!(quote.conflict_status, "pending_conflict");
    assert_eq!(quote.conflict_window_blocked_until, world.state().time);
}

#[test]
fn war_declaration_quote_does_not_use_a_queued_declaration_intensity_as_a_retry_promise() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b", "c", "d", "e"]);
    form_war_alliances(&mut world);
    seed_declaration_resources(&mut world);
    world.submit_action(Action::DeclareWar {
        initiator_agent_id: "a".to_string(),
        war_id: "war.quote.pending.low-intensity".to_string(),
        aggressor_alliance_id: "alliance.red".to_string(),
        defender_alliance_id: "alliance.blue".to_string(),
        objective: "hold belt".to_string(),
        intensity: 1,
    });

    let quote = world
        .war_declaration_quote("a", "alliance.red", "alliance.blue", 10)
        .expect("queued declaration is an advisory blocker before admission");

    assert_eq!(quote.conflict_status, "pending_conflict");
    assert_eq!(quote.conflict_window_blocked_until, world.state().time);
}

#[test]
fn war_declaration_quote_keeps_an_active_conflict_blocker_and_wait_alternative_visible() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b", "c", "d", "e"]);
    form_war_alliances(&mut world);
    seed_declaration_resources(&mut world);
    world.submit_action(Action::DeclareWar {
        initiator_agent_id: "a".to_string(),
        war_id: "war.quote.active".to_string(),
        aggressor_alliance_id: "alliance.red".to_string(),
        defender_alliance_id: "alliance.blue".to_string(),
        objective: "hold belt".to_string(),
        intensity: 3,
    });
    world.step().expect("declare active war");

    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let quote = world
        .war_declaration_quote("a", "alliance.red", "alliance.blue", 3)
        .expect("active-conflict quote must explain why another declaration is blocked");

    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
    assert!(
        quote
            .reentry_cooldown_or_active_conflict_blocker
            .contains("active war")
    );
    assert_eq!(quote.alternative_action, "wait");
    assert_eq!(quote.recommended_war_action, "wait");
    let active_war = world
        .state()
        .wars
        .get("war.quote.active")
        .expect("declared war is active");
    assert_eq!(
        quote.conflict_window_blocked_until,
        active_war
            .declared_at
            .saturating_add(active_war.max_duration_ticks.max(1))
    );
    assert!(quote.why_this_war_is_worth_or_risky.contains("active"));
}
