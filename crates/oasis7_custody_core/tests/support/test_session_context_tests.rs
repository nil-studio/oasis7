use super::*;
use oasis7_client_api::world_service::task_context::*;
use oasis7_custody_core::envelope::*;

#[path = "test_session_context_fixture.rs"]
mod fixture;
use fixture::*;

#[test]
fn context_body_native_sdk_two_slots_six_paid_ports_and_strict_restore() {
    let (mut f, _, _) = claimed();
    let (original, prepared) = prepared_artifact(&f);
    let life = exec_source(&mut f, "context-prepare-source", "d1", 12_000);
    let policy = Policy::new(f.state.draft());
    let provision = policy.current(&f.context);
    let pin = f.pin.clone();
    let ns = scope(&pin);
    let current = live(&provision, &ns, &life, "context-prepare-source", 12_002);
    let before = f.state.test_sessions()[ID].roots.clone();
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
    let prepared_ref = f
        .state
        .claim_test_session_context_write(
            ID,
            ContextBodyWrite {
                attempt: "context-put-prepared",
                body: &body,
                candidate: &record,
            },
            &current,
            UnixMillis::new(12_002),
            &mut StrictCrypto,
        )
        .unwrap();
    let after_write = f.state.clone();
    assert!(
        f.state
            .claim_test_session_context_write(
                ID,
                ContextBodyWrite {
                    attempt: "second-put",
                    body: &body,
                    candidate: &record
                },
                &current,
                UnixMillis::new(12_002),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, after_write);
    f.state
        .record_test_session_context_storage_fact(
            ID,
            "context-put-prepared",
            record.storage_fact().unwrap(),
        )
        .unwrap();
    for (port, attempt) in [
        (ContextBodyPortV1::ReadBefore, "prepared-read-before"),
        (ContextBodyPortV1::ReadAfter, "prepared-read-after"),
    ] {
        f.state
            .claim_test_session_context_read(
                ID,
                ContextBodyRead {
                    slot: ContextBodySlotKindV1::Prepared,
                    port,
                    attempt,
                },
                &current,
                UnixMillis::new(12_002),
                &mut StrictCrypto,
            )
            .unwrap();
        let reopened = f
            .state
            .reopen_test_session_context_body(
                ID,
                attempt,
                body.canonical_bytes(),
                &current,
                UnixMillis::new(12_002),
                &mut StrictCrypto,
            )
            .unwrap();
        assert_eq!(reopened.canonical_bytes(), body.canonical_bytes());
    }
    let (get, prefix, captured) = captured_artifact(&f, &prepared.payload, &original);
    let life2 = exec_source(&mut f, "context-get-source", "d2", 12_010);
    let policy = Policy::new(f.state.draft());
    let provision = policy.current(&f.context);
    let current = live(&provision, &ns, &life2, "context-get-source", 12_012);
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
    let captured_body = f
        .state
        .validate_test_session_captured_body(
            ID,
            &captured,
            &current,
            UnixMillis::new(12_012),
            &mut StrictCrypto,
        )
        .unwrap();
    let captured_record = envelope(captured_body.scope(), captured_body.canonical_bytes().len());
    f.state
        .claim_test_session_context_write(
            ID,
            ContextBodyWrite {
                attempt: "context-put-captured",
                body: &captured_body,
                candidate: &captured_record,
            },
            &current,
            UnixMillis::new(12_012),
            &mut StrictCrypto,
        )
        .unwrap();
    f.state
        .record_test_session_context_storage_fact(
            ID,
            "context-put-captured",
            captured_record.storage_fact().unwrap(),
        )
        .unwrap();
    for (port, attempt) in [
        (ContextBodyPortV1::ReadBefore, "captured-read-before"),
        (ContextBodyPortV1::ReadAfter, "captured-read-after"),
    ] {
        f.state
            .claim_test_session_context_read(
                ID,
                ContextBodyRead {
                    slot: ContextBodySlotKindV1::Captured,
                    port,
                    attempt,
                },
                &current,
                UnixMillis::new(12_012),
                &mut StrictCrypto,
            )
            .unwrap();
        f.state
            .reopen_test_session_context_body(
                ID,
                attempt,
                captured_body.canonical_bytes(),
                &current,
                UnixMillis::new(12_012),
                &mut StrictCrypto,
            )
            .unwrap();
    }
    let s = &f.state.test_sessions()[ID];
    assert_eq!(s.context.as_ref().unwrap().slots.len(), 2);
    assert_eq!(s.context.as_ref().unwrap().attempts.len(), 6);
    assert_eq!(
        before[&SessionPurposeV1::Execution]
            .budgets
            .transport
            .remaining()
            .unwrap()
            - s.roots[&SessionPurposeV1::Execution]
                .budgets
                .transport
                .remaining()
                .unwrap(),
        7,
        "six actor ports plus the separately paid Get-stage Source"
    );
    assert_eq!(
        before[&SessionPurposeV1::Execution]
            .budgets
            .requests
            .remaining()
            .unwrap()
            - s.roots[&SessionPurposeV1::Execution]
                .budgets
                .requests
                .remaining()
                .unwrap(),
        1
    );
    assert_eq!(
        before[&SessionPurposeV1::ReconcileRead],
        s.roots[&SessionPurposeV1::ReconcileRead]
    );
    assert_eq!(
        before[&SessionPurposeV1::TerminationPlan],
        s.roots[&SessionPurposeV1::TerminationPlan]
    );
    assert_eq!(
        prepared_ref.plaintext_bytes.0,
        body.canonical_bytes().len() as u64
    );
    restore(&f.state);
    restore_cases::assert_context_ledger_restore(
        &f.state,
        &before[&SessionPurposeV1::Execution].budgets.requests,
    );
}

#[test]
fn context_body_native_artifact_tamper_size_unknown_and_paid_recovery() {
    let (mut f, _, _) = claimed();
    let (original, response) = prepared_artifact(&f);
    let life = exec_source(&mut f, "context-negative-source", "d3", 12_000);
    let policy = Policy::new(f.state.draft());
    let provision = policy.current(&f.context);
    let pin = f.pin.clone();
    let ns = scope(&pin);
    let current = live(&provision, &ns, &life, "context-negative-source", 12_002);
    f.state
        .register_test_session_context(
            ID,
            original.clone(),
            &current,
            UnixMillis::new(12_002),
            &mut StrictCrypto,
        )
        .unwrap();
    let before = f.state.clone();
    assert!(response.request_digest.starts_with("blake3:"));
    assert_eq!(response.request_digest.len(), 71);
    let wrong_digest = ws::sign_service_response(
        TASK_CONTEXT_PREPARE_PATH,
        format!("blake3:{}", "00".repeat(32)),
        response.payload.clone(),
        &"3f".repeat(32),
    )
    .unwrap();
    // Strong signature alone is insufficient: the exact SDK request binding
    // must still match the immutable registered Prepare original.
    ws::private_goal::verify_owner_signature_strict(
        TASK_CONTEXT_PREPARE_PATH,
        &(&wrong_digest.request_digest, &wrong_digest.payload),
        &f.state.draft().world_identity.service_public_key,
        &wrong_digest.signature_hex,
    )
    .unwrap();
    assert!(
        f.state
            .validate_test_session_prepared_body(
                ID,
                &wrong_digest,
                &current,
                UnixMillis::new(12_002),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    let mut bad = response.clone();
    bad.signature_hex = "00".repeat(64);
    assert!(
        f.state
            .validate_test_session_prepared_body(
                ID,
                &bad,
                &current,
                UnixMillis::new(12_002),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    let mut wrong = response.payload.clone();
    wrong.identity.fence.branch_id = "foreign-branch".into();
    let strong_wrong = ws::sign_service_response(
        TASK_CONTEXT_PREPARE_PATH,
        response.request_digest.clone(),
        wrong,
        &"3f".repeat(32),
    )
    .unwrap();
    verify_private_reply(
        TASK_CONTEXT_PREPARE_PATH,
        &original,
        &strong_wrong,
        &f.state.draft().world_identity.service_public_key,
    )
    .unwrap();
    assert!(
        f.state
            .validate_test_session_prepared_body(
                ID,
                &strong_wrong,
                &current,
                UnixMillis::new(12_002),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    let mut oversized = response.payload.clone();
    oversized.snapshot.actor_observation["bounded-context-test-padding"] =
        serde_json::Value::String("x".repeat(CONTEXT_BODY_MAX_BYTES));
    oversized.identity.request_commitment = oversized
        .commitment("oasis7/private-task-request/v1")
        .unwrap();
    oversized.identity.context_commitment = oversized
        .commitment("oasis7/private-task-context/v1")
        .unwrap();
    oversized.identity.provider_invocation_commitment = oversized
        .commitment("oasis7/private-task-invocation/v1")
        .unwrap();
    oversized.validate_against(&original).unwrap();
    let oversized = ws::sign_service_response(
        TASK_CONTEXT_PREPARE_PATH,
        response.request_digest.clone(),
        oversized,
        &"3f".repeat(32),
    )
    .unwrap();
    assert!(
        f.state
            .validate_test_session_prepared_body(
                ID,
                &oversized,
                &current,
                UnixMillis::new(12_002),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, before);
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
                attempt: "unknown-put-once",
                body: &body,
                candidate: &record,
            },
            &current,
            UnixMillis::new(12_002),
            &mut StrictCrypto,
        )
        .unwrap();
    // Lost PUT reply remains pending. Recovery reads the exact same slot; it
    // never seals/replaces the candidate or creates another write permission.
    let after_unknown = f.state.clone();
    assert!(
        f.state
            .claim_test_session_context_write(
                ID,
                ContextBodyWrite {
                    attempt: "unknown-put-again",
                    body: &body,
                    candidate: &record
                },
                &current,
                UnixMillis::new(12_002),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, after_unknown);
    f.state
        .claim_test_session_context_read(
            ID,
            ContextBodyRead {
                slot: ContextBodySlotKindV1::Prepared,
                port: ContextBodyPortV1::RecoverBody,
                attempt: "recover-original-A",
            },
            &current,
            UnixMillis::new(12_002),
            &mut StrictCrypto,
        )
        .unwrap();
    f.state
        .record_test_session_context_storage_fact(
            ID,
            "recover-original-A",
            record.storage_fact().unwrap(),
        )
        .unwrap();
    f.state
        .reopen_test_session_context_body(
            ID,
            "recover-original-A",
            body.canonical_bytes(),
            &current,
            UnixMillis::new(12_002),
            &mut StrictCrypto,
        )
        .unwrap();
    let charged = f.state.clone();
    assert!(
        f.state
            .claim_test_session_context_read(
                ID,
                ContextBodyRead {
                    slot: ContextBodySlotKindV1::Prepared,
                    port: ContextBodyPortV1::RecoverBody,
                    attempt: "recover-original-A"
                },
                &current,
                UnixMillis::new(12_002),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, charged);
    f.state
        .claim_test_session_context_read(
            ID,
            ContextBodyRead {
                slot: ContextBodySlotKindV1::Prepared,
                port: ContextBodyPortV1::RecoverBody,
                attempt: "recover-original-B",
            },
            &current,
            UnixMillis::new(12_002),
            &mut StrictCrypto,
        )
        .unwrap();
    let c = f.state.test_sessions()[ID].context.as_ref().unwrap();
    assert_eq!(c.attempts.len(), 3);
    assert_eq!(c.slots.len(), 1);
    assert!(c.attempts["unknown-put-once"].observation.is_none());
    restore(&f.state);
}

#[path = "test_session_context_restore_cases.rs"]
mod restore_cases;

#[path = "test_session_context_signature_tests.rs"]
mod signature_tests;

#[path = "test_session_context_export.rs"]
mod export;
