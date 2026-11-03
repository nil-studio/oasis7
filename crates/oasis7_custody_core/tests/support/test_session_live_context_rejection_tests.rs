use super::*;
#[test]
fn live_context_core_rejects_old_domain_nonce_other_get_semantics_and_wrong_cache_atomically() {
    let mut v = live_ready();
    let key = descriptor_world_key();
    verify_private_reply(
        TASK_CONTEXT_READ_PATH,
        &v.descriptor.request.original,
        &v.r.captured,
        &key,
    )
    .unwrap();
    v.r.captured
        .payload
        .validate_against(&v.descriptor.request.original)
        .unwrap();
    let before = v.r.f.state.clone();
    assert!(
        v.r.f
            .state
            .archive_test_session_live_context_read(
                ID,
                claim().attempt,
                evidence(&v.body, &v.r.captured),
                UnixMillis::new(12_023),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(v.r.f.state, before);
    let mut other = v.descriptor.request.clone();
    other.observation_nonce = "e4".repeat(32);
    let wrong_nonce = ws::sign_service_response(
        TASK_CONTEXT_LIVE_READ_PATH,
        ws::request_digest(TASK_CONTEXT_LIVE_READ_PATH, &other).unwrap(),
        v.response.payload.clone(),
        &"3f".repeat(32),
    )
    .unwrap();
    verify_live_context_reply(&other, &wrong_nonce, &key).unwrap();
    assert!(
        v.r.f
            .state
            .archive_test_session_live_context_read(
                ID,
                claim().attempt,
                evidence(&v.body, &wrong_nonce),
                UnixMillis::new(12_023),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(v.r.f.state, before);
    other = v.descriptor.request.clone();
    let mut get = other.original.request.clone();
    get.prepare_operation_id.push_str("-other");
    other.original = ws::sign_read_request(TASK_CONTEXT_READ_PATH, get, &"41".repeat(32)).unwrap();
    verify_live_context_read(&other).unwrap();
    let wrong_get = ws::sign_service_response(
        TASK_CONTEXT_LIVE_READ_PATH,
        ws::request_digest(TASK_CONTEXT_LIVE_READ_PATH, &other).unwrap(),
        v.response.payload.clone(),
        &"3f".repeat(32),
    )
    .unwrap();
    verify_private_reply(TASK_CONTEXT_LIVE_READ_PATH, &other, &wrong_get, &key).unwrap();
    assert!(
        v.r.f
            .state
            .archive_test_session_live_context_read(
                ID,
                claim().attempt,
                evidence(&v.body, &wrong_get),
                UnixMillis::new(12_023),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(v.r.f.state, before);
    let mut malformed = v.response.payload.clone();
    malformed.prepared.reference_salt = "00".repeat(32);
    let malformed = ws::sign_service_response(
        TASK_CONTEXT_LIVE_READ_PATH,
        v.response.request_digest.clone(),
        malformed,
        &"3f".repeat(32),
    )
    .unwrap();
    verify_private_reply(
        TASK_CONTEXT_LIVE_READ_PATH,
        &v.descriptor.request,
        &malformed,
        &key,
    )
    .unwrap();
    assert!(
        v.r.f
            .state
            .archive_test_session_live_context_read(
                ID,
                claim().attempt,
                evidence(&v.body, &malformed),
                UnixMillis::new(12_023),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(v.r.f.state, before);
    let mut nested = v.response.payload.clone();
    let payload = &mut nested.capture_proof.request.signed_payload;
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
            &signed.signature_hex
        )
        .is_err()
    );
    nested.capture_proof.request.correlation.payload_digest =
        ws::request_digest("intent", payload).unwrap();
    nested.capture_proof.request.correlation.validate().unwrap();
    let nested = ws::sign_service_response(
        TASK_CONTEXT_LIVE_READ_PATH,
        v.response.request_digest.clone(),
        nested,
        &"3f".repeat(32),
    )
    .unwrap();
    verify_private_reply(
        TASK_CONTEXT_LIVE_READ_PATH,
        &v.descriptor.request,
        &nested,
        &key,
    )
    .unwrap();
    assert!(
        v.r.f
            .state
            .archive_test_session_live_context_read(
                ID,
                claim().attempt,
                evidence(&v.body, &nested),
                UnixMillis::new(12_023),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(v.r.f.state, before);
    // A different, independently valid response cannot borrow another manifest.
    let checks =
        v.r.f
            .state
            .test_session_live_context_archive_checks(
                ID,
                claim().attempt,
                &evidence(&v.body, &v.response),
            )
            .unwrap();
    let mut payload = v.response.payload.clone();
    payload.current_public_context.actor_observation["new-observation"] =
        serde_json::Value::Bool(true);
    let alternate = ws::sign_service_response(
        TASK_CONTEXT_LIVE_READ_PATH,
        v.response.request_digest.clone(),
        payload,
        &"3f".repeat(32),
    )
    .unwrap();
    verify_live_context_reply(&v.descriptor.request, &alternate, &key).unwrap();
    assert!(
        v.r.f
            .state
            .archive_test_session_live_context_read(
                ID,
                claim().attempt,
                evidence(&v.body, &alternate),
                UnixMillis::new(12_023),
                &mut ExactCache::verified(checks)
            )
            .is_err()
    );
    assert_eq!(v.r.f.state, before);
    v.r.f
        .state
        .archive_test_session_live_context_read(
            ID,
            claim().attempt,
            evidence(&v.body, &v.response),
            UnixMillis::new(12_023),
            &mut StrictCrypto,
        )
        .unwrap();
    let archived = v.r.f.state.clone();
    assert!(
        v.r.f
            .state
            .archive_test_session_live_context_read(
                ID,
                claim().attempt,
                evidence(&v.body, &alternate),
                UnixMillis::new(12_024),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(v.r.f.state, archived);
}
#[test]
fn live_context_core_requires_post_archive_current_same_source_exact_fact_and_new_ticket() {
    let mut v = live_ready();
    let policy = Policy::new(v.r.f.state.draft());
    let context = v.r.f.context.clone();
    let provision = policy.current(&context);
    let pin = v.r.f.pin.clone();
    let ns = scope(&pin);
    let before = v.r.f.state.clone();
    let current = live(&provision, &ns, &v.life, SOURCE, 12_024);
    assert!(
        v.r.f
            .state
            .admit_test_session_live_context_read(
                ID,
                claim().attempt,
                evidence(&v.body, &v.response),
                &current,
                UnixMillis::new(12_024),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(v.r.f.state, before);
    v.r.f
        .state
        .archive_test_session_live_context_read(
            ID,
            claim().attempt,
            evidence(&v.body, &v.response),
            UnixMillis::new(12_023),
            &mut StrictCrypto,
        )
        .unwrap();
    let ticket =
        v.r.f
            .state
            .reserve_test_session_source_read(ID, SOURCE, UnixMillis::new(12_024))
            .unwrap();
    let archived = v.r.f.state.clone();
    let mut old_observation = live(&provision, &ns, &v.life, SOURCE, 12_022);
    old_observation.source_ticket = ticket;
    v.r.f
        .state
        .test_session_live_checks(
            ID,
            SessionPurposeV1::Execution,
            &old_observation,
            UnixMillis::new(12_024),
        )
        .unwrap();
    assert!(
        v.r.f
            .state
            .admit_test_session_live_context_read(
                ID,
                claim().attempt,
                evidence(&v.body, &v.response),
                &old_observation,
                UnixMillis::new(12_024),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(v.r.f.state, archived);
    let old_ticket = live(&provision, &ns, &v.life, SOURCE, 12_024);
    assert!(
        v.r.f
            .state
            .admit_test_session_live_context_read(
                ID,
                claim().attempt,
                evidence(&v.body, &v.response),
                &old_ticket,
                UnixMillis::new(12_024),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(v.r.f.state, archived);
    let life = exec_source(&mut v.r.f, "other-valid-live-source", "e5", 12_023);
    let policy = Policy::new(v.r.f.state.draft());
    let provision = policy.current(&context);
    let mut other = live(&provision, &ns, &life, "other-valid-live-source", 12_025);
    other.source_ticket = v
        .r
        .f
        .state
        .reserve_test_session_source_read(ID, "other-valid-live-source", UnixMillis::new(12_025))
        .unwrap();
    v.r.f
        .state
        .test_session_live_checks(
            ID,
            SessionPurposeV1::Execution,
            &other,
            UnixMillis::new(12_025),
        )
        .unwrap();
    let snapshot = v.r.f.state.clone();
    assert!(
        v.r.f
            .state
            .admit_test_session_live_context_read(
                ID,
                claim().attempt,
                evidence(&v.body, &v.response),
                &other,
                UnixMillis::new(12_025),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(v.r.f.state, snapshot);
}
#[test]
fn live_context_core_late_frozen_archive_never_renews_source_or_original_deadline() {
    let mut v = live_ready();
    v.r.f.state.freeze(1).unwrap();
    v.r.f
        .state
        .archive_test_session_live_context_read(
            ID,
            claim().attempt,
            evidence(&v.body, &v.response),
            UnixMillis::new(60_000),
            &mut StrictCrypto,
        )
        .unwrap();
    restore(&v.r.f.state);
    let policy = Policy::new(v.r.f.state.draft());
    let provision = policy.current(&v.r.f.context);
    let pin = v.r.f.pin.clone();
    let ns = scope(&pin);
    let current = live(&provision, &ns, &v.life, SOURCE, 60_000);
    let before = v.r.f.state.clone();
    assert!(
        v.r.f
            .state
            .admit_test_session_live_context_read(
                ID,
                claim().attempt,
                evidence(&v.body, &v.response),
                &current,
                UnixMillis::new(60_000),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(v.r.f.state, before);
}
#[test]
fn live_context_core_expired_source_and_original_deadline_fail_without_mutation() {
    let mut v = live_ready();
    v.r.f
        .state
        .archive_test_session_live_context_read(
            ID,
            claim().attempt,
            evidence(&v.body, &v.response),
            UnixMillis::new(12_023),
            &mut StrictCrypto,
        )
        .unwrap();
    let ticket =
        v.r.f
            .state
            .reserve_test_session_source_read(ID, SOURCE, UnixMillis::new(12_024))
            .unwrap();
    let policy = Policy::new(v.r.f.state.draft());
    let provision = policy.current(&v.r.f.context);
    let pin = v.r.f.pin.clone();
    let ns = scope(&pin);
    for at in [17_023, 50_000] {
        let mut expired = live(&provision, &ns, &v.life, SOURCE, at);
        expired.source_ticket = ticket;
        let before = v.r.f.state.clone();
        assert!(
            v.r.f
                .state
                .admit_test_session_live_context_read(
                    ID,
                    claim().attempt,
                    evidence(&v.body, &v.response),
                    &expired,
                    UnixMillis::new(at),
                    &mut StrictCrypto
                )
                .is_err()
        );
        assert_eq!(v.r.f.state, before);
    }
}
