//! Genuine roots/claim and real Rust H PoP. Component proof only, not physical
//! Worker routing/normal-clock/Native or full139 qualification.
use super::*;
fn metadata(
    f: &SessionFixture,
    purpose: SessionPurposeV1,
    now: u64,
) -> (Vec<u8>, HttpPossessionReceipt) {
    let q = SessionChallengeRequestV1 {
        api_version: 1,
        session_id: f.session_id.clone(),
        request_id: "execution-five-metadata".into(),
        purpose,
        kind: SessionChallengeKindV1::Tool,
        logical_request_id: "execution-five-action".into(),
        operation_request_digest: "a5".repeat(32),
    };
    let body = serde_json::to_vec(&q).unwrap();
    let http = signed_http(
        f,
        &body,
        &q.request_id,
        (
            HttpPopRoute::ProofChallenges,
            HttpPopProfileV1::CustodyChallenge,
        ),
        None,
        purpose,
        now,
    );
    let context = SessionClientHttpContext {
        public_origin: &f.fact.registration.public_origin,
        custody_account_id: &f.state.draft().entry.custody_account_id,
        provision_id: &f.state.draft().entry.provision_id,
        operation: SessionClientOperation::ToolNonce,
        representation: HttpHeaderRepresentation::RawInstances,
    };
    let prepared = f
        .state
        .prepare_test_session_v2_tool_nonce_http(&http, &context, UnixMillis::new(now))
        .unwrap();
    let proof = verify_http_possession(prepared, UnixMillis::new(now), &mut StrictCrypto).unwrap();
    (body, proof)
}
fn remaining(f: &SessionFixture, purpose: SessionPurposeV1) -> u64 {
    f.state
        .test_session_v2_snapshot(&f.session_id)
        .unwrap()
        .roots()[&purpose]
        .budgets
        .transport
        .remaining()
        .unwrap()
}
#[test]
fn session_v2_execution_nonce_five_actual_pop_charge_issue_and_cold_no_book() {
    let mut f = claimed_execution();
    let (body, proof) = metadata(&f, SessionPurposeV1::Execution, 13_000);
    let request = || SessionProofRequest {
        ingress_attempt_id: "execution-five-ingress",
        original_bytes: &body,
        possession: &proof,
    };
    let before = remaining(&f, SessionPurposeV1::Execution);
    let read = remaining(&f, SessionPurposeV1::ReconcileRead);
    f.state
        .charge_test_session_v2_execution_nonce_five(request(), UnixMillis::new(13_000))
        .unwrap();
    assert_eq!(remaining(&f, SessionPurposeV1::Execution), before - 5);
    assert_eq!(remaining(&f, SessionPurposeV1::ReconcileRead), read);
    let paid = f.state.clone();
    f.state
        .charge_test_session_v2_execution_nonce_five(request(), UnixMillis::new(13_001))
        .unwrap();
    assert_eq!(f.state, paid);
    assert!(
        f.state
            .issue_test_session_v2_tool_nonce(
                request(),
                &URL_SAFE_NO_PAD.encode([91; 32]),
                UnixMillis::new(13_001)
            )
            .is_err()
    );
    assert_eq!(
        f.state, paid,
        "legacy Three issuer cannot consume Five payment"
    );
    let nonce = URL_SAFE_NO_PAD.encode([91; 32]);
    let q = f
        .state
        .issue_test_session_v2_execution_nonce_five(request(), &nonce, UnixMillis::new(13_001))
        .unwrap();
    assert_eq!(q.kind, SessionChallengeKindV1::ToolMetadata);
    assert_eq!(
        q.expires_at.get(),
        (13_001 + 30_000).min(
            f.state
                .test_session_v2_snapshot(&f.session_id)
                .unwrap()
                .roots()[&SessionPurposeV1::Execution]
                .deadline
                .get()
        )
    );
    let json = serde_json::to_value(&f.state).unwrap();
    let row = &json["test_sessions_v2"][&f.session_id];
    assert!(row["execution_book"].is_null());
    assert!(row["actions"].as_object().unwrap().is_empty());
    assert_eq!(row["purpose_transports"].as_object().unwrap().len(), 4);
    let class = &row["purpose_http_admissions"]["execution-five-ingress"]["class"];
    assert_eq!(class["kind"], "execution_nonce_five_v2");
    let restored: AccountProvisionApprovalState = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(restored, f.state);
    let mut missing = json.clone();
    let leaf = class["provider_abuse_attempt"].as_str().unwrap();
    missing["test_sessions_v2"][&f.session_id]["purpose_transports"]
        .as_object_mut()
        .unwrap()
        .remove(leaf);
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(missing).is_err());
    let mut downgrade = json;
    let c = &mut downgrade["test_sessions_v2"][&f.session_id]["purpose_http_admissions"]["execution-five-ingress"]
        ["class"];
    c["kind"] = serde_json::json!("tool_nonce_three");
    c.as_object_mut().unwrap().remove("provider_abuse_attempt");
    c.as_object_mut().unwrap().remove("realm_abuse_attempt");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(downgrade).is_err());
}
#[test]
fn session_v2_execution_nonce_five_failure_preserves_paid_and_rejects_read_cross_lane() {
    let mut f = claimed_execution();
    let (body, proof) = metadata(&f, SessionPurposeV1::Execution, 13_000);
    let request = || SessionProofRequest {
        ingress_attempt_id: "execution-five-ingress",
        original_bytes: &body,
        possession: &proof,
    };
    let before = remaining(&f, SessionPurposeV1::Execution);
    f.state
        .charge_test_session_v2_execution_nonce_five(request(), UnixMillis::new(13_000))
        .unwrap();
    let paid = f.state.clone();
    assert!(
        f.state
            .issue_test_session_v2_execution_nonce_five(
                request(),
                "bad-nonce",
                UnixMillis::new(13_001)
            )
            .is_err()
    );
    assert_eq!(f.state, paid);
    assert_eq!(remaining(&f, SessionPurposeV1::Execution), before - 5);
    let mut read = claimed_execution();
    let (body, proof) = metadata(&read, SessionPurposeV1::ReconcileRead, 13_000);
    let before = read.state.clone();
    assert!(
        read.state
            .charge_test_session_v2_execution_nonce_five(
                SessionProofRequest {
                    ingress_attempt_id: "read-not-execution-five",
                    original_bytes: &body,
                    possession: &proof
                },
                UnixMillis::new(13_000)
            )
            .is_err()
    );
    assert_eq!(read.state, before);
}
