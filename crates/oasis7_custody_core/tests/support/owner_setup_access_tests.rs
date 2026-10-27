use super::*;
fn resign_admission(e: &mut OwnerSetupAccessEvidenceV1, key: &str) {
    e.admission = ws::sign_read_request(
        oasis7_agent_api::game::GAME_PARTICIPANT_ADMISSION_DOMAIN,
        e.admission.request.clone(),
        &key.repeat(32),
    )
    .unwrap();
}
fn resign_policy(e: &mut OwnerSetupAccessEvidenceV1) {
    e.policy.response = ws::sign_service_response(
        PARTICIPANT_POLICY_PATH,
        ws::request_digest(PARTICIPANT_POLICY_PATH, &e.policy.request).unwrap(),
        e.policy.response.payload.clone(),
        &"3f".repeat(32),
    )
    .unwrap();
}
fn resign_status(s: &mut ParticipantSessionEvidenceV1) {
    s.status.signature = URL_SAFE_NO_PAD.encode(
        key()
            .sign(&s.status.status.signing_bytes().unwrap())
            .to_bytes(),
    );
}
#[test]
fn setup_fresh_access_replaces_expired_token_not_original_registration() {
    let (s, c, pin, candidate) = candidate_ready();
    let old = s.participant_original("participant-one").unwrap();
    let stage = s
        .owner_setup_outbox("setup-one", OwnerSetupStageV1::CreateTask)
        .unwrap();
    assert_eq!(
        old.admission.as_ref().unwrap().request.valid_until_tick,
        DecimalU64(100)
    );
    assert_eq!(
        old.admission.as_ref().unwrap().request.policy_generation,
        DecimalU64(1)
    );
    assert_eq!(
        candidate.observation.admission.request.valid_from_tick,
        DecimalU64(110)
    );
    assert_eq!(
        candidate.observation.admission.request.valid_until_tick,
        DecimalU64(210)
    );
    assert_eq!(
        candidate.observation.admission.request.policy_generation,
        DecimalU64(2)
    );
    assert_eq!(
        candidate.observation.owner,
        old.bundle.as_ref().unwrap().owner
    );
    assert_eq!(
        stage.access.as_ref().unwrap().policy.response.payload["logical_tick"],
        json!("110")
    );
    let policy = Policy::new(s.draft());
    let current = policy.current(&c);
    let native_scope = scope(&pin);
    let trusted = OwnerSetupCurrent {
        provision: &current,
        native_scope: &native_scope,
    };
    assert!(
        s.owner_setup_access_current_checks(
            "setup-one",
            OwnerSetupStageV1::CreateTask,
            &trusted,
            6516
        )
        .is_err()
    );
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(serde_json::to_value(&s).unwrap())
            .unwrap(),
        s
    );
}
#[test]
fn setup_authentic_fresh_access_scope_nonce_and_policy_races_reject_atomically() {
    let (mut s, c, pin) = access_prepared();
    let stage = OwnerSetupStageV1::CreateTask;
    s.reserve_owner_setup_read(
        "setup-one",
        stage,
        OwnerSetupReadPurposeV1::Sign,
        Role::Owner,
        1516,
    )
    .unwrap();
    let good = fresh_access(&s, stage);
    let policy = Policy::new(s.draft());
    let current = policy.current(&c);
    let native_scope = scope(&pin);
    let trusted = OwnerSetupCurrent {
        provision: &current,
        native_scope: &native_scope,
    };
    for case in 0..8 {
        let mut bad = good.clone();
        match case {
            0 => {
                bad.policy.request.request.observation_nonce = "dd".repeat(32);
                bad.policy.request = ws::sign_read_request(
                    PARTICIPANT_POLICY_PATH,
                    bad.policy.request.request.clone(),
                    &"40".repeat(32),
                )
                .unwrap();
                resign_policy(&mut bad);
            }
            1 => {
                bad.policy.response.payload["policy"]["identity_namespace"]["instance_epoch"] =
                    json!("7");
                bad.admission.request.identity_namespace.instance_epoch = DecimalU64(7);
                resign_policy(&mut bad);
                resign_admission(&mut bad, "40");
            }
            2 => {
                bad.admission.request.policy_generation = DecimalU64(3);
                resign_admission(&mut bad, "40");
            }
            3 => resign_admission(&mut bad, "41"),
            4 => {
                bad.admission.request.branch_id = "other-branch".into();
                resign_admission(&mut bad, "40");
            }
            5 => {
                bad.admission.request.valid_until_tick = DecimalU64(211);
                resign_admission(&mut bad, "40");
            }
            6 => {
                bad.policy_session.status.status.nonce = "ee".repeat(32);
                resign_status(&mut bad.policy_session);
            }
            _ => {
                bad.authorize_session.status.status.nonce =
                    bad.policy_session.status.status.nonce.clone();
                resign_status(&mut bad.authorize_session);
            }
        }
        let before = s.clone();
        assert!(
            s.record_owner_setup_access("setup-one", stage, bad, &trusted, 1516, &mut StrictCrypto)
                .is_err(),
            "case {case}"
        );
        assert_eq!(s, before);
    }
    s.record_owner_setup_access("setup-one", stage, good, &trusted, 1516, &mut StrictCrypto)
        .unwrap();
    assert!(
        s.owner_setup_signing_request("setup-one", stage, &trusted, 1516)
            .is_ok()
    );
}
#[test]
fn setup_access_pair_is_claimed_once_and_success_cannot_be_reissued() {
    let (mut s, c, pin) = access_prepared();
    let stage = OwnerSetupStageV1::CreateTask;
    let before = s.clone();
    assert!(
        s.claim_owner_setup_access("setup-one", stage, 1514)
            .is_err()
    );
    assert_eq!(s, before);
    s.reserve_owner_setup_read(
        "setup-one",
        stage,
        OwnerSetupReadPurposeV1::Sign,
        Role::Owner,
        1516,
    )
    .unwrap();
    let good = fresh_access(&s, stage);
    let policy = Policy::new(s.draft());
    let current = policy.current(&c);
    let native_scope = scope(&pin);
    let trusted = OwnerSetupCurrent {
        provision: &current,
        native_scope: &native_scope,
    };
    s.record_owner_setup_access(
        "setup-one",
        stage,
        good.clone(),
        &trusted,
        1516,
        &mut StrictCrypto,
    )
    .unwrap();
    let before = s.clone();
    s.record_owner_setup_access(
        "setup-one",
        stage,
        good.clone(),
        &trusted,
        1516,
        &mut StrictCrypto,
    )
    .unwrap();
    assert_eq!(s, before);
    let mut changed = good;
    changed.admission.request.valid_from_tick = DecimalU64(111);
    changed.admission.request.valid_until_tick = DecimalU64(211);
    resign_admission(&mut changed, "40");
    assert!(
        s.record_owner_setup_access(
            "setup-one",
            stage,
            changed,
            &trusted,
            1516,
            &mut StrictCrypto
        )
        .is_err()
    );
    assert_eq!(s, before);
}
