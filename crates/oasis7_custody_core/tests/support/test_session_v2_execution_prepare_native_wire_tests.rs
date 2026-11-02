//! Genuine Core dispatch fixture export only. No private body/token/key export.
use super::*;

#[test]
fn session_v2_native_prepare_export_genuine_funded_request() {
    let (mut ready, paid) = ready_new();
    let observed = private(&mut ready, &paid);
    let authority = native_authority(&mut ready, &paid, &observed);
    let ns = scope(&ready.f.pin);
    let (key, _, pin, floor) = issuer(
        AccountMachinePurposeV2::NativeExecutionPrepareV2,
        ns.http_origin,
    );
    let certificate = authority.certificate().clone();
    let proof = SignedAccountNativeExecutionPrepareV2 {
        signature_hex: encode_hex(key.sign(&certificate.signing_bytes().unwrap()).to_bytes()),
        certificate,
    };
    let dispatch = ready
        .f
        .state
        .record_test_session_v2_native_prepare(
            &ready.f.session_id,
            authority,
            proof,
            &pin,
            &floor,
            UnixMillis::new(14_011),
        )
        .unwrap();
    let request = dispatch.request();
    request.proof.verify_issuer(&pin, &floor, 14_011).unwrap();
    let bytes = request.wire_bytes().unwrap();
    let decoded: NativeExecutionPrepareRequestV2 = serde_json::from_slice(&bytes).unwrap();
    assert!(decoded == *request, "complete typed request roundtrip");
    assert!(bytes.len() <= MAX_NATIVE_EXECUTION_PREPARE_WIRE_BYTES_V2);
    // Only public grant/SDK original and configured public pin are exported.
    // The fixture's tokens, signing keys, SID/CSRF and private body stay in RAM.
    if let Some(path) = std::env::var_os("OASIS7_NATIVE10_PUBLIC_WIRE_FIXTURE") {
        let value = serde_json::json!({
            "request": request, "issuer_pin": pin, "issuer_floor": floor,
            "request_wire_bytes": bytes.len(),
        });
        std::fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    }
    eprintln!("genuine Native10 request wire bytes={}", bytes.len());
    let cold: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&ready.f.state).unwrap()).unwrap();
    assert_eq!(cold, ready.f.state);
}
