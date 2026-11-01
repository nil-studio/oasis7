//! Real SDK signatures and genuine paid Book; archive is immutable history.
use super::*;
fn original(p: &PreparedService) -> ExecutionPrepareSignedOriginalV2 {
    use oasis7_client_api::world_service::{
        self as ws, entry_executor::EntryExecutorAccessV1, task_context::TASK_CONTEXT_PREPARE_PATH,
        task_context_projection::TaskContextPrepareAttestationRequestV2,
    };
    let ExecutionSigningActionV2::Prepare {
        original,
        observation_nonce,
    } = p.bundle.action.as_ref()
    else {
        unreachable!()
    };
    let value = ExecutionPrepareSignedOriginalV2 {
        access: EntryExecutorAccessV1 {
            read: ws::sign_read_request(
                ws::VIEW_JSON_PATH,
                p.bundle.observation.clone(),
                &"41".repeat(32),
            )
            .unwrap(),
            fence: p.bundle.fence.clone(),
        },
        request: TaskContextPrepareAttestationRequestV2 {
            original: ws::sign_read_request(
                TASK_CONTEXT_PREPARE_PATH,
                original.as_ref().clone(),
                &"41".repeat(32),
            )
            .unwrap(),
            observation_nonce: observation_nonce.clone(),
        },
    };
    p.bundle.verify_prepare_original(&value).unwrap();
    value
}
fn record(p: &mut PreparedService) {
    assert!(
        p.f.state
            .record_test_session_v2_prepare_service_permits(
                &p.f.session_id,
                p.proofs.clone(),
                &p.pin,
                &p.floor,
                UnixMillis::new(14_007)
            )
            .unwrap()
            .is_some()
    );
}
#[test]
fn session_v2_prepare_original_real_signatures_archive_cold_and_exact_history_never_dispatch() {
    let mut p = prepared_service(None, |_| {});
    let signed = original(&p);
    let before = p.f.state.clone();
    assert!(
        p.f.state
            .archive_test_session_v2_prepare_original(
                &p.f.session_id,
                signed.clone(),
                UnixMillis::new(14_008)
            )
            .is_err()
    );
    assert_eq!(before, p.f.state);
    record(&mut p);
    let paid = serde_json::to_value(&p.f.state).unwrap();
    p.f.state
        .archive_test_session_v2_prepare_original(
            &p.f.session_id,
            signed.clone(),
            UnixMillis::new(14_009),
        )
        .unwrap();
    let stored = p.f.state.clone();
    let raw = serde_json::to_value(&stored).unwrap();
    assert_eq!(
        paid["test_sessions_v2"][&p.f.session_id]["roots"],
        raw["test_sessions_v2"][&p.f.session_id]["roots"]
    );
    assert_eq!(
        paid["test_sessions_v2"][&p.f.session_id]["purpose_transports"],
        raw["test_sessions_v2"][&p.f.session_id]["purpose_transports"]
    );
    assert!(
        stored
            .test_session_v2_prepare_original_history(&p.f.session_id)
            .unwrap()
            .as_ref()
            == Some(&signed)
    );
    p.f.state
        .archive_test_session_v2_prepare_original(
            &p.f.session_id,
            signed.clone(),
            UnixMillis::new(30_000),
        )
        .unwrap();
    assert_eq!(stored, p.f.state);
    let cold: AccountProvisionApprovalState = serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(cold, stored);
    assert!(
        cold.test_session_v2_prepare_original_history(&p.f.session_id)
            .unwrap()
            .as_ref()
            == Some(&signed)
    );
    assert!(
        p.f.state
            .record_test_session_v2_prepare_service_permits(
                &p.f.session_id,
                p.proofs.clone(),
                &p.pin,
                &p.floor,
                UnixMillis::new(14_009)
            )
            .is_err()
    );
    assert_eq!(stored, p.f.state); // a later clock cannot recreate the original opaque
    let mut invalid = signed;
    invalid.request.original.signature_hex = "00".repeat(64);
    assert!(
        p.f.state
            .archive_test_session_v2_prepare_original(
                &p.f.session_id,
                invalid.clone(),
                UnixMillis::new(14_010)
            )
            .is_err()
    );
    assert_eq!(stored, p.f.state);
    let hex: String = oasis7_wasm_abi::encode_canonical_cbor(&invalid)
        .unwrap()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let mut corrupted = raw.clone();
    corrupted["test_sessions_v2"][&p.f.session_id]["execution_book"]["stages"]["prepare"]["original"]
        ["canonical_cbor_hex"] = serde_json::json!(hex);
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(corrupted).is_err());
    let mut stripped = raw;
    stripped["test_sessions_v2"][&p.f.session_id]["execution_book"]["stages"]["prepare"]["original"] =
        serde_json::Value::Null;
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(stripped).is_err());
}
#[test]
fn session_v2_prepare_original_late_result_after_freeze_is_history_only() {
    let mut p = prepared_service(Some(4), |_| {});
    let signed = original(&p);
    record(&mut p);
    p.f.state.freeze(2).unwrap();
    let before = serde_json::to_value(&p.f.state).unwrap();
    p.f.state
        .archive_test_session_v2_prepare_original(
            &p.f.session_id,
            signed.clone(),
            UnixMillis::new(30_000),
        )
        .unwrap();
    let after = serde_json::to_value(&p.f.state).unwrap();
    assert_eq!(
        before["test_sessions_v2"][&p.f.session_id]["roots"],
        after["test_sessions_v2"][&p.f.session_id]["roots"]
    );
    assert_eq!(
        before["test_sessions_v2"][&p.f.session_id]["closing"],
        after["test_sessions_v2"][&p.f.session_id]["closing"]
    );
    let cold: AccountProvisionApprovalState = serde_json::from_value(after).unwrap();
    assert!(
        cold.test_session_v2_prepare_original_history(&p.f.session_id)
            .unwrap()
            .as_ref()
            == Some(&signed)
    );
    assert!(
        p.f.state
            .record_test_session_v2_prepare_service_permits(
                &p.f.session_id,
                p.proofs.clone(),
                &p.pin,
                &p.floor,
                UnixMillis::new(30_000)
            )
            .is_err()
    );
}
