use super::*;

fn participant_ready() -> (
    AccountProvisionApprovalState,
    ApprovalAdmissionContext,
    NativeBindingPinV1,
) {
    let (mut draft, mut input, mut c) = fixture();
    c.identity_namespace = "8:identity:test".into();
    input.approval.proof.identity_namespace = c.identity_namespace.clone();
    draft.entry.manager.identity_namespace = c.identity_namespace.clone();
    let world_pin =
        oasis7_client_api::world_service::sign_read_request("fixture", (), &"3f".repeat(32))
            .unwrap()
            .subject_public_key;
    draft.world_identity.service_public_key = world_pin.clone();
    draft.plan.transport_limit = DecimalU64(400);
    draft.entry.plan_digest = draft.plan.digest().unwrap();
    if let CustodyIntentV1::Provision {
        native_identity_namespace,
        world_identity,
        plan_digest,
        transport_limit,
        ..
    } = &mut input.request.intent
    {
        *native_identity_namespace = c.identity_namespace.clone();
        world_identity.service_public_key = world_pin;
        *plan_digest = draft.entry.plan_digest.clone();
        *transport_limit = DecimalU64(400);
    }
    input.approval.proof.intent_digest = input.request.intent_digest().unwrap();
    input.approval = sign_proof(input.approval.proof);
    let mut s = AccountProvisionApprovalState::new(draft).unwrap();
    let pending = begin(&mut s, input, &c);
    let p = Policy::new(s.draft());
    s.commit(
        "operation-one",
        verification(&pending, &c),
        &p.current(&c),
        1200,
        &mut StrictCrypto,
    )
    .unwrap();
    for (role, byte) in [("identity", "11"), ("owner", "22"), ("executor", "33")] {
        let mut result = role_result(&s, role, byte);
        let seed = match role {
            "identity" => "3d",
            "owner" => "3e",
            _ => "41",
        };
        result.public_ref.public_key =
            oasis7_client_api::world_service::sign_read_request("fixture", (), &seed.repeat(32))
                .unwrap()
                .subject_public_key;
        s.record_role_key_result(result, &p.current(&c), 1300)
            .unwrap();
    }
    let (mut s, c, pin, binding) = binding_ready_from((s, c), false);
    let permit = claim(
        &mut s,
        &c,
        &pin,
        "login-attempt",
        "binding-one",
        NativeStageKindV1::Login,
        1420,
    );
    let status = ManagedNativeSessionStatusV1 {
        api_version: 1,
        receipt: binding.receipt,
        service_epoch: c.authority.service_epoch,
        native_account_security_epoch: DecimalU64(1),
        native_credential_version: DecimalU64(1),
        session_ref: "native-session".into(),
        nonce: permit.nonce,
        generation: DecimalU64(1),
        active: true,
        frozen: false,
        pending: false,
        observed_at: DecimalU64(1424),
        expires_at: DecimalU64(6424),
    };
    let signed = SignedManagedNativeSessionStatusV1 {
        signature: URL_SAFE_NO_PAD.encode(key().sign(&status.signing_bytes().unwrap()).to_bytes()),
        key_id: "native-key".into(),
        status: status.clone(),
    };
    let digest = status.digest().unwrap();
    let p = Policy::new(s.draft());
    s.complete_native_stage(
        "login-attempt",
        NativeStageResultV1::SessionStatus(signed),
        &scope(&pin),
        &p.current(&c),
        1425,
        Some(&PersistedNativeSessionAttestation {
            status_digest: &digest,
            session_ref: &status.session_ref,
            private_sid_generation: 1,
        }),
        &mut StrictCrypto,
    )
    .unwrap();
    (s, c, pin)
}
fn participant_request(s: &AccountProvisionApprovalState) -> ParticipantProvisionRequestV1 {
    ParticipantProvisionRequestV1 {
        api_version: 1,
        custody_account_id: s.draft().entry.custody_account_id.clone(),
        provision_id: s.draft().entry.provision_id.clone(),
        operation_id: "participant-one".into(),
    }
}
fn nonces() -> ParticipantNoncesV1 {
    ParticipantNoncesV1 {
        source: "a1".repeat(32),
        policy: "a2".repeat(32),
        owner_view: "a3".repeat(32),
        session: "a4".repeat(32),
    }
}
#[test]
fn participant_reserves_exact_104_and_eight_local_tickets_never_grant() {
    let (mut s, c, pin) = participant_ready();
    let before = s.provision().transport_remaining().unwrap();
    let req = participant_request(&s);
    let original = s
        .reserve_participant_provision(req.clone(), nonces(), &scope(&pin), 1430)
        .unwrap();
    assert_eq!(before - s.provision().transport_remaining().unwrap(), 104);
    assert_eq!(
        original.descriptor.register.identity_namespace.service_id,
        "identity"
    );
    assert_eq!(
        original
            .descriptor
            .register
            .identity_namespace
            .environment_id,
        "test"
    );
    assert_eq!(
        original.descriptor.session.private_sid_generation,
        DecimalU64(1)
    );
    let p = Policy::new(s.draft());
    for n in 1..=8 {
        assert_eq!(
            s.reserve_participant_read(&req.operation_id, 1431).unwrap(),
            n
        );
    }
    assert!(s.reserve_participant_read(&req.operation_id, 1431).is_err());
    assert!(
        s.authorize_participant_read(
            &req.operation_id,
            ParticipantReadTicket {
                ticket: 1,
                step: ParticipantStepV1::Policy
            },
            &scope(&pin),
            &p.current(&c),
            1431,
            &mut StrictCrypto
        )
        .is_err()
    );
    assert!(
        s.claim_participant_workflow(
            &req.operation_id,
            &scope(&pin),
            &p.current(&c),
            1431,
            &mut StrictCrypto
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
fn participant_rejected_reservations_are_atomic_and_cannot_replace_original() {
    let (mut s, _, pin) = participant_ready();
    let req = participant_request(&s);
    let before = s.clone();
    let mut duplicate = nonces();
    duplicate.policy = duplicate.source.clone();
    assert!(
        s.reserve_participant_provision(req.clone(), duplicate, &scope(&pin), 1430)
            .is_err()
    );
    assert_eq!(s, before);
    s.reserve_participant_provision(req.clone(), nonces(), &scope(&pin), 1430)
        .unwrap();
    let reserved = s.clone();
    assert!(
        s.reserve_participant_provision(req.clone(), nonces(), &scope(&pin), 1431)
            .is_err()
    );
    assert_eq!(s, reserved);
    let mut replacement = req;
    replacement.operation_id = "replacement".into();
    assert!(
        s.reserve_participant_provision(replacement, nonces(), &scope(&pin), 1431)
            .is_err()
    );
    assert_eq!(s, reserved);
    let mut snapshot = serde_json::to_value(&s).unwrap();
    snapshot["participant_provisions"]["participant-one"]["descriptor"]["register"]["account_id"] =
        json!("other-native");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(snapshot).is_err());
}

#[test]
fn participant_source_is_strongly_verified_and_workflow_send_claim_is_once() {
    let (mut s, c, pin) = participant_ready();
    let req = participant_request(&s);
    s.reserve_participant_provision(req.clone(), nonces(), &scope(&pin), 1430)
        .unwrap();
    let p = Policy::new(s.draft());
    s.claim_participant_source(&req.operation_id, &p.current(&c), 1431)
        .unwrap();
    let mut fact = verification(s.accepted_approval().unwrap(), &c).verification;
    fact.purpose = VerificationPurposeV1::InspectOrigin;
    fact.verify_nonce = s
        .participant_original(&req.operation_id)
        .unwrap()
        .source_inspection
        .verify_nonce
        .clone();
    fact.observed_at = DecimalU64(1432);
    let signed = sign_verification(fact.clone());
    let mut wrong = fact;
    wrong.verify_nonce = "ff".repeat(32);
    assert!(
        s.complete_participant_source(
            &req.operation_id,
            sign_verification(wrong),
            &p.current(&c),
            1433,
            &mut StrictCrypto
        )
        .is_err()
    );
    let before = s.clone();
    let mut forged = signed.clone();
    forged.signature = "AA".repeat(43);
    assert!(
        s.complete_participant_source(
            &req.operation_id,
            forged,
            &p.current(&c),
            1433,
            &mut StrictCrypto
        )
        .is_err()
    );
    assert_eq!(s, before);
    s.complete_participant_source(
        &req.operation_id,
        signed,
        &p.current(&c),
        1433,
        &mut StrictCrypto,
    )
    .unwrap();
    s.claim_participant_workflow(
        &req.operation_id,
        &scope(&pin),
        &p.current(&c),
        1434,
        &mut StrictCrypto,
    )
    .unwrap();
    let claimed = s.clone();
    assert!(
        s.claim_participant_workflow(
            &req.operation_id,
            &scope(&pin),
            &p.current(&c),
            1434,
            &mut StrictCrypto
        )
        .is_err()
    );
    assert_eq!(s, claimed);
    let ticket = s.reserve_participant_read(&req.operation_id, 1435).unwrap();
    s.claim_participant_step(
        &req.operation_id,
        ParticipantStepV1::Policy,
        &scope(&pin),
        &p.current(&c),
        1435,
        &mut StrictCrypto,
    )
    .unwrap();
    let first = s
        .authorize_participant_read(
            &req.operation_id,
            ParticipantReadTicket {
                ticket,
                step: ParticipantStepV1::Policy,
            },
            &scope(&pin),
            &p.current(&c),
            1435,
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(first.target_security_epoch, DecimalU64(1));
    assert!(first.lookup_session_observation_nonce.is_none());
    assert!(
        s.claim_participant_step(
            &req.operation_id,
            ParticipantStepV1::Policy,
            &scope(&pin),
            &p.current(&c),
            1435,
            &mut StrictCrypto
        )
        .is_err()
    );
    assert!(
        s.authorize_participant_read(
            &req.operation_id,
            ParticipantReadTicket {
                ticket,
                step: ParticipantStepV1::Policy
            },
            &scope(&pin),
            &p.current(&c),
            6433,
            &mut StrictCrypto
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
fn participant_wire_is_closed_and_lookup_response_source_is_explicit() {
    let req = ParticipantProvisionRequestV1 {
        api_version: 1,
        custody_account_id: "custody-one".into(),
        provision_id: "provision-one".into(),
        operation_id: "participant-one".into(),
    };
    req.validate().unwrap();
    let mut raw = serde_json::to_value(&req).unwrap();
    raw["verified"] = json!(true);
    assert!(serde_json::from_value::<ParticipantProvisionRequestV1>(raw).is_err());
    assert_eq!(
        ParticipantResultSourceV1::Submit.response_domain(),
        oasis7_client_api::world_service::SUBMIT_PATH
    );
    assert_eq!(
        ParticipantResultSourceV1::Lookup.response_domain(),
        oasis7_client_api::world_service::LOOKUP_PATH
    );
    assert!(serde_json::from_value::<ParticipantResultSourceV1>(json!("pending")).is_err());
}

#[test]
fn participant_rejects_authentic_root_status_for_wrong_nonce_or_binding() {
    use oasis7_client_api::world_service::{
        self as ws,
        participant_discovery::{PARTICIPANT_POLICY_PATH, ParticipantPolicyRequestV1},
    };
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
    let view = s
        .claim_participant_step(
            id,
            ParticipantStepV1::Policy,
            &scope(&pin),
            &p.current(&c),
            1435,
            &mut StrictCrypto,
        )
        .unwrap();
    s.reserve_participant_callback(id, ParticipantCallbackV1::Policy, 1435)
        .unwrap();
    let d = &view.descriptor;
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
    let payload = json!({"world_id":d.world.world_id,"genesis_digest":d.world.genesis_digest,"branch_id":d.world.branch_id,"reorg_epoch":d.world.reorg_epoch,"logical_tick":"10","policy":{
        "api_rev":1,"world_id":d.world.world_id,"genesis_digest":d.world.genesis_digest,"identity_namespace":d.register.identity_namespace,"issuer_public_key":request.subject_public_key,"enabled":true,"registry_complete":true,"generation":"1","max_grant_ticks":"100"}});
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
    let mut status = match s.native_stages()["login-attempt"].result.as_ref().unwrap() {
        NativeStageResultV1::SessionStatus(v) => v.status.clone(),
        _ => panic!("expected actual session"),
    };
    status.nonce = view.session_observation_nonce.clone();
    status.observed_at = DecimalU64(1435);
    status.expires_at = DecimalU64(6435);
    let signed = |status: ManagedNativeSessionStatusV1| SignedManagedNativeSessionStatusV1 {
        signature: URL_SAFE_NO_PAD.encode(key().sign(&status.signing_bytes().unwrap()).to_bytes()),
        key_id: "native-key".into(),
        status,
    };
    let evidence = ParticipantSessionEvidenceV1 {
        status: signed(status.clone()),
        private_sid_generation: DecimalU64(1),
    };
    let mut positive = s.clone();
    positive
        .record_participant_policy(
            id,
            original.clone(),
            evidence,
            &scope(&pin),
            &p.current(&c),
            1436,
            &mut StrictCrypto,
        )
        .unwrap();
    let mut wrong = status.clone();
    wrong.nonce = "fe".repeat(32);
    let mut wrong_binding = status;
    wrong_binding.receipt.native_account_id = "different-native".into();
    // Both packets have genuine configured NativeRoot signatures. Their
    // rejection must come from persisted participant context, not bad crypto.
    for candidate in [wrong, wrong_binding] {
        let packet = signed(candidate);
        let bytes = packet.status.signing_bytes().unwrap();
        let signature = ed25519_dalek::Signature::from_slice(
            &URL_SAFE_NO_PAD.decode(&packet.signature).unwrap(),
        )
        .unwrap();
        key()
            .verifying_key()
            .verify_strict(&bytes, &signature)
            .unwrap();
        let before = s.clone();
        assert!(
            s.record_participant_policy(
                id,
                original.clone(),
                ParticipantSessionEvidenceV1 {
                    status: packet,
                    private_sid_generation: DecimalU64(1)
                },
                &scope(&pin),
                &p.current(&c),
                1436,
                &mut StrictCrypto
            )
            .is_err()
        );
        assert_eq!(s, before);
    }
}

#[path = "owner_setup.rs"]
mod owner_setup;
