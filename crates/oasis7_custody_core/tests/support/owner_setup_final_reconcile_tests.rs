use super::*;
#[test]
fn reconcile_final_new_signed_read_pair_replays_its_own_provenance_to_prepared() {
    let (s, _, _, completed, old) = prepared_by_reconcile();
    assert_eq!(s.provision().stage(), ProvisionStage::Prepared);
    assert_eq!(completed.original, old.original);
    assert_eq!(completed.source, old.source);
    assert_eq!(completed.claimed_at, old.claimed_at);
    assert_eq!(
        s.owner_setup_reconcile_progress("reconcile-one")
            .unwrap()
            .transport_remaining,
        DecimalU64(0)
    );
    assert_eq!(
        s.owner_setup_reconcile_progress("reconcile-one")
            .unwrap()
            .original_transport_remaining,
        DecimalU64(0)
    );
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(serde_json::to_value(&s).unwrap())
            .unwrap(),
        s
    );
    for provenance in [
        serde_json::Value::Null,
        json!({"kind":"normal"}),
        json!({"kind":"reconcile","reconcile_id":"reconcile-one","attempt_id":"wrong-attempt"}),
    ] {
        let mut raw = serde_json::to_value(&s).unwrap();
        raw["owner_setups"]["setup-one"]["stages"]["final_observation"]["result_provenance"] =
            provenance;
        assert!(serde_json::from_value::<AccountProvisionApprovalState>(raw).is_err());
    }
}

pub(super) fn prepared_by_reconcile() -> (
    AccountProvisionApprovalState,
    ApprovalAdmissionContext,
    NativeBindingPinV1,
    OwnerSetupOutboxV1,
    OwnerSetupOutboxV1,
) {
    let input = final_dispatched();
    let (mut s, c, pin, _, permit) = publish_reconcile(approve_reconcile(
        input,
        OwnerSetupStageV1::FinalObservation,
        1,
    ));
    let policy = Policy::new(s.draft());
    let current = policy.current(&c);
    let ns = scope(&pin);
    let trusted = OwnerSetupCurrent {
        provision: &current,
        native_scope: &ns,
    };
    let old = s
        .owner_setup_outbox("setup-one", OwnerSetupStageV1::FinalObservation)
        .unwrap()
        .clone();
    let mut e = OwnerSetupAdmissionEvidenceV1 {
        world: old.world.clone().unwrap(),
        session: fixture::root_session(&s, permit.preflight_session_nonce, 7019),
    };
    e.world.request = s
        .owner_setup_reconcile_attempt("reconcile-one", "attempt-one")
        .unwrap()
        .observation
        .clone()
        .unwrap();
    assert!(old.observation.as_ref() != Some(&e.world.request));
    e.world.response.request_digest =
        ws::request_digest(PARTICIPANT_OWNER_VIEW_PATH, &e.world.request).unwrap();
    e.world.response = ws::sign_service_response(
        PARTICIPANT_OWNER_VIEW_PATH,
        e.world.response.request_digest,
        e.world.response.payload,
        &"3f".repeat(32),
    )
    .unwrap();
    s.reserve_owner_setup_reconcile_callback(
        "reconcile-one",
        "attempt-one",
        OwnerSetupReconcileCallbackV1::Admission,
        7018,
    )
    .unwrap();
    s.record_owner_setup_reconcile_admission(
        "reconcile-one",
        "attempt-one",
        e.clone(),
        &trusted,
        7020,
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
    let request = s
        .owner_setup_reconcile_attempt("reconcile-one", "attempt-one")
        .unwrap()
        .goal_request
        .as_ref()
        .unwrap();
    let Some(OwnerSetupResultV1::PreparedGoal(prepared)) = s
        .owner_setup_outbox("setup-one", OwnerSetupStageV1::PrepareGoal)
        .unwrap()
        .result
        .as_ref()
    else {
        unreachable!()
    };
    let Some(OwnerSetupResultV1::Canonical {
        response: published,
        ..
    }) = s
        .owner_setup_outbox("setup-one", OwnerSetupStageV1::PublishGoal)
        .unwrap()
        .result
        .as_ref()
    else {
        unreachable!()
    };
    let ws::IntentOutcome::Committed {
        commit: activation, ..
    } = &published.payload.outcome
    else {
        unreachable!()
    };
    let goal = ws::sign_service_response(
        ws::private_goal_read::GOAL_CURRENT_PATH,
        ws::request_digest(ws::private_goal_read::GOAL_CURRENT_PATH, request).unwrap(),
        ws::private_goal_read::CurrentGoalResponse {
            schema_revision: 1,
            control: e
                .world
                .response
                .payload
                .control
                .view
                .game_task_control
                .clone()
                .unwrap(),
            read_commit: e.world.response.payload.control.version.commit.clone(),
            activation_commit: Some((**activation).clone()),
            active_goal: Some(prepared.payload.clone()),
        },
        &"3f".repeat(32),
    )
    .unwrap();
    let session = fixture::root_session(&s, post.poststate_session_nonce, 7023);
    let result = OwnerSetupResultV1::FinalObservation {
        owner: Box::new(e.world.response.clone()),
        goal: Box::new(goal),
    };
    let completion = OwnerSetupReconcileCompletionV1 {
        result,
        session,
        lookup_session: None,
        world: e.world,
    };
    // All live signatures, nonces and final post-state checks already pass.
    // The old implementation fails only during its final phase replay.
    s.owner_setup_reconcile_result_checks(
        "reconcile-one",
        "attempt-one",
        &completion,
        &trusted,
        7023,
    )
    .unwrap();
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
    (s, c, pin, completed, old)
}
