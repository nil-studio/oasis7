use super::*;
#[test]
fn test_session_stop_result_admission_requires_the_actual_fresh_world_fence() {
    let (mut f, _, _) = claimed();
    f.state
        .expire_test_sessions(UnixMillis::new(50_000))
        .unwrap();
    let lifetime = funded_source(
        &mut f,
        SessionSourceFundingV1::TerminationPlan,
        SessionPurposeV1::TerminationPlan,
        "fresh-result-source",
        &"d2".repeat(32),
        52_000,
    );
    let policy = Policy::new(f.state.draft());
    let authority = policy.current(&f.context);
    let native_scope = scope(&f.pin);
    let live = TestSessionLiveCurrent {
        provision: &authority,
        native_scope: &native_scope,
        lifetime: &lifetime,
        private_sid_generation: DecimalU64(1),
        source_operation_id: "fresh-result-source",
        source_ticket: 2,
        observed_at: UnixMillis::new(52_002),
    };
    let initial_world = world(
        &f.state,
        TerminationOperationV1::Stop,
        "fresh-result-source",
        None,
    );
    let initial = TestSessionTerminationCurrent {
        live: &live,
        world: &initial_world,
    };
    let allocated = f
        .state
        .allocate_test_session_termination(
            ID,
            TerminationOperationV1::Stop,
            &initial,
            UnixMillis::new(52_003),
            &mut StrictCrypto,
        )
        .unwrap();
    let signed = original(
        &allocated,
        initial_world.response.payload.control.world.clone(),
    );
    f.state
        .publish_test_session_termination(
            ID,
            TerminationOperationV1::Stop,
            signed.clone(),
            &initial,
            UnixMillis::new(52_004),
            &mut StrictCrypto,
        )
        .unwrap();
    f.state
        .reserve_test_session_termination_transport(
            ID,
            TerminationOperationV1::Stop,
            "fresh-result-submit",
            UnixMillis::new(52_005),
        )
        .unwrap();
    f.state
        .claim_test_session_termination(
            ID,
            TerminationOperationV1::Stop,
            false,
            &initial,
            UnixMillis::new(52_005),
            &mut StrictCrypto,
        )
        .unwrap();
    let result = stop_result(&f.state, &signed);
    let fixed_world_key = f.state.draft().world_identity.service_public_key.clone();
    StrictCrypto
        .verify(
            &f.state
                .test_session_termination_archive_checks(
                    ID,
                    TerminationOperationV1::Stop,
                    &result,
                    &fixed_world_key,
                )
                .unwrap(),
        )
        .unwrap();
    let before_archive = f.state.clone();
    f.state
        .archive_test_session_termination_fact(
            ID,
            TerminationOperationV1::Stop,
            result.clone(),
            &fixed_world_key,
            &mut StrictCrypto,
        )
        .unwrap();
    StrictCrypto
        .verify(
            &f.state
                .test_session_live_checks(
                    ID,
                    SessionPurposeV1::TerminationPlan,
                    &live,
                    UnixMillis::new(52_006),
                )
                .unwrap(),
        )
        .unwrap();
    let OwnerSetupResultV1::Canonical { response, .. } = &result else {
        unreachable!()
    };
    let correct_world = world(
        &f.state,
        TerminationOperationV1::Stop,
        "fresh-result-source",
        Some(&response.payload),
    );
    correct_world
        .response
        .payload
        .validate(&correct_world.request)
        .unwrap();
    // Strong positive control proves this is the real receipt-derived post-Stop
    // state. This control is independent and never fills the under-test result.
    let mut positive = f.state.clone();
    positive
        .record_test_session_termination_result(
            ID,
            TerminationOperationV1::Stop,
            result.clone(),
            &TestSessionTerminationCurrent {
                live: &live,
                world: &correct_world,
            },
            UnixMillis::new(52_006),
            &mut StrictCrypto,
        )
        .unwrap();
    StrictCrypto
        .verify(
            &positive
                .publish_test_session_termination_checks(
                    ID,
                    TerminationOperationV1::Stop,
                    &signed,
                    &TestSessionTerminationCurrent {
                        live: &live,
                        world: &correct_world,
                    },
                    UnixMillis::new(52_006),
                )
                .unwrap(),
        )
        .unwrap();
    let mut wrong_world = correct_world.clone();
    let actual = wrong_world
        .response
        .payload
        .control
        .view
        .game_task_control
        .as_mut()
        .unwrap();
    actual.executor_epoch = DecimalU64(actual.executor_epoch.0.checked_add(1).unwrap());
    wrong_world
        .response
        .payload
        .control
        .version
        .projection_revision =
        ws::request_digest("projection", &wrong_world.response.payload.control.view).unwrap();
    wrong_world.response = ws::sign_service_response(
        PARTICIPANT_OWNER_VIEW_PATH,
        ws::request_digest(PARTICIPANT_OWNER_VIEW_PATH, &wrong_world.request).unwrap(),
        wrong_world.response.payload,
        &"3f".repeat(32),
    )
    .unwrap();
    wrong_world
        .response
        .payload
        .validate(&wrong_world.request)
        .unwrap();
    oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
        PARTICIPANT_OWNER_VIEW_PATH,
        &(
            &wrong_world.response.request_digest,
            &wrong_world.response.payload,
        ),
        &fixed_world_key,
        &wrong_world.response.signature_hex,
    )
    .unwrap();
    assert_ne!(
        wrong_world
            .response
            .payload
            .control
            .view
            .game_task_control
            .as_ref()
            .unwrap(),
        correct_world
            .response
            .payload
            .control
            .view
            .game_task_control
            .as_ref()
            .unwrap()
    );
    assert!(
        f.state.test_sessions()[ID].termination_slots[&TerminationOperationV1::Stop]
            .result
            .is_none()
    );
    let mut foreign_task = correct_world.clone();
    foreign_task
        .response
        .payload
        .control
        .view
        .game_task_control
        .as_mut()
        .unwrap()
        .task_id = "foreign-task".into();
    let mut foreign_owner = correct_world.clone();
    foreign_owner
        .response
        .payload
        .control
        .view
        .canonical_agent_owner
        .player_id = "foreign-owner".into();
    foreign_owner
        .response
        .payload
        .control
        .view
        .game_task_control
        .as_mut()
        .unwrap()
        .owner_binding = "foreign-owner".into();
    foreign_owner
        .response
        .payload
        .signer_delegation
        .as_mut()
        .unwrap()
        .owner_binding = "foreign-owner".into();
    let mut foreign_reorg = correct_world.clone();
    let reorg = foreign_reorg
        .response
        .payload
        .control
        .version
        .commit
        .binding
        .reorg_generation
        .checked_add(1)
        .unwrap();
    foreign_reorg
        .response
        .payload
        .control
        .version
        .commit
        .binding
        .reorg_generation = reorg;
    foreign_reorg
        .response
        .payload
        .control
        .view
        .canonical_agent_owner
        .reorg_epoch = reorg;
    foreign_reorg
        .response
        .payload
        .control
        .view
        .game_task_control
        .as_mut()
        .unwrap()
        .reorg_epoch = DecimalU64(reorg);
    for mut actual_world in [foreign_task, foreign_owner, foreign_reorg] {
        actual_world
            .response
            .payload
            .control
            .version
            .projection_revision =
            ws::request_digest("projection", &actual_world.response.payload.control.view).unwrap();
        actual_world.response = ws::sign_service_response(
            PARTICIPANT_OWNER_VIEW_PATH,
            ws::request_digest(PARTICIPANT_OWNER_VIEW_PATH, &actual_world.request).unwrap(),
            actual_world.response.payload,
            &"3f".repeat(32),
        )
        .unwrap();
        actual_world
            .response
            .payload
            .validate(&actual_world.request)
            .unwrap();
        oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
            PARTICIPANT_OWNER_VIEW_PATH,
            &(
                &actual_world.response.request_digest,
                &actual_world.response.payload,
            ),
            &fixed_world_key,
            &actual_world.response.signature_hex,
        )
        .unwrap();
        let mut candidate = f.state.clone();
        let before = candidate.clone();
        assert!(
            candidate
                .record_test_session_termination_result(
                    ID,
                    TerminationOperationV1::Stop,
                    result.clone(),
                    &TestSessionTerminationCurrent {
                        live: &live,
                        world: &actual_world
                    },
                    UnixMillis::new(52_006),
                    &mut StrictCrypto
                )
                .is_err()
        );
        assert_eq!(candidate, before);
    }
    // A valid non-success response may be archived but cannot become an admitted
    // operation result, even under the otherwise correct fresh World/current.
    let mut pending_response = response.payload.clone();
    pending_response.outcome = ws::IntentOutcome::Pending;
    let pending = OwnerSetupResultV1::Canonical {
        from_lookup: false,
        response: ws::sign_service_response(
            ws::SUBMIT_PATH,
            response.request_digest.clone(),
            pending_response,
            &"3f".repeat(32),
        )
        .unwrap(),
    };
    let mut pending_state = before_archive;
    pending_state
        .archive_test_session_termination_fact(
            ID,
            TerminationOperationV1::Stop,
            pending.clone(),
            &fixed_world_key,
            &mut StrictCrypto,
        )
        .unwrap();
    let archived_pending = pending_state.clone();
    assert!(
        pending_state
            .record_test_session_termination_result(
                ID,
                TerminationOperationV1::Stop,
                pending,
                &TestSessionTerminationCurrent {
                    live: &live,
                    world: &correct_world
                },
                UnixMillis::new(52_006),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(pending_state, archived_pending);
    assert!(
        pending_state.test_sessions()[ID].termination_slots[&TerminationOperationV1::Stop]
            .result
            .is_none()
    );
    let incompatible_current = TestSessionTerminationCurrent {
        live: &live,
        world: &wrong_world,
    };
    let before = f.state.clone();
    // This full current is structurally valid and strongly signed, but does not
    // match the exact archived Stop receipt's own resulting executor fence.
    assert!(
        f.state
            .record_test_session_termination_result(
                ID,
                TerminationOperationV1::Stop,
                result,
                &incompatible_current,
                UnixMillis::new(52_006),
                &mut StrictCrypto
            )
            .is_err(),
        "fresh Native/source and a historical signed Stop receipt cannot admit a result against a different current executor fence"
    );
    assert_eq!(f.state, before);
}

#[test]
fn test_session_rejected_stop_is_known_blocked_under_valid_fresh_world() {
    let (mut f, _, _) = claimed();
    f.state
        .expire_test_sessions(UnixMillis::new(50_000))
        .unwrap();
    let lifetime = funded_source(
        &mut f,
        SessionSourceFundingV1::TerminationPlan,
        SessionPurposeV1::TerminationPlan,
        "fresh-result-source",
        &"d2".repeat(32),
        52_000,
    );
    let policy = Policy::new(f.state.draft());
    let authority = policy.current(&f.context);
    let native_scope = scope(&f.pin);
    let live = TestSessionLiveCurrent {
        provision: &authority,
        native_scope: &native_scope,
        lifetime: &lifetime,
        private_sid_generation: DecimalU64(1),
        source_operation_id: "fresh-result-source",
        source_ticket: 2,
        observed_at: UnixMillis::new(52_002),
    };
    let initial_world = world(
        &f.state,
        TerminationOperationV1::Stop,
        "fresh-result-source",
        None,
    );
    let initial = TestSessionTerminationCurrent {
        live: &live,
        world: &initial_world,
    };
    let allocated = f
        .state
        .allocate_test_session_termination(
            ID,
            TerminationOperationV1::Stop,
            &initial,
            UnixMillis::new(52_003),
            &mut StrictCrypto,
        )
        .unwrap();
    let signed = original(
        &allocated,
        initial_world.response.payload.control.world.clone(),
    );
    f.state
        .publish_test_session_termination(
            ID,
            TerminationOperationV1::Stop,
            signed.clone(),
            &initial,
            UnixMillis::new(52_004),
            &mut StrictCrypto,
        )
        .unwrap();
    f.state
        .reserve_test_session_termination_transport(
            ID,
            TerminationOperationV1::Stop,
            "fresh-result-submit",
            UnixMillis::new(52_005),
        )
        .unwrap();
    f.state
        .claim_test_session_termination(
            ID,
            TerminationOperationV1::Stop,
            false,
            &initial,
            UnixMillis::new(52_005),
            &mut StrictCrypto,
        )
        .unwrap();
    let OwnerSetupResultV1::Canonical {
        response: committed_shape,
        ..
    } = stop_result(&f.state, &signed)
    else {
        unreachable!()
    };
    let mut payload = committed_shape.payload;
    payload.outcome = ws::IntentOutcome::Rejected {
        reason: ws::WorldServiceErrorKind::Conflict,
    };
    let rejected = OwnerSetupResultV1::Canonical {
        from_lookup: false,
        response: ws::sign_service_response(
            ws::SUBMIT_PATH,
            committed_shape.request_digest,
            payload,
            &"3f".repeat(32),
        )
        .unwrap(),
    };
    let world_key = f.state.draft().world_identity.service_public_key.clone();
    StrictCrypto
        .verify(
            &f.state
                .test_session_termination_archive_checks(
                    ID,
                    TerminationOperationV1::Stop,
                    &rejected,
                    &world_key,
                )
                .unwrap(),
        )
        .unwrap();
    StrictCrypto
        .verify(
            &f.state
                .test_session_live_checks(
                    ID,
                    SessionPurposeV1::TerminationPlan,
                    &live,
                    UnixMillis::new(52_006),
                )
                .unwrap(),
        )
        .unwrap();
    initial_world
        .response
        .payload
        .validate(&initial_world.request)
        .unwrap();
    oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
        PARTICIPANT_OWNER_VIEW_PATH,
        &(
            &initial_world.response.request_digest,
            &initial_world.response.payload,
        ),
        &world_key,
        &initial_world.response.signature_hex,
    )
    .unwrap();
    assert_eq!(
        initial_world
            .response
            .payload
            .control
            .view
            .game_task_control
            .as_ref()
            .unwrap()
            .task_status,
        oasis7_agent_api::game::GameTaskStatusV1::Active
    );
    let budgets = f.state.test_sessions()[ID].roots.clone();
    let slot = f.state.test_sessions()[ID].termination_slots[&TerminationOperationV1::Stop].clone();
    assert!(slot.claimed_at.is_some());
    assert!(slot.result.is_none());
    // A legitimate strongly signed rejection is known failure, not an unknown
    // write. It must not need the Suspended poststate of a committed Stop.
    assert!(
        f.state
            .record_test_session_termination_result(
                ID,
                TerminationOperationV1::Stop,
                rejected,
                &initial,
                UnixMillis::new(52_006),
                &mut StrictCrypto
            )
            .is_ok(),
        "valid Rejected Stop must be retained and projected as known blocked, not rolled back to Unknown"
    );
    let progress = f
        .state
        .test_session_progress(ID, UnixMillis::new(52_006))
        .unwrap();
    assert_eq!(
        progress.stop_status,
        TestSessionTerminationStatusV1::Rejected
    );
    assert_eq!(progress.stage, TestSessionPhaseV1::Closing);
    assert!(progress.write_closed);
    assert_eq!(f.state.test_sessions()[ID].roots, budgets);
    let after = &f.state.test_sessions()[ID].termination_slots[&TerminationOperationV1::Stop];
    assert_eq!(after.transport_attempts, slot.transport_attempts);
    assert_eq!(after.claimed_at, slot.claimed_at);
    assert!(after.lookup_claimed_at.is_none());
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(
            serde_json::to_value(&f.state).unwrap()
        )
        .unwrap(),
        f.state
    );
}
