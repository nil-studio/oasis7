use super::*;
#[test]
fn shared_intent_helper_preserves_manifest_bytes_and_echo_digest_never_grants_history() {
    #[derive(serde::Serialize)]
    struct OldManifestScope {
        role_scope: oasis7_custody_core::role_key_slot::RoleKeySlotScopeV1,
        identity_namespace: String,
        identity_instance_epoch: DecimalU64,
        registration_operation_id: String,
        initial_login_operation_id: String,
    }
    let (mut s, _c, pin, entry) = archived_initial();
    let old = OldManifestScope {
        role_scope: entry.scope.role_scope.clone(),
        identity_namespace: entry.scope.identity_namespace.clone(),
        identity_instance_epoch: entry.scope.identity_instance_epoch,
        registration_operation_id: entry.scope.registration_operation_id.clone(),
        initial_login_operation_id: entry.scope.initial_login_operation_id.clone(),
    };
    let original = oasis7_wasm_abi::encode_canonical_cbor(&(
        "oasis7.custody.native-manifest-intent.v1",
        old,
        NativeHistoryPhaseV1::InitialLogin,
        &entry.native_operation_id,
    ))
    .unwrap();
    assert_eq!(bytes_digest(&original), entry.stable_intent_digest);
    let mut echo = entry.clone();
    echo.native_operation_id = "caller-selected-logical-operation".into();
    echo.stable_intent_digest =
        stable_native_intent_digest(&echo.scope, echo.phase, &echo.native_operation_id).unwrap();
    assert!(echo.validate().is_ok());
    assert!(
        s.archive_native_history(echo, &public_history_context(&pin), &mut StrictCrypto)
            .is_err()
    );
    let mut forged = entry;
    let NativeStageResultV1::SessionStatus(status) = &mut forged.result else {
        panic!()
    };
    status.status.session_ref = "forged-session".into();
    let a = forged.session_attestation.as_mut().unwrap();
    a.session_ref = status.status.session_ref.clone();
    a.status_digest = status.status.digest().unwrap();
    assert!(forged.validate().is_ok());
    assert!(
        s.archive_native_history(forged, &public_history_context(&pin), &mut StrictCrypto)
            .is_err()
    );
}
