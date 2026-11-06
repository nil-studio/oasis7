// Uses real signed producers in the original fixture hierarchy.
use super::*;
#[test]
fn supplemental_install_rejects_legacy_approval_without_mutating_active_or_closing() {
    for closing in [false, true] {
        let mut f = activation::activated(9_000);
        if closing {
            f.state
                .expire_test_sessions(UnixMillis::new(50_000))
                .unwrap();
        }
        let now = UnixMillis::new(if closing { 52_000 } else { 10_000 });
        let before = f.state.clone();
        let legacy = f.state.test_sessions()["session-one"].input.clone();
        let policy = Policy::new(f.state.draft());
        let current = policy.current(&f.context);
        assert!(
            f.state
                .begin_termination_v3_install(
                    legacy,
                    "de".repeat(32),
                    &current,
                    now,
                    &mut StrictCrypto
                )
                .is_err()
        );
        assert_eq!(f.state, before);
        let restored: AccountProvisionApprovalState =
            serde_json::from_value(serde_json::to_value(&f.state).unwrap()).unwrap();
        assert_eq!(restored, before);
        assert_eq!(
            restored.test_sessions()["session-one"].roots,
            before.test_sessions()["session-one"].roots
        );
        assert_eq!(
            restored.test_sessions()["session-one"].termination,
            before.test_sessions()["session-one"].termination
        );
    }
}

#[cfg(test)]
pub(super) fn supplemental_input(f: &SessionFixture, now: u64) -> ApprovalAdmissionInput {
    use oasis7_custody_core::provisioning::{NativeSubjectRefV3, TerminationPlanV3};
    let old = &f.state.test_sessions()["session-one"];
    let legacy = serde_json::to_value(&old.termination).unwrap();
    let wire: Value = serde_json::from_str(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/termination_v3_approval_request.json")))
    .unwrap();
    let mut value = wire["intent"]["plan"].clone();
    for name in [
        "custody_account_id",
        "provision_id",
        "setup_id",
        "prepared_result_digest",
        "world",
        "owner_key_ref",
        "executor_key_ref",
        "initial_fence",
        "original_delegation",
        "cleanup_until",
    ] {
        value[name] = legacy[name].clone();
    }
    // Legacy delegation integers are canonical decimal strings in V3.
    for name in ["agent_identity_generation", "generation", "nonce"] {
        if let Some(v) = value["original_delegation"][name].as_u64() {
            value["original_delegation"][name] = json!(v.to_string());
        }
    }
    value["subject_session_id"] = json!("session-one");
    value["subject_v1_approval_digest"] = json!(old.input.request.intent_digest().unwrap());
    value["subject_v1_termination_plan_digest"] = json!(old.termination.digest().unwrap());
    value["native_subject"] = serde_json::to_value(
        NativeSubjectRefV3::from_status(&old.native_lifetime.as_ref().unwrap().lifetime.status)
            .unwrap(),
    )
    .unwrap();
    let mut plan: TerminationPlanV3 = serde_json::from_value(value).unwrap();
    plan.stop_operation_id = plan.expected_stop_operation_id().unwrap();
    plan.close_operation_id = plan.expected_close_operation_id().unwrap();
    plan.validate_at(now).unwrap();
    let mut input = old.input.clone();
    input.operation_id = "supplemental-install".into();
    input.request = plan
        .approval_request("supplemental-UV".into(), f.context.authority.clone(), now)
        .unwrap();
    input.approval.proof.context = input.request.context.clone();
    input.approval.proof.intent_digest = input.request.intent_digest().unwrap();
    input.approval.proof.nonce = "supplemental-real-UV-nonce".into();
    input.approval.proof.approval_id = "supplemental-real-UV".into();
    input.approval.proof.approval_request_id = "supplemental-UV".into();
    input.approval.proof.issued_at = DecimalU64(now);
    input.approval.proof.expires_at = DecimalU64(now + 30_000);
    input.approval = sign_proof(input.approval.proof);
    input
}
#[test]
fn supplemental_real_active_closing_once_consume_cold_restore_and_signed_mismatch() {
    use oasis7_custody_core::provisioning::TerminationInstallNativeCurrentV3;
    for closing in [false, true] {
        let mut f = activation::activated(9_000);
        if closing {
            f.state
                .expire_test_sessions(UnixMillis::new(50_000))
                .unwrap();
        }
        let now = if closing { 52_000 } else { 10_000 };
        let input = supplemental_input(&f, now);
        let legacy = f.state.test_sessions()["session-one"].clone();
        let policy = Policy::new(f.state.draft());
        let current = policy.current(&f.context);
        let ns = scope(&f.pin);
        let record = f
            .state
            .begin_termination_v3_install_fixture(
                input.clone(),
                "ef".repeat(32),
                &current,
                UnixMillis::new(now),
                &mut StrictCrypto,
            )
            .unwrap();
        let signed = verification(&record, &f.context);
        let nonce = f
            .state
            .termination_install_native_nonce_v3("session-one")
            .unwrap();
        let native = fixture::root_session(&f.state, nonce, now + 1).status;
        let before = f.state.clone();
        // A correctly signed status for a different nonce is rejected atomically.
        let wrong = fixture::root_session(&f.state, "ff".repeat(32), now + 1).status;
        let bad = TerminationInstallNativeCurrentV3 {
            scope: &ns,
            status: &wrong,
            private_sid_generation: legacy.session.private_sid_generation,
        };
        assert!(
            f.state
                .commit_termination_v3_install_fixture(
                    "session-one",
                    signed.clone(),
                    &current,
                    &bad,
                    UnixMillis::new(now + 1),
                    &mut StrictCrypto
                )
                .is_err()
        );
        assert_eq!(f.state, before);
        let live = TerminationInstallNativeCurrentV3 {
            scope: &ns,
            status: &native,
            private_sid_generation: legacy.session.private_sid_generation,
        };
        f.state
            .commit_termination_v3_install_fixture(
                "session-one",
                signed.clone(),
                &current,
                &live,
                UnixMillis::new(now + 1),
                &mut StrictCrypto,
            )
            .unwrap();
        let consumed = f.state.clone();
        f.state
            .commit_termination_v3_install_fixture(
                "session-one",
                signed,
                &current,
                &live,
                UnixMillis::new(now + 1),
                &mut StrictCrypto,
            )
            .unwrap();
        assert_eq!(f.state, consumed);
        assert_eq!(f.state.test_sessions()["session-one"], legacy);
        let restored: AccountProvisionApprovalState =
            serde_json::from_value(serde_json::to_value(&consumed).unwrap()).unwrap();
        assert_eq!(restored, consumed);
        for field in ["installation_native_status", "installed_at"] {
            let mut corrupt = serde_json::to_value(&consumed).unwrap();
            corrupt["termination_v3"]["session-one"][field] = Value::Null;
            assert!(serde_json::from_value::<AccountProvisionApprovalState>(corrupt).is_err());
        }
        let mut changed = input;
        changed.operation_id = "different-install".into();
        assert!(
            f.state
                .begin_termination_v3_install_fixture(
                    changed,
                    "ed".repeat(32),
                    &current,
                    UnixMillis::new(now + 1),
                    &mut StrictCrypto
                )
                .is_err()
        );
        assert_eq!(f.state, consumed);
    }
}
