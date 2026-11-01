//! Actual Native10 signed original, live namespace rejection and balanced cold
//! Source rename. Core fixture only; not Native SQLite/HTTP qualification.
use super::*;
#[test]
fn session_v2_native_prepare_restore_source_key_cannot_alias_signed_native_port() {
    let (mut r, paid) = ready_new();
    let observed = private(&mut r, &paid);
    let authority = native_authority(&mut r, &paid, &observed);
    let ns = scope(&r.f.pin);
    let (key, _, pin, floor) = issuer(
        AccountMachinePurposeV2::NativeExecutionPrepareV2,
        ns.http_origin,
    );
    let certificate = authority.certificate().clone();
    let signature_hex = key
        .sign(&certificate.signing_bytes().unwrap())
        .to_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let proof = SignedAccountNativeExecutionPrepareV2 {
        signature_hex,
        certificate,
    };
    proof.verify_issuer(&pin, &floor, 14_011).unwrap();
    let dispatch =
        r.f.state
            .record_test_session_v2_native_prepare(
                &r.f.session_id,
                authority,
                proof.clone(),
                &pin,
                &floor,
                UnixMillis::new(14_011),
            )
            .unwrap();
    assert_eq!(dispatch.request().proof, proof);
    let collision = dispatch
        .request()
        .proof
        .certificate
        .ports
        .iter()
        .find(|p| p.edge == ExecutionPreparePhysicalEdgeV2::IdentityActorToNative)
        .unwrap()
        .attempt_id
        .clone();
    assert_eq!(collision.len(), 64);
    r.f.state
        .test_session_v2_native_prepare_issuer_history(&r.f.session_id, &pin, &floor)
        .unwrap();
    let honest: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&r.f.state).unwrap()).unwrap();
    assert_eq!(
        honest, r.f.state,
        "genuine signed Native10 original must cold restore"
    );
    let nonce = oasis7_custody_core::approval::bytes_digest(b"native10-independent-source-B-nonce");
    let before = r.f.state.clone();
    assert!(
        r.f.state
            .reserve_test_session_v2_source(
                &r.f.session_id,
                SessionSourceReservation {
                    operation_id: &collision,
                    nonce: &nonce,
                    funding: SessionSourceFundingV1::Control
                },
                UnixMillis::new(14_012),
            )
            .is_err()
    );
    assert_eq!(
        r.f.state, before,
        "live signed Native10 attempt namespace must reject with wholeStateEq"
    );
    let original = "native10-source-B-reserved";
    r.f.state
        .reserve_test_session_v2_source(
            &r.f.session_id,
            SessionSourceReservation {
                operation_id: original,
                nonce: &nonce,
                funding: SessionSourceFundingV1::Control,
            },
            UnixMillis::new(14_012),
        )
        .unwrap();
    let mut raw = serde_json::to_value(&r.f.state).unwrap();
    let honest: AccountProvisionApprovalState = serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(
        honest, r.f.state,
        "independent actual Source B is a valid paid positive"
    );
    let s = &mut raw["test_sessions_v2"][&r.f.session_id];
    let book = s["execution_book"].clone();
    let roots = s["roots"].clone();
    let transports = s["purpose_transports"].clone();
    let mut source = s["sources"]
        .as_object_mut()
        .unwrap()
        .remove(original)
        .unwrap();
    source["source"]["operation_id"] = serde_json::json!(collision);
    s["sources"][&collision] = source;
    for map in ["source_funding", "source_oracle_reads"] {
        let value = s[map].as_object_mut().unwrap().remove(original).unwrap();
        s[map][&collision] = value;
    }
    let mut renamed = 0;
    for event in s["events"].as_array_mut().unwrap() {
        if event["event"]["kind"] == "source_reserved"
            && event["event"]["source_operation_id"] == original
        {
            event["event"]["source_operation_id"] = serde_json::json!(collision);
            renamed += 1;
        }
    }
    assert_eq!(
        renamed, 1,
        "retain original Source event/ordinal and rebalance its exact reference"
    );
    assert_eq!(
        s["execution_book"], book,
        "all signed Native10/SDK/SourceA originals stay byte-identical"
    );
    assert_eq!(
        s["purpose_transports"], transports,
        "actual ten fee leaves are untouched"
    );
    assert_eq!(
        s["roots"], roots,
        "no budget/revocation/generation scalar changes"
    );
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(raw).is_err(),
        "cold Source key must preserve the live signed Native10 physical attempt namespace"
    );
}
