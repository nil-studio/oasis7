//! Strong Core feature tests; these do not claim actual Native/actor transport.
use super::*;
#[path = "test_session_live_context_rejection_tests.rs"]
mod rejection_tests;
#[path = "test_session_live_context_restore_tests.rs"]
mod restore_tests;
const SOURCE: &str = "live-component-source";
const READS: [&str; 2] = ["live-component-before", "live-component-after"];
struct Ready {
    f: SessionFixture,
    body: ValidatedContextBody,
    record: ContextBodyRecordV1,
    captured: ws::SignedServiceResponse<CapturedTaskContextV1>,
    execution: String,
}
fn pair(
    state: &mut AccountProvisionApprovalState,
    body: &ValidatedContextBody,
    record: &ContextBodyRecordV1,
    current: &TestSessionLiveCurrent<'_, '_>,
    ids: [&str; 2],
    ports: [ContextBodyPortV1; 2],
    at: u64,
) -> ValidatedContextBody {
    let mut reopened = None;
    for (attempt, port) in ids.into_iter().zip(ports) {
        state
            .claim_test_session_context_read(
                ID,
                ContextBodyRead {
                    slot: body.scope().slot,
                    port,
                    attempt,
                },
                current,
                UnixMillis::new(at),
                &mut StrictCrypto,
            )
            .unwrap();
        // Explicit component fixture observation; no claim that an actor HTTP ran.
        state
            .record_test_session_context_storage_fact(ID, attempt, record.storage_fact().unwrap())
            .unwrap();
        reopened = Some(
            state
                .reopen_test_session_context_body(
                    ID,
                    attempt,
                    body.canonical_bytes(),
                    current,
                    UnixMillis::new(at),
                    &mut StrictCrypto,
                )
                .unwrap(),
        );
    }
    reopened.unwrap()
}
fn ready() -> Ready {
    let (mut f, execution, _) = claimed();
    let (original, prepared) = prepared_artifact(&f);
    let life = exec_source(&mut f, "live-prepare-source", "d1", 12_000);
    let policy = Policy::new(f.state.draft());
    let provision = policy.current(&f.context);
    let pin = f.pin.clone();
    let ns = scope(&pin);
    let current = live(&provision, &ns, &life, "live-prepare-source", 12_002);
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
            &prepared,
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
                attempt: "live-prepare-put",
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
            "live-prepare-put",
            record.storage_fact().unwrap(),
        )
        .unwrap();
    let body = pair(
        &mut f.state,
        &body,
        &record,
        &current,
        ["live-prepare-before", "live-prepare-after"],
        [ContextBodyPortV1::ReadBefore, ContextBodyPortV1::ReadAfter],
        12_002,
    );
    let (get, prefix, captured) = captured_artifact(&f, &prepared.payload, &original);
    let life = exec_source(&mut f, "live-get-source", "d2", 12_010);
    let policy = Policy::new(f.state.draft());
    let provision = policy.current(&f.context);
    let current = live(&provision, &ns, &life, "live-get-source", 12_012);
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
            &mut StrictCrypto,
        )
        .unwrap();
    let body = f
        .state
        .validate_test_session_captured_body(
            ID,
            &captured,
            &current,
            UnixMillis::new(12_012),
            &mut StrictCrypto,
        )
        .unwrap();
    let record = envelope(body.scope(), body.canonical_bytes().len());
    f.state
        .claim_test_session_context_write(
            ID,
            ContextBodyWrite {
                attempt: "live-captured-put",
                body: &body,
                candidate: &record,
            },
            &current,
            UnixMillis::new(12_012),
            &mut StrictCrypto,
        )
        .unwrap();
    f.state
        .record_test_session_context_storage_fact(
            ID,
            "live-captured-put",
            record.storage_fact().unwrap(),
        )
        .unwrap();
    let body = pair(
        &mut f.state,
        &body,
        &record,
        &current,
        ["live-captured-before", "live-captured-after"],
        [ContextBodyPortV1::ReadBefore, ContextBodyPortV1::ReadAfter],
        12_012,
    );
    restore(&f.state);
    Ready {
        f,
        body,
        record,
        captured,
        execution,
    }
}
fn fresh_response(
    descriptor: &LiveContextReadDescriptor,
    captured: &ws::SignedServiceResponse<CapturedTaskContextV1>,
) -> ws::SignedServiceResponse<CapturedTaskContextV1> {
    let response = ws::sign_service_response(
        TASK_CONTEXT_LIVE_READ_PATH,
        ws::request_digest(TASK_CONTEXT_LIVE_READ_PATH, &descriptor.request).unwrap(),
        captured.payload.clone(),
        &"3f".repeat(32),
    )
    .unwrap();
    verify_live_context_reply(&descriptor.request, &response, &descriptor_world_key()).unwrap();
    response
}
fn descriptor_world_key() -> String {
    ws::sign_read_request("fixture", (), &"3f".repeat(32))
        .unwrap()
        .subject_public_key
}
fn claim() -> LiveContextReadClaim<'static> {
    // Public fixture nonces, never a production RNG oracle.
    let input = LiveContextReadClaim {
        attempt: "live-observation-A",
        observation_nonce: "f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2f2",
        gateway_observation_nonce: "f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3f3",
        historical_reads: READS,
    };
    assert_eq!(input.observation_nonce.len(), 64);
    assert_eq!(input.gateway_observation_nonce.len(), 64);
    input
}

fn evidence<'a>(
    body: &'a ValidatedContextBody,
    response: &'a ws::SignedServiceResponse<CapturedTaskContextV1>,
) -> LiveContextReadEvidence<'a> {
    LiveContextReadEvidence {
        historical_body: body,
        response,
    }
}
#[test]
fn live_context_core_paid_history_fresh_reply_once_and_exact_archive() {
    let mut r = ready();
    let life = exec_source(&mut r.f, SOURCE, "f1", 12_020);
    let policy = Policy::new(r.f.state.draft());
    let provision = policy.current(&r.f.context);
    let pin = r.f.pin.clone();
    let ns = scope(&pin);
    let current = live(&provision, &ns, &life, SOURCE, 12_022);
    let body = pair(
        &mut r.f.state,
        &r.body,
        &r.record,
        &current,
        READS,
        [ContextBodyPortV1::RecoverBody; 2],
        12_022,
    );
    let old = r.f.state.test_sessions()[ID]
        .context
        .as_ref()
        .unwrap()
        .clone();
    let budgets = r.f.state.test_sessions()[ID].roots[&SessionPurposeV1::Execution]
        .budgets
        .clone();
    let descriptor =
        r.f.state
            .claim_test_session_live_context_read(
                ID,
                claim(),
                &current,
                UnixMillis::new(12_022),
                &mut StrictCrypto,
            )
            .unwrap();
    let session = &r.f.state.test_sessions()[ID];
    assert_eq!(
        session.roots[&SessionPurposeV1::Execution]
            .budgets
            .transport
            .remaining()
            .unwrap(),
        budgets.transport.remaining().unwrap() - 3
    );
    assert_eq!(
        session.roots[&SessionPurposeV1::Execution].budgets.requests,
        budgets.requests
    );
    assert_eq!(
        session.roots[&SessionPurposeV1::Execution].budgets.business,
        budgets.business
    );
    assert_eq!(descriptor.paid_ports.len(), 3);
    let response = fresh_response(&descriptor, &r.captured);
    let before_archive = r.f.state.clone();
    let checks = r
        .f
        .state
        .test_session_live_context_archive_checks(ID, claim().attempt, &evidence(&body, &response))
        .unwrap();
    assert_eq!(checks.checks().len(), 8);
    assert!(
        checks
            .checks()
            .iter()
            .any(|j| j.key_id().contains(TASK_CONTEXT_LIVE_READ_PATH))
    );
    r.f.state
        .archive_test_session_live_context_read(
            ID,
            claim().attempt,
            evidence(&body, &response),
            UnixMillis::new(12_023),
            &mut ExactCache::verified(checks),
        )
        .unwrap();
    assert_ne!(r.f.state, before_archive);
    let archived = r.f.state.clone();
    r.f.state
        .archive_test_session_live_context_read(
            ID,
            claim().attempt,
            evidence(&body, &response),
            UnixMillis::new(12_024),
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(r.f.state, archived);
    let ticket =
        r.f.state
            .reserve_test_session_source_read(ID, SOURCE, UnixMillis::new(12_024))
            .unwrap();
    let mut after = live(&provision, &ns, &life, SOURCE, 12_024);
    after.source_ticket = ticket;
    let checks =
        r.f.state
            .test_session_live_context_checks(
                ID,
                claim().attempt,
                &evidence(&body, &response),
                &after,
                UnixMillis::new(12_024),
            )
            .unwrap();
    let _opaque =
        r.f.state
            .admit_test_session_live_context_read(
                ID,
                claim().attempt,
                evidence(&body, &response),
                &after,
                UnixMillis::new(12_024),
                &mut ExactCache::verified(checks),
            )
            .unwrap();
    let admitted = r.f.state.clone();
    assert!(
        r.f.state
            .admit_test_session_live_context_read(
                ID,
                claim().attempt,
                evidence(&body, &response),
                &after,
                UnixMillis::new(12_024),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(r.f.state, admitted);
    let now = r.f.state.test_sessions()[ID].context.as_ref().unwrap();
    assert!(
        now.original == old.original
            && now.get == old.get
            && now.slots == old.slots
            && now.prepared_source == old.prepared_source
            && now.captured_source == old.captured_source
    );
    restore(&r.f.state);
}

struct LiveReady {
    r: Ready,
    body: ValidatedContextBody,
    life: SignedManagedNativeSessionLifetimeV1,
    descriptor: LiveContextReadDescriptor,
    response: ws::SignedServiceResponse<CapturedTaskContextV1>,
    before_claim: AccountProvisionApprovalState,
}
fn live_ready() -> LiveReady {
    let mut r = ready();
    let life = exec_source(&mut r.f, SOURCE, "f1", 12_020);
    let policy = Policy::new(r.f.state.draft());
    let provision = policy.current(&r.f.context);
    let pin = r.f.pin.clone();
    let ns = scope(&pin);
    let current = live(&provision, &ns, &life, SOURCE, 12_022);
    let body = pair(
        &mut r.f.state,
        &r.body,
        &r.record,
        &current,
        READS,
        [ContextBodyPortV1::RecoverBody; 2],
        12_022,
    );
    let before_claim = r.f.state.clone();
    let descriptor =
        r.f.state
            .claim_test_session_live_context_read(
                ID,
                claim(),
                &current,
                UnixMillis::new(12_022),
                &mut StrictCrypto,
            )
            .unwrap();
    let response = fresh_response(&descriptor, &r.captured);
    LiveReady {
        r,
        body,
        life,
        descriptor,
        response,
        before_claim,
    }
}
