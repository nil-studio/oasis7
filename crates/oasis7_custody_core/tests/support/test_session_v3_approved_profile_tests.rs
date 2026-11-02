//! New signed metadata only. Native V3 options/dispatch remain explicitly closed.
use super::*;

#[test]
fn term_v3_explicit_basis_has_new_consumed_full_approval_and_no_execution_roots() {
    let old = pending_fixture_with_profile(
        80,
        Some(CleanupTransportPolicyV2::TermWideFiveSecondOwnedWorld3V2),
    );
    let new = pending_fixture_with_profile(
        80,
        Some(CleanupTransportPolicyV2::TermWideFiveSecondOwnedDirectCurrent5V3),
    );
    let old_snapshot = old.state.test_session_v2_snapshot(&old.session_id).unwrap();
    let new_snapshot = new.state.test_session_v2_snapshot(&new.session_id).unwrap();
    let CustodyIntentV1::TestSessionV2 { plan: old_plan, .. } =
        &old_snapshot.input().request.intent
    else {
        panic!("actual old full input");
    };
    let CustodyIntentV1::TestSessionV2 { plan: new_plan, .. } =
        &new_snapshot.input().request.intent
    else {
        panic!("actual new full input");
    };
    assert_eq!(
        new_plan.termination.cleanup_transport_policy,
        CleanupTransportPolicyV2::TermWideFiveSecondOwnedDirectCurrent5V3
    );
    assert_ne!(old_plan.digest().unwrap(), new_plan.digest().unwrap());
    assert_ne!(old_snapshot.input().approval, new_snapshot.input().approval);
    assert_eq!(
        new.state
            .test_session_v2_original(&new.session_id)
            .unwrap()
            .stage(),
        oasis7_custody_core::approval_admission::ApprovalAdmissionStage::Consumed
    );
    let proof = &new_snapshot.input().approval.proof;
    let native = scope(&new.pin);
    new_snapshot
        .input()
        .request
        .validate_native_summary_contract(
            proof.model_budget_summary_contract.as_ref(),
            &proof.manager_account_id,
            Some(native.native_root_public_key),
            proof.issued_at.0,
        )
        .unwrap();
    assert!(new_snapshot.roots().is_empty());
    let cold = serde_json::to_value(&new.state).unwrap();
    assert!(cold["test_sessions_v2"][&new.session_id]["execution_book"].is_null());
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(cold).unwrap(),
        new.state
    );
}

#[test]
fn term_v3_metadata_selector_preserves_none_and_does_not_upgrade_old_signed_summary() {
    let mut q = TestSessionBasisRequestV2 {
        api_version: 2,
        request_id: "v3-basis".into(),
        enrollment_id: "enrollment".into(),
        setup_id: "setup".into(),
        cleanup_until: UnixMillis::new(60000),
        cleanup_transport_policy: None,
    };
    let old_wire = serde_json::to_vec(&q).unwrap();
    assert!(
        !String::from_utf8(old_wire.clone())
            .unwrap()
            .contains("cleanup_transport_policy")
    );
    q.cleanup_transport_policy =
        Some(CleanupTransportPolicyV2::TermWideFiveSecondOwnedDirectCurrent5V3);
    q.validate().unwrap();
    let wire = serde_json::to_value(&q).unwrap();
    assert_eq!(
        wire["cleanup_transport_policy"],
        "term_wide_five_second_owned_direct_current5_v3"
    );
    assert_eq!(
        serde_json::from_value::<TestSessionBasisRequestV2>(wire).unwrap(),
        q
    );
    q.cleanup_transport_policy = None;
    assert_eq!(serde_json::to_vec(&q).unwrap(), old_wire);

    let old = pending_fixture_with_profile(
        80,
        Some(CleanupTransportPolicyV2::TermWideFiveSecondOwnedWorld3V2),
    );
    let mut input = old
        .state
        .test_session_v2_snapshot(&old.session_id)
        .unwrap()
        .input()
        .clone();
    let native = scope(&old.pin);
    let proof = input.approval.proof.clone();
    input
        .request
        .validate_native_summary_contract(
            proof.model_budget_summary_contract.as_ref(),
            &proof.manager_account_id,
            Some(native.native_root_public_key),
            proof.issued_at.0,
        )
        .unwrap();
    let CustodyIntentV1::TestSessionV2 { plan, scope } = &mut input.request.intent else {
        panic!("actual old signed input");
    };
    plan.termination.cleanup_transport_policy =
        CleanupTransportPolicyV2::TermWideFiveSecondOwnedDirectCurrent5V3;
    scope.termination_plan_digest = plan.termination.digest().unwrap();
    assert_ne!(input.request.intent_digest().unwrap(), proof.intent_digest);
    assert!(
        input
            .request
            .validate_native_summary_contract(
                proof.model_budget_summary_contract.as_ref(),
                &proof.manager_account_id,
                Some(native.native_root_public_key),
                proof.issued_at.0,
            )
            .is_err()
    );
}
