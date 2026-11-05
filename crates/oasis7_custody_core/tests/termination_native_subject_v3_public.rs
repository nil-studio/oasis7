//! Required binding wire RED only. No real subject, proof or MFA is fabricated.
use oasis7_custody_core::approval::ApprovalOptionsRequestV1;
use serde_json::Value;
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "fixtures/termination_v3_native_subject_request.json"
    ))
    .unwrap()
}
#[test]
fn termination_v3_rejects_missing_native_subject_binding() {
    let mut value = fixture();
    let plan = value["intent"]["plan"].as_object_mut().unwrap();
    let complete = plan
        .remove("native_subject")
        .expect("testfixture must contain all ten binding fields");
    assert_eq!(complete.as_object().unwrap().len(), 10);
    let accepted = serde_json::from_value::<ApprovalOptionsRequestV1>(value)
        .is_ok_and(|request| request.validate(1000).is_ok());
    assert!(
        !accepted,
        "V3 must reject missing signed actual Native subject binding"
    );
}
#[test]
fn termination_v3_native_subject_shape_roundtrips_and_allows_real_zero_security_epoch() {
    let wire = fixture();
    assert_eq!(
        wire["intent"]["plan"]["native_subject"]["native_account_security_epoch"],
        "0"
    );
    let request: ApprovalOptionsRequestV1 = serde_json::from_value(wire.clone()).unwrap();
    request.validate(1000).unwrap();
    assert_eq!(serde_json::to_value(request).unwrap(), wire);
}
