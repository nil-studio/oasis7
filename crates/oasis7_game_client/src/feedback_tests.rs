use super::*;
use crate::operation_store::OperationStore;
use serde_json::json;
fn view(id: &str, seq: u64) -> VerifiedTaskWorldView {
    let (request, mut response) = crate::verified_task_view::tests::fixture("agent:a");
    let digest = format!("blake3:{}", "1".repeat(64));
    response.view["feedback_history"] = json!({"agent_id":"a","records":[{
        "feedback":{"feedback_id":id,"feedback_seq":seq,"agent_subject":"a",
        "agent_session_id":"session","agent_turn_id":"turn","decision_request_id":"request",
        "candidate_action_id":7,"runtime_receipt_id":"receipt","status":"committed",
        "request_digest":digest,"reject_reason":null,"provenance":"runtime_authoritative"},
        "original_envelope_digest":digest,"delivery_state":"pending",
        "receipt_lineage":{"schema_version":"runtime-receipt-lineage.v1","status":"committed",
        "receipt_id":"receipt","receipt_digest":"receipt-digest","envelope_digest":"envelope-digest",
        "action_id":"action:7","agent_id":"a","agent_session_id":"session","agent_turn_id":"turn",
        "decision_request_id":"request","request_digest":digest,"feedback_id":id}}]});
    VerifiedTaskWorldView::new(response, &request).unwrap()
}
#[test]
#[cfg(unix)]
fn persisted_acceptance_retains_original_ack_across_restart_and_identity_conflicts() {
    let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "oasis7-feedback-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let key = hex::encode([71; 32]);
    let public = sign_read_request("key", (), &key)
        .unwrap()
        .subject_public_key;
    let world = WorldIdentity {
        world_id: "world".into(),
        genesis_digest: "genesis".into(),
    };
    let client = || Client {
        authority_mode: None,
        remote: crate::WorldServiceTaskClient::new(crate::WorldServiceClientConfig {
            endpoint: "http://127.0.0.1:1".into(),
            trusted_service_public_key: public.clone(),
            expected_world: world.clone(),
            scope_id: "agent:a".into(),
            read_private_key_hex: key.clone(),
            timeout: std::time::Duration::from_millis(10),
            max_response_bytes: 65536,
        })
        .unwrap(),
        world: world.clone(),
        agent: "a".into(),
        public_key: public.clone(),
        key_hex: key.clone().into(),
        #[cfg(unix)]
        gateway: None,
        state: OperationStore::open(&root, [19; 32]).unwrap(),
    };
    let digest = format!("blake3:{}", "2".repeat(64));
    let accepted_view = |id, seq| {
        view(id, seq).bind_read_authority(client().remote.read_authority_identity().unwrap())
    };
    let first = client();
    // A real signed HTTP read trusted by another configured service must not
    // cause this client's signer to accept or journal the feedback.
    let (read_request, mut response) = crate::verified_task_view::tests::fixture("agent:a");
    response.view = view("foreign-service-feedback", 1).raw_projection().clone();
    let (endpoint, server) = crate::tests::serve_payload(
        VIEW_JSON_PATH,
        serde_json::to_value(response).unwrap(),
        |_| {},
    );
    let other_remote = crate::WorldServiceTaskClient::new(crate::tests::config(endpoint)).unwrap();
    let other_service_view = other_remote.read_view(read_request).unwrap();
    server.join().unwrap();
    assert!(
        first
            .record_caller_persisted_feedback_acceptance(&other_service_view, 0, &digest, 0)
            .unwrap_err()
            .contains("read authority")
    );
    assert!(
        first
            .recover_caller_persisted_feedback_ack("foreign-service-feedback")
            .is_err()
    );
    assert!(first.recover_caller_persisted_feedback_ack("f1").is_err());
    assert!(
        first
            .record_caller_persisted_feedback_acceptance(&accepted_view("f1", 1), 0, "invalid", 0)
            .is_err()
    );
    assert!(first.recover_caller_persisted_feedback_ack("f1").is_err());
    let original = first
        .record_caller_persisted_feedback_acceptance(&accepted_view("f1", 1), 0, &digest, 0)
        .unwrap();
    drop(first);
    let restored = client();
    assert_eq!(
        restored
            .recover_caller_persisted_feedback_ack("f1")
            .unwrap(),
        original
    );
    assert_eq!(
        restored
            .record_caller_persisted_feedback_acceptance(&accepted_view("f1", 1), 0, &digest, 0)
            .unwrap(),
        original
    );
    assert!(
        restored
            .record_caller_persisted_feedback_acceptance(&accepted_view("f2", 1), 0, &digest, 0)
            .is_err()
    );
    assert!(
        restored
            .record_caller_persisted_feedback_acceptance(&accepted_view("f1", 2), 0, &digest, 0)
            .is_err()
    );
    assert_eq!(
        restored
            .recover_caller_persisted_feedback_ack("f1")
            .unwrap(),
        original
    );
    let unknown = IntentResponse::<Value> {
        contract_version: 1,
        formal_profile: None,
        execution_evidence_scope: None,
        correlation: original.correlation.clone(),
        outcome: IntentOutcome::Unknown,
    };
    let (endpoint, server) =
        crate::tests::serve_payload(LOOKUP_PATH, serde_json::to_value(unknown).unwrap(), |_| {});
    let query_remote = crate::WorldServiceTaskClient::new(crate::tests::config(endpoint)).unwrap();
    assert!(matches!(
        query_remote
            .lookup(
                LookupIntentRequest {
                    contract_version: 1,
                    key: original.correlation.key.clone()
                },
                original.signed_payload.clone()
            )
            .unwrap()
            .outcome,
        IntentOutcome::Unknown
    ));
    server.join().unwrap();
    assert_eq!(
        restored
            .recover_caller_persisted_feedback_ack("f1")
            .unwrap(),
        original
    );
    // Receiving a later sequence alone does not automatically accept or ACK it.
    let later = view("f3", 3);
    assert_eq!(
        later.projection().feedback_history[0].feedback.feedback_seq,
        3
    );
    assert!(
        restored
            .recover_caller_persisted_feedback_ack("f3")
            .is_err()
    );
    let seq = restored
        .feedback_index("local-feedback-sequence", ("session", 1_u64))
        .unwrap();
    let name = crate::operation_store::client_state_name(
        &[19; 32],
        ClientStateKind::FeedbackAcceptance,
        &seq,
    )
    .unwrap();
    let path = root.join(name);
    let bytes = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(
        restored
            .recover_caller_persisted_feedback_ack("f1")
            .is_err()
    );
    assert!(
        restored
            .acknowledge_caller_persisted_feedback("f1")
            .unwrap_err()
            .contains("sequence acceptance")
    );
    std::fs::write(&path, b"corrupt").unwrap();
    assert!(
        restored
            .recover_caller_persisted_feedback_ack("f1")
            .is_err()
    );
    std::fs::remove_file(&path).unwrap();
    std::fs::write(&path, bytes).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        restored
            .recover_caller_persisted_feedback_ack("f1")
            .unwrap(),
        original
    );
    drop(restored);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn scoped_feedback_projection_rejects_invalid_metadata_and_identity_collisions() {
    let original = view("f1", 1);
    for field in [
        "digest",
        "delivery",
        "id_collision",
        "sequence_collision",
        "scope",
    ] {
        let (request, mut response) = crate::verified_task_view::tests::fixture("agent:a");
        response.view = original.raw_projection().clone();
        let records = response.view["feedback_history"]["records"]
            .as_array_mut()
            .unwrap();
        match field {
            "digest" => records[0]["original_envelope_digest"] = json!("not-a-digest"),
            "delivery" => records[0]["delivery_state"] = json!("invented"),
            "scope" => records[0]["feedback"]["agent_subject"] = json!("other"),
            "id_collision" | "sequence_collision" => {
                let mut changed = records[0].clone();
                if field == "id_collision" {
                    changed["feedback"]["feedback_seq"] = json!(2);
                } else {
                    changed["feedback"]["feedback_id"] = json!("other");
                    changed["receipt_lineage"]["feedback_id"] = json!("other");
                }
                records.push(changed);
            }
            _ => unreachable!(),
        }
        assert!(
            VerifiedTaskWorldView::new(response, &request).is_err(),
            "{field}"
        );
    }
}
