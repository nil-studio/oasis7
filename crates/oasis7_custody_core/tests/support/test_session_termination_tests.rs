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

#[test]
fn test_session_known_stop_lookup_survives_receipt_derived_suspended_world() {
    // This is a strong-signature Core fixture, not a claim of live dispatcher
    // acceptance or a completed Cleanup24 cold-path budget proof.
    let (mut f, _, _) = claimed();
    f.state
        .expire_test_sessions(UnixMillis::new(50_000))
        .unwrap();
    let life = funded_source(
        &mut f,
        SessionSourceFundingV1::TerminationPlan,
        SessionPurposeV1::TerminationPlan,
        "lost-stop-source",
        &"b4".repeat(32),
        52_000,
    );
    let policy = Policy::new(f.state.draft());
    let current = policy.current(&f.context);
    let ns = scope(&f.pin);
    let live = TestSessionLiveCurrent {
        provision: &current,
        native_scope: &ns,
        lifetime: &life,
        private_sid_generation: DecimalU64(1),
        source_operation_id: "lost-stop-source",
        source_ticket: 2,
        observed_at: UnixMillis::new(52_002),
    };
    let before_world = world(
        &f.state,
        TerminationOperationV1::Stop,
        "lost-stop-source",
        None,
    );
    let initial = TestSessionTerminationCurrent {
        live: &live,
        world: &before_world,
    };
    let stop = f
        .state
        .allocate_test_session_termination(
            ID,
            TerminationOperationV1::Stop,
            &initial,
            UnixMillis::new(52_003),
            &mut StrictCrypto,
        )
        .unwrap();
    let signed = original(&stop, before_world.response.payload.control.world.clone());
    let original_bytes = serde_json::to_vec(&signed).unwrap();
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
            "lost-stop-submit",
            UnixMillis::new(52_005),
        )
        .unwrap();
    assert_eq!(
        f.state
            .claim_test_session_termination(
                ID,
                TerminationOperationV1::Stop,
                false,
                &initial,
                UnixMillis::new(52_005),
                &mut StrictCrypto
            )
            .unwrap(),
        signed
    );
    // Canonical fixture commits the exact typed Stop. Its complete signed result
    // passes current strong checks but is deliberately never recorded in Account.
    let committed = stop_result(&f.state, &signed);
    let mut fact_checked = f.state.clone();
    let world_key = f.state.draft().world_identity.service_public_key.clone();
    fact_checked
        .archive_test_session_termination_fact(
            ID,
            TerminationOperationV1::Stop,
            committed.clone(),
            &world_key,
            &mut StrictCrypto,
        )
        .unwrap();
    let checks = fact_checked
        .test_session_termination_result_checks(
            ID,
            TerminationOperationV1::Stop,
            &committed,
            &live,
            UnixMillis::new(52_006),
        )
        .unwrap();
    StrictCrypto.verify(&checks).unwrap();
    let OwnerSetupResultV1::Canonical { response, .. } = &committed else {
        unreachable!()
    };
    let post_world = world(
        &f.state,
        TerminationOperationV1::Stop,
        "lost-stop-source",
        Some(&response.payload),
    );
    post_world
        .response
        .payload
        .validate(&post_world.request)
        .unwrap();
    // Independent positive control: admitting this same signed receipt into a
    // CLONE makes the exact post-Stop World and original signed request pass
    // the existing full current/publish strong checks. The lost-response state
    // under test below is never modified by this control.
    let mut recorded = f.state.clone();
    recorded
        .record_test_session_termination_result(
            ID,
            TerminationOperationV1::Stop,
            committed.clone(),
            &live,
            UnixMillis::new(52_006),
            &mut StrictCrypto,
        )
        .unwrap();
    let positive = recorded
        .publish_test_session_termination_checks(
            ID,
            TerminationOperationV1::Stop,
            &signed,
            &TestSessionTerminationCurrent {
                live: &live,
                world: &post_world,
            },
            UnixMillis::new(52_006),
        )
        .unwrap();
    StrictCrypto.verify(&positive).unwrap();
    assert!(
        f.state.test_sessions()[ID].termination_slots[&TerminationOperationV1::Stop]
            .result
            .is_none()
    );
    assert_eq!(
        post_world
            .response
            .payload
            .control
            .view
            .game_task_control
            .as_ref()
            .unwrap()
            .task_status,
        oasis7_agent_api::game::GameTaskStatusV1::Suspended
    );
    oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
        PARTICIPANT_OWNER_VIEW_PATH,
        &(
            &post_world.response.request_digest,
            &post_world.response.payload,
        ),
        &f.state.draft().world_identity.service_public_key,
        &post_world.response.signature_hex,
    )
    .unwrap();
    let checks = f
        .state
        .test_session_live_checks(
            ID,
            SessionPurposeV1::TerminationPlan,
            &live,
            UnixMillis::new(52_006),
        )
        .unwrap();
    StrictCrypto.verify(&checks).unwrap();
    let slot = &f.state.test_sessions()[ID].termination_slots[&TerminationOperationV1::Stop];
    assert!(slot.claimed_at.is_some());
    assert!(slot.result.is_none());
    assert!(slot.lookup_claimed_at.is_none());
    assert_eq!(
        serde_json::to_vec(slot.signed.as_ref().unwrap().as_ref()).unwrap(),
        original_bytes
    );
    let remaining = |state: &AccountProvisionApprovalState| {
        state.test_sessions()[ID].roots[&SessionPurposeV1::TerminationPlan]
            .budgets
            .transport
            .remaining()
            .unwrap()
    };
    // Missing new metadata on a genuinely never-lookup legacy slot is inert.
    let mut legacy_unclaimed = serde_json::to_value(&f.state).unwrap();
    legacy_unclaimed["test_sessions"][ID]["termination_slots"]["stop"]
        .as_object_mut()
        .unwrap()
        .remove("lookup_paid_claim");
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(legacy_unclaimed).unwrap(),
        f.state
    );
    // A real source already owns this ID; no cleanup fee or claim may be created.
    let before_collision = f.state.clone();
    assert!(
        f.state
            .claim_known_test_session_termination_lookup(
                ID,
                TerminationOperationV1::Stop,
                "lost-stop-source",
                &live,
                UnixMillis::new(52_006),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, before_collision);
    // Each authorization failure leaves the independent cleanup root untouched.
    for mode in ["frozen", "deadline", "exhausted"] {
        let mut denied = before_collision.clone();
        let time = if mode == "deadline" {
            UnixMillis::new(61_000)
        } else {
            UnixMillis::new(52_006)
        };
        if mode == "frozen" {
            denied.freeze(2).unwrap();
        }
        if mode == "exhausted" {
            for index in 0..24 {
                if remaining(&denied) == 0 {
                    break;
                }
                denied
                    .reserve_test_session_termination_transport(
                        ID,
                        TerminationOperationV1::Stop,
                        &format!("cleanup-exhaustion-{index}"),
                        UnixMillis::new(52_006),
                    )
                    .unwrap();
            }
            assert_eq!(remaining(&denied), 0);
        }
        let exact = denied.clone();
        assert!(
            denied
                .claim_known_test_session_termination_lookup(
                    ID,
                    TerminationOperationV1::Stop,
                    "denied-known-lookup",
                    &live,
                    time,
                    &mut StrictCrypto
                )
                .is_err(),
            "{mode}"
        );
        assert_eq!(denied, exact, "{mode}");
        assert!(
            denied.test_sessions()[ID].termination_slots[&TerminationOperationV1::Stop]
                .lookup_claimed_at
                .is_none()
        );
    }
    let before = remaining(&f.state);
    let lookup = f.state.claim_known_test_session_termination_lookup(
        ID,
        TerminationOperationV1::Stop,
        "lost-stop-known-lookup",
        &live,
        UnixMillis::new(52_006),
        &mut StrictCrypto,
    );
    assert!(
        lookup.is_ok(),
        "the exact stored readonly lookup must not require the pre-Stop Active fence after the signed Stop receipt caused Suspended"
    );
    let OwnerSetupOriginalV1::Canonical {
        lookup: stored_lookup,
        ..
    } = &signed
    else {
        unreachable!()
    };
    assert_eq!(
        serde_json::to_vec(&lookup.unwrap()).unwrap(),
        serde_json::to_vec(stored_lookup.as_ref()).unwrap()
    );
    assert_eq!(
        before - remaining(&f.state),
        1,
        "atomic claim pays exactly one separate Cleanup24 transport"
    );
    assert!(
        f.state.test_sessions()[ID].termination_slots[&TerminationOperationV1::Stop]
            .result
            .is_none(),
        "a lookup claim is not result admission or Closed"
    );
    let claimed = f.state.clone();
    assert!(
        f.state
            .claim_known_test_session_termination_lookup(
                ID,
                TerminationOperationV1::Stop,
                "lost-stop-known-lookup",
                &live,
                UnixMillis::new(52_006),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, claimed);
    assert!(
        f.state
            .claim_known_test_session_termination_lookup(
                ID,
                TerminationOperationV1::Stop,
                "different-second-lookup",
                &live,
                UnixMillis::new(52_006),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, claimed);
    assert!(
        f.state
            .claim_test_session_termination(
                ID,
                TerminationOperationV1::Stop,
                true,
                &TestSessionTerminationCurrent {
                    live: &live,
                    world: &post_world
                },
                UnixMillis::new(52_006),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, claimed);
    // Reverse reservation cannot borrow the already-paid lookup's identity.
    assert!(
        f.state
            .reserve_test_session_source(
                ID,
                SessionSourceReservation {
                    operation_id: "lost-stop-known-lookup",
                    nonce: &"c7".repeat(32),
                    funding: SessionSourceFundingV1::TerminationPlan,
                },
                UnixMillis::new(52_006)
            )
            .is_err()
    );
    assert_eq!(f.state, claimed);
    {
        let mut corrupt = serde_json::to_value(&f.state).unwrap();
        corrupt["test_sessions"][ID]["termination_slots"]["stop"]
            .as_object_mut()
            .unwrap()
            .remove("lookup_paid_claim");
        assert!(serde_json::from_value::<AccountProvisionApprovalState>(corrupt).is_err());
    }
    for (field, value) in [
        ("attempt_id", json!("unpaid")),
        ("original_digest", json!("00".repeat(32))),
        ("claimed_at", json!("52004")),
        ("paid_transport_index", json!(0)),
    ] {
        let mut corrupt = serde_json::to_value(&f.state).unwrap();
        corrupt["test_sessions"][ID]["termination_slots"]["stop"]["lookup_paid_claim"][field] =
            value;
        assert!(serde_json::from_value::<AccountProvisionApprovalState>(corrupt).is_err());
    }
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(
            serde_json::to_value(&f.state).unwrap()
        )
        .unwrap(),
        f.state
    );
    // Archive the real signed lookup fact after live authority expires. It only
    // retains the exact result and cannot admit it or advance Stop/Revoke.
    let OwnerSetupOriginalV1::Canonical { lookup: known, .. } = &signed else {
        unreachable!()
    };
    let OwnerSetupResultV1::Canonical {
        response: committed_response,
        ..
    } = &committed
    else {
        unreachable!()
    };
    let lookup_fact = OwnerSetupResultV1::Canonical {
        from_lookup: true,
        response: ws::sign_service_response(
            ws::LOOKUP_PATH,
            ws::request_digest(ws::LOOKUP_PATH, known).unwrap(),
            committed_response.payload.clone(),
            &"3f".repeat(32),
        )
        .unwrap(),
    };
    let make_lookup_fact =
        |domain: &str, request_digest: String, payload| OwnerSetupResultV1::Canonical {
            from_lookup: true,
            response: ws::sign_service_response(domain, request_digest, payload, &"3f".repeat(32))
                .unwrap(),
        };
    let expected_digest = ws::request_digest(ws::LOOKUP_PATH, known).unwrap();
    // Correctly signed content mutations exercise binding/typed-result checks,
    // rather than relying on a deliberately invalid signature to reject them.
    let mut wrong_correlation = committed_response.payload.clone();
    wrong_correlation.correlation.payload_digest = "11".repeat(32);
    let mut wrong_branch = committed_response.payload.clone();
    let ws::IntentOutcome::Committed { commit, .. } = &mut wrong_branch.outcome else {
        unreachable!()
    };
    commit.binding.branch_id = "different-branch".into();
    let mut wrong_receipt = committed_response.payload.clone();
    let ws::IntentOutcome::Committed { receipt, .. } = &mut wrong_receipt.outcome else {
        unreachable!()
    };
    receipt["task_revision"] = json!(999u64);
    let mut bad_signature = lookup_fact.clone();
    let OwnerSetupResultV1::Canonical { response, .. } = &mut bad_signature else {
        unreachable!()
    };
    response.signature_hex.replace_range(
        ..2,
        if &response.signature_hex[..2] == "00" {
            "01"
        } else {
            "00"
        },
    );
    let mutations = [
        bad_signature,
        make_lookup_fact(
            ws::LOOKUP_PATH,
            "22".repeat(32),
            committed_response.payload.clone(),
        ),
        make_lookup_fact(
            ws::SUBMIT_PATH,
            expected_digest.clone(),
            committed_response.payload.clone(),
        ),
        make_lookup_fact(ws::LOOKUP_PATH, expected_digest.clone(), wrong_correlation),
        make_lookup_fact(ws::LOOKUP_PATH, expected_digest.clone(), wrong_branch),
        make_lookup_fact(ws::LOOKUP_PATH, expected_digest.clone(), wrong_receipt),
    ];
    for fact in mutations {
        let mut target = f.state.clone();
        let exact = target.clone();
        assert!(
            target
                .archive_test_session_termination_fact(
                    ID,
                    TerminationOperationV1::Stop,
                    fact,
                    &world_key,
                    &mut StrictCrypto
                )
                .is_err()
        );
        assert_eq!(target, exact);
    }
    let mut wrong_pin = f.state.clone();
    let exact_pin = wrong_pin.clone();
    assert!(
        wrong_pin
            .archive_test_session_termination_fact(
                ID,
                TerminationOperationV1::Stop,
                lookup_fact.clone(),
                &"00".repeat(32),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(wrong_pin, exact_pin);
    let mut pending_payload = committed_response.payload.clone();
    pending_payload.outcome = ws::IntentOutcome::Pending;
    let distinct_valid_fact = make_lookup_fact(ws::LOOKUP_PATH, expected_digest, pending_payload);
    StrictCrypto
        .verify(
            &f.state
                .test_session_termination_archive_checks(
                    ID,
                    TerminationOperationV1::Stop,
                    &distinct_valid_fact,
                    &world_key,
                )
                .unwrap(),
        )
        .unwrap();
    let mut archived = f.state.clone();
    archived.freeze(2).unwrap();
    let budgets = archived.test_sessions()[ID].roots.clone();
    archived
        .archive_test_session_termination_fact(
            ID,
            TerminationOperationV1::Stop,
            lookup_fact.clone(),
            &world_key,
            &mut StrictCrypto,
        )
        .unwrap();
    let exact = archived.clone();
    archived
        .archive_test_session_termination_fact(
            ID,
            TerminationOperationV1::Stop,
            lookup_fact.clone(),
            &world_key,
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(archived, exact);
    // This other fully valid signed fact was independently accepted by checks
    // against the empty source slot above; immutable same-source conflict alone
    // must now reject it without changing the already archived fact.
    assert!(
        archived
            .archive_test_session_termination_fact(
                ID,
                TerminationOperationV1::Stop,
                distinct_valid_fact,
                &world_key,
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(archived, exact);
    assert_eq!(archived.test_sessions()[ID].roots, budgets);
    assert!(
        archived.test_sessions()[ID].termination_slots[&TerminationOperationV1::Stop]
            .result
            .is_none()
    );
    assert!(
        archived
            .record_test_session_termination_result(
                ID,
                TerminationOperationV1::Stop,
                lookup_fact.clone(),
                &live,
                UnixMillis::new(61_000),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(archived, exact);
    // A legitimate fact without separate archive cannot pass the live admit
    // checks; the backward-compatible wrapper archives+admits atomically.
    assert!(
        f.state
            .test_session_termination_result_checks(
                ID,
                TerminationOperationV1::Stop,
                &lookup_fact,
                &live,
                UnixMillis::new(52_006)
            )
            .is_err()
    );
    f.state
        .record_test_session_termination_result(
            ID,
            TerminationOperationV1::Stop,
            lookup_fact,
            &live,
            UnixMillis::new(52_006),
            &mut StrictCrypto,
        )
        .unwrap();
    assert!(
        f.state.test_sessions()[ID].termination_slots[&TerminationOperationV1::Stop]
            .result
            .is_some()
    );
}
