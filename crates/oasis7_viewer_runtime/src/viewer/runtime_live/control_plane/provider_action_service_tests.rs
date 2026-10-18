use super::*;

#[test]
fn canonical_provider_summary_is_stable_without_io_events() {
    let server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .unwrap();
    let before = server.test_canonical_provider_summary();
    std::thread::sleep(std::time::Duration::from_millis(10));
    assert_eq!(server.test_canonical_provider_summary(), before);
    assert!(before["io_lifecycle"].get("current_age_us").is_none());
    assert!(
        server.test_canonical_provider_diagnostic_summary()["io_lifecycle"]["current_age_us"]
            .is_null()
    );
}

#[test]
fn canonical_wait_notification_without_harness_preserves_world_and_sends_no_http() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .unwrap();
    server.llm_sidecar.provider_service_required = true;
    let before = serde_json::to_value(server.world.snapshot()).unwrap();
    let memory_before = serde_json::to_value(server.llm_sidecar.provider_memory_store()).unwrap();
    assert!(
        server
            .llm_sidecar
            .validate_admitted_service_wait("agent-a")
            .unwrap_err()
            .contains("Harness runner missing")
    );
    assert_eq!(
        serde_json::to_value(server.world.snapshot()).unwrap(),
        before
    );
    assert_eq!(
        serde_json::to_value(server.llm_sidecar.provider_memory_store()).unwrap(),
        memory_before
    );
    assert!(server.world.runtime_feedback_outbox().unwrap().is_empty());
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn canonical_chat_unsigned_refusal_precedes_binding_and_world_mutation() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .unwrap();
    let request = AgentChatRequest {
        agent_id: "agent-a".into(),
        player_id: Some("player-a".into()),
        public_key: None,
        auth: None,
        message: "hello".into(),
        intent_tick: Some(1),
        intent_seq: Some(1),
        world_id: None,
        reorg_epoch: None,
        authority_scope: None,
        replaces_intent_id: None,
        canonical_authority: None,
    };
    server.config.chain_status_bind = Some("127.0.0.1:1".into());
    let before = serde_json::to_value(server.world.snapshot()).unwrap();
    let before_sidecar = server.test_canonical_provider_summary();
    assert_eq!(
        server.handle_agent_chat(request.clone()).unwrap_err().code,
        "auth_proof_required"
    );
    server.complete_agent_chat_if_receipt_bound("agent-a", "intent", "digest");
    assert_eq!(
        server.enqueue_pending_provider_agent_chat_replies()[0].code,
        "canonical_chat_unsupported"
    );
    assert_eq!(
        serde_json::to_value(server.world.snapshot()).unwrap(),
        before
    );
    assert_eq!(server.test_canonical_provider_summary(), before_sidecar);
    server.config.chain_status_bind = None;
    server.config.world_service = None;
    assert_eq!(
        server.handle_agent_chat(request).unwrap_err().code,
        "llm_mode_required"
    );
}

#[test]
fn offline_provider_idle_pass_preserves_world_without_service() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("offline server");
    server.config.world_service = None;
    server.config.chain_status_bind = None;
    assert!(!server.chain_link_enabled());
    let before = serde_json::to_value(server.world.snapshot()).unwrap();
    assert!(server.enqueue_llm_action_from_sidecar().unwrap().is_none());
    server.retry_committed_provider_action().unwrap();
    assert_eq!(
        serde_json::to_value(server.world.snapshot()).unwrap(),
        before
    );
}

#[test]
fn chain_linked_provider_pass_cannot_mutate_projection_when_service_is_unavailable() {
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_chain_status_bind("127.0.0.1:1"),
    )
    .expect("create chain-linked provider server");
    let before = serde_json::to_value(server.world.snapshot()).expect("snapshot before");
    let error = server
        .enqueue_llm_action_from_sidecar()
        .expect_err("canonical service owns mutations");
    assert!(
        error
            .llm_error
            .as_deref()
            .is_some_and(|message| message.contains("provider turn remains pending"))
    );
    assert_eq!(
        serde_json::to_value(server.world.snapshot()).expect("snapshot after"),
        before
    );
    assert!(server.retry_committed_provider_action().is_err());
    assert_eq!(
        serde_json::to_value(server.world.snapshot()).expect("snapshot after recovery"),
        before
    );
}
