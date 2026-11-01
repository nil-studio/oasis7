//! Genuine activation/claim/PoP/strong Source/LF; three paid physical leaves.
use super::*;
fn restored(state: &AccountProvisionApprovalState) -> AccountProvisionApprovalState {
    serde_json::from_value(serde_json::to_value(state).unwrap()).unwrap()
}
#[test]
fn session_v2_prepare_current_three_ports_durable_once_and_exact_restore() {
    let ReadyPrepare {
        mut f,
        body,
        pop,
        fact,
    } = ready_with_current_ticket(None, None, false);
    let paid = prepay(&mut f, &body, &pop);
    let before = f.state.clone();
    let before_json = serde_json::to_value(&before).unwrap();
    // The previous paid-pending wire still omits the new optional record.
    assert!(
        before_json["test_sessions_v2"][&f.session_id]["purpose_transports"][&paid]
            .get("current_read")
            .is_none()
    );
    assert_eq!(restored(&before), before);
    let token = f
        .state
        .claim_test_session_v2_prepare_current_ports(&f.session_id, &paid, UnixMillis::new(14_006))
        .unwrap()
        .unwrap();
    assert_eq!(token.session_id(), f.session_id);
    assert_eq!(token.factory_key(), paid);
    assert_eq!(token.read().source_ticket, 5);
    assert_eq!(token.read().port_ids.len(), 3);
    let committed = f.state.clone();
    assert_eq!(restored(&committed), committed);
    let old = before.test_session_v2_snapshot(&f.session_id).unwrap();
    let new = committed.test_session_v2_snapshot(&f.session_id).unwrap();
    assert_eq!(
        old.roots()[&SessionPurposeV1::Execution]
            .budgets
            .transport
            .remaining()
            .unwrap()
            - 3,
        new.roots()[&SessionPurposeV1::Execution]
            .budgets
            .transport
            .remaining()
            .unwrap()
    );
    let after_json = serde_json::to_value(&committed).unwrap();
    assert_eq!(
        before_json["test_sessions_v2"][&f.session_id]["identity_transport"],
        after_json["test_sessions_v2"][&f.session_id]["identity_transport"]
    );
    // Lost reply is not a second permission to issue any Directory/RF request.
    assert!(
        f.state
            .claim_test_session_v2_prepare_current_ports(
                &f.session_id,
                &paid,
                UnixMillis::new(14_007)
            )
            .unwrap()
            .is_none()
    );
    assert_eq!(f.state, committed);
    let authority = admit(&mut f, &fact, &paid, 5, 14_007).unwrap();
    assert_eq!(authority.paid_key(), paid);
    assert_eq!(restored(&f.state), f.state);
    let mut stripped = after_json.clone();
    stripped["test_sessions_v2"][&f.session_id]["purpose_transports"][&paid]
        .as_object_mut()
        .unwrap()
        .remove("current_read");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(stripped).is_err());
    let mut changed = after_json;
    changed["test_sessions_v2"][&f.session_id]["purpose_transports"][&token.read().port_ids[0]] =
        serde_json::json!("execution");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(changed).is_err());
}
#[test]
fn session_v2_prepare_current_expired_rejects_before_claim_without_refund() {
    let ReadyPrepare {
        mut f, body, pop, ..
    } = ready_with_current_ticket(None, None, false);
    let paid = prepay(&mut f, &body, &pop);
    let paid_state = f.state.clone();
    assert!(
        f.state
            .claim_test_session_v2_prepare_current_ports(
                &f.session_id,
                &paid,
                UnixMillis::new(20_001)
            )
            .is_err()
    );
    assert_eq!(f.state, paid_state);
    assert_eq!(restored(&f.state), paid_state);
}

#[test]
fn session_v2_prepare_current_cold_rejects_after_actual_signed_lf_expiry() {
    let ReadyPrepare {
        mut f,
        body,
        pop,
        fact,
    } = ready_with_current_ticket(Some(1000), None, false);
    assert_eq!(fact.lifetime.status.expires_at.0, 15_002);
    let paid = prepay(&mut f, &body, &pop);
    let funding = f
        .state
        .claim_test_session_v2_prepare_current_ports(&f.session_id, &paid, UnixMillis::new(14_006))
        .unwrap()
        .unwrap();
    assert_eq!(restored(&f.state), f.state);
    // A real live reservation at this later time is rejected before I/O.
    let mut late = ready_with_current_ticket(Some(1000), None, false);
    let late_paid = prepay(&mut late.f, &late.body, &late.pop);
    let unchanged = late.f.state.clone();
    assert!(
        late.f
            .state
            .claim_test_session_v2_prepare_current_ports(
                &late.f.session_id,
                &late_paid,
                UnixMillis::new(15_003)
            )
            .is_err()
    );
    assert_eq!(unchanged, late.f.state);
    let source = f
        .state
        .test_session_v2_source(&f.session_id, "actual-prepare-factory-source")
        .unwrap();
    assert!(source.source.expires_at.0 > 15_003);
    let mut raw = serde_json::to_value(&f.state).unwrap();
    let s = &mut raw["test_sessions_v2"][&f.session_id];
    s["purpose_transports"][&paid]["current_read"]["claimed_at"] = serde_json::json!("15003");
    for id in &funding.read().port_ids {
        s["purpose_transports"][id]["claimed_at"] = serde_json::json!("15003");
    }
    let mut changed_read = 0;
    let mut changed_claim = 0;
    for e in s["events"].as_array_mut().unwrap() {
        let k = e["event"]["kind"].as_str().unwrap();
        if k == "source_read_reserved"
            && e["event"]["ticket"] == serde_json::json!(5)
            && e["event"]["source_operation_id"]
                == serde_json::json!("actual-prepare-factory-source")
        {
            e["at"] = serde_json::json!("15003");
            changed_read += 1;
        } else if k == "execution_prepare_current_ports_claimed" {
            e["at"] = serde_json::json!("15003");
            changed_claim += 1;
        }
    }
    assert_eq!((changed_read, changed_claim), (1, 1));
    // Paid count, original proof, native signature, Source, nonce, root budgets
    // and the entire journal order remain intact; only impossible claim time.
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(raw).is_err(),
        "cold Current3 claim exceeded actual strongly signed LF expiry"
    );
}
