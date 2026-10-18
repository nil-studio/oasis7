use super::*;

#[test]
fn runtime_revoke_social_fact_quote_negotiates_capability() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let mut session = RuntimeLiveSession::new();
    let (mut writer, peer) = test_writer_pair();

    server
        .handle_request(
            ViewerRequest::HelloV2 {
                client: "revoke-quote-capability-probe".to_string(),
                version: VIEWER_PROTOCOL_VERSION,
                capabilities: vec![REVOKE_SOCIAL_FACT_QUOTE_CAPABILITY.to_string()],
            },
            &mut session,
            &mut writer,
        )
        .expect("handle v2 hello");

    let responses = read_available_runtime_live_responses(&peer, Duration::from_millis(25));
    let capabilities = match responses.as_slice() {
        [ViewerResponse::HelloAck { capabilities, .. }] => capabilities,
        other => panic!("expected one hello ack, got {other:?}"),
    };
    assert!(
        capabilities
            .iter()
            .any(|capability| { capability == REVOKE_SOCIAL_FACT_QUOTE_CAPABILITY })
    );
    assert!(viewer_protocol_supports_revoke_social_fact_quote(
        &session.negotiated_protocol
    ));
}

#[test]
fn runtime_revoke_social_fact_quote_without_capability_returns_structured_error() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let mut session = RuntimeLiveSession::new();
    let (mut writer, peer) = test_writer_pair();

    server
        .handle_request(
            ViewerRequest::HelloV2 {
                client: "legacy-v2-revoke-quote-probe".to_string(),
                version: VIEWER_PROTOCOL_VERSION,
                capabilities: Vec::new(),
            },
            &mut session,
            &mut writer,
        )
        .expect("handle legacy v2 hello");
    let hello_responses = read_available_runtime_live_responses(&peer, Duration::from_millis(25));
    let capabilities = match hello_responses.as_slice() {
        [ViewerResponse::HelloAck { capabilities, .. }] => capabilities,
        other => panic!("expected one legacy hello ack, got {other:?}"),
    };
    assert!(capabilities.is_empty());

    server
        .handle_request(
            ViewerRequest::QuoteRevokeSocialFact {
                request: crate::viewer::RevokeSocialFactQuoteRequest {
                    fact_id: 88,
                    reason: "withdraw obsolete evidence".to_string(),
                    player_id: "player-revoke-quote-gate".to_string(),
                    public_key: None,
                    auth: None,
                },
            },
            &mut session,
            &mut writer,
        )
        .expect("unsupported protocol capability should be a response");

    let responses = read_available_runtime_live_responses(&peer, Duration::from_millis(25));
    let ViewerResponse::GameplayActionError { error } =
        responses.first().expect("missing protocol gate response")
    else {
        panic!("expected structured protocol gate error, got {responses:?}");
    };
    assert_eq!(error.code, "protocol_upgrade_required");
    assert_eq!(error.action_id.as_deref(), Some("quote_revoke_social_fact"));
    assert!(error.message.contains(REVOKE_SOCIAL_FACT_QUOTE_CAPABILITY));
}
