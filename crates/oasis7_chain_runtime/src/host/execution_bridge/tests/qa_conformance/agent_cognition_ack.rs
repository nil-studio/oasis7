use super::*;

pub(super) fn run() {
    let fixture = Fixture::with_controlled_commits(true);
    fixture.client.describe().unwrap();
    let registration = fixture.delegation();
    fixture.client.submit(registration.clone()).unwrap();
    commit_request(
        &mut fixture.driver.lock().unwrap(),
        2,
        Some(registration.clone()),
    );
    fixture.committed(&registration);
    let original = cognition_request(&fixture);
    fixture.client.submit(original.clone()).unwrap();
    commit_request(
        &mut fixture.driver.lock().unwrap(),
        3,
        Some(original.clone()),
    );
    let commit = fixture.committed(&original);
    let outcome = fixture
        .client
        .lookup(
            LookupIntentRequest {
                contract_version: 1,
                key: original.correlation.key.clone(),
            },
            original.signed_payload.clone(),
        )
        .unwrap();
    let IntentOutcome::Committed { receipt, .. } = outcome.outcome else {
        panic!("missing cognition receipt")
    };
    assert!(receipt.get("commit_record").is_some());
    assert!(receipt.get("lineage").is_some());
    assert!(receipt.get("feedback").is_some());
    let canonical = fixture.driver.lock().unwrap().execution_world.clone();
    let decision = &canonical.cognition()["agent_delegation"]["decisions"]["qa-cognition"];
    assert_eq!(decision["context"]["reason"], "move towards goal");
    assert_eq!(
        decision["context"]["correction_refs"],
        serde_json::json!(["correction:qa-public"])
    );
    let causal = canonical.agent_causal_receipts("agent-a").unwrap();
    assert_eq!(causal.len(), 1);
    assert_eq!(causal[0].correction_refs, vec!["correction:qa-public"]);
    assert_eq!(causal[0].dissent.as_deref(), Some("waiting is safer"));
    assert_eq!(
        causal[0].receipt_id,
        receipt["lineage"]["receipt_id"].as_str().unwrap()
    );
    let before = canonical.cognition().clone();
    let WorldServicePayloadV1::Cognition(signed) = original.signed_payload.clone() else {
        unreachable!()
    };
    let mut retry = canonical.clone();
    assert_eq!(
        retry
            .commit_authenticated_cognition(signed.clone())
            .unwrap(),
        receipt
    );
    assert_eq!(retry.cognition(), &before);
    let mut changed = signed.request.clone();
    changed.causal_proposal.as_mut().unwrap().reason = Some("different explanation".into());
    let changed = sign_read_request("cognition", changed, &hex::encode([8u8; 32])).unwrap();
    assert!(
        retry
            .commit_authenticated_cognition(changed)
            .unwrap_err()
            .contains("causal proposal idempotency conflict")
    );
    assert_eq!(retry.cognition(), &before);
    // Legacy absence must remain absent after typed decoding: no signature-
    // breaking null field is introduced for older signed requests.
    let mut legacy = signed.request.clone();
    legacy.causal_proposal = None;
    let legacy_json = serde_json::to_value(&legacy).unwrap();
    assert!(legacy_json.get("causal_proposal").is_none());
    let decoded: CognitionIntentV1 = serde_json::from_value(legacy_json.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), legacy_json);
    let legacy_signed = sign_read_request("cognition", legacy, &hex::encode([8u8; 32])).unwrap();
    let roundtrip: SignedReadRequest<CognitionIntentV1> =
        serde_json::from_value(serde_json::to_value(legacy_signed).unwrap()).unwrap();
    oasis7::world_service::authority::verify_read_request("cognition", &roundtrip).unwrap();
    let mut forged_context = serde_json::to_value(signed.request.causal_proposal).unwrap();
    forged_context["override_actor"] = serde_json::json!("owner");
    assert!(serde_json::from_value::<CognitionCausalProposalV1>(forged_context).is_err());

    let original_record = canonical
        .runtime_feedback_outbox()
        .unwrap()
        .into_iter()
        .find(|r| r.feedback_id == receipt["lineage"]["feedback_id"].as_str().unwrap())
        .unwrap();
    let ack = FeedbackAckIntentV1 {
        agent_id: original_record.agent_subject.clone(),
        agent_session_id: original_record.agent_session_id.clone(),
        agent_turn_id: original_record.agent_turn_id.clone(),
        decision_request_id: original_record.decision_request_id.clone(),
        request_digest: original_record.request_digest.clone(),
        feedback_id: original_record.feedback_id.clone(),
        feedback_seq: original_record.feedback_seq,
        original_envelope_digest: original_record.envelope_digest.clone(),
        // This core test signs an opaque Agent declaration. Actual native
        // private-memory acceptance ordering is tested by the App path.
        private_acceptance_digest: oasis7::simulator::h_v1(
            "oasis7.qa.signed-acceptance-statement.v1",
            &"ack core declaration",
        )
        .to_string(),
        runtime_receipt_id: Some(causal[0].receipt_id.clone()),
        delegation_generation: 1,
    };
    let sign_ack =
        |value| sign_read_request("feedback_ack", value, &hex::encode([8u8; 32])).unwrap();
    let signed_ack = sign_ack(ack.clone());
    let mut runtime_ack = canonical.clone();
    let before_native_ack = serde_json::to_value(canonical.snapshot()).unwrap();
    let mut wrong_digest = ack.clone();
    wrong_digest.original_envelope_digest = format!("blake3:{}", "0".repeat(64));
    assert!(
        runtime_ack
            .apply_authenticated_feedback_ack(sign_ack(wrong_digest.clone()))
            .unwrap_err()
            .contains("identity mismatch")
    );
    assert_eq!(runtime_ack.cognition(), &before);
    assert_eq!(
        serde_json::to_value(runtime_ack.snapshot()).unwrap(),
        before_native_ack
    );
    let mut wrong_audience = ack.clone();
    wrong_audience.agent_session_id = "another-session".into();
    assert!(
        runtime_ack
            .apply_authenticated_feedback_ack(sign_ack(wrong_audience))
            .unwrap_err()
            .contains("identity mismatch")
    );
    assert_eq!(runtime_ack.cognition(), &before);
    assert_eq!(
        serde_json::to_value(runtime_ack.snapshot()).unwrap(),
        before_native_ack
    );
    let wrong_signer =
        sign_read_request("feedback_ack", ack.clone(), &hex::encode([9u8; 32])).unwrap();
    assert!(
        runtime_ack
            .apply_authenticated_feedback_ack(wrong_signer)
            .is_err()
    );
    assert_eq!(runtime_ack.cognition(), &before);
    assert_eq!(
        serde_json::to_value(runtime_ack.snapshot()).unwrap(),
        before_native_ack
    );
    let mut tampered_signature = signed_ack.clone();
    tampered_signature.request.feedback_seq += 1;
    assert!(
        runtime_ack
            .apply_authenticated_feedback_ack(tampered_signature)
            .is_err()
    );
    assert_eq!(runtime_ack.cognition(), &before);
    assert_eq!(
        serde_json::to_value(runtime_ack.snapshot()).unwrap(),
        before_native_ack
    );
    let native_result = runtime_ack
        .apply_authenticated_feedback_ack(signed_ack.clone())
        .unwrap();
    let acked = runtime_ack.cognition().clone();
    assert_eq!(
        runtime_ack.runtime_feedback_outbox().unwrap()[0].state,
        "acked"
    );
    assert_eq!(
        runtime_ack
            .apply_authenticated_feedback_ack(signed_ack.clone())
            .unwrap(),
        native_result
    );
    assert_eq!(runtime_ack.cognition(), &acked);
    let WorldServicePayloadV1::Cognition(original_cognition) = original.signed_payload.clone()
    else {
        unreachable!()
    };
    assert_eq!(
        runtime_ack
            .commit_authenticated_cognition(original_cognition)
            .unwrap(),
        receipt
    );
    assert_eq!(runtime_ack.cognition(), &acked);

    let invalid_ack = fixture.request(WorldServicePayloadV1::FeedbackAck(sign_ack(wrong_digest)));
    assert!(fixture.client.submit(invalid_ack).is_err());
    let ack_request = fixture.request(WorldServicePayloadV1::FeedbackAck(signed_ack));
    fixture.client.submit(ack_request.clone()).unwrap();
    commit_request(
        &mut fixture.driver.lock().unwrap(),
        4,
        Some(ack_request.clone()),
    );
    let ack_commit = fixture.committed(&ack_request);
    let ack_outcome = fixture
        .client
        .lookup(
            LookupIntentRequest {
                contract_version: 1,
                key: ack_request.correlation.key.clone(),
            },
            ack_request.signed_payload.clone(),
        )
        .unwrap();
    let IntentOutcome::Committed {
        receipt: ack_receipt,
        ..
    } = ack_outcome.outcome
    else {
        panic!("missing canonical acknowledgement receipt")
    };
    assert_eq!(ack_receipt, native_result);
    fixture.client.submit(ack_request.clone()).unwrap();
    let retry_outcome = fixture
        .client
        .lookup(
            LookupIntentRequest {
                contract_version: 1,
                key: ack_request.correlation.key.clone(),
            },
            ack_request.signed_payload.clone(),
        )
        .unwrap();
    let IntentOutcome::Committed {
        receipt: retry_receipt,
        ..
    } = retry_outcome.outcome
    else {
        panic!("missing original acknowledgement retry receipt")
    };
    assert_eq!(retry_receipt, ack_receipt);
    // Changing a signed acceptance statement cannot resolve to the original
    // published receipt under the same feedback correlation key.
    let mut changed_acceptance = ack.clone();
    changed_acceptance.private_acceptance_digest = oasis7::simulator::h_v1(
        "oasis7.qa.signed-acceptance-statement.v1",
        &"different declaration",
    )
    .to_string();
    let changed_request = fixture.request(WorldServicePayloadV1::FeedbackAck(sign_ack(
        changed_acceptance,
    )));
    let before_conflicting_ack =
        serde_json::to_value(fixture.driver.lock().unwrap().execution_world.snapshot()).unwrap();
    assert!(fixture.client.submit(changed_request.clone()).is_err());
    assert!(
        fixture
            .client
            .lookup(
                LookupIntentRequest {
                    contract_version: 1,
                    key: changed_request.correlation.key.clone()
                },
                changed_request.signed_payload
            )
            .is_err()
    );
    assert_eq!(
        serde_json::to_value(fixture.driver.lock().unwrap().execution_world.snapshot()).unwrap(),
        before_conflicting_ack
    );

    let mut scoped_config = fixture.client.config().clone();
    scoped_config.scope_id = "agent:agent-a".into();
    let scoped_client = RemoteWorldServiceClient::new(scoped_config).unwrap();
    let mut scoped_request = fixture.view(Some(ack_commit));
    scoped_request.scope_id = "agent:agent-a".into();
    let ack_view = scoped_client.read_view(scoped_request).unwrap();
    let projection = ack_view.projection();
    assert_eq!(
        projection.feedback_history.as_ref().unwrap().records[0].delivery_state,
        "acked"
    );
    let view = fixture
        .client
        .read_view(fixture.view(Some(commit)))
        .unwrap();
    fixture
        .client
        .read_changes(ReadWorldChangesRequest {
            contract_version: 1,
            cursor: view.continuation().clone(),
            max_items: 32,
            max_bytes: 65_536,
        })
        .unwrap();
    let WorldServicePayloadV1::Delegation(delegation) = registration.signed_payload else {
        unreachable!()
    };
    let mut revoked = delegation.request;
    revoked.revoked = true;
    revoked.generation = 2;
    revoked.nonce = 2;
    let revocation = fixture.request(WorldServicePayloadV1::Delegation(
        sign_read_request("delegation", revoked, &fixture.owner).unwrap(),
    ));
    fixture.client.submit(revocation.clone()).unwrap();
    commit_request(
        &mut fixture.driver.lock().unwrap(),
        5,
        Some(revocation.clone()),
    );
    fixture.committed(&revocation);
    let after_revocation = fixture.driver.lock().unwrap().execution_world.snapshot();
    assert!(fixture.client.submit(ack_request.clone()).is_err());
    assert!(
        fixture
            .client
            .lookup(
                LookupIntentRequest {
                    contract_version: 1,
                    key: ack_request.correlation.key.clone()
                },
                ack_request.signed_payload
            )
            .is_err()
    );
    assert_eq!(
        serde_json::to_value(fixture.driver.lock().unwrap().execution_world.snapshot()).unwrap(),
        serde_json::to_value(after_revocation).unwrap()
    );
}
