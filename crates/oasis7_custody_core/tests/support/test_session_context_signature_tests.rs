use super::*;

#[test]
fn context_body_exact_cache_binds_all_prepared_signatures_body_scope_and_current() {
    let (mut f, _, _) = claimed();
    let (original, response) = prepared_artifact(&f);
    let life = exec_source(&mut f, "context-crypto-source", "d5", 12_000);
    let policy = Policy::new(f.state.draft());
    let provision = policy.current(&f.context);
    let pin = f.pin.clone();
    let ns = scope(&pin);
    let current = live(&provision, &ns, &life, "context-crypto-source", 12_002);
    let original_checks = f
        .state
        .test_session_context_original_checks(ID, &original, &current, UnixMillis::new(12_002))
        .unwrap();
    f.state
        .register_test_session_context(
            ID,
            original.clone(),
            &current,
            UnixMillis::new(12_002),
            &mut ExactCache::verified(original_checks.clone()),
        )
        .unwrap();
    let base = f
        .state
        .test_session_live_checks(
            ID,
            SessionPurposeV1::Execution,
            &current,
            UnixMillis::new(12_002),
        )
        .unwrap();
    let checks = f
        .state
        .test_session_prepared_context_checks(ID, &response, &current, UnixMillis::new(12_002))
        .unwrap();
    assert_eq!(checks.checks().len(), base.checks().len() + 3);
    let owner = &response.payload.snapshot.active_goal.owner_proof;
    let jobs = &checks.checks()[base.checks().len()..];
    assert!(
        jobs[0].canonical_bytes()
            == ws::signing_bytes(TASK_CONTEXT_PREPARE_PATH, &original.request).unwrap()
    );
    assert!(
        jobs[1].canonical_bytes()
            == ws::signing_bytes(
                oasis7_agent_api::game::GAME_GOAL_PREPARE_DOMAIN,
                &owner.request
            )
            .unwrap()
    );
    assert!(
        jobs[2].canonical_bytes()
            == ws::signing_bytes(
                TASK_CONTEXT_PREPARE_PATH,
                &(&response.request_digest, &response.payload)
            )
            .unwrap()
    );
    let before = f.state.clone();
    // A genuinely verified subset is still insufficient; no caller can mark
    // the missing nested owner/body jobs as "verified" in serialized input.
    assert!(
        f.state
            .validate_test_session_prepared_body(
                ID,
                &response,
                &current,
                UnixMillis::new(12_002),
                &mut ExactCache::verified(original_checks)
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    let mut alternate = response.payload.clone();
    alternate.snapshot.actor_observation["alternate-valid-body"] = true.into();
    resign_commitments(&mut alternate);
    alternate.validate_against(&original).unwrap();
    let alternate = ws::sign_service_response(
        TASK_CONTEXT_PREPARE_PATH,
        response.request_digest.clone(),
        alternate,
        &"3f".repeat(32),
    )
    .unwrap();
    let swapped = f
        .state
        .test_session_prepared_context_checks(ID, &alternate, &current, UnixMillis::new(12_002))
        .unwrap();
    assert!(checks != swapped);
    assert!(
        f.state
            .validate_test_session_prepared_body(
                ID,
                &response,
                &current,
                UnixMillis::new(12_002),
                &mut ExactCache::verified(swapped)
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    let mut nested_bad = response.payload.clone();
    nested_bad.snapshot.active_goal.owner_proof.signature_hex = "00".repeat(64);
    resign_commitments(&mut nested_bad);
    let nested_bad = ws::sign_service_response(
        TASK_CONTEXT_PREPARE_PATH,
        response.request_digest.clone(),
        nested_bad,
        &"3f".repeat(32),
    )
    .unwrap();
    ws::private_goal::verify_owner_signature_strict(
        TASK_CONTEXT_PREPARE_PATH,
        &(&nested_bad.request_digest, &nested_bad.payload),
        &f.state.draft().world_identity.service_public_key,
        &nested_bad.signature_hex,
    )
    .unwrap();
    assert!(
        f.state
            .test_session_prepared_context_checks(
                ID,
                &nested_bad,
                &current,
                UnixMillis::new(12_002)
            )
            .is_err()
    );
    let body = f
        .state
        .validate_test_session_prepared_body(
            ID,
            &response,
            &current,
            UnixMillis::new(12_002),
            &mut ExactCache::verified(checks.clone()),
        )
        .unwrap();
    let record = envelope(body.scope(), body.canonical_bytes().len());
    let mut wrong_scope = record.clone();
    wrong_scope.scope.session_id = "foreign-session".into();
    assert!(
        f.state
            .claim_test_session_context_write(
                ID,
                ContextBodyWrite {
                    attempt: "wrong-scope-write",
                    body: &body,
                    candidate: &wrong_scope
                },
                &current,
                UnixMillis::new(12_002),
                &mut ExactCache::verified(checks.clone())
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    let mut fresh_lifetime = life.clone();
    fresh_lifetime.lifetime.status.observed_at = DecimalU64(12_003);
    fresh_lifetime.lifetime.status.expires_at = DecimalU64(17_003);
    fresh_lifetime.signature = URL_SAFE_NO_PAD.encode(
        key()
            .sign(&fresh_lifetime.lifetime.signing_bytes().unwrap())
            .to_bytes(),
    );
    let fresh = live(
        &provision,
        &ns,
        &fresh_lifetime,
        "context-crypto-source",
        12_003,
    );
    let fresh_checks = f
        .state
        .test_session_prepared_context_checks(ID, &response, &fresh, UnixMillis::new(12_003))
        .unwrap();
    StrictCrypto.verify(&fresh_checks).unwrap();
    assert!(fresh_checks != checks);
    assert!(
        f.state
            .claim_test_session_context_write(
                ID,
                ContextBodyWrite {
                    attempt: "wrong-current-cache",
                    body: &body,
                    candidate: &record
                },
                &fresh,
                UnixMillis::new(12_003),
                &mut ExactCache::verified(checks.clone())
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    f.state
        .claim_test_session_context_write(
            ID,
            ContextBodyWrite {
                attempt: "crypto-paid-write",
                body: &body,
                candidate: &record,
            },
            &current,
            UnixMillis::new(12_002),
            &mut ExactCache::verified(checks),
        )
        .unwrap();
    restore(&f.state);
}
fn resign_commitments(prepared: &mut PreparedTaskContextV1) {
    prepared.identity.request_commitment = prepared
        .commitment("oasis7/private-task-request/v1")
        .unwrap();
    prepared.identity.context_commitment = prepared
        .commitment("oasis7/private-task-context/v1")
        .unwrap();
    prepared.identity.provider_invocation_commitment = prepared
        .commitment("oasis7/private-task-invocation/v1")
        .unwrap();
}

#[test]
fn context_body_captured_jobs_include_original_prepared_and_prefix_provenance() {
    let (mut f, _, _) = claimed();
    let (original, response) = prepared_artifact(&f);
    let life = exec_source(&mut f, "context-prepared-jobs", "d6", 12_000);
    let policy = Policy::new(f.state.draft());
    let provision = policy.current(&f.context);
    let pin = f.pin.clone();
    let ns = scope(&pin);
    let current = live(&provision, &ns, &life, "context-prepared-jobs", 12_002);
    f.state
        .register_test_session_context(
            ID,
            original.clone(),
            &current,
            UnixMillis::new(12_002),
            &mut StrictCrypto,
        )
        .unwrap();
    let body = f
        .state
        .validate_test_session_prepared_body(
            ID,
            &response,
            &current,
            UnixMillis::new(12_002),
            &mut StrictCrypto,
        )
        .unwrap();
    let record = envelope(body.scope(), body.canonical_bytes().len());
    f.state
        .claim_test_session_context_write(
            ID,
            ContextBodyWrite {
                attempt: "jobs-prepared-write",
                body: &body,
                candidate: &record,
            },
            &current,
            UnixMillis::new(12_002),
            &mut StrictCrypto,
        )
        .unwrap();
    f.state
        .record_test_session_context_storage_fact(
            ID,
            "jobs-prepared-write",
            record.storage_fact().unwrap(),
        )
        .unwrap();
    let (get, prefix, captured) = captured_artifact(&f, &response.payload, &original);
    let life2 = exec_source(&mut f, "context-captured-jobs", "d7", 12_010);
    let policy = Policy::new(f.state.draft());
    let provision = policy.current(&f.context);
    let current = live(&provision, &ns, &life2, "context-captured-jobs", 12_012);
    let get_checks = f
        .state
        .test_session_context_get_checks(
            ID,
            &body,
            &get,
            &prefix,
            &current,
            UnixMillis::new(12_012),
        )
        .unwrap();
    let base = f
        .state
        .test_session_live_checks(
            ID,
            SessionPurposeV1::Execution,
            &current,
            UnixMillis::new(12_012),
        )
        .unwrap();
    assert_eq!(get_checks.checks().len(), base.checks().len() + 6);
    f.state
        .register_test_session_context_get(
            ID,
            ContextGetRegistration {
                prepared: &body,
                original: &get,
                prefix: &prefix,
            },
            &current,
            UnixMillis::new(12_012),
            &mut ExactCache::verified(get_checks),
        )
        .unwrap();
    let checks = f
        .state
        .test_session_captured_context_checks(ID, &captured, &current, UnixMillis::new(12_012))
        .unwrap();
    assert_eq!(checks.checks().len(), base.checks().len() + 7);
    let outer_prepared = ws::signing_bytes(
        TASK_CONTEXT_PREPARE_PATH,
        &(&response.request_digest, &response.payload),
    )
    .unwrap();
    assert!(
        checks
            .checks()
            .iter()
            .any(|j| j.canonical_bytes() == outer_prepared)
    );
    let before = f.state.clone();
    let mut nested_bad = captured.payload.clone();
    let payload = &mut nested_bad.capture_proof.request.signed_payload;
    let oasis7_client_api::world_service_wire::WorldServicePayloadV1::Scheduler(signed) = payload
    else {
        unreachable!()
    };
    signed.signature_hex = "00".repeat(64);
    assert!(
        ws::private_goal::verify_owner_signature_strict(
            "scheduler",
            &signed.request,
            &signed.subject_public_key,
            &signed.signature_hex,
        )
        .is_err()
    );
    // The correlation key is unchanged (same subject/request); only the SDK
    // payload digest changes with signature bytes. The full derive operation
    // intentionally rejects this signature before comparing correlation, so it
    // must not be used as a successful fixture constructor for a bad signature.
    nested_bad.capture_proof.request.correlation.payload_digest =
        ws::request_digest("intent", payload).unwrap();
    nested_bad
        .capture_proof
        .request
        .correlation
        .validate()
        .unwrap();
    let nested_bad = ws::sign_service_response(
        TASK_CONTEXT_READ_PATH,
        captured.request_digest.clone(),
        nested_bad,
        &"3f".repeat(32),
    )
    .unwrap();
    ws::private_goal::verify_owner_signature_strict(
        TASK_CONTEXT_READ_PATH,
        &(&nested_bad.request_digest, &nested_bad.payload),
        &f.state.draft().world_identity.service_public_key,
        &nested_bad.signature_hex,
    )
    .unwrap();
    assert!(
        f.state
            .test_session_captured_context_checks(
                ID,
                &nested_bad,
                &current,
                UnixMillis::new(12_012)
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    assert!(
        f.state
            .validate_test_session_captured_body(
                ID,
                &captured,
                &current,
                UnixMillis::new(12_012),
                &mut ExactCache::verified(base)
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    let captured_body = f
        .state
        .validate_test_session_captured_body(
            ID,
            &captured,
            &current,
            UnixMillis::new(12_012),
            &mut ExactCache::verified(checks.clone()),
        )
        .unwrap();
    let candidate = envelope(captured_body.scope(), captured_body.canonical_bytes().len());
    f.state
        .claim_test_session_context_write(
            ID,
            ContextBodyWrite {
                attempt: "jobs-captured-write",
                body: &captured_body,
                candidate: &candidate,
            },
            &current,
            UnixMillis::new(12_012),
            &mut ExactCache::verified(checks),
        )
        .unwrap();
    restore(&f.state);
}
