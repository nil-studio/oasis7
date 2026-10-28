use super::*;
fn request(purpose: SessionPurposeV1, suffix: &str) -> SessionChallengeRequestV1 {
    SessionChallengeRequestV1 {
        api_version: 1,
        session_id: ID.into(),
        request_id: format!("metadata-nonce-{suffix}"),
        purpose,
        kind: SessionChallengeKindV1::Tool,
        logical_request_id: format!("business-{suffix}"),
        operation_request_digest: "ab".repeat(32),
    }
}
fn signed(
    f: &SessionFixture,
    q: &SessionChallengeRequestV1,
    token: &str,
    now: u64,
) -> (Vec<u8>, HttpPossessionReceipt) {
    let body = serde_json::to_vec(q).unwrap();
    let receipt = signed_octets(f, q, &body, token, now);
    (body, receipt)
}
fn signed_octets(
    f: &SessionFixture,
    q: &SessionChallengeRequestV1,
    body: &[u8],
    token: &str,
    now: u64,
) -> HttpPossessionReceipt {
    let target = CanonicalHttpTarget::new(
        &f.fact.registration.public_origin,
        HttpPopRoute::ScopedClient {
            provision_id: f.state.draft().entry.provision_id.clone(),
            custody_account_id: f.state.draft().entry.custody_account_id.clone(),
            route: Box::new(HttpPopRoute::ProofChallenges),
        },
    )
    .unwrap();
    let root = &f.state.test_sessions()[ID].roots[&q.purpose];
    let material = build_http_pop_signing_material(&HttpPopSigningInput {
        profile: HttpPopProfileV1::CustodyChallenge,
        target: &target,
        key_id: &f.fact.registration.key_id,
        binding_id: ID,
        request_id: &q.request_id,
        nonce: None,
        token: Some(token),
        created: UnixSeconds::new(now / 1000),
        expires: UnixSeconds::new((now / 1000 + 10).min(root.deadline.get() / 1000)),
        body,
    })
    .unwrap();
    let key = SigningKey::from_bytes(&[8; 32]);
    let raw = SessionClientHttpRequestV1 {
        session_id: ID.into(),
        method: target.method().into(),
        target_uri: target.uri().into(),
        headers: material
            .signed_headers(&key.sign(material.signature_base()).to_bytes())
            .into_iter()
            .map(|h| SessionClientHeaderV1 {
                name: h.name,
                value: h.value,
            })
            .collect(),
        body: String::from_utf8(body.to_vec()).unwrap(),
        nonce_hint: None,
    };
    let context = SessionClientHttpContext {
        public_origin: &f.fact.registration.public_origin,
        custody_account_id: &f.state.draft().entry.custody_account_id,
        provision_id: &f.state.draft().entry.provision_id,
        operation: SessionClientOperation::ToolNonce,
        representation: HttpHeaderRepresentation::RawInstances,
    };
    let prepared = f
        .state
        .prepare_test_session_client_http(&raw, &context, UnixMillis::new(now))
        .unwrap();
    verify_http_possession(prepared, UnixMillis::new(now), &mut StrictCrypto).unwrap()
}
fn proof<'a>(
    ingress: &'a str,
    body: &'a [u8],
    receipt: &'a HttpPossessionReceipt,
) -> SessionProofRequest<'a> {
    SessionProofRequest {
        ingress_attempt_id: ingress,
        original_bytes: body,
        possession: receipt,
    }
}
fn remaining(f: &SessionFixture, purpose: SessionPurposeV1) -> u64 {
    f.state.test_sessions()[ID].roots[&purpose]
        .budgets
        .transport
        .remaining()
        .unwrap()
}
fn restore(state: &AccountProvisionApprovalState) {
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(
            serde_json::to_value(state).unwrap()
        )
        .unwrap(),
        *state
    );
}
#[test]
fn tool_nonce_fixed_three_fees_replay_and_independent_issuer_conflict() {
    for purpose in [SessionPurposeV1::Execution, SessionPurposeV1::ReconcileRead] {
        let (mut f, execution, read) = claimed();
        let token = if purpose == SessionPurposeV1::Execution {
            &execution
        } else {
            &read
        };
        let q = request(purpose, "one");
        let (body, receipt) = signed(&f, &q, token, 12_000);
        let before = f.state.clone();
        let available = remaining(&f, purpose);
        f.state
            .charge_test_session_tool_nonce_http_attempt(
                proof("ingress-A", &body, &receipt),
                UnixMillis::new(12_000),
            )
            .unwrap();
        assert_eq!(available - remaining(&f, purpose), 3);
        let paid = f.state.clone();
        f.state
            .charge_test_session_tool_nonce_http_attempt(
                proof("ingress-A", &body, &receipt),
                UnixMillis::new(12_001),
            )
            .unwrap();
        assert_eq!(f.state, paid);
        let nonce = URL_SAFE_NO_PAD.encode([90; 32]);
        let first = f
            .state
            .issue_test_session_tool_nonce(
                proof("ingress-A", &body, &receipt),
                &nonce,
                UnixMillis::new(12_001),
            )
            .unwrap();
        assert_eq!(first.kind, SessionChallengeKindV1::ToolMetadata);
        let issued = f.state.clone();
        f.state
            .charge_test_session_tool_nonce_http_attempt(
                proof("ingress-B", &body, &receipt),
                UnixMillis::new(12_002),
            )
            .unwrap();
        assert_eq!(available - remaining(&f, purpose), 6);
        let paid_b = f.state.clone();
        let retry = f
            .state
            .issue_test_session_tool_nonce(
                proof("ingress-B", &body, &receipt),
                &URL_SAFE_NO_PAD.encode([91; 32]),
                UnixMillis::new(12_002),
            )
            .unwrap();
        assert_eq!(retry, first);
        assert_eq!(f.state, paid_b);
        assert_eq!(
            f.state.test_sessions()[ID].tool_nonce_links[&q.request_id].first_ingress,
            "ingress-A"
        );
        let mut changed = q.clone();
        changed.operation_request_digest = "cd".repeat(32);
        let (changed_body, changed_receipt) = signed(&f, &changed, token, 12_003);
        assert!(
            f.state
                .charge_test_session_tool_nonce_http_attempt(
                    proof("ingress-A", &changed_body, &changed_receipt),
                    UnixMillis::new(12_003)
                )
                .is_err()
        );
        assert_eq!(f.state, paid_b, "same ingress cannot change tuple");
        f.state
            .charge_test_session_tool_nonce_http_attempt(
                proof("ingress-C", &changed_body, &changed_receipt),
                UnixMillis::new(12_003),
            )
            .unwrap();
        let paid_c = f.state.clone();
        assert!(
            f.state
                .issue_test_session_tool_nonce(
                    proof("ingress-C", &changed_body, &changed_receipt),
                    &nonce,
                    UnixMillis::new(12_003)
                )
                .is_err()
        );
        assert_eq!(
            f.state, paid_c,
            "issuer conflict preserves the already committed three fees"
        );
        assert_eq!(available - remaining(&f, purpose), 9);
        let old = &before.test_sessions()[ID];
        let new = &f.state.test_sessions()[ID];
        assert_eq!(old.sources, new.sources);
        assert_eq!(old.actions, new.actions);
        assert_eq!(old.control, new.control);
        assert_eq!(old.identity_transport, new.identity_transport);
        assert_eq!(old.connector_transport, new.connector_transport);
        assert_eq!(
            old.roots[&SessionPurposeV1::TerminationPlan],
            new.roots[&SessionPurposeV1::TerminationPlan]
        );
        assert_eq!(
            issued.test_sessions()[ID].tool_nonce_links,
            new.tool_nonce_links
        );
        restore(&f.state);
    }
}
#[test]
fn tool_nonce_generic_host_admission_cannot_be_upgraded_to_three_fee_class() {
    let (mut f, token, _) = claimed();
    let q = request(SessionPurposeV1::Execution, "generic");
    let (body, receipt) = signed(&f, &q, &token, 12_000);
    f.state
        .charge_test_session_purpose_http_attempt(
            ID,
            q.purpose,
            SessionHttpAdmission {
                ingress_attempt_id: "generic-A",
                possession: &receipt,
            },
            UnixMillis::new(12_000),
        )
        .unwrap();
    let before = f.state.clone();
    assert!(
        f.state
            .charge_test_session_tool_nonce_http_attempt(
                proof("generic-A", &body, &receipt),
                UnixMillis::new(12_001)
            )
            .is_err()
    );
    assert!(
        f.state
            .issue_test_session_tool_nonce(
                proof("generic-A", &body, &receipt),
                &URL_SAFE_NO_PAD.encode([91; 32]),
                UnixMillis::new(12_001)
            )
            .is_err()
    );
    assert_eq!(f.state, before);
}
#[test]
fn tool_nonce_three_fee_budget_exhaustion_is_atomic() {
    let (mut f, token, _) = claimed();
    let purpose = SessionPurposeV1::Execution;
    let initial = remaining(&f, purpose);
    for n in 0..initial - 2 {
        f.state
            .reserve_test_session_purpose_transport(
                ID,
                purpose,
                &format!("ordinary-paid-{n}"),
                UnixMillis::new(12_000),
            )
            .unwrap();
    }
    assert_eq!(remaining(&f, purpose), 2);
    let q = request(purpose, "exhausted");
    let (body, receipt) = signed(&f, &q, &token, 12_000);
    let before = f.state.clone();
    assert!(
        f.state
            .charge_test_session_tool_nonce_http_attempt(
                proof("exhausted-A", &body, &receipt),
                UnixMillis::new(12_000)
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    restore(&f.state);
}
#[test]
fn tool_nonce_shared_pending_and_per_second_limits_do_not_refund_fees() {
    let (mut f, token, _) = claimed();
    // The real existing Claim challenges also occupy the shared historical slots.
    for n in 0..8u8 {
        let now = 12_000 + u64::from(n / 2) * 1000;
        let q = request(SessionPurposeV1::Execution, &format!("slot-{n}"));
        let (body, receipt) = signed(&f, &q, &token, now);
        let ingress = format!("slot-ingress-{n}");
        f.state
            .charge_test_session_tool_nonce_http_attempt(
                proof(&ingress, &body, &receipt),
                UnixMillis::new(now),
            )
            .unwrap();
        f.state
            .issue_test_session_tool_nonce(
                proof(&ingress, &body, &receipt),
                &URL_SAFE_NO_PAD.encode([100 + n; 32]),
                UnixMillis::new(now),
            )
            .unwrap();
        if n == 1 {
            let third = request(SessionPurposeV1::Execution, "rate-third");
            let (bytes, signed) = signed(&f, &third, &token, now);
            f.state
                .charge_test_session_tool_nonce_http_attempt(
                    proof("rate-third", &bytes, &signed),
                    UnixMillis::new(now),
                )
                .unwrap();
            let paid = f.state.clone();
            assert!(
                f.state
                    .issue_test_session_tool_nonce(
                        proof("rate-third", &bytes, &signed),
                        &URL_SAFE_NO_PAD.encode([120; 32]),
                        UnixMillis::new(now)
                    )
                    .is_err()
            );
            assert_eq!(f.state, paid);
        }
    }
    let q = request(SessionPurposeV1::Execution, "ninth");
    let (body, receipt) = signed(&f, &q, &token, 16_000);
    f.state
        .charge_test_session_tool_nonce_http_attempt(
            proof("ninth", &body, &receipt),
            UnixMillis::new(16_000),
        )
        .unwrap();
    let paid = f.state.clone();
    assert!(
        f.state
            .issue_test_session_tool_nonce(
                proof("ninth", &body, &receipt),
                &URL_SAFE_NO_PAD.encode([122; 32]),
                UnixMillis::new(16_000)
            )
            .is_err()
    );
    assert_eq!(f.state, paid);
    restore(&f.state);
}

#[path = "test_session_tool_nonce_restore_tests.rs"]
mod restore_tests;
#[path = "test_session_tool_nonce_rotation_tests.rs"]
mod rotation_tests;

#[test]
fn tool_nonce_last_three_execution_transports_persist_budget_exhausted_closing() {
    let (mut f, token, read_token) = claimed();
    let purpose = SessionPurposeV1::Execution;
    let initial = remaining(&f, purpose);
    for n in 0..initial - 3 {
        f.state
            .reserve_test_session_purpose_transport(
                ID,
                purpose,
                &format!("before-last-three-{n}"),
                UnixMillis::new(12_000),
            )
            .unwrap();
    }
    assert_eq!(remaining(&f, purpose), 3);
    let before = f
        .state
        .test_session_progress(ID, UnixMillis::new(12_000))
        .unwrap();
    assert_eq!(before.stage, TestSessionPhaseV1::Active);
    assert!(!before.write_closed);
    assert_eq!(
        f.state.test_sessions()[ID].roots[&purpose]
            .budgets
            .business
            .remaining()
            .unwrap(),
        0,
        "the legitimate zero business limit must not close an otherwise live session"
    );
    let cleanup = f.state.test_sessions()[ID].roots[&SessionPurposeV1::TerminationPlan].clone();
    let q = request(purpose, "last-three");
    // This helper performs the actual registered-key/current-token RFC strict verification.
    let (body, receipt) = signed(&f, &q, &token, 12_000);
    f.state
        .charge_test_session_tool_nonce_http_attempt(
            proof("last-three-A", &body, &receipt),
            UnixMillis::new(12_000),
        )
        .unwrap();
    assert_eq!(remaining(&f, purpose), 0);
    let after = f
        .state
        .test_session_progress(ID, UnixMillis::new(12_000))
        .unwrap();
    assert_eq!(
        after.stage,
        TestSessionPhaseV1::Closing,
        "the transaction consuming the last execution transport must persist Closing"
    );
    assert!(after.write_closed);
    assert_eq!(
        after.closing_reason,
        Some(TestSessionClosingReasonV1::BudgetExhausted)
    );
    assert_eq!(
        f.state.test_sessions()[ID].roots[&SessionPurposeV1::TerminationPlan],
        cleanup
    );
    let paid = f.state.clone();
    assert!(
        f.state
            .issue_test_session_tool_nonce(
                proof("last-three-A", &body, &receipt),
                &URL_SAFE_NO_PAD.encode([133; 32]),
                UnixMillis::new(12_001),
            )
            .is_err()
    );
    assert_eq!(
        f.state, paid,
        "issuance cannot reopen a fee-exhausted execution root"
    );
    f.state
        .charge_test_session_tool_nonce_http_attempt(
            proof("last-three-A", &body, &receipt),
            UnixMillis::new(12_001),
        )
        .unwrap();
    assert_eq!(
        f.state, paid,
        "exact paid callback is a read of the original fee fact"
    );
    let first_closing = f.state.test_sessions()[ID].closing;
    let read_request = request(SessionPurposeV1::ReconcileRead, "after-execution-closed");
    let (read_body, read_receipt) = signed(&f, &read_request, &read_token, 12_002);
    f.state
        .charge_test_session_tool_nonce_http_attempt(
            proof("read-after-execution-A", &read_body, &read_receipt),
            UnixMillis::new(12_002),
        )
        .unwrap();
    f.state
        .issue_test_session_tool_nonce(
            proof("read-after-execution-A", &read_body, &read_receipt),
            &URL_SAFE_NO_PAD.encode([134; 32]),
            UnixMillis::new(12_002),
        )
        .unwrap();
    assert_eq!(f.state.test_sessions()[ID].closing, first_closing);
    assert_eq!(
        f.state.test_sessions()[ID].roots[&SessionPurposeV1::TerminationPlan],
        cleanup
    );
    f.state
        .expire_test_sessions(UnixMillis::new(100_000))
        .unwrap();
    assert_eq!(
        f.state.test_sessions()[ID].closing,
        first_closing,
        "later absolute expiry must not overwrite the first persisted closing reason/time"
    );
    restore(&f.state);
}
#[path = "test_session_tool_nonce_lifecycle_tests.rs"]
mod lifecycle_tests;
