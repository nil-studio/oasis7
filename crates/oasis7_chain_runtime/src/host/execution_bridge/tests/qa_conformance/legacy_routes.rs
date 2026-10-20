//! Compatibility uses the real production listener and original authenticated bytes.
use super::*;
struct ProductionServer(crate::status_server_support::ChainStatusServer);
impl Drop for ProductionServer {
    fn drop(&mut self) {
        crate::status_server_support::stop_chain_status_server(&mut self.0);
    }
}
#[test]
fn real_tcp_legacy_status_and_signed_gameplay_preserve_original_auth_bytes() {
    let (fixture, original, public) = legacy_nonce::legacy_seed(false);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let _server = ProductionServer(
        crate::status_admission::start_test_server(
            &fixture.root,
            &endpoint,
            fixture.node.clone(),
            crate::feedback_submit_api::FeedbackSubmitSigner {
                private_key_hex: hex::encode([9u8; 32]),
                public_key_hex: sign_read_request("service", (), &hex::encode([9u8; 32]))
                    .unwrap()
                    .subject_public_key,
            },
        )
        .unwrap(),
    );
    let http = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let status = http
        .get(format!("{endpoint}/v1/chain/status"))
        .send()
        .unwrap();
    assert_eq!(status.status().as_u16(), 200);
    let WorldServicePayloadV1::GameplayJson(bytes) = &original.signed_payload else {
        unreachable!()
    };
    let response = http
        .post(format!("{endpoint}/v1/chain/gameplay/submit"))
        .header("content-type", "application/json")
        .body(bytes.clone())
        .send()
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let accepted: serde_json::Value = response.json().unwrap();
    let action_id = accepted["action_id"].as_u64().expect("legacyrouteactionid");
    let actions = vec![
        NodeConsensusAction::from_payload(
            action_id,
            "node-a",
            correlation::encode_consensus_intent(&original).unwrap(),
        )
        .unwrap(),
    ];
    let mut driver = fixture.driver.lock().unwrap();
    let height = driver.state.last_applied_committed_height + 1;
    driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".into(),
            node_id: "node-a".into(),
            proposer_id: "node-a".into(),
            height,
            slot: height,
            epoch: 0,
            node_block_hash: format!("legacy-http-h{height}"),
            action_root: compute_consensus_action_root(&actions).unwrap(),
            committed_actions: actions,
            committed_at_unix_ms: height as i64 * 1000,
        })
        .unwrap();
    let key = correlation::key_digest(&original.correlation.key).unwrap();
    let result: wire::CanonicalIntentResultV1 = serde_json::from_value(
        driver
            .execution_world
            .capability_revocation_state()
            .world_service_results[&key]
            .clone(),
    )
    .unwrap();
    assert!(result.rejected.is_none());
    assert_eq!(result.request.signed_payload, original.signed_payload);
    assert_eq!(
        driver
            .execution_world
            .state()
            .authenticated_collect_data_last_nonces["owner-a"][&public],
        901
    );
    drop(driver);
    fixture.committed(&original);
    println!(
        "PRE2_LEGACY_STATUS_GAMEPLAY_AUTH_BYTES_PASSED returned_action_id={action_id} original_signed_bytes=true"
    );
}

#[test]
fn real_tcp_new_service_rejects_distinct_legacy_claim_transfer_and_unknown_codecs() {
    let (fixture, original, public) = legacy_nonce::legacy_seed(false);
    fixture.node.lock().unwrap().stop().unwrap();
    let claim = crate::agent_claim_api::ChainAgentClaimSubmitRequest {
        claimer_agent_id: "agent-a".into(),
        target_agent_id: "agent-b".into(),
    };
    // This is the separate legacy transfer wire type, not GameplayActionRequest.
    let transfer:crate::transfer_submit_api::ChainTransferSubmitRequest=serde_json::from_value(serde_json::json!({"from_account_id":"agent-a","to_account_id":"agent-b","amount":1,"nonce":1,"public_key":public,"signature":"00".repeat(64)})).unwrap();
    let unsupported = [
        ("legacy_agent_claim", serde_json::to_value(claim).unwrap()),
        ("legacy_transfer", serde_json::to_value(transfer).unwrap()),
        (
            "unknown_enum",
            serde_json::json!({"type":"unknown_future_service_action"}),
        ),
    ];
    let before = fixture
        .node
        .lock()
        .unwrap()
        .snapshot()
        .consensus
        .pending_consensus_actions
        .submit_buffer_action_count;
    let height = fixture
        .driver
        .lock()
        .unwrap()
        .state
        .last_applied_committed_height;
    for (kind, value) in unsupported {
        let payload = WorldServicePayloadV1::GameplayJson(serde_json::to_vec(&value).unwrap());
        assert!(
            derive_correlation(fixture.client.config().expected_world.clone(), &payload).is_err(),
            "unsupported distinct codec must not receive a signed service identity"
        );
        let mut request = original.clone();
        request.signed_payload = payload;
        let response = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap()
            .post(format!(
                "{}/v1/world/submit",
                fixture.client.config().endpoint
            ))
            .json(&request)
            .send()
            .unwrap();
        assert_eq!(response.status().as_u16(), 503);
        assert!(response.headers().get("retry-after").is_none());
        let error: serde_json::Value = response.json().unwrap();
        assert_eq!(error["error"], "world_service_unavailable");
        assert_eq!(error["reason"], "service request cannot be served");
        assert!(error.get("payload").is_none() && error.get("signature_hex").is_none());
        assert_eq!(
            fixture
                .node
                .lock()
                .unwrap()
                .snapshot()
                .consensus
                .pending_consensus_actions
                .submit_buffer_action_count,
            before
        );
        assert_eq!(
            fixture
                .driver
                .lock()
                .unwrap()
                .state
                .last_applied_committed_height,
            height
        );
        println!(
            "new_service_distinct_codec_rejected kind={kind} no_node_enqueue=true no_signed_assertion=true"
        );
    }
    println!("PRE2_DISTINCT_LEGACY_CODECS_NEW_SERVICE_REJECTED");
}
