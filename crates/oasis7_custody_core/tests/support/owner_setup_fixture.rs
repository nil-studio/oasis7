use super::*;
use oasis7_agent_api::game::*;
use oasis7_client_api::world_service::{self as ws, participant_discovery::*};
pub(super) fn root_session(
    s: &AccountProvisionApprovalState,
    nonce: String,
    now: u64,
) -> ParticipantSessionEvidenceV1 {
    let mut status = match s.native_stages()["login-attempt"].result.as_ref().unwrap() {
        NativeStageResultV1::SessionStatus(v) => v.status.clone(),
        _ => panic!("session"),
    };
    status.nonce = nonce;
    status.observed_at = DecimalU64(now);
    status.expires_at = DecimalU64(now + 5000);
    ParticipantSessionEvidenceV1 {
        status: SignedManagedNativeSessionStatusV1 {
            signature: URL_SAFE_NO_PAD
                .encode(key().sign(&status.signing_bytes().unwrap()).to_bytes()),
            key_id: "native-key".into(),
            status,
        },
        private_sid_generation: DecimalU64(1),
    }
}
pub(super) fn participant_bound() -> (
    AccountProvisionApprovalState,
    ApprovalAdmissionContext,
    NativeBindingPinV1,
) {
    let (mut s, c, pin) = participant_ready();
    let req = participant_request(&s);
    let id = &req.operation_id;
    s.reserve_participant_provision(req.clone(), nonces(), &scope(&pin), 1430)
        .unwrap();
    let p = Policy::new(s.draft());
    s.claim_participant_source(id, &p.current(&c), 1431)
        .unwrap();
    let mut fact = verification(s.accepted_approval().unwrap(), &c).verification;
    fact.purpose = VerificationPurposeV1::InspectOrigin;
    fact.verify_nonce = s
        .participant_original(id)
        .unwrap()
        .source_inspection
        .verify_nonce
        .clone();
    fact.observed_at = DecimalU64(1432);
    s.complete_participant_source(
        id,
        sign_verification(fact),
        &p.current(&c),
        1433,
        &mut StrictCrypto,
    )
    .unwrap();
    s.claim_participant_workflow(id, &scope(&pin), &p.current(&c), 1434, &mut StrictCrypto)
        .unwrap();
    let policy = s
        .claim_participant_step(
            id,
            ParticipantStepV1::Policy,
            &scope(&pin),
            &p.current(&c),
            1435,
            &mut StrictCrypto,
        )
        .unwrap();
    for _ in 0..2 {
        s.reserve_participant_read(id, 1435).unwrap();
    }
    let d = &policy.descriptor;
    let request = ws::sign_read_request(
        PARTICIPANT_POLICY_PATH,
        ParticipantPolicyRequestV1 {
            contract_version: 1,
            expected_world: ws::WorldIdentity {
                world_id: d.world.world_id.clone(),
                genesis_digest: d.world.genesis_digest.clone(),
            },
            trust_config_ref: d.world.service_public_key.clone(),
            observation_nonce: d.policy_nonce.clone(),
        },
        &"40".repeat(32),
    )
    .unwrap();
    let payload = json!({"world_id":d.world.world_id,"genesis_digest":d.world.genesis_digest,"branch_id":d.world.branch_id,"reorg_epoch":d.world.reorg_epoch,"logical_tick":"10","policy":{"api_rev":1,"world_id":d.world.world_id,"genesis_digest":d.world.genesis_digest,"identity_namespace":d.register.identity_namespace,"issuer_public_key":request.subject_public_key,"enabled":true,"registry_complete":true,"generation":"1","max_grant_ticks":"100"}});
    let original = ParticipantPolicyOriginalV1 {
        response: ws::sign_service_response(
            PARTICIPANT_POLICY_PATH,
            ws::request_digest(PARTICIPANT_POLICY_PATH, &request).unwrap(),
            payload,
            &"3f".repeat(32),
        )
        .unwrap(),
        request,
    };
    s.reserve_participant_callback(id, ParticipantCallbackV1::Policy, 1436)
        .unwrap();
    let evidence = root_session(&s, policy.session_observation_nonce, 1435);
    s.record_participant_policy(
        id,
        original,
        evidence,
        &scope(&pin),
        &p.current(&c),
        1436,
        &mut StrictCrypto,
    )
    .unwrap();
    s.claim_participant_step(
        id,
        ParticipantStepV1::SignBundle,
        &scope(&pin),
        &p.current(&c),
        1437,
        &mut StrictCrypto,
    )
    .unwrap();
    for _ in 0..2 {
        s.reserve_participant_read(id, 1437).unwrap();
    }
    let ticket = s.reserve_participant_read(id, 1437).unwrap();
    let bundle = ParticipantSignBundleV1 {
        owner: ws::sign_read_request(
            GAME_PARTICIPANT_REGISTER_DOMAIN,
            d.register.clone(),
            &"3e".repeat(32),
        )
        .unwrap(),
        read: ws::sign_read_request(ws::VIEW_JSON_PATH, d.owner_read.clone(), &"3e".repeat(32))
            .unwrap(),
    };
    s.publish_participant_bundle(
        id,
        ticket,
        bundle,
        &scope(&pin),
        &p.current(&c),
        1437,
        &mut StrictCrypto,
    )
    .unwrap();
    let admit = s
        .claim_participant_step(
            id,
            ParticipantStepV1::Authorize,
            &scope(&pin),
            &p.current(&c),
            1438,
            &mut StrictCrypto,
        )
        .unwrap();
    s.reserve_participant_read(id, 1438).unwrap();
    let admission = ws::sign_read_request(
        GAME_PARTICIPANT_ADMISSION_DOMAIN,
        GameParticipantAdmissionV1 {
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
            policy_generation: DecimalU64(1),
            valid_from_tick: DecimalU64(10),
            valid_until_tick: DecimalU64(100),
        },
        &"40".repeat(32),
    )
    .unwrap();
    s.reserve_participant_callback(id, ParticipantCallbackV1::Admission, 1438)
        .unwrap();
    let evidence = root_session(&s, admit.session_observation_nonce, 1438);
    s.record_participant_admission(
        id,
        admission,
        evidence,
        &scope(&pin),
        &p.current(&c),
        1438,
        &mut StrictCrypto,
    )
    .unwrap();
    let submit = s
        .claim_participant_step(
            id,
            ParticipantStepV1::Submit,
            &scope(&pin),
            &p.current(&c),
            1439,
            &mut StrictCrypto,
        )
        .unwrap();
    s.reserve_participant_read(id, 1439).unwrap();
    let submit_session = root_session(&s, submit.session_observation_nonce, 1439);
    let view = s
        .claim_participant_step(
            id,
            ParticipantStepV1::OwnerView,
            &scope(&pin),
            &p.current(&c),
            1440,
            &mut StrictCrypto,
        )
        .unwrap();
    s.reserve_participant_read(id, 1440).unwrap();
    let owner_session = root_session(&s, view.session_observation_nonce, 1440);
    let account = ws::request_digest(
        "participant-account-world",
        &(
            &d.register.identity_namespace.service_id,
            &d.register.identity_namespace.environment_id,
            &d.register.account_id,
            &d.register.world_id,
            &d.register.genesis_digest,
        ),
    )
    .unwrap();
    let agent = format!("participant:{}", account.strip_prefix("blake3:").unwrap());
    let receipt = GameParticipantReceiptV1 {
        api_rev: 1,
        identity_namespace: d.register.identity_namespace.clone(),
        account_id: d.register.account_id.clone(),
        world_id: d.world.world_id.clone(),
        genesis_digest: d.world.genesis_digest.clone(),
        agent_id: agent.clone(),
        owner_public_key: d.owner_key.public_key.clone(),
        owner_binding: account.clone(),
        owner_generation: DecimalU64(1),
        client_operation_id: d.register.client_operation_id.clone(),
        business_digest: oasis7_client_api::world_service_correlation::participant_business_digest(
            &d.register,
        )
        .unwrap(),
        registered_tick: DecimalU64(11),
        imported: false,
    };
    let commit = ws::CommitRef {
        world: ws::WorldIdentity {
            world_id: d.world.world_id.clone(),
            genesis_digest: d.world.genesis_digest.clone(),
        },
        binding: ws::ExecutionBinding {
            provider_world_id: d.world.world_id.clone(),
            branch_id: d.world.branch_id.clone(),
            finality_ref: "finality".into(),
            reorg_generation: d.world.reorg_epoch.0,
            governing_manifest_ref: "manifest".into(),
            authority_generation: 1,
            permission_generation: 1,
        },
        position: 1,
        execution_block_hash: "block".into(),
        state_root_ref: "root".into(),
    };
    let request = submit.submit.unwrap();
    let response = ws::sign_service_response(
        ws::SUBMIT_PATH,
        ws::request_digest(ws::SUBMIT_PATH, &request).unwrap(),
        ws::IntentResponse {
            formal_profile: None,
            execution_evidence_scope: None,
            contract_version: 1,
            correlation: request.correlation.clone(),
            outcome: ws::IntentOutcome::Committed {
                commit: Box::new(commit.clone()),
                receipt: serde_json::to_value(receipt).unwrap(),
            },
        },
        &"3f".repeat(32),
    )
    .unwrap();
    let owner_view:ParticipantOwnerControlViewV1=serde_json::from_value(json!({"canonical_agent_owner":{"agent_id":agent,"player_id":account,"public_key":d.owner_key.public_key,"world_id":d.world.world_id,"reorg_epoch":d.world.reorg_epoch.0,"canonical_authority":{"branch_id":d.world.branch_id,"agent_identity_generation":1},"current_intent_id":null},"game_task_control":null})).unwrap();
    let payload = ParticipantOwnerControlResponseV1 {
        schema_revision: 1,
        world: commit.world.clone(),
        version: ws::ProjectionVersion {
            commit,
            projection_revision: ws::request_digest("projection", &owner_view).unwrap(),
            visibility_scope: format!("agent:{agent}"),
        },
        logical_tick: 11,
        view: owner_view,
    };
    let owner_request = view.owner_view.unwrap();
    let owner_response = ws::sign_service_response(
        PARTICIPANT_OWNER_VIEW_PATH,
        ws::request_digest(PARTICIPANT_OWNER_VIEW_PATH, &owner_request).unwrap(),
        payload,
        &"3f".repeat(32),
    )
    .unwrap();
    s.reserve_participant_callback(id, ParticipantCallbackV1::Final, 1441)
        .unwrap();
    s.complete_participant(
        id,
        ParticipantFinalResultV1 {
            response_source: ParticipantResultSourceV1::Submit,
            response,
            owner_response,
            submit_session,
            lookup_session: None,
        },
        owner_session,
        &scope(&pin),
        &p.current(&c),
        1441,
        &mut StrictCrypto,
    )
    .unwrap();
    assert_eq!(s.provision().stage(), ProvisionStage::ParticipantBound);
    (s, c, pin)
}
