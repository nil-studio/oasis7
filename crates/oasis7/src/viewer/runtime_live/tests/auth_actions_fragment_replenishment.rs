use super::*;

fn signed_fragment_refill_preview_request(
    chunk: oasis7_agent_api::world_geometry::ChunkCoord,
    player_id: &str,
    nonce: u64,
    public_key_hex: &str,
    private_key_hex: &str,
) -> crate::viewer::FragmentRefillPreviewRequest {
    let mut request = crate::viewer::FragmentRefillPreviewRequest {
        chunk,
        player_id: player_id.to_string(),
        public_key: Some(public_key_hex.to_string()),
        auth: None,
    };
    request.auth = Some(
        crate::viewer::sign_fragment_refill_preview_auth_proof(
            &request,
            nonce,
            public_key_hex,
            private_key_hex,
        )
        .expect("sign fragment refill preview auth"),
    );
    request
}

#[test]
fn runtime_fragment_refill_preview_disables_local_named_scenario_without_mutation() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::AsteroidFragmentBootstrap)
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
    let (public_key, private_key) = test_signer(254);
    register_runtime_session(
        &mut server,
        "player-fragment-refill-preview",
        Some(agent_id.as_str()),
        281,
        public_key.as_str(),
        private_key.as_str(),
    );
    let chunk = oasis7_agent_api::world_geometry::ChunkCoord { x: 0, y: 0, z: 0 };
    assert!(server.llm_sidecar.chunk_runtime.asteroid_fragment_enabled);
    for _ in 0..40 {
        server
            .world
            .step()
            .expect("advance runtime world to preview tick");
    }
    let preview_tick = server.world.state().time;
    assert!(preview_tick > 0 && preview_tick < 100);
    let state_before = server.world.state().clone();
    let request = signed_fragment_refill_preview_request(
        chunk,
        "player-fragment-refill-preview",
        282,
        public_key.as_str(),
        private_key.as_str(),
    );

    let quote = server
        .handle_fragment_refill_preview(request.clone())
        .expect("signed fragment refill preview");

    assert!(!quote.replenishment_enabled);
    assert!(!quote.replenishment_due);
    assert_eq!(quote.next_replenish_tick, None);
    assert_eq!(quote.ticks_until_replenish, None);
    assert_eq!(quote.wait_cost_ticks, 0);
    assert_eq!(quote.estimated_replenished_frag_count, 0);
    assert!(quote.chunk_remaining_summary.contains("unavailable"));
    assert!(!quote.next_industrial_goal_relevance.is_empty());
    assert_eq!(
        quote.recommended_resource_action,
        "move_or_switch_material_route"
    );
    assert_eq!(
        server
            .handle_fragment_refill_preview(request)
            .expect("repeat preview"),
        quote
    );
    assert_eq!(server.world.state(), &state_before);
}

#[test]
fn runtime_fragment_refill_preview_does_not_classify_unknown_service_projection_as_disabled() {
    let _guard = lock_test_llm_env();
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let (public_key, private_key) = test_signer(253);
    register_runtime_session(
        &mut server,
        "player-service-refill-preview",
        Some(agent_id.as_str()),
        283,
        public_key.as_str(),
        private_key.as_str(),
    );
    server.config.world_service = Some(crate::world_service::client::WorldServiceClientConfig {
        endpoint: "http://127.0.0.1:1".to_string(),
        trusted_service_public_key: public_key.clone(),
        expected_world: oasis7_client_api::world_service::WorldIdentity {
            world_id: "service-refill-world".into(),
            genesis_digest: "genesis".into(),
        },
        scope_id: "public".into(),
        read_private_key_hex: private_key.clone(),
        timeout: Duration::from_secs(1),
        formal_read_trust: None,
        max_response_bytes: 4096,
    });
    let request = signed_fragment_refill_preview_request(
        oasis7_agent_api::world_geometry::ChunkCoord { x: 0, y: 0, z: 0 },
        "player-service-refill-preview",
        284,
        &public_key,
        &private_key,
    );
    let before = server.world.state().clone();
    let error = server
        .handle_fragment_refill_preview(request.clone())
        .expect_err("unavailable service projection is not a local disabled quote");
    assert_eq!(error.code, "fragment_refill_preview_projection_failed");
    assert!(error.message.contains("unavailable from the world service"));
    assert_eq!(server.world.state(), &before);

    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::AsteroidFragmentBootstrap, &config);
    let (kernel, _) =
        crate::simulator::initialize_kernel(config, init).expect("initialize fragment seed");
    assert!(!kernel.model().chunk_resource_budgets.is_empty());
    server.seed_model = Some(kernel.model().clone());
    let error_with_seed = server
        .handle_fragment_refill_preview(request)
        .expect_err("a startup seed is not an authoritative service resource projection");
    assert_eq!(error_with_seed.code, error.code);
    assert_eq!(error_with_seed.message, error.message);
    assert_eq!(server.world.state(), &before);
}

#[test]
fn runtime_fragment_refill_preview_requires_an_auth_proof() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");

    let error = server
        .handle_fragment_refill_preview(crate::viewer::FragmentRefillPreviewRequest {
            chunk: oasis7_agent_api::world_geometry::ChunkCoord { x: 0, y: 0, z: 0 },
            player_id: "player-fragment-refill-preview".to_string(),
            public_key: None,
            auth: None,
        })
        .expect_err("unsigned preview must fail");

    assert_eq!(error.code, "auth_proof_required");
    assert_eq!(
        error.action_id.as_deref(),
        Some("preview_fragment_replenishment")
    );
}
