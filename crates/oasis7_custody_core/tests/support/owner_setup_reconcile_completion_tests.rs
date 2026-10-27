use super::*;
fn completion_ready() -> (
    AccountProvisionApprovalState,
    ApprovalAdmissionContext,
    NativeBindingPinV1,
    OwnerSetupReconcileCompletionV1,
) {
    let (mut s, c, pin, candidate, permit) = published();
    let policy = Policy::new(s.draft());
    let current = policy.current(&c);
    let ns = scope(&pin);
    let trusted = OwnerSetupCurrent {
        provision: &current,
        native_scope: &ns,
    };
    s.reserve_owner_setup_reconcile_callback(
        "reconcile-one",
        "attempt-one",
        OwnerSetupReconcileCallbackV1::Admission,
        7018,
    )
    .unwrap();
    let mut evidence = admission(&s, &candidate, 7019);
    evidence.world.request = s
        .owner_setup_reconcile_attempt("reconcile-one", "attempt-one")
        .unwrap()
        .observation
        .clone()
        .unwrap();
    let OwnerSetupOriginalV1::Canonical { submit, .. } = &candidate.original else {
        unreachable!()
    };
    let legacy = serde_json::from_value(serde_json::to_value(submit).unwrap()).unwrap();
    let task = oasis7_client_api::legacy_task_results::create_control(&legacy).unwrap();
    evidence
        .world
        .response
        .payload
        .control
        .view
        .game_task_control = Some(task.clone());
    evidence.world.response.request_digest =
        ws::request_digest(PARTICIPANT_OWNER_VIEW_PATH, &evidence.world.request).unwrap();
    evidence
        .world
        .response
        .payload
        .control
        .version
        .projection_revision =
        ws::request_digest("projection", &evidence.world.response.payload.control.view).unwrap();
    evidence.world.response = ws::sign_service_response(
        PARTICIPANT_OWNER_VIEW_PATH,
        evidence.world.response.request_digest,
        evidence.world.response.payload,
        &"3f".repeat(32),
    )
    .unwrap();
    evidence.session = fixture::root_session(&s, permit.preflight_session_nonce, 7019);
    s.record_owner_setup_reconcile_admission(
        "reconcile-one",
        "attempt-one",
        evidence.clone(),
        &trusted,
        7020,
        &mut StrictCrypto,
    )
    .unwrap();
    s.reserve_owner_setup_reconcile_callback(
        "reconcile-one",
        "attempt-one",
        OwnerSetupReconcileCallbackV1::Lookup,
        7020,
    )
    .unwrap();
    let bytes = s
        .claim_owner_setup_reconcile_lookup(
            "reconcile-one",
            "attempt-one",
            &trusted,
            7021,
            &mut StrictCrypto,
        )
        .unwrap();
    let OwnerSetupOriginalV1::Canonical { lookup, .. } = &candidate.original else {
        unreachable!()
    };
    assert_eq!(bytes, serde_json::to_string(lookup).unwrap());
    assert!(
        s.claim_owner_setup_reconcile_lookup(
            "reconcile-one",
            "attempt-one",
            &trusted,
            7021,
            &mut StrictCrypto
        )
        .is_err()
    );
    let commit = evidence
        .world
        .response
        .payload
        .control
        .version
        .commit
        .clone();
    let fact = archive_tests::fact(
        &candidate,
        OwnerSetupCanonicalSourceV1::Lookup,
        ws::IntentOutcome::Committed {
            commit: Box::new(commit),
            receipt: serde_json::to_value(task).unwrap(),
        },
    );
    let key = s.draft().world_identity.service_public_key.clone();
    s.archive_owner_setup_reconcile_fact(
        "reconcile-one",
        "attempt-one",
        fact.clone(),
        &OwnerSetupArchiveContext {
            world_public_key: &key,
        },
        &mut StrictCrypto,
    )
    .unwrap();
    s.reserve_owner_setup_reconcile_callback(
        "reconcile-one",
        "attempt-one",
        OwnerSetupReconcileCallbackV1::Result,
        7022,
    )
    .unwrap();
    let post = s
        .owner_setup_reconcile_poststate_permit(
            "reconcile-one",
            "attempt-one",
            &trusted,
            7022,
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(post.kind, OwnerSetupReconcilePermitKindV1::Poststate);
    let lookup_session = fixture::root_session(&s, permit.lookup_session_nonce, 7021);
    let session = fixture::root_session(&s, post.poststate_session_nonce, 7023);
    let mut final_world = evidence.world.clone();
    final_world.response.payload.control.logical_tick = 112;
    final_world.response = ws::sign_service_response(
        PARTICIPANT_OWNER_VIEW_PATH,
        final_world.response.request_digest,
        final_world.response.payload,
        &"3f".repeat(32),
    )
    .unwrap();
    assert_ne!(final_world.response, evidence.world.response);
    let result = OwnerSetupResultV1::Canonical {
        response: fact.response,
        from_lookup: true,
    };
    (
        s,
        c,
        pin,
        OwnerSetupReconcileCompletionV1 {
            result,
            lookup_session: Some(lookup_session),
            session,
            world: final_world,
        },
    )
}
#[test]
fn reconcile_signed_late_commit_requires_poststate_and_new_current_then_records_provenance() {
    let (mut s, c, pin, completion) = completion_ready();
    let policy = Policy::new(s.draft());
    let current = policy.current(&c);
    let ns = scope(&pin);
    let trusted = OwnerSetupCurrent {
        provision: &current,
        native_scope: &ns,
    };
    for case in 0..3 {
        let mut bad = completion.clone();
        match case {
            0 => {
                bad.world.response.payload.control.view.game_task_control = None;
                bad.world
                    .response
                    .payload
                    .control
                    .version
                    .projection_revision =
                    ws::request_digest("projection", &bad.world.response.payload.control.view)
                        .unwrap();
                bad.world.response = ws::sign_service_response(
                    PARTICIPANT_OWNER_VIEW_PATH,
                    bad.world.response.request_digest,
                    bad.world.response.payload,
                    &"3f".repeat(32),
                )
                .unwrap();
            }
            1 => bad.lookup_session = None,
            _ => bad.session = bad.lookup_session.clone().unwrap(),
        }
        let mut rejected = s.clone();
        let before = rejected.clone();
        assert!(
            rejected
                .complete_owner_setup_reconcile(
                    "reconcile-one",
                    "attempt-one",
                    bad,
                    &trusted,
                    7023,
                    &mut StrictCrypto
                )
                .is_err(),
            "bad result case {case}"
        );
        assert_eq!(rejected, before);
    }
    let old = s
        .owner_setup_outbox("setup-one", OwnerSetupStageV1::CreateTask)
        .unwrap()
        .clone();
    let completed = s
        .complete_owner_setup_reconcile(
            "reconcile-one",
            "attempt-one",
            completion,
            &trusted,
            7023,
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(completed.source, old.source);
    assert_eq!(completed.original, old.original);
    assert_eq!(completed.claimed_at, old.claimed_at);
    assert!(matches!(
        completed.result_provenance,
        Some(OwnerSetupResultProvenanceV1::Reconcile { .. })
    ));
    assert!(completed.result.unwrap().succeeded());
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(serde_json::to_value(&s).unwrap())
            .unwrap(),
        s
    );
    // A different, genuinely reserved attempt exists, but has no admitted result.
    let mut wrong = serde_json::to_value(&s).unwrap();
    wrong["owner_setups"]["setup-one"]["stages"]["create_task"]["result_provenance"]["attempt_id"] =
        "other-attempt".into();
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(wrong).is_err());
}
#[path = "owner_setup_reconcile_revocation_tests.rs"]
mod revocation_tests;
