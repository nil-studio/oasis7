use super::*;

fn signed_collect_data_command(
    mode: crate::viewer::CollectDataCommand,
    nonce: u64,
    public_key_hex: &str,
    private_key_hex: &str,
) -> crate::viewer::CollectDataCommand {
    let mut command = mode;
    match &mut command {
        crate::viewer::CollectDataCommand::Preflight { request }
        | crate::viewer::CollectDataCommand::Submit { request } => {
            request.public_key = Some(public_key_hex.to_string());
        }
    }
    let proof = crate::viewer::sign_collect_data_auth_proof(
        &command,
        nonce,
        public_key_hex,
        private_key_hex,
    )
    .expect("sign collect_data auth");
    match &mut command {
        crate::viewer::CollectDataCommand::Preflight { request }
        | crate::viewer::CollectDataCommand::Submit { request } => request.auth = Some(proof),
    }
    command
}

fn signed_refine_quote_request(
    compound_mass_g: i64,
    player_id: &str,
    nonce: u64,
    public_key_hex: &str,
    private_key_hex: &str,
) -> crate::viewer::RefineQuoteRequest {
    let mut request = crate::viewer::RefineQuoteRequest {
        compound_mass_g,
        player_id: player_id.to_string(),
        public_key: Some(public_key_hex.to_string()),
        auth: None,
    };
    request.auth = Some(
        crate::viewer::sign_refine_quote_auth_proof(
            &request,
            nonce,
            public_key_hex,
            private_key_hex,
        )
        .expect("sign refine quote auth"),
    );
    request
}

fn signed_product_validation_quote_request(
    product_id: &str,
    amount: i64,
    player_id: &str,
    nonce: u64,
    public_key_hex: &str,
    private_key_hex: &str,
) -> crate::viewer::ProductValidationQuoteRequest {
    let mut request = crate::viewer::ProductValidationQuoteRequest {
        product_id: product_id.to_string(),
        amount,
        player_id: player_id.to_string(),
        public_key: Some(public_key_hex.to_string()),
        auth: None,
    };
    request.auth = Some(
        crate::viewer::sign_product_validation_quote_auth_proof(
            &request,
            nonce,
            public_key_hex,
            private_key_hex,
        )
        .expect("sign product validation quote auth"),
    );
    request
}

fn signed_power_survival_quote_request(
    seller_agent_id: &str,
    amount: i64,
    requested_price_per_pu: i64,
    player_id: &str,
    nonce: u64,
    public_key_hex: &str,
    private_key_hex: &str,
) -> crate::viewer::PowerSurvivalQuoteRequest {
    let mut request = crate::viewer::PowerSurvivalQuoteRequest {
        seller_agent_id: seller_agent_id.to_string(),
        amount,
        requested_price_per_pu,
        recovery_action: crate::viewer::PowerSurvivalRecoveryAction::BuyPower,
        player_id: player_id.to_string(),
        public_key: Some(public_key_hex.to_string()),
        auth: None,
    };
    request.auth = Some(
        crate::viewer::sign_power_survival_quote_auth_proof(
            &request,
            nonce,
            public_key_hex,
            private_key_hex,
        )
        .expect("sign power survival quote auth"),
    );
    request
}

fn signed_market_quote_decision_request(
    material: &str,
    amount: i64,
    player_id: &str,
    nonce: u64,
    public_key_hex: &str,
    private_key_hex: &str,
) -> crate::viewer::MarketQuoteDecisionRequest {
    let mut request = crate::viewer::MarketQuoteDecisionRequest {
        consume: vec![crate::viewer::MarketQuoteMaterialRequest {
            material: material.to_string(),
            amount,
        }],
        player_id: player_id.to_string(),
        public_key: Some(public_key_hex.to_string()),
        auth: None,
    };
    request.auth = Some(
        crate::viewer::sign_market_quote_decision_auth_proof(
            &request,
            nonce,
            public_key_hex,
            private_key_hex,
        )
        .expect("sign market decision quote auth"),
    );
    request
}

#[test]
fn runtime_market_quote_decision_preflight_is_signed_read_only_and_player_readable() {
    let _guard = lock_test_llm_env();
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
    let (public_key, private_key) = test_signer(251);
    register_runtime_session(
        &mut server,
        "player-market-quote",
        Some(agent_id.as_str()),
        251,
        public_key.as_str(),
        private_key.as_str(),
    );
    server
        .world
        .set_ledger_material_balance(
            crate::runtime::MaterialLedgerId::agent(agent_id.as_str()),
            "iron_ingot",
            1,
        )
        .expect("seed local inventory");
    server
        .world
        .set_material_balance("iron_ingot", 2)
        .expect("seed world inventory");
    let state_before = server.world.state().clone();

    let quote = server
        .handle_market_quote_decision(signed_market_quote_decision_request(
            "iron_ingot",
            4,
            "player-market-quote",
            262,
            public_key.as_str(),
            private_key.as_str(),
        ))
        .expect("signed market quote");

    assert_eq!(quote.consuming_agent_id, agent_id);
    assert_eq!(quote.contributions[0].material, "Iron ingot");
    assert_eq!(quote.contributions[0].requested_amount, 4);
    assert_eq!(quote.contributions[0].local_available_amount, 1);
    assert_eq!(quote.contributions[0].world_cover_amount, 2);
    assert_eq!(quote.contributions[0].shortfall_amount, 2);
    assert_eq!(quote.total_shortfall_amount, 2);
    assert!(!quote.submission_allowed);
    assert_eq!(
        quote.recommendation,
        "Reduce the request or obtain more materials"
    );
    assert_eq!(
        quote.next_action,
        "Reduce requested amounts or source the missing materials"
    );
    assert!(quote.conditional_notice.contains("conditional preview"));
    assert_eq!(server.world.state(), &state_before);
}

#[test]
fn runtime_market_quote_decision_requires_valid_auth() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let unsigned = crate::viewer::MarketQuoteDecisionRequest {
        consume: vec![crate::viewer::MarketQuoteMaterialRequest {
            material: "iron_ingot".to_string(),
            amount: 1,
        }],
        player_id: "player-market-quote".to_string(),
        public_key: None,
        auth: None,
    };
    assert_eq!(
        server
            .handle_market_quote_decision(unsigned)
            .expect_err("unsigned market preview must fail")
            .code,
        "auth_proof_required"
    );

    let (public_key, private_key) = test_signer(253);
    let mut tampered = signed_market_quote_decision_request(
        "iron_ingot",
        1,
        "player-market-quote",
        264,
        public_key.as_str(),
        private_key.as_str(),
    );
    tampered.auth.as_mut().expect("signature").signature = "00".repeat(64);
    assert_eq!(
        server
            .handle_market_quote_decision(tampered)
            .expect_err("tampered market preview must fail")
            .code,
        "auth_signature_invalid"
    );
}

#[test]
fn runtime_product_validation_quote_is_signed_read_only_and_advisory_before_submission() {
    let _guard = lock_test_llm_env();
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
    let (public_key, private_key) = test_signer(251);
    register_runtime_session(
        &mut server,
        "player-product-quote",
        Some(agent_id.as_str()),
        251,
        public_key.as_str(),
        private_key.as_str(),
    );
    let state_before = server.world.state().clone();
    let quote = server
        .handle_product_validation_quote(signed_product_validation_quote_request(
            "logistics_drone",
            1,
            "player-product-quote",
            252,
            public_key.as_str(),
            private_key.as_str(),
        ))
        .expect("signed product validation quote");

    assert_eq!(quote.product_id, "logistics_drone");
    assert_eq!(quote.product_role, "explore");
    assert_eq!(quote.stage_before, "bootstrap");
    assert_eq!(quote.stage_after, "bootstrap");
    assert_eq!(quote.unlock_or_value_class, "scale_out");
    assert!(
        quote.submission_allowed,
        "stage guidance must remain advisory"
    );
    assert_eq!(quote.missing_prerequisite, "industry_stage=scale_out");
    assert_eq!(
        quote.reachable_advance_or_recovery,
        "complete_reachable_industry_progress"
    );
    assert_eq!(server.world.state(), &state_before);
}

#[test]
fn runtime_product_validation_quote_requires_auth_proof() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let err = server
        .handle_product_validation_quote(crate::viewer::ProductValidationQuoteRequest {
            product_id: "logistics_drone".to_string(),
            amount: 1,
            player_id: "player-product-quote".to_string(),
            public_key: None,
            auth: None,
        })
        .expect_err("unsigned product validation quote must fail");

    assert_eq!(err.code, "auth_proof_required");
    assert_eq!(err.action_id.as_deref(), Some("quote_validate_product"));
}

#[test]
fn runtime_product_validation_quote_rejects_tampered_auth_proof() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let (public_key, private_key) = test_signer(253);
    let mut request = signed_product_validation_quote_request(
        "logistics_drone",
        1,
        "player-product-quote",
        254,
        public_key.as_str(),
        private_key.as_str(),
    );
    request.auth.as_mut().expect("signed auth").signature = "00".repeat(64);

    let err = server
        .handle_product_validation_quote(request)
        .expect_err("tampered product validation quote auth must fail");

    assert_eq!(err.code, "auth_signature_invalid");
    assert_eq!(err.action_id.as_deref(), Some("quote_validate_product"));
}

#[test]
fn runtime_refine_quote_is_signed_deterministic_and_non_mutating() {
    let _guard = lock_test_llm_env();
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
    let (public_key, private_key) = test_signer(201);
    register_runtime_session(
        &mut server,
        "player-refine",
        Some(agent_id.as_str()),
        201,
        public_key.as_str(),
        private_key.as_str(),
    );
    server.snapshot_config.economy.factory_build_hardware_cost = 5;
    server
        .snapshot_config
        .economy
        .refine_electricity_cost_per_kg = 1;
    server.snapshot_config.economy.refine_hardware_yield_ppm = 1_000;
    server
        .world
        .set_agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            50,
        )
        .expect("seed electricity");
    server
        .world
        .set_agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Data,
            2_500,
        )
        .expect("seed compound");
    let state_before = server.world.state().clone();

    let request = signed_refine_quote_request(
        2_500,
        "player-refine",
        202,
        public_key.as_str(),
        private_key.as_str(),
    );
    let quote = server
        .handle_refine_quote(request.clone())
        .expect("quote exact advance");
    assert_eq!(quote.owner_agent_id, agent_id);
    assert_eq!(quote.electricity_cost, 3);
    assert_eq!(quote.electricity_after, 47);
    assert_eq!(quote.hardware_output, 2);
    assert_eq!(quote.target_id, "factory_build_hardware");
    assert_eq!(quote.target_gap_before, 5);
    assert_eq!(quote.target_gap_after, 3);
    assert_eq!(
        quote.target_linkage,
        "reduces_factory_build_hardware_shortfall"
    );
    assert_eq!(quote.value_classification, "partial_progress");
    assert_eq!(
        server.handle_refine_quote(request).expect("repeat quote"),
        quote
    );
    assert_eq!(server.world.state(), &state_before);

    server
        .world
        .set_agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Data,
            6_002,
        )
        .expect("seed existing progress");
    let enough = server
        .handle_refine_quote(signed_refine_quote_request(
            6_000,
            "player-refine",
            203,
            public_key.as_str(),
            private_key.as_str(),
        ))
        .expect("quote advance");
    assert_eq!(enough.value_classification, "enough_to_advance");
    assert_eq!(enough.target_gap_after, 0);

    server
        .world
        .set_agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Data,
            6_005,
        )
        .expect("seed satisfied goal");
    let poor = server
        .handle_refine_quote(signed_refine_quote_request(
            1_000,
            "player-refine",
            204,
            public_key.as_str(),
            private_key.as_str(),
        ))
        .expect("quote poor tradeoff");
    assert_eq!(poor.value_classification, "poor_power_tradeoff");

    server
        .world
        .set_agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            0,
        )
        .expect("drain electricity");
    let insufficient = server
        .handle_refine_quote(signed_refine_quote_request(
            1_000,
            "player-refine",
            205,
            public_key.as_str(),
            private_key.as_str(),
        ))
        .expect_err("insufficient electricity rejected");
    assert_eq!(insufficient.code, "refine_quote_rejected");
}

#[test]
fn runtime_power_survival_quote_is_signed_deterministic_non_mutating_and_binds_buy_parameters() {
    let _guard = lock_test_llm_env();
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
    let (public_key, private_key) = test_signer(27);
    register_runtime_session(
        &mut server,
        "player-power-survival-quote",
        Some(agent_id.as_str()),
        271,
        public_key.as_str(),
        private_key.as_str(),
    );
    server
        .world
        .set_agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            5,
        )
        .expect("seed critical electricity");
    let state_before = server.world.state().clone();
    let request = signed_power_survival_quote_request(
        agent_id.as_str(),
        1,
        1,
        "player-power-survival-quote",
        272,
        public_key.as_str(),
        private_key.as_str(),
    );

    let quote = server
        .handle_power_survival_quote(request.clone())
        .expect("signed buy power survival quote");
    assert_eq!(quote.buyer_agent_id, agent_id);
    assert_eq!(quote.seller_agent_id, request.seller_agent_id);
    assert_eq!(quote.recovery_action, "buy_power");
    assert_eq!(quote.recovery_amount, request.amount);
    assert_eq!(quote.requested_price_per_pu, request.requested_price_per_pu);
    assert_eq!(quote.price_per_pu, 1);
    assert_eq!(quote.power_state_before, "critical");
    assert_eq!(quote.power_state_after_recovery, "low_power");
    assert_eq!(quote.power_gain_estimate, 1);
    assert_eq!(
        server
            .handle_power_survival_quote(request)
            .expect("repeat quote"),
        quote
    );
    assert_eq!(server.world.state(), &state_before);

    let invalid = server
        .handle_power_survival_quote(signed_power_survival_quote_request(
            agent_id.as_str(),
            0,
            1,
            "player-power-survival-quote",
            273,
            public_key.as_str(),
            private_key.as_str(),
        ))
        .expect_err("invalid buy amount preserves simulator rejection");
    assert_eq!(invalid.code, "power_survival_quote_rejected");
    assert_eq!(invalid.action_id.as_deref(), Some("quote_power_survival"));

    let mut tampered = signed_power_survival_quote_request(
        agent_id.as_str(),
        1,
        1,
        "player-power-survival-quote",
        274,
        public_key.as_str(),
        private_key.as_str(),
    );
    tampered.requested_price_per_pu = 2;
    let tampered_error = server
        .handle_power_survival_quote(tampered)
        .expect_err("signed buy parameters must reject tampering");
    assert_eq!(tampered_error.code, "auth_signature_invalid");
}

#[test]
fn runtime_power_survival_quote_accepts_legacy_signed_buy_power_request_without_recovery_action() {
    let _guard = lock_test_llm_env();
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
    let (public_key, private_key) = test_signer(208);
    register_runtime_session(
        &mut server,
        "player-legacy-power-survival-quote",
        Some(agent_id.as_str()),
        208,
        public_key.as_str(),
        private_key.as_str(),
    );
    server
        .world
        .set_agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            5,
        )
        .expect("seed critical electricity");
    let mut request: crate::viewer::PowerSurvivalQuoteRequest =
        serde_json::from_value(serde_json::json!({
            "seller_agent_id": agent_id,
            "amount": 1,
            "requested_price_per_pu": 1,
            "player_id": "player-legacy-power-survival-quote",
            "public_key": public_key,
        }))
        .expect("legacy request omits recovery_action");
    assert_eq!(
        request.recovery_action,
        crate::viewer::PowerSurvivalRecoveryAction::BuyPower
    );
    let legacy_payload = crate::viewer::GameplayActionRequest {
        action_id: "quote_power_survival".to_string(),
        target_agent_id: format!(
            "seller_agent_id:{}|amount:{}|requested_price_per_pu:{}",
            request.seller_agent_id, request.amount, request.requested_price_per_pu
        ),
        actor_agent_id: None,
        player_id: request.player_id.clone(),
        public_key: request.public_key.clone(),
        auth: None,
    };
    request.auth = Some(
        crate::viewer::sign_gameplay_action_auth_proof(
            &legacy_payload,
            279,
            public_key.as_str(),
            private_key.as_str(),
        )
        .expect("sign legacy buy power quote payload"),
    );
    let quote = server
        .handle_power_survival_quote(request)
        .expect("legacy signed buy power quote remains valid");
    assert_eq!(quote.recovery_action, "buy_power");
}

#[test]
fn runtime_power_survival_quote_supports_signed_non_mutating_harvest_radiation_recovery() {
    let _guard = lock_test_llm_env();
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
    let (public_key, private_key) = test_signer(205);
    register_runtime_session(
        &mut server,
        "player-harvest-survival-quote",
        Some(agent_id.as_str()),
        205,
        public_key.as_str(),
        private_key.as_str(),
    );
    server
        .world
        .set_agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            5,
        )
        .expect("seed critical electricity");
    let state_before = server.world.state().clone();

    // RED API contract: harvest is a recovery action in the same signed preflight family, not a
    // fake BuyPower request with a synthetic seller or price.
    let mut request = crate::viewer::PowerSurvivalQuoteRequest {
        seller_agent_id: String::new(),
        amount: 20,
        requested_price_per_pu: 0,
        recovery_action: crate::viewer::PowerSurvivalRecoveryAction::HarvestRadiation,
        player_id: "player-harvest-survival-quote".to_string(),
        public_key: Some(public_key.clone()),
        auth: None,
    };
    request.auth = Some(
        crate::viewer::sign_power_survival_quote_auth_proof(
            &request,
            276,
            public_key.as_str(),
            private_key.as_str(),
        )
        .expect("sign harvest survival quote auth"),
    );

    let quote = server
        .handle_power_survival_quote(request.clone())
        .expect("signed harvest radiation survival quote");
    assert_eq!(quote.buyer_agent_id, agent_id);
    assert_eq!(quote.recovery_action, "harvest_radiation");
    assert_eq!(quote.recovery_amount, 20);
    assert!(quote.power_gain_estimate > 0);
    assert_eq!(quote.price_or_time_cost, 0);
    assert_eq!(
        server
            .handle_power_survival_quote(request)
            .expect("repeat harvest quote"),
        quote
    );
    assert_eq!(server.world.state(), &state_before);

    let mut tampered = signed_power_survival_quote_request(
        agent_id.as_str(),
        1,
        1,
        "player-harvest-survival-quote",
        277,
        public_key.as_str(),
        private_key.as_str(),
    );
    tampered.recovery_action = crate::viewer::PowerSurvivalRecoveryAction::HarvestRadiation;
    let tampered_error = server
        .handle_power_survival_quote(tampered)
        .expect_err("recovery action must be bound by the signed quote payload");
    assert_eq!(tampered_error.code, "auth_signature_invalid");
}

#[test]
fn runtime_harvest_survival_quote_rejection_redacts_location_id() {
    let _guard = lock_test_llm_env();
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
    let location_id = server.compat_snapshot(None).model.agents[&agent_id]
        .location_id
        .clone();
    let (public_key, private_key) = test_signer(207);
    register_runtime_session(
        &mut server,
        "player-harvest-redaction",
        Some(agent_id.as_str()),
        207,
        public_key.as_str(),
        private_key.as_str(),
    );
    server.snapshot_config.physics.radiation_floor = 0;
    server.snapshot_config.physics.radiation_floor_cap_per_tick = 0;
    let mut request = crate::viewer::PowerSurvivalQuoteRequest {
        seller_agent_id: String::new(),
        amount: 1,
        requested_price_per_pu: 0,
        recovery_action: crate::viewer::PowerSurvivalRecoveryAction::HarvestRadiation,
        player_id: "player-harvest-redaction".to_string(),
        public_key: Some(public_key.clone()),
        auth: None,
    };
    request.auth = Some(
        crate::viewer::sign_power_survival_quote_auth_proof(
            &request,
            278,
            public_key.as_str(),
            private_key.as_str(),
        )
        .expect("sign harvest quote auth"),
    );
    let error = server
        .handle_power_survival_quote(request)
        .expect_err("no-radiation harvest quote rejected");
    assert_eq!(error.code, "power_survival_quote_rejected");
    assert!(!error.message.contains(location_id.as_str()));
}

#[test]
fn runtime_refine_quote_does_not_require_or_mutate_gameplay_readiness() {
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
    let (public_key, private_key) = test_signer(206);
    register_runtime_session(
        &mut server,
        "player-refine-script",
        Some(agent_id.as_str()),
        206,
        public_key.as_str(),
        private_key.as_str(),
    );
    server
        .world
        .set_agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Data,
            2_500,
        )
        .expect("seed compound");
    let state_before = server.world.state().clone();
    let feedback_before = server.latest_player_gameplay_feedback.clone();

    let quote = server
        .handle_refine_quote(signed_refine_quote_request(
            2_500,
            "player-refine-script",
            207,
            public_key.as_str(),
            private_key.as_str(),
        ))
        .expect("script-mode quote remains read-only");

    assert_eq!(quote.owner_agent_id, agent_id);
    assert_eq!(server.world.state(), &state_before);
    assert_eq!(server.latest_player_gameplay_feedback, feedback_before);
}

#[test]
fn runtime_collect_data_quotes_and_submits_the_exact_authenticated_request() {
    let _guard = lock_test_llm_env();
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
    let (public_key, private_key) = test_signer(181);
    register_runtime_session(
        &mut server,
        "player-collect",
        Some(agent_id.as_str()),
        181,
        public_key.as_str(),
        private_key.as_str(),
    );
    server
        .world
        .set_agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            20,
        )
        .expect("seed electricity");
    let data_before = server
        .world
        .agent_resource_balance(
            agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Data,
        )
        .expect("initial data balance");

    let preflight = signed_collect_data_command(
        crate::viewer::CollectDataCommand::Preflight {
            request: crate::viewer::CollectDataRequest {
                electricity_cost: 7,
                data_amount: 11,
                player_id: "player-collect".to_string(),
                public_key: None,
                auth: None,
            },
        },
        182,
        public_key.as_str(),
        private_key.as_str(),
    );
    let crate::viewer::runtime_live::player_gameplay::CollectDataResult::Preflight(quote) = server
        .handle_collect_data(preflight)
        .expect("authoritative preflight")
    else {
        panic!("expected preflight quote");
    };
    assert_eq!(quote.collector_agent_id, agent_id);
    assert_eq!(quote.data_owner_agent_id, quote.collector_agent_id);
    assert_eq!(quote.data_recipient_agent_id, quote.collector_agent_id);
    assert_eq!(quote.electricity_after, 13);
    assert_eq!(quote.permission_status, "self_owned_no_grant_required");

    let submit = signed_collect_data_command(
        crate::viewer::CollectDataCommand::Submit {
            request: crate::viewer::CollectDataRequest {
                electricity_cost: 7,
                data_amount: 11,
                player_id: "player-collect".to_string(),
                public_key: None,
                auth: None,
            },
        },
        183,
        public_key.as_str(),
        private_key.as_str(),
    );
    let crate::viewer::runtime_live::player_gameplay::CollectDataResult::Submit(ack) = server
        .handle_collect_data(submit)
        .expect("collect data queued")
    else {
        panic!("expected submit acknowledgement");
    };
    assert_eq!(ack.action_id, "collect_data");
    assert_eq!(ack.target_agent_id, agent_id);
    server.world.step().expect("apply collect data");
    assert_eq!(
        server
            .world
            .agent_resource_balance(
                agent_id.as_str(),
                oasis7_agent_api::world_action::ResourceKind::Data
            )
            .expect("data balance"),
        data_before + 11
    );
}

#[path = "auth_actions_collect_data_service.rs"]
mod service;
