//! Actual Core Read PoP/fee/RNG-input/cold-reverse component only.
//! Worker five physical route counters and actual service RNG are separate.
use super::*;

fn read_metadata(f: &SessionFixture, now: u64) -> (Vec<u8>, HttpPossessionReceipt) {
    let action = SessionActionRequestV2 {
        api_version: 2,
        session_id: f.session_id.clone(),
        request_id: "read-five-action".into(),
        action: SessionToolActionV1::ReconcileOperation {
            operation_id: "exact-known-original-candidate".into(),
        },
    };
    let q = SessionChallengeRequestV1 {
        api_version: 1,
        session_id: f.session_id.clone(),
        request_id: "read-five-metadata".into(),
        purpose: SessionPurposeV1::ReconcileRead,
        kind: SessionChallengeKindV1::Tool,
        logical_request_id: action.request_id.clone(),
        operation_request_digest: action.digest().unwrap(),
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
        SessionPurposeV1::ReconcileRead,
        now,
    );
    let prepared = f
        .state
        .prepare_test_session_v2_tool_nonce_http(
            &http,
            &SessionClientHttpContext {
                public_origin: &f.fact.registration.public_origin,
                custody_account_id: &f.state.draft().entry.custody_account_id,
                provision_id: &f.state.draft().entry.provision_id,
                operation: SessionClientOperation::ToolNonce,
                representation: HttpHeaderRepresentation::RawInstances,
            },
            UnixMillis::new(now),
        )
        .unwrap();
    let proof = verify_http_possession(prepared, UnixMillis::new(now), &mut StrictCrypto).unwrap();
    (body, proof)
}

#[test]
fn session_v2_read_nonce_five_exact_paid_callback_new_ingress_cold_no_book() {
    let mut f = claimed_execution();
    let (body, proof) = read_metadata(&f, 13_000);
    let request = |ingress| SessionProofRequest {
        ingress_attempt_id: ingress,
        original_bytes: &body,
        possession: &proof,
    };
    let read = remaining(&f, SessionPurposeV1::ReconcileRead);
    let requests = f
        .state
        .test_session_v2_snapshot(&f.session_id)
        .unwrap()
        .roots()[&SessionPurposeV1::ReconcileRead]
        .budgets
        .requests
        .remaining()
        .unwrap();
    let execution = remaining(&f, SessionPurposeV1::Execution);
    f.state
        .charge_test_session_v2_read_nonce_five(request("read-five-first"), UnixMillis::new(13_000))
        .unwrap();
    assert_eq!(remaining(&f, SessionPurposeV1::ReconcileRead), read - 5);
    assert_eq!(remaining(&f, SessionPurposeV1::Execution), execution);
    let paid = f.state.clone();
    f.state
        .charge_test_session_v2_read_nonce_five(request("read-five-first"), UnixMillis::new(13_001))
        .unwrap();
    assert_eq!(f.state, paid);
    let nonce = URL_SAFE_NO_PAD.encode([92; 32]);
    assert!(
        f.state
            .issue_test_session_v2_execution_nonce_five(
                request("read-five-first"),
                &nonce,
                UnixMillis::new(13_001)
            )
            .is_err()
    );
    assert!(
        f.state
            .issue_test_session_v2_tool_nonce(
                request("read-five-first"),
                &nonce,
                UnixMillis::new(13_001)
            )
            .is_err()
    );
    assert_eq!(f.state, paid);
    let original = f
        .state
        .issue_test_session_v2_read_nonce_five(
            request("read-five-first"),
            &nonce,
            UnixMillis::new(13_001),
        )
        .unwrap();
    let after = f.state.clone();
    let old = f
        .state
        .issue_test_session_v2_read_nonce_five(
            request("read-five-first"),
            &URL_SAFE_NO_PAD.encode([93; 32]),
            UnixMillis::new(13_002),
        )
        .unwrap();
    assert_eq!(original, old);
    assert_eq!(f.state, after);
    f.state
        .charge_test_session_v2_read_nonce_five(
            request("read-five-new-ingress"),
            UnixMillis::new(13_002),
        )
        .unwrap();
    assert_eq!(remaining(&f, SessionPurposeV1::ReconcileRead), read - 10);
    assert_eq!(
        f.state
            .test_session_v2_snapshot(&f.session_id)
            .unwrap()
            .roots()[&SessionPurposeV1::ReconcileRead]
            .budgets
            .requests
            .remaining()
            .unwrap(),
        requests,
        "metadata never consumes logical original-read quota"
    );
    let old = f
        .state
        .issue_test_session_v2_read_nonce_five(
            request("read-five-new-ingress"),
            &nonce,
            UnixMillis::new(13_002),
        )
        .unwrap();
    assert_eq!(original, old);
    let json = serde_json::to_value(&f.state).unwrap();
    let row = &json["test_sessions_v2"][&f.session_id];
    assert!(row["execution_book"].is_null());
    assert!(row["actions"].as_object().unwrap().is_empty());
    assert_eq!(row["purpose_transports"].as_object().unwrap().len(), 8);
    let class = &row["purpose_http_admissions"]["read-five-first"]["class"];
    assert_eq!(class["kind"], "read_nonce_five_v2");
    let restored: AccountProvisionApprovalState = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(restored, f.state);
    let mut missing = json.clone();
    missing["test_sessions_v2"][&f.session_id]["purpose_transports"]
        .as_object_mut()
        .unwrap()
        .remove(class["provider_abuse_attempt"].as_str().unwrap());
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(missing).is_err());
    let mut cross = json;
    cross["test_sessions_v2"][&f.session_id]["purpose_http_admissions"]["read-five-first"]["class"]
        ["kind"] = serde_json::json!("execution_nonce_five_v2");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(cross).is_err());
}
#[test]
fn session_v2_read_nonce_five_unknown_bad_rng_and_execution_cross_keep_fee() {
    let mut f = claimed_execution();
    // The opaque digest is not proof of known original existence. No Book is
    // created, no Source is acquired, and failed issuance never refunds5.
    let (body, proof) = read_metadata(&f, 13_000);
    let request = || SessionProofRequest {
        ingress_attempt_id: "read-five-unknown",
        original_bytes: &body,
        possession: &proof,
    };
    f.state
        .charge_test_session_v2_read_nonce_five(request(), UnixMillis::new(13_000))
        .unwrap();
    let paid = f.state.clone();
    assert!(
        f.state
            .issue_test_session_v2_read_nonce_five(
                request(),
                "bad-rng-encoding",
                UnixMillis::new(13_001)
            )
            .is_err()
    );
    assert_eq!(f.state, paid);
    let mut execution = claimed_execution();
    let (body, proof) = metadata(&execution, SessionPurposeV1::Execution, 13_000);
    let before = execution.state.clone();
    assert!(
        execution
            .state
            .charge_test_session_v2_read_nonce_five(
                SessionProofRequest {
                    ingress_attempt_id: "execution-not-read-five",
                    original_bytes: &body,
                    possession: &proof,
                },
                UnixMillis::new(13_000)
            )
            .is_err()
    );
    assert_eq!(execution.state, before);
}

#[test]
fn session_v2_read_nonce_five_exhaustion_or_less_than_five_never_issues() {
    let mut f = claimed_execution();
    let (body, proof) = read_metadata(&f, 13_000);
    let mut last = String::new();
    for i in 0..60 {
        if remaining(&f, SessionPurposeV1::ReconcileRead) < 5 {
            break;
        }
        last = format!("read-five-exhaustion-{i}");
        f.state
            .charge_test_session_v2_read_nonce_five(
                SessionProofRequest {
                    ingress_attempt_id: &last,
                    original_bytes: &body,
                    possession: &proof,
                },
                UnixMillis::new(13_000),
            )
            .unwrap();
    }
    assert!(remaining(&f, SessionPurposeV1::ReconcileRead) < 5);
    let paid = f.state.clone();
    assert!(
        f.state
            .charge_test_session_v2_read_nonce_five(
                SessionProofRequest {
                    ingress_attempt_id: "read-five-cannot-overdraw",
                    original_bytes: &body,
                    possession: &proof,
                },
                UnixMillis::new(13_001)
            )
            .is_err()
    );
    assert_eq!(f.state, paid);
    if remaining(&f, SessionPurposeV1::ReconcileRead) == 0 {
        assert!(
            f.state
                .issue_test_session_v2_read_nonce_five(
                    SessionProofRequest {
                        ingress_attempt_id: &last,
                        original_bytes: &body,
                        possession: &proof,
                    },
                    &URL_SAFE_NO_PAD.encode([94; 32]),
                    UnixMillis::new(13_001)
                )
                .is_err()
        );
        assert_eq!(f.state, paid);
    }
    let cold: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&f.state).unwrap()).unwrap();
    assert_eq!(cold, f.state);
}
