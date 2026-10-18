use super::*;

fn signed_schedule_recipe_quote_request(
    factory_id: &str,
    recipe_id: &str,
    batches: i64,
    player_id: &str,
    nonce: u64,
    public_key_hex: &str,
    private_key_hex: &str,
) -> crate::viewer::ScheduleRecipeQuoteRequest {
    let mut request = crate::viewer::ScheduleRecipeQuoteRequest {
        factory_id: factory_id.to_string(),
        recipe_id: recipe_id.to_string(),
        batches,
        player_id: player_id.to_string(),
        public_key: Some(public_key_hex.to_string()),
        auth: None,
    };
    request.auth = Some(
        crate::viewer::sign_schedule_recipe_quote_auth_proof(
            &request,
            nonce,
            public_key_hex,
            private_key_hex,
        )
        .expect("sign schedule recipe quote auth"),
    );
    request
}

#[test]
fn runtime_schedule_recipe_quote_is_authenticated_read_only_and_exposes_player_preflight() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Script),
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
    let (public_key, private_key) = test_signer(254);
    register_runtime_session_with_options(
        &mut server,
        "local-test-player-schedule-quote",
        Some(agent_id.as_str()),
        true,
        269,
        public_key.as_str(),
        private_key.as_str(),
    );
    server
        .seed_smelter_affordability_debug_scenario()
        .expect("seed smelter factory");
    server
        .world
        .set_agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            32,
        )
        .expect("seed electricity");
    server
        .world
        .set_agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Data,
            16,
        )
        .expect("seed data");
    server.world.set_resource_balance(
        oasis7_agent_api::world_action::ResourceKind::Electricity,
        32,
    );
    let site_ledger = crate::runtime::MaterialLedgerId::site("site-smelter");
    server
        .world
        .set_ledger_material_balance(site_ledger.clone(), "iron_ore", 8)
        .expect("seed local iron ore");
    server
        .world
        .set_ledger_material_balance(site_ledger, "carbon_fuel", 2)
        .expect("seed local carbon fuel");
    server
        .world
        .set_material_balance("iron_ore", 20)
        .expect("seed world iron ore fallback");
    server
        .world
        .set_material_balance("carbon_fuel", 20)
        .expect("seed world carbon fuel fallback");
    server
        .world
        .upsert_recipe_profile(crate::runtime::RecipeProfileV1 {
            recipe_id: "recipe.smelter.iron_ingot".to_string(),
            bottleneck_tags: vec!["iron_ore".to_string()],
            stage_gate: "bootstrap".to_string(),
            preferred_factory_tags: vec!["smelter".to_string()],
        })
        .expect("seed recipe scarcity profile");
    let state_before = server.world.state().clone();

    let request = signed_schedule_recipe_quote_request(
        crate::viewer::FACTORY_SMELTER_MK1,
        "recipe.smelter.iron_ingot",
        2,
        "local-test-player-schedule-quote",
        270,
        public_key.as_str(),
        private_key.as_str(),
    );
    let quote = server
        .handle_schedule_recipe_quote(request.clone())
        .expect("authenticated schedule recipe quote");

    assert_eq!(quote.owner_agent_id, agent_id);
    assert_eq!(quote.factory_id, crate::viewer::FACTORY_SMELTER_MK1);
    assert_eq!(quote.recipe_id, "recipe.smelter.iron_ingot");
    assert_eq!(quote.batches, 2);
    assert_eq!(quote.electricity_cost, 16);
    assert_eq!(quote.electricity_after, 16);
    assert_eq!(quote.hardware_cost, 0);
    assert_eq!(quote.data_output, 0);
    assert_eq!(quote.finished_product_id, "iron_ingot");
    assert_eq!(quote.finished_product_units, 6);
    assert_eq!(quote.local_shortage_delay_ticks, 0);
    assert_eq!(quote.shortage_reason, "none");
    assert_eq!(quote.continue_production_risk, "low");
    assert_eq!(quote.recommended_pre_step, "schedule_now");
    assert_eq!(quote.recommended_maintenance_action, "continue_production");
    assert_eq!(
        server
            .handle_schedule_recipe_quote(request)
            .expect("repeat schedule recipe quote"),
        quote
    );
    assert_eq!(server.world.state(), &state_before);
}

#[test]
fn runtime_schedule_recipe_quote_matches_recipe_started_duration_for_same_snapshot() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Script),
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
    let (public_key, private_key) = test_signer(255);
    register_runtime_session_with_options(
        &mut server,
        "local-test-player-schedule-quote-parity",
        Some(agent_id.as_str()),
        true,
        270,
        public_key.as_str(),
        private_key.as_str(),
    );
    server
        .seed_smelter_affordability_debug_scenario()
        .expect("seed smelter factory");
    server
        .world
        .set_agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            32,
        )
        .expect("seed electricity");
    server
        .world
        .set_agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Data,
            16,
        )
        .expect("seed data");
    server.world.set_resource_balance(
        oasis7_agent_api::world_action::ResourceKind::Electricity,
        32,
    );
    let site_ledger = crate::runtime::MaterialLedgerId::site("site-smelter");
    server
        .world
        .set_ledger_material_balance(site_ledger.clone(), "iron_ore", 8)
        .expect("seed local iron ore");
    server
        .world
        .set_ledger_material_balance(site_ledger, "carbon_fuel", 2)
        .expect("seed local carbon fuel");
    server
        .world
        .set_material_balance("iron_ore", 20)
        .expect("seed world iron ore fallback");
    server
        .world
        .set_material_balance("carbon_fuel", 20)
        .expect("seed world carbon fuel fallback");
    server
        .world
        .upsert_recipe_profile(crate::runtime::RecipeProfileV1 {
            recipe_id: "recipe.smelter.iron_ingot".to_string(),
            bottleneck_tags: vec!["iron_ore".to_string()],
            stage_gate: "bootstrap".to_string(),
            preferred_factory_tags: vec!["smelter".to_string()],
        })
        .expect("seed recipe scarcity profile");

    let request = signed_schedule_recipe_quote_request(
        crate::viewer::FACTORY_SMELTER_MK1,
        "recipe.smelter.iron_ingot",
        2,
        "local-test-player-schedule-quote-parity",
        271,
        public_key.as_str(),
        private_key.as_str(),
    );
    let quote = server
        .handle_schedule_recipe_quote(request)
        .expect("authenticated schedule recipe quote");
    assert_eq!(quote.electricity_after, 16);
    let runtime_action = crate::viewer::gameplay_actions::runtime_schedule_recipe_action(
        agent_id.as_str(),
        crate::viewer::FACTORY_SMELTER_MK1,
        "recipe.smelter.iron_ingot",
        2,
    )
    .expect("canonical runtime recipe plan");
    server.world.submit_action(runtime_action);
    server.world.step().expect("start scheduled recipe");

    let started = server
        .world
        .journal()
        .events
        .last()
        .expect("recipe started");
    match &started.body {
        crate::runtime::WorldEventBody::Domain(crate::runtime::DomainEvent::RecipeStarted {
            power_required,
            power_owner_agent_id,
            duration_ticks,
            consume,
            produce,
            ready_at,
            ..
        }) => {
            assert_eq!((*power_required), quote.electricity_cost);
            assert_eq!(power_owner_agent_id.as_deref(), Some(agent_id.as_str()));
            assert_eq!(
                consume
                    .iter()
                    .find(|stack| stack.kind == "iron_ore")
                    .map(|stack| stack.amount),
                Some(8)
            );
            assert_eq!(
                produce
                    .iter()
                    .find(|stack| stack.kind == quote.finished_product_id)
                    .map(|stack| stack.amount),
                Some(quote.finished_product_units)
            );
            assert_eq!(
                i64::from(*duration_ticks),
                quote.base_duration_ticks + quote.local_shortage_delay_ticks,
                "quote total duration must equal emitted RecipeStarted duration"
            );
            assert_eq!(
                *ready_at,
                server
                    .world
                    .state()
                    .time
                    .saturating_add(u64::from(*duration_ticks))
            );
        }
        other => panic!("expected RecipeStarted, got {other:?}"),
    }
    assert_eq!(
        server
            .world
            .agent_resource_balance(
                agent_id.as_str(),
                oasis7_agent_api::world_action::ResourceKind::Electricity
            )
            .expect("builder electricity after recipe start"),
        quote.electricity_after
    );
    assert_eq!(
        server
            .world
            .resource_balance(oasis7_agent_api::world_action::ResourceKind::Electricity),
        32,
        "the world electricity pool is not the recipe payer"
    );
}

#[test]
fn runtime_schedule_recipe_quote_uses_factory_site_materials_not_global_stock() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Script),
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
    let (public_key, private_key) = test_signer(249);
    register_runtime_session_with_options(
        &mut server,
        "local-test-player-schedule-quote-global-ledger",
        Some(agent_id.as_str()),
        true,
        273,
        public_key.as_str(),
        private_key.as_str(),
    );
    server
        .seed_smelter_affordability_debug_scenario()
        .expect("seed smelter factory");
    server
        .world
        .set_agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            32,
        )
        .expect("seed builder electricity");
    server
        .world
        .set_agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Data,
            0,
        )
        .expect("seed low local data");
    server
        .world
        .set_resource_balance(oasis7_agent_api::world_action::ResourceKind::Electricity, 0);
    let site_ledger = crate::runtime::MaterialLedgerId::site("site-smelter");
    server
        .world
        .set_ledger_material_balance(site_ledger.clone(), "iron_ore", 8)
        .expect("seed local iron ore");
    server
        .world
        .set_ledger_material_balance(site_ledger, "carbon_fuel", 2)
        .expect("seed local carbon fuel");
    server
        .world
        .set_material_balance("iron_ore", 20)
        .expect("seed world iron ore fallback");
    server
        .world
        .set_material_balance("carbon_fuel", 20)
        .expect("seed world carbon fuel fallback");

    let request = signed_schedule_recipe_quote_request(
        crate::viewer::FACTORY_SMELTER_MK1,
        "recipe.smelter.iron_ingot",
        2,
        "local-test-player-schedule-quote-global-ledger",
        274,
        public_key.as_str(),
        private_key.as_str(),
    );
    let quote = server
        .handle_schedule_recipe_quote(request)
        .expect("factory site materials should authorize quote");
    assert_eq!(quote.electricity_cost, 16);
    assert_eq!(quote.electricity_after, 16);
    assert_eq!(quote.hardware_cost, 0);
    assert_eq!(quote.data_output, 0);

    let runtime_action = crate::viewer::gameplay_actions::runtime_schedule_recipe_action(
        agent_id.as_str(),
        crate::viewer::FACTORY_SMELTER_MK1,
        "recipe.smelter.iron_ingot",
        2,
    )
    .expect("canonical runtime recipe plan");
    server.world.submit_action(runtime_action);
    server.world.step().expect("start scheduled recipe");
    assert!(matches!(
        server
            .world
            .journal()
            .events
            .last()
            .map(|event| &event.body),
        Some(crate::runtime::WorldEventBody::Domain(
            crate::runtime::DomainEvent::RecipeStarted { .. }
        ))
    ));
    assert_eq!(
        server
            .world
            .agent_resource_balance(
                agent_id.as_str(),
                oasis7_agent_api::world_action::ResourceKind::Electricity
            )
            .expect("builder electricity after recipe start"),
        16
    );
    assert_eq!(
        server
            .world
            .resource_balance(oasis7_agent_api::world_action::ResourceKind::Electricity),
        0,
        "global electricity remains irrelevant to the owner-held power debit"
    );
}

#[test]
fn runtime_schedule_recipe_quote_requires_factory_builder_electricity_not_world_pool() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Script),
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
    let (public_key, private_key) = test_signer(180);
    register_runtime_session_with_options(
        &mut server,
        "local-test-player-schedule-quote-owner-power",
        Some(agent_id.as_str()),
        true,
        282,
        public_key.as_str(),
        private_key.as_str(),
    );
    server
        .seed_smelter_affordability_debug_scenario()
        .expect("seed smelter factory");
    server
        .world
        .set_agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            0,
        )
        .expect("drain factory builder electricity");
    server
        .world
        .set_agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Data,
            16,
        )
        .expect("seed builder data");
    server.world.set_resource_balance(
        oasis7_agent_api::world_action::ResourceKind::Electricity,
        32,
    );

    let err = server
        .handle_schedule_recipe_quote(signed_schedule_recipe_quote_request(
            crate::viewer::FACTORY_SMELTER_MK1,
            "recipe.smelter.iron_ingot",
            2,
            "local-test-player-schedule-quote-owner-power",
            283,
            public_key.as_str(),
            private_key.as_str(),
        ))
        .expect_err("world electricity must not fund a quote for an unpowered builder");

    assert_eq!(err.code, "schedule_recipe_quote_rejected");
    assert!(
        err.message.contains("requires 16 electricity, available 0"),
        "owner-held power should be the quote affordability source: {}",
        err.message
    );
    assert_eq!(err.target_agent_id.as_deref(), Some(agent_id.as_str()));
}

#[test]
fn runtime_schedule_recipe_quote_reports_factory_builder_electricity_after_not_world_pool() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Script),
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
    let (public_key, private_key) = test_signer(182);
    register_runtime_session_with_options(
        &mut server,
        "local-test-player-schedule-quote-owner-after",
        Some(agent_id.as_str()),
        true,
        285,
        public_key.as_str(),
        private_key.as_str(),
    );
    server
        .seed_smelter_affordability_debug_scenario()
        .expect("seed smelter factory");
    server
        .world
        .set_agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            32,
        )
        .expect("seed builder electricity");
    server
        .world
        .set_agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Data,
            16,
        )
        .expect("seed builder data");
    server
        .world
        .set_resource_balance(oasis7_agent_api::world_action::ResourceKind::Electricity, 0);

    let quote = server
        .handle_schedule_recipe_quote(signed_schedule_recipe_quote_request(
            crate::viewer::FACTORY_SMELTER_MK1,
            "recipe.smelter.iron_ingot",
            2,
            "local-test-player-schedule-quote-owner-after",
            286,
            public_key.as_str(),
            private_key.as_str(),
        ))
        .expect("builder-held electricity must authorize the quote");

    assert_eq!(quote.owner_agent_id, agent_id);
    assert_eq!(quote.electricity_cost, 16);
    assert_eq!(
        quote.electricity_after, 16,
        "electricity_after must be derived from the factory builder's stock"
    );
}

#[test]
fn runtime_schedule_recipe_quote_rejects_when_factory_builder_electricity_is_insufficient() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Script),
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
    let (public_key, private_key) = test_signer(248);
    register_runtime_session_with_options(
        &mut server,
        "local-test-player-schedule-quote-global-power",
        Some(agent_id.as_str()),
        true,
        275,
        public_key.as_str(),
        private_key.as_str(),
    );
    server
        .seed_smelter_affordability_debug_scenario()
        .expect("seed smelter factory");
    server
        .world
        .set_agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            15,
        )
        .expect("seed insufficient builder electricity");
    server
        .world
        .set_agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Data,
            100,
        )
        .expect("seed high local data");
    server.world.set_resource_balance(
        oasis7_agent_api::world_action::ResourceKind::Electricity,
        100,
    );

    let request = signed_schedule_recipe_quote_request(
        crate::viewer::FACTORY_SMELTER_MK1,
        "recipe.smelter.iron_ingot",
        2,
        "local-test-player-schedule-quote-global-power",
        276,
        public_key.as_str(),
        private_key.as_str(),
    );
    let err = server
        .handle_schedule_recipe_quote(request)
        .expect_err("builder-agent electricity must gate quote acceptance");
    assert_eq!(err.code, "schedule_recipe_quote_rejected");
    assert!(
        err.message
            .contains("requires 16 electricity, available 15")
    );
    assert_eq!(
        server
            .world
            .agent_resource_balance(
                agent_id.as_str(),
                oasis7_agent_api::world_action::ResourceKind::Electricity
            )
            .expect("builder electricity after rejected quote"),
        15
    );
    assert_eq!(
        server
            .world
            .resource_balance(oasis7_agent_api::world_action::ResourceKind::Electricity),
        100,
        "an insufficient builder quote must not inspect or debit the world pool"
    );
    assert!(!server.world.journal().events.iter().any(|event| matches!(
        event.body,
        crate::runtime::WorldEventBody::Domain(crate::runtime::DomainEvent::RecipeStarted { .. })
    )));

    // Once builder electricity is restored, the runtime material ledger remains
    // authoritative as the next independent acceptance gate.  The world pool
    // is intentionally left unchanged because it is not the recipe payer.
    server
        .world
        .set_agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            32,
        )
        .expect("restore builder electricity");
    server
        .world
        .set_material_balance("iron_ore", 0)
        .expect("clear world iron ore");
    server
        .world
        .set_material_balance("carbon_fuel", 0)
        .expect("clear world carbon fuel");
    let site_ledger = crate::runtime::MaterialLedgerId::site("site-smelter");
    server
        .world
        .set_ledger_material_balance(site_ledger.clone(), "iron_ore", 0)
        .expect("clear local iron ore");
    server
        .world
        .set_ledger_material_balance(site_ledger, "carbon_fuel", 0)
        .expect("clear local carbon fuel");
    let material_request = signed_schedule_recipe_quote_request(
        crate::viewer::FACTORY_SMELTER_MK1,
        "recipe.smelter.iron_ingot",
        2,
        "local-test-player-schedule-quote-global-power",
        277,
        public_key.as_str(),
        private_key.as_str(),
    );
    let material_err = server
        .handle_schedule_recipe_quote(material_request)
        .expect_err("global runtime material ledger must gate quote acceptance");
    assert_eq!(material_err.code, "schedule_recipe_quote_rejected");
    assert!(material_err.message.contains("insufficient material"));
    assert_eq!(
        server
            .world
            .agent_resource_balance(
                agent_id.as_str(),
                oasis7_agent_api::world_action::ResourceKind::Electricity
            )
            .expect("builder electricity after material rejection"),
        32
    );
    assert_eq!(
        server
            .world
            .resource_balance(oasis7_agent_api::world_action::ResourceKind::Electricity),
        100
    );
}

#[test]
fn runtime_schedule_recipe_quote_rejects_alias_without_canonical_runtime_plan() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Script),
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
    let (public_key, private_key) = test_signer(252);
    register_runtime_session_with_options(
        &mut server,
        "local-test-player-schedule-quote-alias",
        Some(agent_id.as_str()),
        true,
        271,
        public_key.as_str(),
        private_key.as_str(),
    );
    server
        .seed_smelter_affordability_debug_scenario()
        .expect("seed smelter factory");
    server
        .world
        .set_agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            32,
        )
        .expect("seed electricity");
    server
        .world
        .set_agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Data,
            16,
        )
        .expect("seed data");

    let request = signed_schedule_recipe_quote_request(
        crate::viewer::FACTORY_SMELTER_MK1,
        "recipe.iron_ingot",
        1,
        "local-test-player-schedule-quote-alias",
        272,
        public_key.as_str(),
        private_key.as_str(),
    );
    let err = server
        .handle_schedule_recipe_quote(request)
        .expect_err("alias without a canonical runtime plan must not return zero/none");
    assert_eq!(err.code, "schedule_recipe_quote_rejected");
    assert_eq!(err.action_id.as_deref(), Some("quote_schedule_recipe"));
}

#[test]
fn runtime_schedule_recipe_quote_requires_auth_proof() {
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Script),
    )
    .expect("runtime server");

    let err = server
        .handle_schedule_recipe_quote(crate::viewer::ScheduleRecipeQuoteRequest {
            factory_id: crate::viewer::FACTORY_SMELTER_MK1.to_string(),
            recipe_id: "recipe.smelter.iron_ingot".to_string(),
            batches: 1,
            player_id: "player-schedule-quote".to_string(),
            public_key: None,
            auth: None,
        })
        .expect_err("unsigned schedule recipe quote must fail");

    assert_eq!(err.code, "auth_proof_required");
    assert_eq!(err.action_id.as_deref(), Some("quote_schedule_recipe"));
}

#[test]
fn runtime_schedule_recipe_quote_rejects_tampered_auth_proof() {
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Script),
    )
    .expect("runtime server");
    let (public_key, private_key) = test_signer(251);
    let mut request = signed_schedule_recipe_quote_request(
        crate::viewer::FACTORY_SMELTER_MK1,
        "recipe.smelter.iron_ingot",
        1,
        "player-schedule-quote",
        252,
        public_key.as_str(),
        private_key.as_str(),
    );
    request.auth.as_mut().expect("signed auth").signature = "00".repeat(64);

    let err = server
        .handle_schedule_recipe_quote(request)
        .expect_err("tampered schedule recipe quote auth must fail");

    assert_eq!(err.code, "auth_signature_invalid");
    assert_eq!(err.action_id.as_deref(), Some("quote_schedule_recipe"));
}

#[test]
fn runtime_schedule_recipe_quote_rejects_foreign_factory_ownership() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::TwoBases)
            .with_decision_mode(ViewerLiveDecisionMode::Script),
    )
    .expect("runtime server");
    let agent_ids: Vec<_> = server.world.state().agents.keys().cloned().collect();
    assert!(agent_ids.len() >= 2, "two_bases must seed two agents");
    server
        .seed_smelter_affordability_debug_scenario()
        .expect("seed factory owned by the first agent");
    let foreign_owner = &agent_ids[1];
    let (public_key, private_key) = test_signer(253);
    register_runtime_session(
        &mut server,
        "player-foreign-schedule-quote",
        Some(foreign_owner.as_str()),
        254,
        public_key.as_str(),
        private_key.as_str(),
    );

    let err = server
        .handle_schedule_recipe_quote(signed_schedule_recipe_quote_request(
            crate::viewer::FACTORY_SMELTER_MK1,
            "recipe.smelter.iron_ingot",
            1,
            "player-foreign-schedule-quote",
            255,
            public_key.as_str(),
            private_key.as_str(),
        ))
        .expect_err("foreign player must not quote another agent's factory");

    assert_eq!(err.code, "schedule_recipe_quote_rejected");
    assert!(err.message.contains("factory owner mismatch"));
    assert_eq!(err.action_id.as_deref(), Some("quote_schedule_recipe"));
    assert_eq!(err.target_agent_id.as_deref(), Some(foreign_owner.as_str()));
}
