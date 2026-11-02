//! Fresh authenticated external ingress differs from same-ingress SQL idempotence.
use super::*;
#[test]
fn session_v2_pending_start_authenticated_pending_master_replay_pays_only_new_first_five() {
    let mut f = ready();
    let (body, pop, http) = action_with_http(&mut f, "paid-replay-action", 13000);
    let first = || SessionProofRequest {
        ingress_attempt_id: "paid-replay-first-ingress",
        original_bytes: &body,
        possession: &pop,
    };
    f.state
        .charge_test_session_v2_execution_pending_ingress(first(), UnixMillis::new(13002))
        .unwrap();
    let accepted = f
        .state
        .reserve_test_session_v2_execution_pending(
            first(),
            &"cd".repeat(32),
            &scope(&f.pin),
            UnixMillis::new(13003),
            &mut StrictCrypto,
        )
        .unwrap();
    assert!(
        accepted.is_some(),
        "genuine master first commit before replay target"
    );
    let before = f.state.clone();
    let budget = remaining(&f);
    let before_json = serde_json::to_value(&before).unwrap();
    let restored: AccountProvisionApprovalState =
        serde_json::from_value(before_json.clone()).unwrap();
    assert_eq!(restored, before);
    let original_nonce = before_json["test_sessions_v2"][&f.session_id]
        .get("challenges")
        .expect("real challenge inventory")
        .get("metadata-paid-replay-action")
        .expect("actual action helper issued metadata challenge");
    assert!(
        original_nonce
            .get("consumed_by")
            .expect("required consumed_by field")
            .is_null(),
        "pending is not Book birth and has not consumed the action nonce"
    );
    // Reverify the actual original H HTTP bytes after pending master reservation.
    // This tests pending conflict, not a consumed action nonce. All bindings remain valid.
    let prepared = f
        .state
        .prepare_test_session_v2_action_http(
            &http,
            HttpHeaderRepresentation::RawInstances,
            UnixMillis::new(13004),
        )
        .unwrap();
    let replay =
        verify_http_possession(prepared, UnixMillis::new(13004), &mut StrictCrypto).unwrap();
    assert_eq!(
        replay.transport_attempt_digest(),
        pop.transport_attempt_digest()
    );
    let next = || SessionProofRequest {
        ingress_attempt_id: "paid-replay-second-ingress",
        original_bytes: &body,
        possession: &replay,
    };
    let fee = f
        .state
        .charge_test_session_v2_execution_pending_ingress(next(), UnixMillis::new(13004));
    assert!(
        fee.is_ok(),
        "valid external new ingress commits its first five before pending master conflict"
    );
    assert_eq!(remaining(&f), budget - 5);
    let paid = f.state.clone();
    assert!(
        f.state
            .reserve_test_session_v2_execution_pending(
                next(),
                &"de".repeat(32),
                &scope(&f.pin),
                UnixMillis::new(13005),
                &mut StrictCrypto
            )
            .is_err(),
        "existing pending master cannot create a second master or opaque"
    );
    assert_eq!(
        f.state, paid,
        "second-stage rejection preserves committed ingress fee"
    );
    let after = serde_json::to_value(&f.state).unwrap();
    assert_eq!(
        after["test_sessions_v2"][&f.session_id]["execution_pending_start"],
        before_json["test_sessions_v2"][&f.session_id]["execution_pending_start"]
    );
    assert_eq!(
        after["test_sessions_v2"][&f.session_id]["execution_book"],
        serde_json::Value::Null
    );
    assert_eq!(
        after["test_sessions_v2"][&f.session_id]["sources"],
        before_json["test_sessions_v2"][&f.session_id]["sources"]
    );
    let cold: AccountProvisionApprovalState = serde_json::from_value(after).unwrap();
    assert_eq!(cold, f.state);
}
