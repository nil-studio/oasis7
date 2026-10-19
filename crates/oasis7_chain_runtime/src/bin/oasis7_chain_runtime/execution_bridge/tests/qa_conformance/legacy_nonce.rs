//! A real legacy commit consumes a nonce without creating a service outcome index.
use super::*;

pub(super) fn legacy_seed(
    commit: bool,
) -> (Fixture, SubmitIntentRequest<WorldServicePayloadV1>, String) {
    use oasis7_viewer_runtime::viewer::{
        CollectDataCommand, CollectDataRequest, sign_collect_data_auth_proof,
    };
    let fixture = Fixture::with_controlled_commits(true);
    let public = sign_read_request("owner", (), &fixture.owner)
        .unwrap()
        .subject_public_key;
    let mut command = CollectDataCommand::Submit {
        request: CollectDataRequest {
            electricity_cost: 7,
            data_amount: 11,
            player_id: "owner-a".into(),
            public_key: Some(public.clone()),
            auth: None,
        },
    };
    let proof = sign_collect_data_auth_proof(&command, 901, &public, &fixture.owner).unwrap();
    let CollectDataCommand::Submit { request } = &mut command else {
        unreachable!()
    };
    request.auth = Some(proof);
    let payload = WorldServicePayloadV1::GameplayJson(serde_json::to_vec(&command).unwrap());
    let original = SubmitIntentRequest {
        contract_version: 1,
        correlation: derive_correlation(fixture.client.config().expected_world.clone(), &payload)
            .unwrap(),
        deadline_unix_ms: None,
        signed_payload: payload,
    };
    if commit {
        commit_legacy(&fixture, &original, &public);
    }
    (fixture, original, public)
}

fn commit_legacy(
    fixture: &Fixture,
    original: &SubmitIntentRequest<WorldServicePayloadV1>,
    public: &str,
) {
    use oasis7::consensus_action_payload::{
        ConsensusActionPayloadEnvelope, encode_consensus_action_payload,
    };
    let key = correlation::key_digest(&original.correlation.key).unwrap();
    {
        let mut driver = fixture.driver.lock().unwrap();
        let WorldServicePayloadV1::GameplayJson(bytes) = &original.signed_payload else {
            unreachable!()
        };
        let action =
            oasis7::world_service::gameplay::authenticated_action(&driver.execution_world, bytes)
                .unwrap();
        let bytes = encode_consensus_action_payload(
            &ConsensusActionPayloadEnvelope::from_runtime_action(action),
        )
        .unwrap();
        let actions = vec![NodeConsensusAction::from_payload(901, "node-a", bytes).unwrap()];
        let height = driver.state.last_applied_committed_height + 1;
        driver
            .on_commit(NodeExecutionCommitContext {
                world_id: "w1".into(),
                node_id: "node-a".into(),
                proposer_id: "node-a".into(),
                height,
                slot: height,
                epoch: 0,
                node_block_hash: format!("legacy-node-h{height}"),
                action_root: compute_consensus_action_root(&actions).unwrap(),
                committed_actions: actions,
                committed_at_unix_ms: height as i64 * 1000,
            })
            .unwrap();
        assert_eq!(
            driver
                .execution_world
                .state()
                .authenticated_collect_data_last_nonces["owner-a"][public],
            901
        );
        assert!(
            !driver
                .execution_world
                .capability_revocation_state()
                .world_service_results
                .contains_key(&key)
        );
    }
}

#[test]
fn real_tcp_consumed_legacy_nonce_without_service_index_is_unknown() {
    let (fixture, original, _) = legacy_seed(true);
    let key = correlation::key_digest(&original.correlation.key).unwrap();
    let height = fixture
        .driver
        .lock()
        .unwrap()
        .state
        .last_applied_committed_height;
    let outcome = fixture
        .client
        .lookup(
            LookupIntentRequest {
                contract_version: 1,
                key: original.correlation.key.clone(),
            },
            original.signed_payload,
        )
        .unwrap();
    assert!(
        matches!(outcome.outcome, IntentOutcome::Unknown),
        "nonce consumption cannot prove a service outcome: {:?}",
        outcome.outcome
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
    assert!(
        !fixture
            .lookup_digests
            .lock()
            .unwrap()
            .iter()
            .any(|value| value == &format!("submit:{key}"))
    );
    println!(
        "PRE2_LEGACY_NONCE_WITHOUT_INDEX_UNKNOWN_PASSED nonce_used=true original_lookup=true submit_count=0"
    );
}

#[test]
fn real_tcp_consumed_legacy_nonce_submit_preserves_history_unavailable() {
    let (fixture, original, _) = legacy_seed(true);
    fixture.node.lock().unwrap().stop().unwrap();
    let key = correlation::key_digest(&original.correlation.key).unwrap();
    let before = fixture
        .node
        .lock()
        .unwrap()
        .snapshot()
        .consensus
        .pending_consensus_actions
        .submit_buffer_action_count;
    let result = fixture.client.submit(original.clone());
    println!(
        "legacy_consumed_nonce_submit_transport_ok={}",
        result.is_ok()
    );
    let after = fixture
        .node
        .lock()
        .unwrap()
        .snapshot()
        .consensus
        .pending_consensus_actions
        .submit_buffer_action_count;
    assert_eq!(
        after, before,
        "consumed nonce without outcome history must not enter node queue"
    );
    assert!(
        !fixture
            .driver
            .lock()
            .unwrap()
            .execution_world
            .capability_revocation_state()
            .world_service_results
            .contains_key(&key)
    );
    let lookup = fixture
        .client
        .lookup(
            LookupIntentRequest {
                contract_version: 1,
                key: original.correlation.key,
            },
            original.signed_payload,
        )
        .unwrap();
    assert!(matches!(lookup.outcome, IntentOutcome::Unknown));
    assert!(
        matches!(result.unwrap(), SubmitObservation::Response(response) if matches!(response.outcome, IntentOutcome::HistoryUnavailable)),
        "Submit must explicitly retain history uncertainty"
    );
}

#[test]
fn real_tcp_legacy_nonce_execution_race_does_not_create_false_rejection_index() {
    let (fixture, original, public) = legacy_seed(false);
    fixture.client.submit(original.clone()).unwrap();
    commit_legacy(&fixture, &original, &public);
    commit_request(
        &mut fixture.driver.lock().unwrap(),
        0,
        Some(original.clone()),
    );
    let key = correlation::key_digest(&original.correlation.key).unwrap();
    assert!(
        !fixture
            .driver
            .lock()
            .unwrap()
            .execution_world
            .capability_revocation_state()
            .world_service_results
            .contains_key(&key),
        "execution race must preserve absent history instead of rejecting original uncertainty"
    );
    let lookup = fixture
        .client
        .lookup(
            LookupIntentRequest {
                contract_version: 1,
                key: original.correlation.key,
            },
            original.signed_payload,
        )
        .unwrap();
    assert!(matches!(lookup.outcome, IntentOutcome::Unknown));
}
