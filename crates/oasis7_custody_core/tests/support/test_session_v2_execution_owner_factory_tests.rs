//! Actual PoP and already-admitted Native LF, no caller units or balance edits.
use super::*;
fn spent(raw: &serde_json::Value, id: &str) -> u64 {
    serde_json::from_value::<DecimalU64>(
        raw["test_sessions_v2"][id]["roots"]["execution"]["root"]["budgets"]["transport"]["spent"]
            .clone(),
    )
    .unwrap()
    .0
}
#[test]
fn session_v2_owner_factory_two_physical_leaves_and_cold_reverse_are_exact() {
    let ReadyPrepare {
        mut f, body, pop, ..
    } = ready_with_current_ticket(None, None, false);
    let a = allocation();
    let before = f.state.clone();
    let raw_before = serde_json::to_value(&before).unwrap();
    let key = f
        .state
        .charge_test_session_v2_execution_prepare_owner_factory_attempt(
            &f.session_id,
            &a,
            SessionProofRequest {
                ingress_attempt_id: &a.factory_ingress_id,
                original_bytes: &body,
                possession: &pop,
            },
            UnixMillis::new(14_006),
        )
        .unwrap();
    let raw = serde_json::to_value(&f.state).unwrap();
    assert_eq!(
        spent(&raw, &f.session_id),
        spent(&raw_before, &f.session_id) + 2
    );
    let paid = &raw["test_sessions_v2"][&f.session_id]["purpose_transports"][&key];
    assert_eq!(paid["kind"], "owner_service_and_account_v2");
    let ports = paid["factory_ports"].as_array().unwrap();
    assert_eq!(ports.len(), 2);
    assert_eq!(ports[0]["hop"], "identity_to_owner_service");
    assert_eq!(ports[1]["hop"], "owner_service_to_account_actor");
    let first = ports[0]["physical_id"].as_str().unwrap();
    let second = ports[1]["physical_id"].as_str().unwrap();
    assert_ne!(first, second);
    for physical in [first, second] {
        let digest = physical.strip_prefix("execution-factory-hop-v2:").unwrap();
        assert_eq!(digest.len(), 64);
        assert!(
            digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
        assert!(physical.len() <= 256);
    }
    let stable = f.state.clone();
    assert_eq!(
        f.state
            .charge_test_session_v2_execution_prepare_owner_factory_attempt(
                &f.session_id,
                &a,
                SessionProofRequest {
                    ingress_attempt_id: &a.factory_ingress_id,
                    original_bytes: &body,
                    possession: &pop
                },
                UnixMillis::new(14_006)
            )
            .unwrap(),
        key
    );
    assert_eq!(f.state, stable);
    assert!(
        f.state
            .charge_test_session_v2_execution_prepare_attempt(
                &f.session_id,
                &a,
                SessionProofRequest {
                    ingress_attempt_id: &a.factory_ingress_id,
                    original_bytes: &body,
                    possession: &pop
                },
                UnixMillis::new(14_006)
            )
            .is_err()
    );
    assert_eq!(f.state, stable);
    let cold: AccountProvisionApprovalState = serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(cold, stable);
    let mut missing = raw.clone();
    missing["test_sessions_v2"][&f.session_id]["purpose_transports"][&key]
        .as_object_mut()
        .unwrap()
        .remove("factory_ports");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(missing).is_err());
    let mut alias = raw.clone();
    alias["test_sessions_v2"][&f.session_id]["purpose_transports"][&key]["factory_ports"][1]["physical_id"] =
        serde_json::json!(first);
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(alias).is_err());
    let mut downgraded = raw;
    downgraded["test_sessions_v2"][&f.session_id]["purpose_transports"][&key] =
        serde_json::json!("execution");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(downgraded).is_err());
}
#[test]
fn session_v2_owner_factory_does_not_upgrade_or_double_bill_old_one_fee_history() {
    let ReadyPrepare {
        mut f, body, pop, ..
    } = ready_with_current_ticket(None, None, false);
    let a = allocation();
    let before = serde_json::to_value(&f.state).unwrap();
    let key = f
        .state
        .charge_test_session_v2_execution_prepare_attempt(
            &f.session_id,
            &a,
            SessionProofRequest {
                ingress_attempt_id: &a.factory_ingress_id,
                original_bytes: &body,
                possession: &pop,
            },
            UnixMillis::new(14_006),
        )
        .unwrap();
    let raw = serde_json::to_value(&f.state).unwrap();
    assert_eq!(
        spent(&raw, &f.session_id),
        spent(&before, &f.session_id) + 1
    );
    assert!(
        raw["test_sessions_v2"][&f.session_id]["purpose_transports"][&key]
            .get("factory_ports")
            .is_none()
    );
    let stable = f.state.clone();
    assert!(
        f.state
            .charge_test_session_v2_execution_prepare_owner_factory_attempt(
                &f.session_id,
                &a,
                SessionProofRequest {
                    ingress_attempt_id: &a.factory_ingress_id,
                    original_bytes: &body,
                    possession: &pop
                },
                UnixMillis::new(14_006)
            )
            .is_err()
    );
    assert_eq!(stable, f.state);
    let cold: AccountProvisionApprovalState = serde_json::from_value(raw).unwrap();
    assert_eq!(cold, stable);
}
