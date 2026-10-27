use super::*;
use oasis7_client_api::world_service::participant_discovery::*;
use oasis7_client_api::{
    world_service as ws,
    world_service_wire::{AuthenticatedLookup, WorldServicePayloadV1},
};
use oasis7_custody_core::http::Role;
fn fresh_access(
    s: &AccountProvisionApprovalState,
    stage: OwnerSetupStageV1,
) -> OwnerSetupAccessEvidenceV1 {
    let nonce = s.owner_setup_access_nonces("setup-one", stage).unwrap();
    let d = &s
        .participant_original("participant-one")
        .unwrap()
        .descriptor;
    let request = ws::sign_read_request(
        PARTICIPANT_POLICY_PATH,
        ParticipantPolicyRequestV1 {
            contract_version: 1,
            expected_world: ws::WorldIdentity {
                world_id: d.world.world_id.clone(),
                genesis_digest: d.world.genesis_digest.clone(),
            },
            trust_config_ref: d.world.service_public_key.clone(),
            observation_nonce: nonce.policy,
        },
        &"40".repeat(32),
    )
    .unwrap();
    let payload = json!({"world_id":d.world.world_id,"genesis_digest":d.world.genesis_digest,"branch_id":d.world.branch_id,"reorg_epoch":d.world.reorg_epoch,"logical_tick":"110","policy":{"api_rev":1,"world_id":d.world.world_id,"genesis_digest":d.world.genesis_digest,"identity_namespace":d.register.identity_namespace,"issuer_public_key":request.subject_public_key,"enabled":true,"registry_complete":true,"generation":"2","max_grant_ticks":"100"}});
    let policy = ParticipantPolicyOriginalV1 {
        response: ws::sign_service_response(
            PARTICIPANT_POLICY_PATH,
            ws::request_digest(PARTICIPANT_POLICY_PATH, &request).unwrap(),
            payload,
            &"3f".repeat(32),
        )
        .unwrap(),
        request,
    };
    let admission = ws::sign_read_request(
        oasis7_agent_api::game::GAME_PARTICIPANT_ADMISSION_DOMAIN,
        oasis7_agent_api::game::GameParticipantAdmissionV1 {
            api_rev: 1,
            business_digest:
                oasis7_client_api::world_service_correlation::participant_business_digest(
                    &d.register,
                )
                .unwrap(),
            identity_namespace: d.register.identity_namespace.clone(),
            account_id: d.register.account_id.clone(),
            world_id: d.world.world_id.clone(),
            genesis_digest: d.world.genesis_digest.clone(),
            owner_public_key: d.owner_key.public_key.clone(),
            branch_id: d.world.branch_id.clone(),
            reorg_epoch: d.world.reorg_epoch,
            policy_generation: DecimalU64(2),
            valid_from_tick: DecimalU64(110),
            valid_until_tick: DecimalU64(210),
        },
        &"40".repeat(32),
    )
    .unwrap();
    OwnerSetupAccessEvidenceV1 {
        policy,
        policy_session: fixture::root_session(s, nonce.policy_session, 1515),
        admission,
        authorize_session: fixture::root_session(s, nonce.authorize_session, 1515),
    }
}
fn access_prepared() -> (
    AccountProvisionApprovalState,
    ApprovalAdmissionContext,
    NativeBindingPinV1,
) {
    let (mut s, mut c, pin) = fixture::participant_bound();
    let plan = plan(&s);
    c.handoff_until = plan.deadline;
    let mut input = s.accepted_approval().unwrap().input().clone();
    input.operation_id = plan.setup_id.clone();
    input.request.context = c.authority.clone();
    input.request.approval_request_id = "setup-request".into();
    input.request.intent = plan.intent().unwrap();
    input.approval.proof.context = input.request.context.clone();
    input.approval.proof.intent_digest = input.request.intent_digest().unwrap();
    input.approval.proof.nonce = "setup-new-mfa-nonce".into();
    input.approval.proof.approval_id = "setup-proof".into();
    input.approval.proof.approval_request_id = "setup-request".into();
    input.approval.proof.issued_at = DecimalU64(1500);
    input.approval.proof.expires_at = DecimalU64(31500);
    input.approval = sign_proof(input.approval.proof);
    let policy = Policy::new(s.draft());
    let record = s
        .begin_owner_setup(
            OwnerSetupAdmissionV1 {
                input,
                plan,
                verify_nonce: "b1".repeat(32),
            },
            &policy.current(&c),
            1500,
            &mut StrictCrypto,
        )
        .unwrap();
    let mut fact = verification(&record, &c).verification;
    fact.observed_at = DecimalU64(1501);
    s.commit_owner_setup(
        CommitOwnerSetupV1 {
            id: "setup-one".into(),
            signed: sign_verification(fact),
            source_nonce: "b2".repeat(32),
        },
        &scope(&pin),
        &policy.current(&c),
        1502,
        &mut StrictCrypto,
    )
    .unwrap();
    s.claim_owner_setup_source("setup-one", &policy.current(&c), 1503)
        .unwrap();
    let mut fact = verification(s.owner_setup_original("setup-one").unwrap(), &c).verification;
    fact.purpose = VerificationPurposeV1::InspectOrigin;
    fact.verify_nonce = s
        .owner_setup_source("setup-one")
        .unwrap()
        .verify_nonce
        .clone();
    fact.observed_at = DecimalU64(1504);
    s.complete_owner_setup_source(
        "setup-one",
        sign_verification(fact),
        &policy.current(&c),
        1505,
        &mut StrictCrypto,
    )
    .unwrap();
    let stage = OwnerSetupStageV1::CreateTask;
    s.reserve_owner_setup_stage("setup-one", stage, &"b3".repeat(32), 1510)
        .unwrap();
    s.claim_owner_setup_stage_source("setup-one", stage, &policy.current(&c), 1511)
        .unwrap();
    let mut fact = verification(s.owner_setup_original("setup-one").unwrap(), &c).verification;
    fact.purpose = VerificationPurposeV1::InspectOrigin;
    fact.verify_nonce = s
        .owner_setup_outbox("setup-one", stage)
        .unwrap()
        .source
        .verify_nonce
        .clone();
    fact.observed_at = DecimalU64(1512);
    s.complete_owner_setup_stage_source(
        "setup-one",
        stage,
        sign_verification(fact),
        &policy.current(&c),
        1513,
        &mut StrictCrypto,
    )
    .unwrap();
    let native_scope = scope(&pin);
    let current = policy.current(&c);
    let trusted = OwnerSetupCurrent {
        provision: &current,
        native_scope: &native_scope,
    };
    for ticket in 1..=2 {
        let read = s
            .reserve_owner_setup_read(
                "setup-one",
                stage,
                OwnerSetupReadPurposeV1::Session,
                Role::Identity,
                1514,
            )
            .unwrap();
        s.authorize_owner_setup_read(
            "setup-one",
            read,
            Role::Identity,
            &trusted,
            1514,
            &mut StrictCrypto,
        )
        .unwrap();
        if ticket == 2 {
            s.claim_owner_setup_access("setup-one", stage, 1514)
                .unwrap();
        }
    }
    (s, c, pin)
}
fn candidate_ready() -> (
    AccountProvisionApprovalState,
    ApprovalAdmissionContext,
    NativeBindingPinV1,
    OwnerSetupCandidateV1,
) {
    let (mut s, c, pin) = access_prepared();
    let stage = OwnerSetupStageV1::CreateTask;
    let policy = Policy::new(s.draft());
    let current = policy.current(&c);
    let native_scope = scope(&pin);
    let trusted = OwnerSetupCurrent {
        provision: &current,
        native_scope: &native_scope,
    };
    let evidence = fresh_access(&s, stage);
    let read = s
        .reserve_owner_setup_read(
            "setup-one",
            stage,
            OwnerSetupReadPurposeV1::Sign,
            Role::Owner,
            1516,
        )
        .unwrap();
    s.record_owner_setup_access(
        "setup-one",
        stage,
        evidence,
        &trusted,
        1516,
        &mut StrictCrypto,
    )
    .unwrap();
    s.authorize_owner_setup_read(
        "setup-one",
        read,
        Role::Owner,
        &trusted,
        1516,
        &mut StrictCrypto,
    )
    .unwrap();
    let read = s
        .reserve_owner_setup_read(
            "setup-one",
            stage,
            OwnerSetupReadPurposeV1::Sign,
            Role::Owner,
            1516,
        )
        .unwrap();
    s.authorize_owner_setup_read(
        "setup-one",
        read,
        Role::Owner,
        &trusted,
        1516,
        &mut StrictCrypto,
    )
    .unwrap();
    let bundle = s
        .owner_setup_signing_request("setup-one", stage, &trusted, 1516)
        .unwrap();
    let OwnerSetupSigningRequestV1::CreateTask(request) = bundle.action else {
        panic!("fixed create")
    };
    let payload = WorldServicePayloadV1::GameTaskCreate(
        ws::sign_read_request(
            oasis7_agent_api::game::GAME_TASK_CREATE_DOMAIN,
            request,
            &"3e".repeat(32),
        )
        .unwrap(),
    );
    let correlation = oasis7_client_api::world_service_correlation::derive_correlation(
        bundle.observation.world.clone(),
        &payload,
    )
    .unwrap();
    let submit = ws::SubmitIntentRequest {
        contract_version: 1,
        correlation: correlation.clone(),
        deadline_unix_ms: Some(80_000),
        signed_payload: payload.clone(),
    };
    let lookup = AuthenticatedLookup {
        request: ws::LookupIntentRequest {
            contract_version: 1,
            key: correlation.key,
        },
        original: payload,
    };
    let participant = s.participant_original("participant-one").unwrap();
    let observation = ParticipantOwnerViewRequestV1 {
        owner: participant.bundle.as_ref().unwrap().owner.clone(),
        admission: bundle.participant_access.admission,
        read: ws::sign_read_request(ws::VIEW_JSON_PATH, bundle.observation, &"3e".repeat(32))
            .unwrap(),
        response_profile: Some(ParticipantOwnerViewProfileV1::OwnerSetupV1),
    };
    let candidate = OwnerSetupCandidateV1 {
        original: OwnerSetupOriginalV1::Canonical {
            submit: Box::new(submit),
            lookup: Box::new(lookup),
        },
        observation,
    };
    let read = s
        .reserve_owner_setup_read(
            "setup-one",
            stage,
            OwnerSetupReadPurposeV1::Sign,
            Role::Owner,
            1516,
        )
        .unwrap();
    s.publish_owner_setup_original(
        "setup-one",
        read,
        candidate.clone(),
        &trusted,
        1516,
        &mut StrictCrypto,
    )
    .unwrap();
    (s, c, pin, candidate)
}
fn admission(
    s: &AccountProvisionApprovalState,
    candidate: &OwnerSetupCandidateV1,
    now: u64,
) -> OwnerSetupAdmissionEvidenceV1 {
    let mut control = s
        .participant_original("participant-one")
        .unwrap()
        .result
        .as_ref()
        .unwrap()
        .owner_response
        .payload
        .clone();
    control.logical_tick = 111;
    let response = ws::sign_service_response(
        PARTICIPANT_OWNER_VIEW_PATH,
        ws::request_digest(PARTICIPANT_OWNER_VIEW_PATH, &candidate.observation).unwrap(),
        OwnerSetupObservationV1 {
            api_version: 1,
            control,
            signer_delegation: None,
        },
        &"3f".repeat(32),
    )
    .unwrap();
    let nonce = candidate
        .observation
        .read
        .request
        .observation_nonce
        .clone()
        .unwrap();
    OwnerSetupAdmissionEvidenceV1 {
        world: OwnerSetupWorldEvidenceV1 {
            request: candidate.observation.clone(),
            response,
        },
        session: fixture::root_session(s, nonce, now),
    }
}
#[test]
fn setup_candidate_does_not_grant_send_and_world_admission_is_once() {
    let (mut s, c, pin, candidate) = candidate_ready();
    let policy = Policy::new(s.draft());
    let current = policy.current(&c);
    let native_scope = scope(&pin);
    let trusted = OwnerSetupCurrent {
        provision: &current,
        native_scope: &native_scope,
    };
    let stage = OwnerSetupStageV1::CreateTask;
    let before = s.clone();
    assert!(
        s.claim_owner_setup_send("setup-one", stage, &trusted, 1517, &mut StrictCrypto)
            .is_err()
    );
    assert_eq!(s, before);
    s.reserve_owner_setup_admission_callback("setup-one", stage, 1518)
        .unwrap();
    assert!(
        s.reserve_owner_setup_admission_callback("setup-one", stage, 1518)
            .is_err()
    );
    let evidence = admission(&s, &candidate, 1519);
    s.record_owner_setup_admission(
        "setup-one",
        stage,
        evidence,
        &trusted,
        1520,
        &mut StrictCrypto,
    )
    .unwrap();
    assert_eq!(s.provision().stage(), ProvisionStage::ReadyForSetup);
    s.claim_owner_setup_send("setup-one", stage, &trusted, 1521, &mut StrictCrypto)
        .unwrap();
    assert!(
        s.claim_owner_setup_send("setup-one", stage, &trusted, 1521, &mut StrictCrypto)
            .is_err()
    );
    let permit = s
        .owner_setup_effect_permit("setup-one", stage, &trusted, 1521, &mut StrictCrypto)
        .unwrap();
    assert_eq!(permit.kind, OwnerSetupPermitKindV1::Effect);
    s.reserve_owner_setup_callback("setup-one", stage, 1522)
        .unwrap();
    assert!(
        s.reserve_owner_setup_callback("setup-one", stage, 1522)
            .is_err()
    );
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(serde_json::to_value(&s).unwrap())
            .unwrap(),
        s
    );
}
#[test]
fn setup_authentic_changed_world_keeps_published_original_pending() {
    let (mut s, c, pin, candidate) = candidate_ready();
    let policy = Policy::new(s.draft());
    let current = policy.current(&c);
    let native_scope = scope(&pin);
    let trusted = OwnerSetupCurrent {
        provision: &current,
        native_scope: &native_scope,
    };
    let stage = OwnerSetupStageV1::CreateTask;
    s.reserve_owner_setup_admission_callback("setup-one", stage, 1518)
        .unwrap();
    let mut evidence = admission(&s, &candidate, 1519);
    let OwnerSetupOriginalV1::Canonical { submit, .. } = &candidate.original else {
        unreachable!()
    };
    let legacy = serde_json::from_value(serde_json::to_value(submit).unwrap()).unwrap();
    evidence
        .world
        .response
        .payload
        .control
        .view
        .game_task_control =
        Some(oasis7_client_api::legacy_task_results::create_control(&legacy).unwrap());
    let view = &evidence.world.response.payload.control.view;
    evidence
        .world
        .response
        .payload
        .control
        .version
        .projection_revision = ws::request_digest("projection", view).unwrap();
    evidence.world.response = ws::sign_service_response(
        PARTICIPANT_OWNER_VIEW_PATH,
        evidence.world.response.request_digest,
        evidence.world.response.payload,
        &"3f".repeat(32),
    )
    .unwrap();
    let before = s.clone();
    assert!(
        s.record_owner_setup_admission(
            "setup-one",
            stage,
            evidence,
            &trusted,
            1520,
            &mut StrictCrypto
        )
        .is_err()
    );
    assert_eq!(s, before);
    assert_eq!(
        s.owner_setup_outbox("setup-one", stage)
            .unwrap()
            .original
            .as_ref(),
        Some(&candidate.original)
    );
    assert_eq!(s.provision().stage(), ProvisionStage::ParticipantBound);
    assert!(
        s.claim_owner_setup_send("setup-one", stage, &trusted, 1521, &mut StrictCrypto)
            .is_err()
    );
}

#[path = "owner_setup_archive_tests.rs"]
mod archive_tests;

#[path = "owner_setup_access_tests.rs"]
mod access_tests;

#[path = "owner_setup_reconcile_tests.rs"]
mod reconcile_tests;
