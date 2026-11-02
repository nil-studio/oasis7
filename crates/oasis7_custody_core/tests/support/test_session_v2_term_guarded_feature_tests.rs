//! Actual Core approval/World3 Ready/paid legacy source/strong services.
//! This targets the existing allocator's wrong Stop mode only. It does not
//! qualify the new six-edge Term caller or declare the fixture World to be live.
use super::*;
use oasis7_custody_core::native_binding::{
    ManagedNativeSessionLifetimeV1, SignedManagedNativeSessionLifetimeV1,
};
fn allocated_json(
    state: &mut AccountProvisionApprovalState,
    id: &str,
    current: &TestSessionTerminationCurrent<'_, '_, '_>,
    now: UnixMillis,
) -> serde_json::Value {
    // Baseline actual API; only the adapter changes to the independent guarded
    // successor after genuine RED. No fixture/target relaxation.
    serde_json::to_value(
        state
            .allocate_test_session_guarded_stop_v2(id, current.live, now, &mut StrictCrypto)
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn term_world3_actual_guarded_policy_must_allocate_owner_grant_in_first_stop_preimage() {
    guarded_current_window_case(false);
}
#[test]
fn term_guarded_stop_respects_a_valid_earlier_current_observation_window() {
    guarded_current_window_case(true);
}
fn guarded_current_window_case(earlier: bool) {
    let mut f = machine_tests::ready_world3(9000);
    let now = 31_000;
    f.state
        .expire_test_sessions_v2(UnixMillis::new(now))
        .unwrap();
    assert!(
        !serde_json::to_value(&f.state).unwrap()["test_sessions_v2"][&f.session_id]["closing"]
            .is_null(),
        "actual approved write deadline caused Closing"
    );
    let original = f.state.test_session_v2_original(&f.session_id).unwrap();
    let CustodyIntentV1::TestSessionV2 { plan, .. } = &original.input().request.intent else {
        unreachable!()
    };
    assert_eq!(
        plan.termination.cleanup_transport_policy,
        CleanupTransportPolicyV2::TermWideFiveSecondOwnedWorld3V2
    );
    assert_eq!(
        plan.termination.stop_policy,
        WorldTaskStopPolicyV2::GuardedCommittedUtcStopV2
    );
    let term = plan.termination.as_ref().clone();
    let source_id = "term-guarded-actual-source";
    let nonce = oasis7_custody_core::approval::bytes_digest(source_id.as_bytes());
    f.state
        .reserve_test_session_v2_source(
            &f.session_id,
            SessionSourceReservation {
                operation_id: source_id,
                nonce: &nonce,
                funding: SessionSourceFundingV1::TerminationPlan,
            },
            UnixMillis::new(now),
        )
        .unwrap();
    let policy = Policy::new(f.state.draft());
    let authority = policy.current(&f.context);
    assert_eq!(
        f.state
            .reserve_test_session_v2_source_read(&f.session_id, source_id, UnixMillis::new(now))
            .unwrap(),
        1
    );
    f.state
        .claim_test_session_v2_source(&f.session_id, source_id, &authority, UnixMillis::new(now))
        .unwrap();
    let mut v = verification(
        f.state.test_session_v2_original(&f.session_id).unwrap(),
        &f.context,
    )
    .verification;
    v.key_id = f
        .state
        .test_session_v2_original(&f.session_id)
        .unwrap()
        .native_verify_request()
        .unwrap()
        .approval
        .proof
        .key_id;
    v.purpose = VerificationPurposeV1::InspectOrigin;
    v.verify_nonce = nonce;
    v.observed_at = DecimalU64(now + 1);
    v.proof_state = if now + 1
        >= f.state
            .test_session_v2_original(&f.session_id)
            .unwrap()
            .input()
            .approval
            .proof
            .expires_at
            .0
    {
        ProofStateV1::Expired
    } else {
        ProofStateV1::Active
    };
    f.state
        .complete_test_session_v2_source(
            &f.session_id,
            source_id,
            sign_verification(v),
            &authority,
            UnixMillis::new(now + 1),
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(
        f.state
            .reserve_test_session_v2_source_read(&f.session_id, source_id, UnixMillis::new(now + 2))
            .unwrap(),
        2
    );
    let nonce = f
        .state
        .test_session_v2_lifetime_nonce(&f.session_id, source_id)
        .unwrap();
    let life = ManagedNativeSessionLifetimeV1 {
        api_version: 1,
        status: fixture::root_session(&f.state, nonce, now + 2)
            .status
            .status,
        session_expires_at: DecimalU64(90_000),
        session_idle_expires_at: DecimalU64(90_000),
    };
    let lifetime = SignedManagedNativeSessionLifetimeV1 {
        key_id: scope(&f.pin).native_root_key_id.into(),
        signature: URL_SAFE_NO_PAD.encode(key().sign(&life.signing_bytes().unwrap()).to_bytes()),
        lifetime: life,
    };
    let access = f
        .state
        .test_session_v2_snapshot(&f.session_id)
        .unwrap()
        .preflight_access()
        .unwrap()
        .clone();
    let old = f
        .state
        .participant_original("participant-one")
        .unwrap()
        .bundle
        .as_ref()
        .unwrap();
    let read = f
        .state
        .test_session_v2_termination_read(&f.session_id, source_id)
        .unwrap();
    let request = ws::participant_discovery::ParticipantOwnerViewRequestV1 {
        owner: old.owner.clone(),
        admission: access.admission,
        read: ws::sign_read_request(ws::VIEW_JSON_PATH, read, &"3e".repeat(32)).unwrap(),
        response_profile: Some(ParticipantOwnerViewProfileV1::OwnerSetupV1),
    };
    let Some(OwnerSetupResultV1::FinalObservation { owner, .. }) = f
        .state
        .owner_setup_outbox("setup-one", OwnerSetupStageV1::FinalObservation)
        .unwrap()
        .result
        .as_ref()
    else {
        unreachable!()
    };
    let response = ws::sign_service_response(
        PARTICIPANT_OWNER_VIEW_PATH,
        ws::request_digest(PARTICIPANT_OWNER_VIEW_PATH, &request).unwrap(),
        owner.payload.clone(),
        &"3f".repeat(32),
    )
    .unwrap();
    let world = OwnerSetupWorldEvidenceV1 { request, response };
    let native_scope = scope(&f.pin);
    let current_observed = if earlier { now + 3 - 4999 } else { now + 2 };
    let live = TestSessionLiveCurrent {
        provision: &authority,
        native_scope: &native_scope,
        lifetime: &lifetime,
        private_sid_generation: DecimalU64(1),
        source_operation_id: source_id,
        source_ticket: 2,
        observed_at: UnixMillis::new(current_observed),
    };
    if earlier {
        use oasis7_custody_core::approval_admission::ApprovalSignatureVerifier;
        let checks = f
            .state
            .test_session_v2_live_scope(
                &f.session_id,
                SessionPurposeV1::TerminationPlan,
                &live,
                UnixMillis::new(now + 3),
            )
            .unwrap();
        StrictCrypto.verify(&checks).unwrap();
    }
    let current = TestSessionTerminationCurrent {
        live: &live,
        world: &world,
    };
    let mut old_entry = f.state.clone();
    let old_before = serde_json::to_value(&old_entry).unwrap();
    let old_result = old_entry.allocate_test_session_stop_v2(
        &f.session_id,
        &current,
        UnixMillis::new(now + 3),
        &mut StrictCrypto,
    );
    if let Ok(path) = std::env::var("OASIS7_TERM_LEGACY_SIGNED_FIXTURE_EXPORT") {
        // Execute only on the actual old producer before its gate fix. The
        // cold fixture preserves complete original state, never a hand-filled slot.
        let payload = old_result.unwrap();
        let signed_payload =
            oasis7_client_api::world_service_wire::WorldServicePayloadV1::GameExecutorControl(
                ws::sign_read_request(
                    oasis7_agent_api::game::GAME_EXECUTOR_CONTROL_DOMAIN,
                    payload,
                    &"3e".repeat(32),
                )
                .unwrap(),
            );
        let correlation = oasis7_client_api::world_service_correlation::derive_correlation(
            ws::WorldIdentity {
                world_id: term.world.world_id.clone(),
                genesis_digest: term.world.genesis_digest.clone(),
            },
            &signed_payload,
        )
        .unwrap();
        let original = OwnerSetupOriginalV1::Canonical {
            submit: Box::new(ws::SubmitIntentRequest {
                contract_version: 1,
                correlation: correlation.clone(),
                deadline_unix_ms: Some(
                    old_entry
                        .test_session_stop_v2_original_until(&f.session_id)
                        .unwrap()
                        .get(),
                ),
                signed_payload: signed_payload.clone(),
            }),
            lookup: Box::new(oasis7_client_api::world_service_wire::AuthenticatedLookup {
                request: ws::LookupIntentRequest {
                    contract_version: 1,
                    key: correlation.key,
                },
                original: signed_payload,
            }),
        };
        old_entry
            .publish_test_session_stop_v2_original(
                &f.session_id,
                original.clone(),
                &current,
                UnixMillis::new(now + 3),
                &mut StrictCrypto,
            )
            .unwrap();
        let state = serde_json::to_value(&old_entry).unwrap();
        let cold: AccountProvisionApprovalState = serde_json::from_value(state.clone()).unwrap();
        assert_eq!(serde_json::to_value(cold).unwrap(), state);
        let fixture = serde_json::json!({"api_version":1,"state":state,"original":original,
            "session_id":f.session_id,"context":f.context,"native_pin":f.pin,"world":world,
            "observed_at":now+3});
        let bytes = serde_json::to_vec(&fixture).unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        for forbidden in ["\"token\":", "\"private_key\":", "\"signing_seed\":"] {
            assert!(
                !text.contains(forbidden),
                "fixture cannot export raw credentials or signer secrets"
            );
        }
        std::fs::write(path, bytes).unwrap();
        return;
    }
    assert!(
        old_result.is_err(),
        "actual Guarded consent cannot use legacy V2 Stop allocator"
    );
    assert_eq!(serde_json::to_value(&old_entry).unwrap(), old_before);
    let allocated = allocated_json(
        &mut f.state,
        &f.session_id,
        &current,
        UnixMillis::new(now + 3),
    );
    assert!(
        allocated.get("initial_fence").is_some()
            && allocated
                .get("observation_grant")
                .is_some_and(|v| !v.is_null()),
        "the actual consumed Guarded policy must return GuardedStop plus the Owner-signed Some scope, never legacy GameExecutorControlV1"
    );
    assert_eq!(
        allocated["initial_fence"],
        serde_json::to_value(&term.initial_fence).unwrap()
    );
    assert_eq!(allocated["client_operation_id"], term.stop_operation_id);
    assert_eq!(
        allocated["observation_grant"]["approved_cleanup_until_ms"],
        term.cleanup_until.get().to_string()
    );
    if earlier {
        let until: DecimalU64 =
            serde_json::from_value(allocated["execute_until_ms"].clone()).unwrap();
        assert!(
            until.0 <= current_observed + 5000,
            "Guarded original cannot outlive the actual earlier Current observation window"
        );
        return;
    }
    let payload: oasis7_agent_api::game::GameTaskStopV2 =
        serde_json::from_value(allocated).unwrap();
    let before_vault = serde_json::to_value(&f.state).unwrap();
    let token = f
        .state
        .claim_test_session_guarded_stop_vault_v2(
            &f.session_id,
            "term-guarded-owner-vault-one",
            &live,
            UnixMillis::new(now + 3),
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(token.payload(), &payload);
    assert_eq!(
        token.owner_key().public_key,
        term.initial_fence.owner_public_key
    );
    let paid = serde_json::to_value(&f.state).unwrap();
    let spent = |v: &serde_json::Value| -> u64 {
        v["test_sessions_v2"][&f.session_id]["roots"]["termination_plan"]["root"]["budgets"]["transport"]["spent"]
            .as_str().unwrap().parse().unwrap()
    };
    assert_eq!(
        spent(&paid),
        spent(&before_vault) + 1,
        "the direct Actor edge is independently charged"
    );
    let cold_unknown: AccountProvisionApprovalState = serde_json::from_value(paid.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(cold_unknown).unwrap(),
        paid,
        "cold unknown preserves the paid claim without recreating its handle"
    );

    assert!(
        f.state
            .claim_test_session_guarded_stop_vault_v2(
                &f.session_id,
                "term-guarded-owner-vault-repeat",
                &live,
                UnixMillis::new(now + 3),
                &mut StrictCrypto,
            )
            .is_err()
    );
    assert_eq!(serde_json::to_value(&f.state).unwrap(), paid);
    let signed = ws::sign_read_request(
        oasis7_agent_api::game::GAME_TASK_STOP_DOMAIN_V2,
        payload.clone(),
        &"3e".repeat(32),
    )
    .unwrap();
    let signed = oasis7_agent_api::game::SignedGameTaskStopV2 {
        request: signed.request,
        subject_public_key: signed.subject_public_key,
        signature_hex: signed.signature_hex,
    };
    assert_eq!(
        signed.subject_public_key,
        term.initial_fence.owner_public_key
    );
    let before = serde_json::to_value(&f.state).unwrap();
    let mut wrong = signed.clone();
    wrong.request.observation_grant = None;
    assert!(
        f.state
            .record_test_session_guarded_stop_signature_v2(&f.session_id, wrong)
            .is_err()
    );
    assert_eq!(serde_json::to_value(&f.state).unwrap(), before);
    f.state
        .record_test_session_guarded_stop_signature_v2(&f.session_id, signed)
        .unwrap();
    let raw = serde_json::to_value(&f.state).unwrap();
    let restored: AccountProvisionApprovalState = serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap(), raw);
    // Independent second physical Actor claim. Source1 fixture remains a Core
    // component and does not stand in for the installed Term6 caller.
    let before_close = serde_json::to_value(&f.state).unwrap();
    let close = f
        .state
        .allocate_test_session_close_v2(
            &f.session_id,
            &live,
            UnixMillis::new(now + 4),
            &mut StrictCrypto,
        )
        .unwrap();
    let close_first = f
        .state
        .claim_test_session_close_vault_v2(
            &f.session_id,
            "term-close-owner-vault-one",
            &live,
            UnixMillis::new(now + 4),
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(close_first.payload(), &close);
    assert_ne!(close_first.paid_record_digest(), token.paid_record_digest());
    let after_close = serde_json::to_value(&f.state).unwrap();
    assert_eq!(spent(&after_close), spent(&before_close) + 1);
    let repeat = f.state.claim_test_session_close_vault_v2(
        &f.session_id,
        "term-close-owner-vault-repeat",
        &live,
        UnixMillis::new(now + 4),
        &mut StrictCrypto,
    );
    assert!(repeat.is_err());
    assert_eq!(serde_json::to_value(&f.state).unwrap(), after_close);
    let signed_close = ws::sign_read_request(
        oasis7_agent_api::game::GAME_TASK_CLEANUP_DOMAIN_V2,
        close,
        &"3e".repeat(32),
    )
    .unwrap();
    f.state
        .record_test_session_close_v2_signature_history(&f.session_id, signed_close)
        .unwrap();
    let cold_close: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&f.state).unwrap()).unwrap();
    assert_eq!(cold_close, f.state);
    let mut corrupt = raw.clone();
    corrupt["test_sessions_v2"][&f.session_id]["termination_state"]["guarded_stop"]["signed"]["signature_hex"] =
        serde_json::json!("00".repeat(64));
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(corrupt).is_err());
    let mut stripped = raw;
    stripped["test_sessions_v2"][&f.session_id]["termination_state"]
        .as_object_mut()
        .unwrap()
        .remove("guarded_stop");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(stripped).is_err());
}

#[test]
fn term_inherited_signed_legacy_publish_is_history_only_not_live_write() {
    inherited_legacy_write(false);
}
#[test]
fn term_inherited_signed_legacy_submit_is_history_only_not_live_write() {
    inherited_legacy_write(true);
}
fn inherited_legacy_write(submit: bool) {
    use oasis7_custody_core::approval_admission::{
        ApprovalAdmissionContext, ApprovalSignatureVerifier,
    };
    use oasis7_custody_core::native_binding::NativeBindingPinV1;
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "test_session_v2_term_signed_legacy_fixture.json"
    ))
    .unwrap();
    assert_eq!(fixture["api_version"], 1);
    let id = fixture["session_id"].as_str().unwrap();
    let raw = fixture["state"].clone();
    let mut state: AccountProvisionApprovalState = serde_json::from_value(raw.clone()).unwrap();
    let mut expected_migration = raw.clone();
    expected_migration["test_sessions_v2"][id]["termination_state"]
        .as_object_mut()
        .unwrap()
        .insert("guarded_stop".into(), serde_json::Value::Null);
    assert_eq!(
        serde_json::to_value(&state).unwrap(),
        expected_migration,
        "only the documented new nullable slot may be added; all original legacy bytes/data stay fixed"
    );
    let context: ApprovalAdmissionContext =
        serde_json::from_value(fixture["context"].clone()).unwrap();
    let pin: NativeBindingPinV1 = serde_json::from_value(fixture["native_pin"].clone()).unwrap();
    let world: OwnerSetupWorldEvidenceV1 =
        serde_json::from_value(fixture["world"].clone()).unwrap();
    let original: OwnerSetupOriginalV1 =
        serde_json::from_value(fixture["original"].clone()).unwrap();
    let slot = &raw["test_sessions_v2"][id]["termination_state"]["stop"];
    assert!(!slot["signed"].is_null());
    assert!(slot["recovery"]["submit_claim"].is_null());
    assert_eq!(slot["signed"], serde_json::to_value(&original).unwrap());
    let lifetime: SignedManagedNativeSessionLifetimeV1 =
        serde_json::from_value(slot["recovery"]["native_lifetime"].clone()).unwrap();
    let source_id = slot["recovery"]["source_operation_id"].as_str().unwrap();
    let generation: DecimalU64 = serde_json::from_value(
        raw["test_sessions_v2"][id]["session"]["private_sid_generation"].clone(),
    )
    .unwrap();
    let policy = Policy::new(state.draft());
    let authority = policy.current(&context);
    let native_scope = scope(&pin);
    let now = UnixMillis::new(fixture["observed_at"].as_u64().unwrap());
    let live = TestSessionLiveCurrent {
        provision: &authority,
        native_scope: &native_scope,
        lifetime: &lifetime,
        private_sid_generation: generation,
        source_operation_id: source_id,
        source_ticket: 2,
        observed_at: now,
    };
    let checks = state
        .test_session_v2_live_scope(id, SessionPurposeV1::TerminationPlan, &live, now)
        .unwrap();
    StrictCrypto.verify(&checks).unwrap();
    let current = TestSessionTerminationCurrent {
        live: &live,
        world: &world,
    };
    let before = serde_json::to_value(&state).unwrap();
    if submit {
        let result = state.claim_test_session_stop_v2_world_submit(
            id,
            "inherited-legacy-submit-must-remain-closed",
            &current,
            now,
            &mut StrictCrypto,
        );
        assert!(
            result.is_err(),
            "cached signed legacy cannot dispatch under actual Guarded policy"
        );
    } else {
        let result = state.publish_test_session_stop_v2_original(
            id,
            original,
            &current,
            now,
            &mut StrictCrypto,
        );
        assert!(
            result.is_err(),
            "cached signed legacy cannot use live publish under actual Guarded policy"
        );
    }
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
}

#[test]
fn term_legacy_migration_requires_signed_original_and_rejects_other_missing_fields() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "test_session_v2_term_signed_legacy_fixture.json"
    ))
    .unwrap();
    let id = fixture["session_id"].as_str().unwrap();
    let raw = fixture["state"]["test_sessions_v2"][id]["termination_state"].clone();
    let state: TestSessionTerminationStateV2 = serde_json::from_value(raw.clone()).unwrap();
    let mut expected = raw.clone();
    expected
        .as_object_mut()
        .unwrap()
        .insert("guarded_stop".into(), serde_json::Value::Null);
    assert_eq!(serde_json::to_value(state).unwrap(), expected);
    for field in [
        "stop",
        "close",
        "archived_fact",
        "transports",
        "termination_plan_digest",
    ] {
        let mut missing = raw.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<TestSessionTerminationStateV2>(missing).is_err());
    }
    for signed in [false, true] {
        let mut invalid = raw.clone();
        if signed {
            invalid["stop"]["signed"]["original"]["submit"]["signed_payload"]["payload"]["signature_hex"] =
                serde_json::json!("00".repeat(64));
            invalid["stop"]["signed"]["original"]["lookup"]["original"]["payload"]["signature_hex"] =
                serde_json::json!("00".repeat(64));
        } else {
            invalid["stop"]["signed"] = serde_json::Value::Null;
        }
        assert!(serde_json::from_value::<TestSessionTerminationStateV2>(invalid).is_err());
    }
    let mut no_legacy = raw;
    no_legacy["stop"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<TestSessionTerminationStateV2>(no_legacy).is_err());
}
