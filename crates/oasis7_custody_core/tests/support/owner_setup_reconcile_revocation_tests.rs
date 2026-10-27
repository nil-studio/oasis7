use super::*;
#[test]
fn reconcile_same_signed_mfa_cannot_enter_a_second_book_and_rejection_is_atomic() {
    let (mut s, c, _, _) = approved();
    let mut input = s
        .owner_setup_reconcile_original("reconcile-one")
        .unwrap()
        .input()
        .clone();
    // Only the Account operation/book ID changes. The Native-signed request
    // and MFA proof remain exactly the first consumed original.
    let request = input.request.clone();
    let approval = input.approval.clone();
    input.operation_id = "second-book".into();
    let mut plan = reconcile_plan(&s);
    plan.reconcile_id = input.operation_id.clone();
    assert_eq!(input.request, request);
    assert_eq!(input.approval, approval);
    let policy = Policy::new(s.draft());
    let before = s.clone();
    let result = s.begin_owner_setup_reconcile(
        OwnerSetupReconcileAdmissionV1 {
            input,
            plan,
            verify_nonce: "f1".repeat(32),
        },
        &policy.current(&c),
        7010,
        &mut StrictCrypto,
    );
    // Distinguish global nonce rejection from a later plan/body mismatch.
    assert_eq!(
        result.unwrap_err(),
        ProvisionApprovalError::Admission(ApprovalAdmissionError::NonceReused)
    );
    assert_eq!(s, before);
}
#[test]
fn reconcile_archived_commit_cannot_cross_freeze_expiry_revocation_or_post_nonce() {
    let (s, c, pin, completion) = completion_ready();
    assert!(
        s.owner_setup_reconcile_attempt("reconcile-one", "attempt-one")
            .unwrap()
            .fact
            .is_some()
    );
    for case in 0..5 {
        let mut state = s.clone();
        let mut observed = c.clone();
        let mut evidence = completion.clone();
        let now = match case {
            0 => {
                state.freeze(2).unwrap();
                7023
            }
            1 => 80_000, // Both old plan and independently approved reconcile end.
            2 => {
                observed.source.manager_security_version.0 += 1;
                7023
            }
            3 => 12_013, // Stage source observed at 7012: 5001ms, not a renewed grant.
            _ => {
                evidence.session = fixture::root_session(&state, "ff".repeat(32), 7023);
                7023
            }
        };
        let policy = Policy::new(state.draft());
        let current = policy.current(&observed);
        let ns = scope(&pin);
        let trusted = OwnerSetupCurrent {
            provision: &current,
            native_scope: &ns,
        };
        let before = state.clone();
        assert!(
            state
                .complete_owner_setup_reconcile(
                    "reconcile-one",
                    "attempt-one",
                    evidence,
                    &trusted,
                    now,
                    &mut StrictCrypto
                )
                .is_err(),
            "reconcile authority case {case}"
        );
        assert_eq!(
            state, before,
            "rejected transition case {case} changed state"
        );
        let original = state
            .owner_setup_outbox("setup-one", OwnerSetupStageV1::CreateTask)
            .unwrap();
        assert!(original.result.is_none());
        assert!(original.result_provenance.is_none());
        assert_ne!(state.provision().stage(), ProvisionStage::Prepared);
        assert_eq!(
            state
                .owner_setup_reconcile_attempt("reconcile-one", "attempt-one")
                .unwrap()
                .fact,
            s.owner_setup_reconcile_attempt("reconcile-one", "attempt-one")
                .unwrap()
                .fact
        );
    }
}
