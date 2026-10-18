use super::*;

#[expect(
    clippy::too_many_arguments,
    reason = "Transfer quote test helper mirrors the stable signed request payload"
)]
fn signed_transfer_material_quote_request_json(
    requester_agent_id: &str,
    from_ledger: &str,
    to_ledger: &str,
    kind: &str,
    amount: i64,
    distance_km: i64,
    player_id: &str,
    nonce: u64,
    public_key_hex: &str,
    private_key_hex: &str,
) -> serde_json::Value {
    let signing_target = serde_json::json!({
        "requester_agent_id": requester_agent_id,
        "from_ledger": from_ledger,
        "to_ledger": to_ledger,
        "kind": kind,
        "amount": amount,
        "distance_km": distance_km,
        "requested_priority": null,
    });
    let signing_request = crate::viewer::GameplayActionRequest {
        action_id: "quote_transfer_material".to_string(),
        target_agent_id: serde_json::to_string(&signing_target)
            .expect("transfer-material quote auth target serializes"),
        actor_agent_id: None,
        player_id: player_id.to_string(),
        public_key: Some(public_key_hex.to_string()),
        auth: None,
    };
    let proof = crate::viewer::sign_gameplay_action_auth_proof(
        &signing_request,
        nonce,
        public_key_hex,
        private_key_hex,
    )
    .expect("sign transfer-material quote auth");

    serde_json::json!({
        "type": "quote_transfer_material",
        "request": {
            "requester_agent_id": requester_agent_id,
            "from_ledger": from_ledger,
            "to_ledger": to_ledger,
            "kind": kind,
            "amount": amount,
            "distance_km": distance_km,
            "player_id": player_id,
            "public_key": public_key_hex,
            "auth": proof,
        }
    })
}

fn read_transfer_material_quote_response(peer: &TcpStream) -> serde_json::Value {
    let line = read_response_line(peer, Duration::from_secs(1))
        .expect("transfer-material quote response line");
    let response: crate::viewer::ViewerResponse =
        serde_json::from_str(line.trim()).expect("decode transfer-material quote response");
    serde_json::to_value(response).expect("encode transfer-material quote response")
}

#[test]
fn runtime_transfer_material_quote_dispatch_is_signed_read_only_and_preserves_state() {
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
    let (public_key, private_key) = test_signer(241);
    register_runtime_session(
        &mut server,
        "player-transfer-quote",
        Some(agent_id.as_str()),
        241,
        public_key.as_str(),
        private_key.as_str(),
    );
    let source = crate::runtime::MaterialLedgerId::site("transfer-quote-source");
    let destination = crate::runtime::MaterialLedgerId::site("transfer-quote-destination");
    server
        .world
        .set_ledger_material_balance(source.clone(), "iron_ingot", 40)
        .expect("seed transfer source");
    let state_before = server.world.state().clone();
    let journal_before = server.world.journal().clone();

    let request_json = signed_transfer_material_quote_request_json(
        agent_id.as_str(),
        source.to_string().as_str(),
        destination.to_string().as_str(),
        "iron_ingot",
        20,
        200,
        "player-transfer-quote",
        242,
        public_key.as_str(),
        private_key.as_str(),
    );
    let request: crate::viewer::ViewerRequest =
        serde_json::from_value(request_json.clone()).expect("decode signed transfer quote");
    let (mut writer, peer) = test_writer_pair();
    let mut session = RuntimeLiveSession::new_with_playing(false);
    server
        .handle_request(request.clone(), &mut session, &mut writer)
        .expect("dispatch signed transfer quote");
    writer.flush().expect("flush transfer quote response");
    let response = read_transfer_material_quote_response(&peer);
    assert_eq!(response["type"], "transfer_material_quote_preflight");
    let quote = &response["quote"];
    assert_eq!(quote["requester_agent_id"], agent_id.as_str());
    assert_eq!(quote["from_ledger"], source.to_string());
    assert_eq!(quote["to_ledger"], destination.to_string());
    assert_eq!(quote["kind"], "iron_ingot");
    assert_eq!(quote["requested_amount"], 20);
    assert_eq!(quote["submission_feasible"], true);
    assert_eq!(quote["max_transferable_amount"], 40);
    assert_eq!(quote["sent_amount"], 20);
    assert_eq!(quote["expected_received_amount"], 18);
    assert_eq!(quote["recommendation"], "submit_transfer");
    assert_eq!(quote["conditional"], true);

    server
        .handle_request(request, &mut session, &mut writer)
        .expect("repeat signed transfer quote");
    writer
        .flush()
        .expect("flush repeated transfer quote response");
    let repeated = read_transfer_material_quote_response(&peer);
    assert_eq!(repeated, response);
    assert_eq!(server.world.state(), &state_before);
    assert_eq!(server.world.journal(), &journal_before);
}

#[test]
fn runtime_transfer_material_quote_surfaces_capacity_and_source_recommendations() {
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
    let (public_key, private_key) = test_signer(243);
    register_runtime_session(
        &mut server,
        "player-transfer-quote-capacity",
        Some(agent_id.as_str()),
        243,
        public_key.as_str(),
        private_key.as_str(),
    );
    let source = crate::runtime::MaterialLedgerId::site("transfer-capacity-source");
    server
        .world
        .set_ledger_material_balance(source.clone(), "iron_ingot", 100)
        .expect("seed capacity source");
    for index in 0..2 {
        server
            .world
            .submit_action(crate::runtime::Action::TransferMaterial {
                requester_agent_id: agent_id.clone(),
                from_ledger: source.clone(),
                to_ledger: crate::runtime::MaterialLedgerId::site(format!(
                    "transfer-capacity-destination-{index}"
                )),
                kind: "iron_ingot".to_string(),
                amount: 20,
                distance_km: 200,
                priority: None,
                route_id: None,
                route_ids: Vec::new(),
                auto_reroute: false,
            });
    }
    server
        .world
        .step()
        .expect("start capacity-filling transits");
    assert_eq!(server.world.pending_material_transits_len(), 2);
    let state_before = server.world.state().clone();
    let journal_before = server.world.journal().clone();
    let destination = crate::runtime::MaterialLedgerId::site("transfer-capacity-request");
    let (mut writer, peer) = test_writer_pair();
    let mut session = RuntimeLiveSession::new_with_playing(false);

    let capacity_request: crate::viewer::ViewerRequest =
        serde_json::from_value(signed_transfer_material_quote_request_json(
            agent_id.as_str(),
            source.to_string().as_str(),
            destination.to_string().as_str(),
            "iron_ingot",
            20,
            200,
            "player-transfer-quote-capacity",
            244,
            public_key.as_str(),
            private_key.as_str(),
        ))
        .expect("decode capacity transfer quote");
    server
        .handle_request(capacity_request, &mut session, &mut writer)
        .expect("dispatch capacity transfer quote");
    writer.flush().expect("flush capacity quote response");
    let capacity_response = read_transfer_material_quote_response(&peer);
    let capacity_quote = &capacity_response["quote"];
    assert_eq!(capacity_quote["submission_feasible"], false);
    assert_eq!(capacity_quote["sent_amount"], 0);
    assert_eq!(capacity_quote["expected_received_amount"], 0);
    assert_eq!(
        capacity_quote["inflight_before"],
        capacity_quote["inflight_capacity"]
    );
    assert_eq!(
        capacity_quote["recommendation"],
        "wait_for_transit_capacity"
    );

    let source_request: crate::viewer::ViewerRequest =
        serde_json::from_value(signed_transfer_material_quote_request_json(
            agent_id.as_str(),
            source.to_string().as_str(),
            destination.to_string().as_str(),
            "iron_ingot",
            61,
            200,
            "player-transfer-quote-capacity",
            245,
            public_key.as_str(),
            private_key.as_str(),
        ))
        .expect("decode source-shortfall transfer quote");
    server
        .handle_request(source_request, &mut session, &mut writer)
        .expect("dispatch source-shortfall transfer quote");
    writer.flush().expect("flush source-shortfall response");
    let source_response = read_transfer_material_quote_response(&peer);
    let source_quote = &source_response["quote"];
    assert_eq!(source_quote["submission_feasible"], false);
    assert_eq!(source_quote["max_transferable_amount"], 60);
    assert_eq!(source_quote["sent_amount"], 0);
    assert_eq!(
        source_quote["recommendation"],
        "reduce_amount_or_source_materials"
    );
    assert_eq!(server.world.state(), &state_before);
    assert_eq!(server.world.journal(), &journal_before);
}
