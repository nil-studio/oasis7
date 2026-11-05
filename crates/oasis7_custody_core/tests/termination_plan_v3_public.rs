//! Public wire/shape/domain regression only: no proof, signer seed, MFA or authority is fabricated.
use oasis7_custody_core::approval::{ApprovalOptionsRequestV1, bytes_digest};
use serde_json::Value;
fn request() -> Value {
    serde_json::from_str(include_str!(
        "fixtures/termination_v3_approval_request.json"
    ))
    .unwrap()
}
#[test]
fn termination_v3_public_options_tag_shape_and_independent_domain() {
    let wire = request();
    let parsed: ApprovalOptionsRequestV1 = serde_json::from_value(wire.clone())
        .expect("new explicit TerminationPlanV3 public tag is missing");
    parsed
        .validate(1000)
        .expect("complete V3 approval wire shape must validate; this grants no authority");
    assert_eq!(serde_json::to_value(&parsed).unwrap(), wire);
    let expected = bytes_digest(
        &oasis7_wasm_abi::encode_canonical_cbor(&(
            "oasis7.custody.termination-approval.v3",
            &parsed,
        ))
        .unwrap(),
    );
    assert_eq!(
        parsed.intent_digest().unwrap(),
        expected,
        "V3 must not reuse either old approval intent domain"
    );
    for old in ["oasis7.custody.intent.v1", "oasis7.custody.intent.v2"] {
        assert_ne!(
            expected,
            bytes_digest(&oasis7_wasm_abi::encode_canonical_cbor(&(old, &parsed)).unwrap())
        );
    }
}
#[test]
fn termination_v3_public_options_do_not_accept_version_or_cap_smuggling() {
    let wire = request();
    // Unknown top-level alias and integer/overflow budgets are not canonical approval input.
    for (field, value) in [
        ("api_version", serde_json::json!(2)),
        ("transport_limit", serde_json::json!(24)),
    ] {
        let mut bad = wire.clone();
        bad["intent"]["plan"][field] = value;
        assert!(
            serde_json::from_value::<ApprovalOptionsRequestV1>(bad)
                .and_then(|v| v.validate(1000).map_err(serde::de::Error::custom))
                .is_err()
        );
    }
    let mut unknown = wire;
    unknown["intent"]["plan"]["verified"] = serde_json::json!(true);
    assert!(serde_json::from_value::<ApprovalOptionsRequestV1>(unknown).is_err());
}
