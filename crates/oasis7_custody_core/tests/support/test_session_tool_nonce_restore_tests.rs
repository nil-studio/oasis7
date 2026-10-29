use super::*;
#[test]
fn tool_nonce_restore_rejects_stripped_links_fees_and_changed_bindings() {
    let (mut f, token, _) = claimed();
    let q = request(SessionPurposeV1::Execution, "restore");
    let (body, receipt) = signed(&f, &q, &token, 12_000);
    f.state
        .charge_test_session_tool_nonce_http_attempt(
            proof("restore-A", &body, &receipt),
            UnixMillis::new(12_000),
        )
        .unwrap();
    f.state
        .issue_test_session_tool_nonce(
            proof("restore-A", &body, &receipt),
            &URL_SAFE_NO_PAD.encode([123; 32]),
            UnixMillis::new(12_001),
        )
        .unwrap();
    restore(&f.state);
    let value = serde_json::to_value(&f.state).unwrap();
    let internal = f.state.test_sessions()[ID].tool_nonce_ingresses["restore-A"]
        .owner_attempt
        .clone();
    for case in 0..11 {
        let mut bad = value.clone();
        let s = &mut bad["test_sessions"][ID];
        match case {
            0 => {
                s.as_object_mut().unwrap().remove("tool_nonce_ingresses");
            }
            1 => {
                s.as_object_mut().unwrap().remove("tool_nonce_links");
            }
            2 => {
                s["purpose_http_admissions"]
                    .as_object_mut()
                    .unwrap()
                    .remove("restore-A");
            }
            3 => {
                s["purpose_transports"]
                    .as_object_mut()
                    .unwrap()
                    .remove(&internal);
            }
            4 => {
                s["tool_nonce_links"][&q.request_id]["first_ingress"] = "absent".into();
            }
            5 => {
                s["tool_nonce_ingresses"]["restore-A"]["metadata_uri"] =
                    "https://foreign.example/v1/proof-challenges".into();
            }
            6 => {
                s["tool_nonce_links"][&q.request_id]["action_uri"] =
                    "https://foreign.example/v1/actions".into();
            }
            7 => {
                s["tool_nonce_links"][&q.request_id]["credential_generation"] = "2".into();
            }
            8 => {
                s["purpose_http_admissions"]["restore-A"]["class"] =
                    serde_json::json!({"kind":"generic_one"});
            }
            9 => {
                s["tool_nonce_links"][&q.request_id]["signed_attempt_digest"] =
                    "dd".repeat(32).into();
            }
            _ => {
                s["tool_nonce_links"][&q.request_id]["request"]["logical_request_id"] =
                    "other-business".into();
            }
        }
        assert!(
            serde_json::from_value::<AccountProvisionApprovalState>(bad).is_err(),
            "restore case {case}"
        );
    }
    // Legacy empty maps remain compatible only when no new issuance/three-fee class exists.
    let (legacy, _, _) = claimed();
    let mut old = serde_json::to_value(&legacy.state).unwrap();
    old["test_sessions"][ID]
        .as_object_mut()
        .unwrap()
        .remove("tool_nonce_ingresses");
    old["test_sessions"][ID]
        .as_object_mut()
        .unwrap()
        .remove("tool_nonce_links");
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(old).unwrap(),
        legacy.state
    );
}
#[test]
fn tool_nonce_wrong_fixed_target_profile_and_token_never_charge() {
    let (mut f, token, read) = claimed();
    let q = request(SessionPurposeV1::Execution, "target");
    let body = serde_json::to_vec(&q).unwrap();
    for profile in [
        HttpPopProfileV1::CustodyChallenge,
        HttpPopProfileV1::McpChallenge,
    ] {
        // Both are genuine verified proofs, but on the old unscoped URI (and one wrong profile).
        let receipt = http(
            &f,
            &body,
            (profile, HttpPopRoute::ProofChallenges),
            &q.request_id,
            None,
            Some(&token),
            12_000,
        );
        let before = f.state.clone();
        assert!(
            f.state
                .charge_test_session_tool_nonce_http_attempt(
                    proof("wrong-target", &body, &receipt),
                    UnixMillis::new(12_000)
                )
                .is_err()
        );
        assert_eq!(f.state, before);
    }
    let context = SessionClientHttpContext {
        public_origin: &f.fact.registration.public_origin,
        custody_account_id: &f.state.draft().entry.custody_account_id,
        provision_id: &f.state.draft().entry.provision_id,
        operation: SessionClientOperation::ToolNonce,
        representation: HttpHeaderRepresentation::RawInstances,
    };
    let raw = raw_request(
        &f,
        &body,
        (
            HttpPopProfileV1::CustodyChallenge,
            HttpPopRoute::ProofChallenges,
            &q.request_id,
            None,
            Some(&read),
            12_000,
        ),
    );
    let before = f.state.clone();
    assert!(
        f.state
            .prepare_test_session_client_http(&raw, &context, UnixMillis::new(12_000))
            .is_err()
    );
    let wrong_origin = SessionClientHttpContext {
        public_origin: "https://foreign.example",
        ..context
    };
    assert!(
        f.state
            .prepare_test_session_client_http(&raw, &wrong_origin, UnixMillis::new(12_000))
            .is_err()
    );
    assert_eq!(f.state, before);
}
#[test]
fn tool_nonce_retry_never_extends_expiry_and_rejects_untrusted_random_bytes() {
    let (mut f, _, token) = claimed();
    let q = request(SessionPurposeV1::ReconcileRead, "expiry");
    let (body, receipt) = signed(&f, &q, &token, 12_000);
    f.state
        .charge_test_session_tool_nonce_http_attempt(
            proof("expiry-A", &body, &receipt),
            UnixMillis::new(12_000),
        )
        .unwrap();
    let paid = f.state.clone();
    for invalid_nonce in ["", "short", &URL_SAFE_NO_PAD.encode([1; 31])] {
        assert!(
            f.state
                .issue_test_session_tool_nonce(
                    proof("expiry-A", &body, &receipt),
                    invalid_nonce,
                    UnixMillis::new(12_001)
                )
                .is_err()
        );
        assert_eq!(f.state, paid);
    }
    assert!(
        f.state.test_sessions()[ID].roots[&q.purpose].deadline.get() > 42_001,
        "expiry oracle must exercise the nonce TTL rather than only the root cutoff"
    );
    let first = f
        .state
        .issue_test_session_tool_nonce(
            proof("expiry-A", &body, &receipt),
            &URL_SAFE_NO_PAD.encode([124; 32]),
            UnixMillis::new(12_001),
        )
        .unwrap();
    assert_eq!(
        first.expires_at.get(),
        42_001.min(f.state.test_sessions()[ID].roots[&q.purpose].deadline.get())
    );
    // A later authenticated real ingress can read the same expired metadata fact, never renew it.
    let now = first.expires_at.get() + 1;
    let (new_body, new_receipt) = signed(&f, &q, &token, now);
    f.state
        .charge_test_session_tool_nonce_http_attempt(
            proof("expiry-B", &new_body, &new_receipt),
            UnixMillis::new(now),
        )
        .unwrap();
    let before = f.state.clone();
    assert_eq!(
        f.state
            .issue_test_session_tool_nonce(
                proof("expiry-B", &new_body, &new_receipt),
                &URL_SAFE_NO_PAD.encode([125; 32]),
                UnixMillis::new(now)
            )
            .unwrap(),
        first
    );
    assert_eq!(f.state, before);
}
#[test]
fn tool_nonce_internal_attempts_cannot_be_reused_by_other_ledgers() {
    let (mut f, token, _) = claimed();
    let q = request(SessionPurposeV1::Execution, "collision");
    let (body, receipt) = signed(&f, &q, &token, 12_000);
    let paid = f
        .state
        .charge_test_session_tool_nonce_http_attempt(
            proof("collision-A", &body, &receipt),
            UnixMillis::new(12_000),
        )
        .unwrap();
    let before = f.state.clone();
    assert!(
        f.state
            .reserve_test_session_purpose_transport(
                ID,
                q.purpose,
                &paid.owner_attempt,
                UnixMillis::new(12_000)
            )
            .is_err()
    );
    assert!(
        f.state
            .charge_test_session_control_http_attempt(
                ID,
                SessionHttpAdmission {
                    ingress_attempt_id: &paid.account_attempt,
                    possession: &receipt
                },
                UnixMillis::new(12_000)
            )
            .is_err()
    );
    assert!(
        f.state
            .charge_test_session_purpose_http_attempt(
                ID,
                q.purpose,
                SessionHttpAdmission {
                    ingress_attempt_id: &paid.owner_attempt,
                    possession: &receipt
                },
                UnixMillis::new(12_000)
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    // A prior legitimate generic purpose allocation also blocks the derived fee ID atomically.
    let (mut other, other_token, _) = claimed();
    let (bytes, signed) = signed(&other, &q, &other_token, 12_000);
    other
        .state
        .reserve_test_session_purpose_transport(
            ID,
            q.purpose,
            &paid.owner_attempt,
            UnixMillis::new(12_000),
        )
        .unwrap();
    let old = other.state.clone();
    assert!(
        other
            .state
            .charge_test_session_tool_nonce_http_attempt(
                proof("collision-A", &bytes, &signed),
                UnixMillis::new(12_000)
            )
            .is_err()
    );
    assert_eq!(other.state, old);
}

#[test]
fn tool_nonce_body_octets_conflict_after_fee_even_when_typed_fields_match() {
    let (mut f, token, _) = claimed();
    let q = request(SessionPurposeV1::Execution, "raw-body");
    let (body, receipt) = signed(&f, &q, &token, 12_000);
    f.state
        .charge_test_session_tool_nonce_http_attempt(
            proof("raw-A", &body, &receipt),
            UnixMillis::new(12_000),
        )
        .unwrap();
    f.state
        .issue_test_session_tool_nonce(
            proof("raw-A", &body, &receipt),
            &URL_SAFE_NO_PAD.encode([130; 32]),
            UnixMillis::new(12_001),
        )
        .unwrap();
    let mut different = vec![b' '];
    different.extend(&body);
    assert_eq!(
        serde_json::from_slice::<SessionChallengeRequestV1>(&different).unwrap(),
        q
    );
    let other_receipt = signed_octets(&f, &q, &different, &token, 12_002);
    let available = remaining(&f, q.purpose);
    f.state
        .charge_test_session_tool_nonce_http_attempt(
            proof("raw-B", &different, &other_receipt),
            UnixMillis::new(12_002),
        )
        .unwrap();
    assert_eq!(available - remaining(&f, q.purpose), 3);
    let paid = f.state.clone();
    assert!(
        f.state
            .issue_test_session_tool_nonce(
                proof("raw-B", &different, &other_receipt),
                &URL_SAFE_NO_PAD.encode([131; 32]),
                UnixMillis::new(12_002)
            )
            .is_err()
    );
    assert_eq!(f.state, paid);
    restore(&f.state);
}
#[test]
fn tool_nonce_expiry_is_independently_clamped_to_original_root_deadline() {
    let (mut f, _, token) = claimed();
    let q = request(SessionPurposeV1::ReconcileRead, "deadline");
    let deadline = f.state.test_sessions()[ID].roots[&q.purpose].deadline;
    let now = deadline.get() - 5_000;
    let (body, receipt) = signed(&f, &q, &token, now);
    f.state
        .charge_test_session_tool_nonce_http_attempt(
            proof("deadline-A", &body, &receipt),
            UnixMillis::new(now),
        )
        .unwrap();
    let c = f
        .state
        .issue_test_session_tool_nonce(
            proof("deadline-A", &body, &receipt),
            &URL_SAFE_NO_PAD.encode([132; 32]),
            UnixMillis::new(now),
        )
        .unwrap();
    assert_eq!(c.expires_at, deadline);
    restore(&f.state);
}
