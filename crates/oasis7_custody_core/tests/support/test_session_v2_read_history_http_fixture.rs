use super::*;
pub(super) fn read_pop(
    f: &mut SessionFixture,
    id: &str,
    operation: &str,
    now: u64,
) -> (Vec<u8>, HttpPossessionReceipt) {
    let q = SessionActionRequestV2 {
        api_version: 2,
        session_id: f.session_id.clone(),
        request_id: id.into(),
        action: SessionToolActionV1::ReconcileOperation {
            operation_id: operation.into(),
        },
    };
    let metadata = SessionChallengeRequestV1 {
        api_version: 1,
        session_id: f.session_id.clone(),
        request_id: format!("read-metadata-{id}"),
        purpose: SessionPurposeV1::ReconcileRead,
        kind: SessionChallengeKindV1::Tool,
        logical_request_id: id.into(),
        operation_request_digest: q.digest().unwrap(),
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
        SessionPurposeV1::ReconcileRead,
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
    let pop = verify_http_possession(prepared, UnixMillis::new(now), &mut StrictCrypto).unwrap();
    let ingress = format!("read-metadata-ingress-{id}");
    f.state
        .charge_test_session_v2_tool_nonce_http_attempt(
            SessionProofRequest {
                ingress_attempt_id: &ingress,
                original_bytes: &body,
                possession: &pop,
            },
            UnixMillis::new(now),
        )
        .unwrap();
    let nonce = URL_SAFE_NO_PAD.encode(Sha256::digest(format!("read-history-{id}").as_bytes()));
    let challenge = f
        .state
        .issue_test_session_v2_tool_nonce(
            SessionProofRequest {
                ingress_attempt_id: &ingress,
                original_bytes: &body,
                possession: &pop,
            },
            &nonce,
            UnixMillis::new(now + 1),
        )
        .unwrap();
    let bytes = serde_json::to_vec(&q).unwrap();
    let http = signed_http(
        f,
        &bytes,
        id,
        (HttpPopRoute::Actions, HttpPopProfileV1::Custody),
        Some(&challenge.nonce),
        SessionPurposeV1::ReconcileRead,
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
    )
}
