//! Actual paid originals and real alternate valid eighth issuance, not fake facts.
use super::*;
struct Paid {
    body: Vec<u8>,
    proof: HttpPossessionReceipt,
    ingress: String,
    request: String,
    nonce: String,
}
fn prepay(f: &mut SessionFixture, index: u8) -> Paid {
    let id = format!("capacity-action-{index}");
    let plan = f.state.test_session_v2_header(&f.session_id).unwrap().0;
    let action = SessionActionRequestV2 {
        api_version: 2,
        session_id: f.session_id.clone(),
        request_id: id.clone(),
        action: SessionToolActionV1::Execute(plan.actions[0].clone()),
    };
    let q = SessionChallengeRequestV1 {
        api_version: 1,
        session_id: f.session_id.clone(),
        request_id: format!("capacity-challenge-{index}"),
        purpose: SessionPurposeV1::Execution,
        kind: SessionChallengeKindV1::Tool,
        logical_request_id: id,
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
        q.purpose,
        13_000,
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
        .prepare_test_session_v2_tool_nonce_http(&http, &context, UnixMillis::new(13_000))
        .unwrap();
    let proof =
        verify_http_possession(prepared, UnixMillis::new(13_000), &mut StrictCrypto).unwrap();
    let ingress = format!("capacity-ingress-{index}");
    f.state
        .charge_test_session_v2_tool_nonce_http_attempt(
            SessionProofRequest {
                ingress_attempt_id: &ingress,
                original_bytes: &body,
                possession: &proof,
            },
            UnixMillis::new(13_000),
        )
        .unwrap();
    Paid {
        body,
        proof,
        ingress,
        request: q.request_id,
        nonce: URL_SAFE_NO_PAD.encode(Sha256::digest(
            format!("fixture-public-capacity-v2:{index}").as_bytes(),
        )),
    }
}
fn issue(
    s: &mut AccountProvisionApprovalState,
    p: &Paid,
    at: u64,
) -> std::result::Result<SessionChallengeV1, ProvisionApprovalError> {
    s.issue_test_session_v2_tool_nonce(
        SessionProofRequest {
            ingress_attempt_id: &p.ingress,
            original_bytes: &p.body,
            possession: &p.proof,
        },
        &p.nonce,
        UnixMillis::new(at),
    )
}
#[test]
fn session_v2_tool_metadata_restore_rejects_ninth_actual_paid_pending_challenge() {
    let mut f = claimed_execution();
    let before = f
        .state
        .test_session_v2_snapshot(&f.session_id)
        .unwrap()
        .roots()[&SessionPurposeV1::Execution]
        .budgets
        .transport
        .remaining()
        .unwrap();
    let paid: Vec<_> = (0..9).map(|i| prepay(&mut f, i)).collect();
    assert_eq!(
        f.state
            .test_session_v2_snapshot(&f.session_id)
            .unwrap()
            .roots()[&SessionPurposeV1::Execution]
            .budgets
            .transport
            .remaining()
            .unwrap(),
        before - 27
    );
    // Nine actual fees are committed first; all copied facts below are issued
    // by the real mutator in a valid branch with only seven prior pending rows.
    for (i, p) in paid.iter().take(7).enumerate() {
        issue(&mut f.state, p, 14_000 + i as u64 * 1000).unwrap();
    }
    let mut branch = f.state.clone();
    let ninth = issue(&mut branch, &paid[8], 22_000).unwrap();
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(
            serde_json::to_value(&branch).unwrap()
        )
        .unwrap(),
        branch
    );
    issue(&mut f.state, &paid[7], 21_000).unwrap();
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(
            serde_json::to_value(&f.state).unwrap()
        )
        .unwrap(),
        f.state
    );
    let before_ninth = f.state.clone();
    assert!(issue(&mut f.state, &paid[8], 22_000).is_err());
    assert_eq!(f.state, before_ninth);
    let mut main = serde_json::to_value(&f.state).unwrap();
    let branch = serde_json::to_value(&branch).unwrap();
    let main_session = &mut main["test_sessions_v2"][&f.session_id];
    let branch_session = &branch["test_sessions_v2"][&f.session_id];
    assert_eq!(
        branch_session["challenges"][&paid[8].request],
        serde_json::to_value(&ninth).unwrap()
    );
    main_session["challenges"][&paid[8].request] =
        branch_session["challenges"][&paid[8].request].clone();
    main_session["tool_nonce_links"][&paid[8].request] =
        branch_session["tool_nonce_links"][&paid[8].request].clone();
    let event = branch_session["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            e["event"]["kind"] == "tool_metadata_nonce_issued"
                && e["event"]["request_id"] == paid[8].request
        })
        .unwrap()
        .clone();
    main_session["events"].as_array_mut().unwrap().push(event);
    // Same nine paid ingress records and root balance: no invented free fee,
    // no timing/rate violation, no missing event to reject before capacity.
    assert_eq!(
        main_session["purpose_http_admissions"],
        branch_session["purpose_http_admissions"]
    );
    assert_eq!(main_session["roots"], branch_session["roots"]);
    let pending = main_session["challenges"]
        .as_object()
        .unwrap()
        .values()
        .filter(|v| {
            v["consumed_by"].is_null()
                && serde_json::from_value::<UnixMillis>(v["expires_at"].clone())
                    .unwrap()
                    .get()
                    > 22_000
        })
        .count();
    assert_eq!(pending, 9);
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(main).is_err(),
        "nine pending metadata challenges must fail the same shared live capacity during restore"
    );
}
