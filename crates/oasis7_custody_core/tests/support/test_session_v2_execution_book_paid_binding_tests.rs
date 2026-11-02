//! Two real independently paid, differently signed action originals.
use super::*;
fn genuine_book_and_second_paid() -> (SessionFixture, String, String) {
    let mut f = claimed_execution();
    let (body_a, pop_a) = action_pop(&mut f, "paid-original-a", 13_000, false);
    charge(
        &mut f,
        &body_a,
        &pop_a,
        "paid-origin-ingress-a",
        SessionPurposeV1::Execution,
        13_002,
    )
    .unwrap();
    let (body_b, pop_b) = action_pop(&mut f, "paid-original-b", 13_100, false);
    charge(
        &mut f,
        &body_b,
        &pop_b,
        "paid-origin-ingress-b",
        SessionPurposeV1::Execution,
        13_102,
    )
    .unwrap();
    let a = pop_a.transport_attempt_digest();
    let b = pop_b.transport_attempt_digest();
    assert_ne!(a, b);
    assert_ne!(body_a, body_b);
    assert_ne!(pop_a.nonce(), pop_b.nonce());
    execution_source(
        &mut f,
        "paid-binding-source",
        (&body_a, &pop_a, "paid-origin-ingress-a"),
        14_000,
    );
    let policy = Policy::new(f.state.draft());
    let c = policy.current(&f.context);
    let nonce = f
        .state
        .claim_test_session_v2_execution_lifetime(
            &f.session_id,
            "paid-binding-source",
            SessionProofRequest {
                ingress_attempt_id: "paid-origin-ingress-a",
                original_bytes: &body_a,
                possession: &pop_a,
            },
            &c,
            UnixMillis::new(14_001),
            &mut StrictCrypto,
        )
        .unwrap();
    let fact = lifetime(&f, nonce, 14_002);
    let ns = scope(&f.pin);
    f.state
        .archive_test_session_v2_execution_lifetime(
            &f.session_id,
            &fact,
            &ns,
            UnixMillis::new(14_003),
            &mut StrictCrypto,
        )
        .unwrap();
    for checks in f
        .state
        .test_session_v2_native_restore_checks(&f.session_id, &ns)
        .unwrap()
    {
        StrictCrypto.verify(&checks).unwrap();
    }
    let roundtrip: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&f.state).unwrap()).unwrap();
    assert_eq!(roundtrip, f.state);
    (f, a, b)
}
#[test]
fn session_v2_book_cannot_rebind_only_ingress_to_other_real_paid_original() {
    let (f, _, _) = genuine_book_and_second_paid();
    let mut json = serde_json::to_value(&f.state).unwrap();
    let before = json["test_sessions_v2"][&f.session_id].clone();
    json["test_sessions_v2"][&f.session_id]["execution_book"]["http_ingress_attempt_id"] =
        serde_json::json!("paid-origin-ingress-b");
    assert_eq!(
        json["test_sessions_v2"][&f.session_id]["sources"],
        before["sources"]
    );
    assert_eq!(
        json["test_sessions_v2"][&f.session_id]["roots"],
        before["roots"]
    );
    assert_eq!(
        json["test_sessions_v2"][&f.session_id]["purpose_http_admissions"],
        before["purpose_http_admissions"]
    );
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(json).is_err(),
        "Book A must not borrow independently paid original B"
    );
}
#[test]
fn session_v2_book_cannot_rebind_ingress_and_public_paid_digest_together() {
    let (f, a, b) = genuine_book_and_second_paid();
    let mut json = serde_json::to_value(&f.state).unwrap();
    let s = &mut json["test_sessions_v2"][&f.session_id];
    assert_eq!(
        s["purpose_http_admissions"]["paid-origin-ingress-b"]["signed_attempt_digest"],
        serde_json::json!(b)
    );
    let sources = s["sources"].clone();
    let roots = s["roots"].clone();
    let events = s["events"].clone();
    s["execution_book"]["http_ingress_attempt_id"] = serde_json::json!("paid-origin-ingress-b");
    // This second existing field makes a plain saved-digest comparison unsafe:
    // no token/header/signature or Native source/fact is forged or refreshed.
    s["purpose_http_admissions"]["paid-origin-ingress-b"]["signed_attempt_digest"] =
        serde_json::json!(a);
    assert_eq!(s["sources"], sources);
    assert_eq!(s["roots"], roots);
    assert_eq!(s["events"], events);
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(json).is_err(),
        "changing two public bookkeeping fields must not replace the actual paid-original binding"
    );
}
