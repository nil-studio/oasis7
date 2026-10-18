use super::*;

fn signed_power_sale_quote_request(
    buyer_agent_id: &str,
    amount: i64,
    requested_price_per_pu: i64,
    player_id: &str,
    nonce: u64,
    public_key_hex: &str,
    private_key_hex: &str,
) -> crate::viewer::PowerSaleQuoteRequest {
    let mut request = crate::viewer::PowerSaleQuoteRequest {
        buyer_agent_id: buyer_agent_id.to_string(),
        amount,
        requested_price_per_pu,
        player_id: player_id.to_string(),
        public_key: Some(public_key_hex.to_string()),
        auth: None,
    };
    request.auth = Some(
        crate::viewer::sign_power_sale_quote_auth_proof(
            &request,
            nonce,
            public_key_hex,
            private_key_hex,
        )
        .expect("sign power sale quote auth"),
    );
    request
}

#[test]
fn runtime_power_sale_quote_is_seller_session_bound_signed_deterministic_and_maps_dangerous_sale() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let seller_agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed seller agent");
    let (public_key, private_key) = test_signer(181);
    register_runtime_session(
        &mut server,
        "seller-power-sale-quote",
        Some(seller_agent_id.as_str()),
        281,
        public_key.as_str(),
        private_key.as_str(),
    );
    server
        .world
        .set_agent_resource_balance(
            seller_agent_id.as_str(),
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            15,
        )
        .expect("seed seller electricity");
    let state_before = server.world.state().clone();
    let request = signed_power_sale_quote_request(
        seller_agent_id.as_str(),
        10,
        3,
        "seller-power-sale-quote",
        282,
        public_key.as_str(),
        private_key.as_str(),
    );

    let quote = server
        .handle_power_sale_quote(request.clone())
        .expect("signed seller power sale quote");
    assert_eq!(quote.seller_agent_id, seller_agent_id);
    assert_eq!(quote.buyer_agent_id, request.buyer_agent_id);
    assert_eq!(quote.sale_amount, 10);
    assert_eq!(quote.price_per_pu, 3);
    assert_eq!(quote.expected_revenue, 30);
    assert_eq!(quote.power_state_after_sale, "critical");
    assert_eq!(quote.remaining_runway_ticks, 5);
    assert_eq!(quote.next_action_affordability_after_sale, "limited");
    assert!(quote.production_interrupt_risk);
    assert_eq!(quote.recommended_sale_action, "defer_sale");
    assert_eq!(
        server
            .handle_power_sale_quote(request.clone())
            .expect("repeat quote"),
        quote
    );
    assert_eq!(server.world.state(), &state_before);

    let mut missing_auth = request.clone();
    missing_auth.auth = None;
    let missing_auth_error = server
        .handle_power_sale_quote(missing_auth)
        .expect_err("sale quote requires seller authentication");
    assert_eq!(missing_auth_error.code, "auth_proof_required");
    assert_eq!(
        missing_auth_error.action_id.as_deref(),
        Some("quote_power_sale")
    );

    let mut tampered_buyer = request;
    tampered_buyer.buyer_agent_id = "unapproved-buyer".to_string();
    let tampered_error = server
        .handle_power_sale_quote(tampered_buyer)
        .expect_err("signed buyer binding must reject tampering");
    assert_eq!(tampered_error.code, "auth_signature_invalid");
}
