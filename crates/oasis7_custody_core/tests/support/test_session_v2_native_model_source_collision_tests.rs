//! Actual paid N/Host original plus balanced Source history; no forged signature.
use super::*;
#[test]
fn session_v2_native_model_restore_source_key_cannot_alias_actual_paid_n_ingress() {
    let mut f = ready();
    let signed = query(&f);
    let proof = pop(&f, &signed);
    let body = serde_json::to_vec(&signed).unwrap();
    let collision = "model-source-collision-ingress";
    f.state
        .charge_test_session_v2_native_model_http_attempt(
            &f.session_id,
            SessionProofRequest {
                ingress_attempt_id: collision,
                original_bytes: &body,
                possession: &proof,
            },
            UnixMillis::new(13000),
        )
        .unwrap();
    let nonce =
        oasis7_custody_core::approval::bytes_digest(b"native-model-source-B-independent-nonce");
    let before = f.state.clone();
    assert!(
        f.state
            .reserve_test_session_v2_source(
                &f.session_id,
                SessionSourceReservation {
                    operation_id: collision,
                    nonce: &nonce,
                    funding: SessionSourceFundingV1::Control,
                },
                UnixMillis::new(14000)
            )
            .is_err()
    );
    assert_eq!(
        f.state, before,
        "live occupied N ingress must reject without mutation"
    );
    let original = "model-source-B-reserved";
    f.state
        .reserve_test_session_v2_source(
            &f.session_id,
            SessionSourceReservation {
                operation_id: original,
                nonce: &nonce,
                funding: SessionSourceFundingV1::Control,
            },
            UnixMillis::new(14000),
        )
        .unwrap();
    let mut raw = serde_json::to_value(&f.state).unwrap();
    let positive: AccountProvisionApprovalState = serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(positive, f.state);
    let session = &mut raw["test_sessions_v2"][&f.session_id];
    let mut source = session["sources"]
        .as_object_mut()
        .unwrap()
        .remove(original)
        .unwrap();
    source["source"]["operation_id"] = serde_json::json!(collision);
    session["sources"][collision] = source;
    for map in ["source_funding", "source_oracle_reads"] {
        let value = session[map]
            .as_object_mut()
            .unwrap()
            .remove(original)
            .unwrap();
        session[map][collision] = value;
    }
    let mut renamed = 0;
    for event in session["events"].as_array_mut().unwrap() {
        if event["event"]["kind"] == "source_reserved"
            && event["event"]["source_operation_id"] == original
        {
            event["event"]["source_operation_id"] = serde_json::json!(collision);
            renamed += 1;
        }
    }
    assert_eq!(
        renamed, 1,
        "keep the real Source event, exact ordinal and paid records"
    );
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(raw).is_err(),
        "cold Source key must preserve the live N paid-ingress namespace guard"
    );
}
