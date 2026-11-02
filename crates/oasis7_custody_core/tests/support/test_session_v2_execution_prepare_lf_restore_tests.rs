//! Genuine strong shorter LF, then a balanced impossible history timestamp.
use super::*;
#[test]
fn session_v2_prepare_factory_cold_stage_time_must_be_inside_actual_lf_window() {
    let ReadyPrepare {
        mut f,
        body,
        pop,
        fact,
    } = ready_with_status_window(Some(1000));
    let paid = prepay(&mut f, &body, &pop);
    let authority = admit(&mut f, &fact, &paid, 5, 14_007).unwrap();
    assert!(authority.expires_at().get() <= fact.lifetime.status.expires_at.0);
    let genuine = serde_json::to_value(&f.state).unwrap();
    let restored: AccountProvisionApprovalState = serde_json::from_value(genuine.clone()).unwrap();
    assert_eq!(restored, f.state);
    fact.verify_root(
        scope(&f.pin).native_root_key_id,
        scope(&f.pin).native_root_public_key,
    )
    .unwrap();
    let impossible_at = fact.lifetime.status.expires_at.0.checked_add(1).unwrap();
    assert_eq!(impossible_at, 15_003);
    let source = f
        .state
        .test_session_v2_source(&f.session_id, "actual-prepare-factory-source")
        .unwrap();
    let observed = source
        .source
        .verification
        .as_ref()
        .unwrap()
        .verification
        .observed_at
        .0;
    assert!(observed <= impossible_at && impossible_at - observed < 5000);
    assert!(impossible_at < source.source.expires_at.0);
    assert!(
        impossible_at
            < f.state
                .test_session_v2_snapshot(&f.session_id)
                .unwrap()
                .roots()[&SessionPurposeV1::Execution]
                .deadline
                .get()
    );
    let lease: UnixMillis =
        serde_json::from_value(genuine["test_sessions_v2"][&f.session_id]["lease_until"].clone())
            .unwrap();
    assert!(impossible_at < lease.get());
    assert!(impossible_at < f.context.handoff_until.0);
    let mut changed = genuine;
    let session = &mut changed["test_sessions_v2"][&f.session_id];
    assert_eq!(
        session["execution_book"]["stages"]["prepare"]["signing_admitted_at"],
        serde_json::json!("14007")
    );
    session["execution_book"]["stages"]["prepare"]["signing_admitted_at"] =
        serde_json::json!(impossible_at.to_string());
    let mut matched = 0;
    for event in session["events"].as_array_mut().unwrap() {
        if event["event"]["kind"] == "execution_prepare_allocated" {
            assert_eq!(event["at"], serde_json::json!("14007"));
            event["at"] = serde_json::json!(impossible_at.to_string());
            matched += 1;
        }
    }
    assert_eq!(matched, 1);
    // No signed body, fee, Source/LF, nonce, ordinal count, candidate or budget
    // changes. Source/root/lease history is valid; only LF status is expired.
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(changed).is_err(),
        "cold stage admission history exceeds actual strong LF validity"
    );
}
