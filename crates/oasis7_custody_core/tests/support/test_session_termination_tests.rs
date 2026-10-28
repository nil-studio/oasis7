use super::*;
fn world(
    s: &AccountProvisionApprovalState,
    kind: TerminationOperationV1,
    source_id: &str,
    poststop: Option<&ws::IntentResponse<Value>>,
) -> OwnerSetupWorldEvidenceV1 {
    let read = s
        .test_session_termination_read(ID, kind, source_id)
        .unwrap();
    let old = s
        .participant_original("participant-one")
        .unwrap()
        .bundle
        .as_ref()
        .unwrap();
    let access = s.test_sessions()[ID].preflight_access.as_ref().unwrap();
    let request = ws::participant_discovery::ParticipantOwnerViewRequestV1 {
        owner: old.owner.clone(),
        admission: access.admission.clone(),
        read: ws::sign_read_request(ws::VIEW_JSON_PATH, read, &"3e".repeat(32)).unwrap(),
        response_profile: Some(ParticipantOwnerViewProfileV1::OwnerSetupV1),
    };
    let Some(OwnerSetupResultV1::FinalObservation { owner, .. }) = s
        .owner_setup_outbox("setup-one", OwnerSetupStageV1::FinalObservation)
        .unwrap()
        .result
        .as_ref()
    else {
        unreachable!()
    };
    let mut payload = owner.payload.clone();
    if let Some(ws::IntentResponse {
        outcome: ws::IntentOutcome::Committed { receipt, .. },
        ..
    }) = poststop
    {
        let stop: oasis7_client_api::legacy_task_results::ExecutorReceipt =
            serde_json::from_value(receipt.clone()).unwrap();
        let task = payload.control.view.game_task_control.as_mut().unwrap();
        task.task_revision = stop.task_revision;
        task.executor_epoch = stop.executor_epoch;
        task.executor_binding = stop.executor_binding;
        task.executor_id = None;
        task.task_status = stop.task_status;
        payload.control.version.projection_revision =
            ws::request_digest("projection", &payload.control.view).unwrap();
    }
    let response = ws::sign_service_response(
        PARTICIPANT_OWNER_VIEW_PATH,
        ws::request_digest(PARTICIPANT_OWNER_VIEW_PATH, &request).unwrap(),
        payload,
        &"3f".repeat(32),
    )
    .unwrap();
    OwnerSetupWorldEvidenceV1 { request, response }
}
fn original(r: &TerminationOriginalV1, world: ws::WorldIdentity) -> OwnerSetupOriginalV1 {
    let payload = match &r.payload {
        TerminationPayloadV1::Stop(r) => WorldServicePayloadV1::GameExecutorControl(
            ws::sign_read_request(
                GAME_EXECUTOR_CONTROL_DOMAIN,
                r.as_ref().clone(),
                &"3e".repeat(32),
            )
            .unwrap(),
        ),
        TerminationPayloadV1::Revoke(r) => WorldServicePayloadV1::Delegation(
            ws::sign_read_request("delegation", r.clone(), &"3e".repeat(32)).unwrap(),
        ),
    };
    let correlation =
        oasis7_client_api::world_service_correlation::derive_correlation(world, &payload).unwrap();
    OwnerSetupOriginalV1::Canonical {
        submit: Box::new(ws::SubmitIntentRequest {
            contract_version: 1,
            correlation: correlation.clone(),
            deadline_unix_ms: Some(60_000),
            signed_payload: payload.clone(),
        }),
        lookup: Box::new(AuthenticatedLookup {
            request: ws::LookupIntentRequest {
                contract_version: 1,
                key: correlation.key,
            },
            original: payload,
        }),
    }
}
fn stop_result(
    s: &AccountProvisionApprovalState,
    original: &OwnerSetupOriginalV1,
) -> OwnerSetupResultV1 {
    let OwnerSetupOriginalV1::Canonical { submit, .. } = original else {
        unreachable!()
    };
    let legacy = serde_json::from_value(serde_json::to_value(submit).unwrap()).unwrap();
    let receipt = serde_json::to_value(
        oasis7_client_api::legacy_task_results::executor_receipt(&legacy).unwrap(),
    )
    .unwrap();
    let Some(OwnerSetupResultV1::FinalObservation { owner, .. }) = s
        .owner_setup_outbox("setup-one", OwnerSetupStageV1::FinalObservation)
        .unwrap()
        .result
        .as_ref()
    else {
        unreachable!()
    };
    OwnerSetupResultV1::Canonical {
        from_lookup: false,
        response: ws::sign_service_response(
            ws::SUBMIT_PATH,
            ws::request_digest(ws::SUBMIT_PATH, submit).unwrap(),
            ws::IntentResponse {
                formal_profile: None,
                execution_evidence_scope: None,
                contract_version: 1,
                correlation: submit.correlation.clone(),
                outcome: ws::IntentOutcome::Committed {
                    commit: Box::new(owner.payload.control.version.commit.clone()),
                    receipt,
                },
            },
            &"3f".repeat(32),
        )
        .unwrap(),
    }
}
#[test]
fn test_session_cleanup_after_write_expiry_has_independent_once_slots_and_monotonic_revoke() {
    let (mut f, _, _) = claimed();
    f.state
        .expire_test_sessions(UnixMillis::new(50_000))
        .unwrap();
    let life = funded_source(
        &mut f,
        SessionSourceFundingV1::TerminationPlan,
        SessionPurposeV1::TerminationPlan,
        "cleanup-source",
        &"b1".repeat(32),
        52_000,
    );
    let policy = Policy::new(f.state.draft());
    let p = policy.current(&f.context);
    let ns = scope(&f.pin);
    let live = TestSessionLiveCurrent {
        provision: &p,
        native_scope: &ns,
        lifetime: &life,
        private_sid_generation: DecimalU64(1),
        source_operation_id: "cleanup-source",
        source_ticket: 2,
        observed_at: UnixMillis::new(52_002),
    };
    let stopworld = world(
        &f.state,
        TerminationOperationV1::Stop,
        "cleanup-source",
        None,
    );
    let current = TestSessionTerminationCurrent {
        live: &live,
        world: &stopworld,
    };
    let stop = f
        .state
        .allocate_test_session_termination(
            ID,
            TerminationOperationV1::Stop,
            &current,
            UnixMillis::new(52_003),
            &mut StrictCrypto,
        )
        .unwrap();
    let before = f.state.clone();
    assert_eq!(
        f.state
            .allocate_test_session_termination(
                ID,
                TerminationOperationV1::Stop,
                &current,
                UnixMillis::new(52_003),
                &mut StrictCrypto
            )
            .unwrap(),
        stop
    );
    assert_eq!(f.state, before);
    let signed = original(&stop, stopworld.response.payload.control.world.clone());
    f.state
        .publish_test_session_termination(
            ID,
            TerminationOperationV1::Stop,
            signed.clone(),
            &current,
            UnixMillis::new(52_004),
            &mut StrictCrypto,
        )
        .unwrap();
    f.state
        .reserve_test_session_termination_transport(
            ID,
            TerminationOperationV1::Stop,
            "stop-submit",
            UnixMillis::new(52_005),
        )
        .unwrap();
    f.state
        .claim_test_session_termination(
            ID,
            TerminationOperationV1::Stop,
            false,
            &current,
            UnixMillis::new(52_005),
            &mut StrictCrypto,
        )
        .unwrap();
    let before = f.state.clone();
    assert!(
        f.state
            .claim_test_session_termination(
                ID,
                TerminationOperationV1::Stop,
                false,
                &current,
                UnixMillis::new(52_005),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    // Stop unknown cannot occupy Revoke's own safety slot.
    let revokeworld = world(
        &f.state,
        TerminationOperationV1::Revoke,
        "cleanup-source",
        None,
    );
    let revoke = f
        .state
        .allocate_test_session_termination(
            ID,
            TerminationOperationV1::Revoke,
            &TestSessionTerminationCurrent {
                live: &live,
                world: &revokeworld,
            },
            UnixMillis::new(52_005),
            &mut StrictCrypto,
        )
        .unwrap();
    let TerminationPayloadV1::Revoke(r) = &revoke.payload else {
        unreachable!()
    };
    assert!(r.revoked);
    assert_eq!(r.nonce, 2);
    assert_eq!(r.generation, 2);
    let result = stop_result(&f.state, &signed);
    f.state
        .record_test_session_termination_result(
            ID,
            TerminationOperationV1::Stop,
            result.clone(),
            &live,
            UnixMillis::new(52_006),
            &mut StrictCrypto,
        )
        .unwrap();
    let OwnerSetupResultV1::Canonical { response, .. } = &result else {
        unreachable!()
    };
    let post = world(
        &f.state,
        TerminationOperationV1::Revoke,
        "cleanup-source",
        Some(&response.payload),
    );
    let postcurrent = TestSessionTerminationCurrent {
        live: &live,
        world: &post,
    };
    assert_eq!(
        f.state
            .allocate_test_session_termination(
                ID,
                TerminationOperationV1::Revoke,
                &postcurrent,
                UnixMillis::new(52_007),
                &mut StrictCrypto
            )
            .unwrap(),
        revoke
    );
    let signedrevoke = original(&revoke, post.response.payload.control.world.clone());
    f.state
        .publish_test_session_termination(
            ID,
            TerminationOperationV1::Revoke,
            signedrevoke.clone(),
            &postcurrent,
            UnixMillis::new(52_007),
            &mut StrictCrypto,
        )
        .unwrap();
    f.state
        .reserve_test_session_termination_transport(
            ID,
            TerminationOperationV1::Revoke,
            "revoke-submit",
            UnixMillis::new(52_008),
        )
        .unwrap();
    f.state
        .claim_test_session_termination(
            ID,
            TerminationOperationV1::Revoke,
            false,
            &postcurrent,
            UnixMillis::new(52_008),
            &mut StrictCrypto,
        )
        .unwrap();
    let before = f.state.clone();
    assert!(
        f.state
            .claim_test_session_termination(
                ID,
                TerminationOperationV1::Revoke,
                false,
                &postcurrent,
                UnixMillis::new(52_008),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(
            serde_json::to_value(&f.state).unwrap()
        )
        .unwrap(),
        f.state
    );
    let mut wrong = serde_json::to_value(&f.state).unwrap();
    wrong["test_sessions"][ID]["termination_slots"]["revoke"]["payload"]["request"]["nonce"] =
        1.into();
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(wrong).is_err());
    let before = f.state.clone();
    f.state
        .revoke_test_session(ID, UnixMillis::new(52_009))
        .unwrap();
    let revoked = f.state.clone();
    assert!(
        f.state
            .publish_test_session_termination(
                ID,
                TerminationOperationV1::Revoke,
                signedrevoke,
                &postcurrent,
                UnixMillis::new(52_009),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, revoked);
    assert_ne!(before, f.state);
}

#[test]
fn test_session_control_exhaustion_closes_writes_but_cannot_spend_cleanup_auth_budget() {
    let (mut f, _, _) = claimed();
    let policy = Policy::new(f.state.draft());
    let current = policy.current(&f.context);
    let available = f.state.test_sessions()[ID]
        .identity_transport
        .as_ref()
        .unwrap()
        .remaining()
        .unwrap();
    // Conservatively reserve real native attempts before dispatch; unknown never refunds.
    for i in 0..available {
        f.state
            .charge_test_session_internal_control_transport(
                ID,
                &format!("native-unknown-{i}"),
                &current,
                UnixMillis::new(10_000),
            )
            .unwrap();
    }
    assert_eq!(
        f.state.test_sessions()[ID]
            .identity_transport
            .as_ref()
            .unwrap()
            .remaining()
            .unwrap(),
        0
    );
    f.state
        .expire_test_sessions(UnixMillis::new(10_001))
        .unwrap();
    assert_eq!(
        f.state.test_sessions()[ID].closing.as_ref().unwrap().0,
        TestSessionClosingReasonV1::BudgetExhausted
    );
    let control = f.state.test_sessions()[ID].control.clone();
    let exhausted = f.state.test_sessions()[ID].identity_transport.clone();
    let life = funded_source(
        &mut f,
        SessionSourceFundingV1::TerminationPlan,
        SessionPurposeV1::TerminationPlan,
        "independent-cleanup",
        &"b2".repeat(32),
        52_000,
    );
    let policy = Policy::new(f.state.draft());
    let p = policy.current(&f.context);
    let ns = scope(&f.pin);
    let live = TestSessionLiveCurrent {
        provision: &p,
        native_scope: &ns,
        lifetime: &life,
        private_sid_generation: DecimalU64(1),
        source_operation_id: "independent-cleanup",
        source_ticket: 2,
        observed_at: UnixMillis::new(52_002),
    };
    let observed = world(
        &f.state,
        TerminationOperationV1::Stop,
        "independent-cleanup",
        None,
    );
    let slot = f
        .state
        .allocate_test_session_termination(
            ID,
            TerminationOperationV1::Stop,
            &TestSessionTerminationCurrent {
                live: &live,
                world: &observed,
            },
            UnixMillis::new(52_003),
            &mut StrictCrypto,
        )
        .unwrap();
    assert!(matches!(slot.payload, TerminationPayloadV1::Stop(_)));
    assert_eq!(f.state.test_sessions()[ID].control, control);
    assert_eq!(f.state.test_sessions()[ID].identity_transport, exhausted);
    assert_eq!(
        f.state.test_sessions()[ID].roots[&SessionPurposeV1::TerminationPlan]
            .budgets
            .transport
            .remaining()
            .unwrap(),
        23
    );
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(
            serde_json::to_value(&f.state).unwrap()
        )
        .unwrap(),
        f.state
    );
}
