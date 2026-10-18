//! Application-only provider exercise: no node directory or execution driver handle.
use super::*;
use oasis7::simulator::MemoryWriteIntent;
use oasis7::viewer::{
    ViewerLiveDecisionMode, ViewerRuntimeLiveServer, ViewerRuntimeLiveServerConfig,
};
use oasis7::world_service::client::WorldServiceAgentSignerConfig;
use oasis7_simulator::WorldScenario;

pub(super) fn verify_provider_closure(public_client: &RemoteWorldServiceClient) {
    let mut connection = public_client.config().clone();
    connection.scope_id = "agent:agent-a".into();
    let client = RemoteWorldServiceClient::new(connection.clone()).unwrap();
    let mut config = ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal);
    config.world_id = "w1".into();
    config.world_service = Some(connection);
    config.world_service_agent_signer = Some(WorldServiceAgentSignerConfig {
        private_key_hex: hex::encode([8u8; 32]),
        delegation_generation: 1,
    });
    config.decision_mode = ViewerLiveDecisionMode::Llm;
    config.provider_lineage_store = Some(
        std::env::current_dir()
            .unwrap()
            .join("provider-closure-lineage.json"),
    );
    let mut server = ViewerRuntimeLiveServer::new(config).unwrap();
    let action = oasis7_simulator::Action::MoveAgent {
        agent_id: "agent-a".into(),
        to: "runtime:2:2:0".into(),
    };
    let cognition = server
        .test_prepare_canonical_provider_response_with_memory(
            "agent-a",
            action.clone(),
            vec![MemoryWriteIntent {
                scope: "session_private".into(),
                summary: "canonical receipt releases QA memory".into(),
                tags: vec!["receipt-gate".into()],
            }],
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match server.test_queue_canonical_provider_response(cognition.clone(), action.clone()) {
            Ok(()) => break,
            Err(error)
                if error.contains("canonical scheduler remains unresolved: Received")
                    || error.contains("canonical scheduler remains unresolved: Pending")
                    || error.contains("outcome unknown") => {}
            Err(error) => panic!("provider queue failed: {error}"),
        }
        assert!(Instant::now() < deadline, "provider queue timeout");
        thread::sleep(Duration::from_millis(10));
    }
    let before = server.test_canonical_provider_summary();
    assert!(
        !before["memory_store"]
            .to_string()
            .contains("canonical receipt releases QA memory"),
        "memory precedes canonical receipt"
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let summary = loop {
        match server.test_poll_canonical_provider_response() {
            Ok(()) => {}
            Err(error)
                if error.contains("pending")
                    || error.contains("remains unresolved")
                    || error.contains("outcome unknown") => {}
            Err(error) => panic!("provider finalization failed: {error}"),
        }
        let summary = server.test_canonical_provider_summary();
        if summary["pending_intent_count"] == 0
            && summary["memory_store"]
                .to_string()
                .contains("canonical receipt releases QA memory")
        {
            break summary;
        }
        assert!(
            Instant::now() < deadline,
            "provider closure timeout: {summary}"
        );
        thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(summary["pending_action_count"], 0);
    let terminal = &summary["terminal_states"]["agent-a"];
    assert_eq!(terminal["status"], "committed");
    let feedback_id = terminal["feedback_id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .expect("canonical feedback identity missing");
    println!("provider_terminal_feedback_id={feedback_id}");
    let view = client
        .read_view(ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: 1,
            world: client.config().expected_world.clone(),
            scope_id: client.config().scope_id.clone(),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        })
        .unwrap();
    assert_eq!(
        view.projection().state.agents["agent-a"].state.pos,
        oasis7_agent_api::world_action::GeoPos::new(2, 2, 0)
    );
    assert!(
        view.projection()
            .cognition_leases
            .iter()
            .any(
                |lease| lease.status == oasis7_runtime::CognitionLeaseStatusV1::Settled
                    && lease.settled_amount > 0
            ),
        "canonical lease settlement missing"
    );
    client
        .read_changes(ReadWorldChangesRequest {
            contract_version: 1,
            cursor: view.continuation().clone(),
            max_items: 32,
            max_bytes: 65_536,
        })
        .unwrap();
    println!(
        "PRE2_APPLICATION_PROVIDER_ENQUEUE_RECEIPT_SETTLEMENT_MEMORY_PASSED wake_case=not_run"
    );
}
