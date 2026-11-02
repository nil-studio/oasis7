//! Core acceptance only. Real World3/Root/claims and real Host RFC originals;
//! service/Actor physical counts belong to the separate mounted receiver tests.
use super::*;
fn action_with_http(
    f: &mut SessionFixture,
    id: &str,
    now: u64,
) -> (Vec<u8>, HttpPossessionReceipt, SessionClientHttpRequestV1) {
    let plan = f.state.test_session_v2_header(&f.session_id).unwrap().0;
    let selected = SessionToolActionV1::Execute(plan.actions[0].clone());
    let action = SessionActionRequestV2 {
        api_version: 2,
        session_id: f.session_id.clone(),
        request_id: id.into(),
        action: selected,
    };
    let metadata = SessionChallengeRequestV1 {
        api_version: 1,
        session_id: f.session_id.clone(),
        request_id: format!("metadata-{id}"),
        purpose: action.purpose(),
        kind: SessionChallengeKindV1::Tool,
        logical_request_id: id.into(),
        operation_request_digest: action.digest().unwrap(),
    };
    let body = serde_json::to_vec(&metadata).unwrap();
    let http = signed_http(
        f,
        &body,
        &metadata.request_id,
        (
            HttpPopRoute::ProofChallenges,
            HttpPopProfileV1::CustodyChallenge,
        ),
        None,
        action.purpose(),
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
    let ingress = format!("metadata-ingress-{id}");
    let admission = || SessionProofRequest {
        ingress_attempt_id: &ingress,
        original_bytes: &body,
        possession: &proof,
    };
    f.state
        .charge_test_session_v2_execution_nonce_five(admission(), UnixMillis::new(now))
        .unwrap();
    let nonce = URL_SAFE_NO_PAD.encode(Sha256::digest(
        format!("fixture-public-metadata-v2:{id}").as_bytes(),
    ));
    let challenge = f
        .state
        .issue_test_session_v2_execution_nonce_five(admission(), &nonce, UnixMillis::new(now + 1))
        .unwrap();
    assert_eq!(challenge.kind, SessionChallengeKindV1::ToolMetadata);
    let bytes = serde_json::to_vec(&action).unwrap();
    let http = signed_http(
        f,
        &bytes,
        id,
        (HttpPopRoute::Actions, HttpPopProfileV1::Custody),
        Some(&challenge.nonce),
        action.purpose(),
        now + 2,
    );
    let prepared = f
        .state
        .prepare_test_session_v2_action_http(
            &http,
            HttpHeaderRepresentation::RawInstances,
            UnixMillis::new(now + 2),
        )
        .unwrap();
    (
        bytes,
        verify_http_possession(prepared, UnixMillis::new(now + 2), &mut StrictCrypto).unwrap(),
        http,
    )
}
fn action(f: &mut SessionFixture, id: &str, now: u64) -> (Vec<u8>, HttpPossessionReceipt) {
    let (body, receipt, _) = action_with_http(f, id, now);
    (body, receipt)
}
fn ready() -> SessionFixture {
    claimed_execution_from(super::super::super::machine_tests::ready_world3(9000))
}
fn remaining(f: &SessionFixture) -> u64 {
    f.state
        .test_session_v2_snapshot(&f.session_id)
        .unwrap()
        .roots()[&SessionPurposeV1::Execution]
        .budgets
        .transport
        .remaining()
        .unwrap()
}
#[test]
fn session_v2_pending_start_first_five_then_master134_no_book_and_cold() {
    let mut f = ready();
    let (body, pop) = action(&mut f, "pending-start", 13000);
    let proof = || SessionProofRequest {
        ingress_attempt_id: "pending-start-ingress",
        original_bytes: &body,
        possession: &pop,
    };
    let before = remaining(&f);
    f.state
        .charge_test_session_v2_execution_pending_ingress(proof(), UnixMillis::new(13002))
        .unwrap();
    assert_eq!(remaining(&f), before - 5);
    let token = f
        .state
        .reserve_test_session_v2_execution_pending(
            proof(),
            &"a1".repeat(32),
            &scope(&f.pin),
            UnixMillis::new(13003),
            &mut StrictCrypto,
        )
        .unwrap()
        .unwrap();
    assert_eq!(remaining(&f), before - 139);
    let value = serde_json::to_value(&f.state).unwrap();
    let row = &value["test_sessions_v2"][&f.session_id];
    assert!(row["execution_book"].is_null());
    let nonce = &row["challenges"]["metadata-pending-start"];
    assert!(
        nonce["consumed_by"].is_null(),
        "only real Book birth may consume the nonce"
    );
    let cold: AccountProvisionApprovalState = serde_json::from_value(value).unwrap();
    assert_eq!(cold, f.state);
    let before_history = f.state.clone();
    assert!(
        f.state
            .reserve_test_session_v2_execution_pending(
                proof(),
                &"a1".repeat(32),
                &scope(&f.pin),
                UnixMillis::new(13004),
                &mut StrictCrypto
            )
            .unwrap()
            .is_none()
    );
    assert_eq!(
        before_history, f.state,
        "history cannot return a second first-dispatch handle"
    );
    let directory = f
        .state
        .claim_test_session_v2_pending_first_directory(
            token,
            &scope(&f.pin),
            UnixMillis::new(13004),
            &mut StrictCrypto,
        )
        .unwrap();
    assert!(directory.until().get() <= 18003);
    let cold: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&f.state).unwrap()).unwrap();
    assert_eq!(cold, f.state);
}
#[test]
fn session_v2_pending_start_138_retains_first_five_when_master134_fails() {
    let mut f = ready();
    let (body, pop) = action(&mut f, "pending-insufficient", 13000);
    let mut n = 0;
    while remaining(&f) > 138 {
        charge(
            &mut f,
            &body,
            &pop,
            &format!("pending-drain-{n}"),
            SessionPurposeV1::Execution,
            13002,
        )
        .unwrap();
        n += 1;
    }
    assert_eq!(remaining(&f), 138);
    let proof = || SessionProofRequest {
        ingress_attempt_id: "pending-insufficient-ingress",
        original_bytes: &body,
        possession: &pop,
    };
    f.state
        .charge_test_session_v2_execution_pending_ingress(proof(), UnixMillis::new(13003))
        .unwrap();
    assert_eq!(remaining(&f), 133);
    let paid = f.state.clone();
    assert!(
        f.state
            .reserve_test_session_v2_execution_pending(
                proof(),
                &"a2".repeat(32),
                &scope(&f.pin),
                UnixMillis::new(13004),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(paid, f.state);
    let value = serde_json::to_value(&f.state).unwrap();
    let row = &value["test_sessions_v2"][&f.session_id];
    assert!(row["execution_book"].is_null());
    assert!(row["execution_pending_start"].is_null());
    assert!(row["sources"].as_object().unwrap().is_empty());
    let cold: AccountProvisionApprovalState = serde_json::from_value(value).unwrap();
    assert_eq!(cold, f.state);
}

#[path = "test_session_v2_execution_pending_fence_tests.rs"]
mod fence_tests;

#[path = "test_session_v2_pending_action_replay_fee_tests.rs"]
mod action_replay_fee_tests;
