//! Actual canonical Release ambiguity and original Lookup recovery, through real provider helpers.
use super::*;
#[test]
fn real_tcp_application_release_lost_ack_reconciles_original_checkpoint() {
    super::run_isolated_application(false, false, true, false, false, false);
}
pub(super) fn verify_release(public: &RemoteWorldServiceClient) {
    use oasis7::viewer::{
        ViewerLiveDecisionMode, ViewerRuntimeLiveServer, ViewerRuntimeLiveServerConfig,
    };
    let mut connection = public.config().clone();
    connection.scope_id = "agent:agent-a".into();
    let client = RemoteWorldServiceClient::new(connection.clone()).unwrap();
    let mut config = ViewerRuntimeLiveServerConfig::new(oasis7_simulator::WorldScenario::Minimal);
    config.world_id = "w1".into();
    config.world_service = Some(connection);
    config.provider_lineage_store = Some(
        std::env::current_dir()
            .unwrap()
            .join("release-private-lineage.json"),
    );
    config.world_service_agent_signer = Some(
        oasis7::world_service::client::WorldServiceAgentSignerConfig {
            private_key_hex: hex::encode([8u8; 32]),
            delegation_generation: 1,
        },
    );
    config.decision_mode = ViewerLiveDecisionMode::Llm;
    let mut server = ViewerRuntimeLiveServer::new(config).unwrap();
    let original = server
        .test_prepare_canonical_provider_response(
            "agent-a",
            oasis7_simulator::Action::MoveAgent {
                agent_id: "agent-a".into(),
                to: "runtime:2:2:0".into(),
            },
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let reserved = loop {
        match server.test_reserve_canonical_provider_context(original.clone()) {
            Ok(value) => break value,
            Err(error)
                if error.contains("pending")
                    || error.contains("unknown")
                    || error.contains("unresolved") => {}
            Err(error) => panic!("real reserve before Release: {error}"),
        }
        assert!(Instant::now() < deadline, "actual reserve timeout");
        thread::sleep(Duration::from_millis(10));
    };
    let lease_id = reserved["cognition_lease"]["lease_id"]
        .as_str()
        .unwrap()
        .to_string();
    let request_digest = reserved["request"]["request_context"]["request_digest"]
        .as_str()
        .unwrap()
        .to_string();
    let error = server
        .test_release_canonical_provider_context(reserved.clone())
        .unwrap_err();
    assert!(
        error.contains("unknown") || error.contains("pending") || error.contains("retained"),
        "unexpected Release ambiguity: {error}"
    );
    let lineage: serde_json::Value =
        serde_json::from_slice(&fs::read("release-private-lineage.json").unwrap()).unwrap();
    let originals: Vec<SubmitIntentRequest<WorldServicePayloadV1>> = lineage["provider_scheduler_pending"]
        .as_object().unwrap().values().filter_map(|pending| {
            let payload: WorldServicePayloadV1 = serde_json::from_value(pending["payload"].clone()).unwrap();
            if !matches!(&payload, WorldServicePayloadV1::Scheduler(signed)
                if matches!(&signed.request.operation, SchedulerOperationV1::ReleaseLease { lease_id: original } if original == &lease_id)) {
                return None;
            }
            Some(SubmitIntentRequest { contract_version: 1,
                correlation: serde_json::from_value(pending["correlation"].clone()).unwrap(),
                deadline_unix_ms: None, signed_payload: payload })
        }).collect();
    assert_eq!(
        originals.len(),
        1,
        "one durable original Release checkpoint required"
    );
    let original_release = &originals[0];
    assert_eq!(
        derive_correlation(
            original_release.correlation.key.world.clone(),
            &original_release.signed_payload
        )
        .unwrap(),
        original_release.correlation
    );
    let release_deadline = Instant::now() + Duration::from_secs(10);
    let release_commit = loop {
        assert!(
            Instant::now() < release_deadline,
            "original Release Lookup timeout"
        );
        let response = client
            .lookup(
                LookupIntentRequest {
                    contract_version: 1,
                    key: original_release.correlation.key.clone(),
                },
                original_release.signed_payload.clone(),
            )
            .unwrap();
        assert!(
            Instant::now() < release_deadline,
            "original Release response arrived after deadline"
        );
        response.validate(&original_release.correlation).unwrap();
        match response.outcome {
            IntentOutcome::Committed { commit, receipt } => {
                let receipt: oasis7_runtime::CognitionReceiptV1 =
                    serde_json::from_value(receipt).unwrap();
                receipt.validate().unwrap();
                assert_eq!(receipt.operation, "release");
                assert_eq!(
                    receipt.status,
                    oasis7_runtime::CognitionLeaseStatusV1::Released
                );
                assert_eq!(receipt.lease_id, lease_id);
                let expected = &reserved["cognition_lease"];
                let actual = serde_json::to_value(&receipt).unwrap();
                for field in [
                    "agent_id",
                    "agent_session_id",
                    "agent_turn_id",
                    "decision_request_id",
                    "request_digest",
                    "account_id",
                    "idempotency_key",
                    "quote",
                    "reserved_amount",
                ] {
                    assert_eq!(
                        actual[field], expected[field],
                        "original Release receipt identity {field}"
                    );
                }
                assert_eq!(receipt.released_amount, receipt.reserved_amount);
                assert_eq!(receipt.consumed_amount, 0);
                assert_eq!(receipt.refunded_amount, 0);
                assert_eq!(receipt.net_amount, 0);
                break commit;
            }
            IntentOutcome::Unknown | IntentOutcome::Received { .. } | IntentOutcome::Pending => {
                thread::sleep(Duration::from_millis(10))
            }
            other => panic!("original Release failed: {other:?}"),
        }
    };
    let view_request = ReadWorldViewRequest {
        observation_nonce: None,
        formal_anchor: None,
        contract_version: 1,
        world: client.config().expected_world.clone(),
        scope_id: client.config().scope_id.clone(),
        min_commit: Some(*release_commit),
        fixed_commit: None,
        deadline_unix_ms: None,
    };
    let observed = client.read_view(view_request.clone()).unwrap();
    assert!(
        observed
            .projection()
            .cognition_leases
            .iter()
            .any(|lease| lease.lease_id == lease_id
                && lease.status == oasis7_runtime::CognitionLeaseStatusV1::Released)
    );
    let before = server.test_canonical_provider_summary();
    assert!(
        before["mirrored_lease_identities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|mirror| mirror["lease_id"] == lease_id)
    );
    println!(
        "release_ambiguous_checkpoint lease_id={lease_id} request_digest={request_digest} canonical_view_released=true mirror_retained=true"
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut last_error = String::new();
    loop {
        match server.test_poll_canonical_provider_response() {
            Ok(()) => last_error.clear(),
            Err(error) => last_error = error,
        }
        let summary = server.test_canonical_provider_summary();
        let retained = summary["mirrored_lease_identities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|mirror| mirror["lease_id"] == lease_id);
        if !retained && last_error.is_empty() {
            assert_eq!(summary["pending_intent_count"], 0);
            println!(
                "PRE2_RELEASE_LOST_ACK_ORIGINAL_LOOKUP_RECOVERY_PASSED lease_id={lease_id} request_digest={request_digest}"
            );
            return;
        }
        assert!(
            Instant::now() < deadline,
            "original Release checkpoint did not reconcile; mirror_retained={retained}; last_error={last_error}; pending_intents={}",
            summary["pending_intent_count"]
        );
        thread::sleep(Duration::from_millis(20));
    }
}
pub(super) fn validate_output(fixture: &Fixture, output: &std::process::Output) {
    assert!(
        output.status.success(),
        "Release application failed: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let world = fixture.driver.lock().unwrap().execution_world.clone();
    let releases=world.capability_revocation_state().world_service_results.iter()
        .filter_map(|(key,value)|serde_json::from_value::<wire::CanonicalIntentResultV1>(value.clone()).ok().map(|result|(key.clone(),result)))
        .filter(|(_,result)|matches!(&result.request.signed_payload,WorldServicePayloadV1::Scheduler(signed) if matches!(signed.request.operation,SchedulerOperationV1::ReleaseLease {..}))).collect::<Vec<_>>();
    assert_eq!(releases.len(), 1, "one real canonical Release expected");
    let (key, result) = &releases[0];
    assert!(result.rejected.is_none());
    let observations = fixture.lookup_digests.lock().unwrap();
    let submits = observations
        .iter()
        .filter(|digest| **digest == format!("submit:{key}"))
        .count();
    let lookups = observations.iter().filter(|digest| *digest == key).count();
    println!(
        "release_original_identity_witness correlation_digest={key} submit_count={submits} lookup_count={lookups} canonical_result_count=1 rejected=false"
    );
    assert_eq!(submits, 1, "ambiguous Release must not be resubmitted");
    assert!(
        lookups > 0,
        "original Release must reconcile by actual Lookup, not signed View alone"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("PRE2_RELEASE_LOST_ACK_ORIGINAL_LOOKUP_RECOVERY_PASSED"));
    println!("{stdout}");
}
