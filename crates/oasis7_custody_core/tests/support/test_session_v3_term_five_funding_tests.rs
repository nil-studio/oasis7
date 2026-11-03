//! Genuine Core full approval/three World receipts/Ready/HumanFinish prefix.
//! Service keys are fixtures; no actual Actor, Native IO or Current is claimed.
use super::*;
use oasis7_custody_core::provisioning::{
    PreparedTermFiveFundingV3, TermObservationPortV3 as Port, TermObservationRoundV3 as Round,
    TermOwnedInvocationV2,
};

fn finish(mut f: SessionFixture) -> (SessionFixture, TermOwnedInvocationV2) {
    let mut q = request(&f, "v3-term-initial-claim", None);
    q.purposes = vec![TestSessionPurposeV1::Execute, TestSessionPurposeV1::Read];
    let mut read = allocation(32);
    read.purpose = SessionPurposeV1::ReconcileRead;
    assert!(
        claim(&mut f, &q, &[allocation(31), read], 41, 12_000)
            .unwrap()
            .newly_claimed
    );
    let q = SessionControlRequestV1 {
        api_version: 1,
        session_id: f.session_id.clone(),
        request_id: "v3-term-human-finish".into(),
        kind: SessionControlKindV1::Finish,
    };
    let nonce = control_tests::challenge(&mut f, &q, SessionPurposeV1::Execution, 71, 13_000);
    let bytes = serde_json::to_vec(&q).unwrap();
    let pop = control_tests::control_pop(
        &f,
        &bytes,
        (
            HttpPopRoute::Finish(f.session_id.clone()),
            HttpPopProfileV1::Custody,
        ),
        &q.request_id,
        Some(&nonce),
        SessionPurposeV1::Execution,
        13_003,
    );
    let ingress = "v3-term-human-finish-http";
    control_tests::control_paid(&mut f, &pop, ingress, 13_003);
    let (ack, owned) = f
        .state
        .control_test_session_v2_and_begin_owned_term(
            SessionProofRequest {
                ingress_attempt_id: ingress,
                original_bytes: &bytes,
                possession: &pop,
            },
            UnixMillis::new(13_005),
            &mut NoNativeChecks,
        )
        .unwrap();
    assert_eq!(ack.kind, SessionControlKindV1::Finish);
    (
        f,
        owned.expect("only actual first HumanFinish in Ready owns this continuation"),
    )
}
fn spent(f: &SessionFixture) -> u64 {
    serde_json::to_value(&f.state).unwrap()["test_sessions_v2"][&f.session_id]
        ["roots"]["termination_plan"]["root"]["budgets"]["transport"]["spent"]
        .as_str().unwrap().parse().unwrap()
}
fn ports(f: &mut SessionFixture, round: &PreparedTermFiveFundingV3, at: u64, paid: u32) {
    for (n, port) in [
        Port::DirectoryActor,
        Port::RfDispatcherActor,
        Port::FencesActor,
        Port::NativeSessionsActor,
        Port::NativeHttp,
    ]
    .into_iter()
    .enumerate()
    {
        let token = f
            .state
            .claim_test_session_term_five_port_v3(round, port, UnixMillis::new(at + n as u64))
            .unwrap();
        assert_eq!(token.kind(), port);
        assert_eq!(token.paid_index(), paid + n as u32);
        assert_eq!(token.until(), round.until());
        let exact = f.state.clone();
        assert!(
            f.state
                .claim_test_session_term_five_port_v3(round, port, UnixMillis::new(at + n as u64))
                .is_err()
        );
        assert_eq!(f.state, exact);
        assert_eq!(
            serde_json::from_value::<AccountProvisionApprovalState>(
                serde_json::to_value(&f.state).unwrap()
            )
            .unwrap(),
            f.state
        );
    }
}

#[test]
fn term_v3_real_human_finish_pays_five_twice_without_qualification_and_claims_once() {
    let (mut f, owned) = finish(super::super::machine_tests::ready_v3(9000));
    assert_eq!(spent(&f), 0);
    let exact = f.state.clone();
    assert!(
        f.state
            .claim_test_session_term_owner_slot_read_v2(
                &owned,
                "must-not-add-qualification",
                UnixMillis::new(13_006)
            )
            .is_err()
    );
    assert!(
        f.state
            .reserve_test_session_term_six_v2(
                &owned,
                oasis7_custody_core::provisioning::TermObservationRoundV2::First,
                UnixMillis::new(13_006)
            )
            .is_err()
    );
    assert_eq!(f.state, exact);
    let first = f
        .state
        .reserve_test_session_term_five_v3(&owned, Round::First, UnixMillis::new(13_007))
        .unwrap();
    assert_eq!(spent(&f), 5);
    let exact = f.state.clone();
    assert!(
        f.state
            .reserve_test_session_term_five_v3(&owned, Round::First, UnixMillis::new(13_008))
            .is_err()
    );
    assert!(
        f.state
            .reserve_test_session_term_five_v3(&owned, Round::Fresh, UnixMillis::new(13_008))
            .is_err()
    );
    assert_eq!(f.state, exact);
    ports(&mut f, &first, 13_010, 0);
    // Ownership survives the first fact window; this does not renew Current.
    let fresh = f
        .state
        .reserve_test_session_term_five_v3(&owned, Round::Fresh, UnixMillis::new(19_000))
        .unwrap();
    assert_eq!(spent(&f), 10);
    assert_eq!(first.until(), UnixMillis::new(18_007));
    assert_eq!(fresh.until(), UnixMillis::new(24_000));
    ports(&mut f, &fresh, 19_001, 5);
    let exact = f.state.clone();
    assert!(
        f.state
            .reserve_test_session_term_five_v3(&owned, Round::Fresh, UnixMillis::new(19_010))
            .is_err()
    );
    assert_eq!(f.state, exact);
    let raw = serde_json::to_value(&f.state).unwrap();
    assert!(
        raw["test_sessions_v2"][&f.session_id]["termination_state"]["qualification_read"].is_null()
    );
    assert!(
        raw["test_sessions_v2"][&f.session_id]["termination_state"]
            .get("term_six_funding")
            .is_none()
    );
}

#[test]
fn term_v3_cold_five_ledger_rejects_missing_slot_old_ports_holes_and_expiry_changes() {
    let (mut f, owned) = finish(super::super::machine_tests::ready_v3(9000));
    let first = f
        .state
        .reserve_test_session_term_five_v3(&owned, Round::First, UnixMillis::new(13_007))
        .unwrap();
    ports(&mut f, &first, 13_010, 0);
    let raw = serde_json::to_value(&f.state).unwrap();
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(raw.clone()).unwrap(),
        f.state
    );
    for mode in 0..5 {
        let mut bad = raw.clone();
        let term = &mut bad["test_sessions_v2"][&f.session_id]["termination_state"];
        match mode {
            0 => {
                term.as_object_mut().unwrap().remove("term_five_funding");
            }
            1 => {
                term["term_five_funding"][0]["header"]["ports"][3]["kind"] =
                    "identity_service".into()
            }
            2 => term["term_five_funding"][0]["header"]["ports"][0]["paid_index"] = 7.into(),
            3 => term["term_five_funding"][0]["header"]["until"] = "18008".into(),
            4 => term["term_five_funding"][0]["claims"]["native_http"]["ordinal"] = 1.into(),
            _ => unreachable!(),
        }
        assert!(
            serde_json::from_value::<AccountProvisionApprovalState>(bad).is_err(),
            "cold variant {mode}"
        );
    }
    let (mut old, old_owned) = finish(super::super::machine_tests::ready_world3(9000));
    let exact = old.state.clone();
    assert!(
        old.state
            .reserve_test_session_term_five_v3(&old_owned, Round::First, UnixMillis::new(13_007))
            .is_err()
    );
    assert_eq!(old.state, exact);
    let mut old_raw = serde_json::to_value(&old.state).unwrap();
    assert!(
        old_raw["test_sessions_v2"][&old.session_id]["termination_state"]
            .get("term_five_funding")
            .is_none()
    );
    old_raw["test_sessions_v2"][&old.session_id]["termination_state"]["term_five_funding"] =
        serde_json::json!([]);
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(old_raw).is_err());
}

/// Genuine Core Abort PoP preserves its ordinary Ack, not a new V3 payer.
/// Native/service signatures remain the existing explicit fixture boundary.
#[test]
fn term_v3_human_abort_ack_does_not_birth_owned_five_payer() {
    let mut f = super::super::machine_tests::ready_v3(9000);
    let mut initial = request(&f, "v3-abort-initial-claim", None);
    initial.purposes = vec![TestSessionPurposeV1::Execute, TestSessionPurposeV1::Read];
    let mut read = allocation(32);
    read.purpose = SessionPurposeV1::ReconcileRead;
    assert!(
        claim(&mut f, &initial, &[allocation(31), read], 41, 12_000)
            .unwrap()
            .newly_claimed
    );
    let request = SessionControlRequestV1 {
        api_version: 1,
        session_id: f.session_id.clone(),
        request_id: "v3-term-human-abort".into(),
        kind: SessionControlKindV1::Abort,
    };
    let nonce = control_tests::challenge(&mut f, &request, SessionPurposeV1::Execution, 71, 13_000);
    let bytes = serde_json::to_vec(&request).unwrap();
    let pop = control_tests::control_pop(
        &f,
        &bytes,
        (
            HttpPopRoute::Abort(f.session_id.clone()),
            HttpPopProfileV1::Custody,
        ),
        &request.request_id,
        Some(&nonce),
        SessionPurposeV1::Execution,
        13_003,
    );
    let ingress = "v3-term-human-abort-http";
    control_tests::control_paid(&mut f, &pop, ingress, 13_003);
    let mut ordinary = f.state.clone();
    let expected_ack = ordinary
        .control_test_session_v2(
            SessionProofRequest {
                ingress_attempt_id: ingress,
                original_bytes: &bytes,
                possession: &pop,
            },
            None,
            UnixMillis::new(13_005),
            &mut NoNativeChecks,
        )
        .unwrap();
    let (ack, owned) = f
        .state
        .control_test_session_v2_and_begin_owned_term(
            SessionProofRequest {
                ingress_attempt_id: ingress,
                original_bytes: &bytes,
                possession: &pop,
            },
            UnixMillis::new(13_005),
            &mut NoNativeChecks,
        )
        .unwrap();
    assert_eq!(ack.kind, SessionControlKindV1::Abort);
    assert_eq!(
        serde_json::to_value(&ack).unwrap(),
        serde_json::to_value(&expected_ack).unwrap(),
        "actual Abort must preserve its complete ordinary Ack"
    );
    assert_eq!(spent(&f), 0, "actual Abort Ack must not spend Term24");
    // Optional actual pre-fix artifact, never fabricated or printed. Root can
    // replay this exact service-key fixture after the cold eligibility fix.
    if owned.is_some() {
        if let Some(path) = std::env::var_os("OASIS7_TERM_V3_ABORT_LEGACY_STATE_OUT") {
            use std::io::Write;
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(path).unwrap();
            file.write_all(&serde_json::to_vec(&f.state).unwrap())
                .unwrap();
            file.sync_all().unwrap();
            println!("term_v3_actual_abort_owned_legacy_fixture_saved=true");
        }
    }
    if let Some(path) = std::env::var_os("OASIS7_TERM_V3_ABORT_LEGACY_STATE_IN") {
        assert!(
            serde_json::from_slice::<AccountProvisionApprovalState>(&std::fs::read(path).unwrap())
                .is_err(),
            "exact actual pre-fix Abort-owned state must fail cold validation"
        );
    }
    if let Some(ref previous_owned) = owned {
        let after_ack = f.state.clone();
        assert!(
            f.state
                .reserve_test_session_term_five_v3(
                    previous_owned,
                    Round::First,
                    UnixMillis::new(13_007)
                )
                .is_err(),
            "HumanAbort must not produce the HumanFinish-only V3 payer"
        );
        assert_eq!(f.state, after_ack);
    }
    assert!(
        owned.is_none(),
        "V3 Abort Ack must not return a HumanFinish-only first continuation"
    );
    assert_eq!(
        f.state, ordinary,
        "ordinary Abort appends no new V3 ownership/qualification/funding"
    );
    assert_eq!(spent(&f), 0);
    let cold: AccountProvisionApprovalState =
        serde_json::from_slice(&serde_json::to_vec(&f.state).unwrap()).unwrap();
    assert_eq!(cold, f.state);
}

#[test]
fn term_v3_direct_paid_pair_uses_actual_ready_finish_and_keeps_cold_originals() {
    use ed25519_dalek::Signer as _;
    use oasis7_custody_core::native_cleanup_v2::*;
    use oasis7_custody_core::provisioning::TermDirectIssuerV3;
    let (mut f, owned) = finish(super::super::machine_tests::ready_v3(9000));
    let first = f
        .state
        .reserve_test_session_term_five_v3(&owned, Round::First, UnixMillis::new(13_006))
        .unwrap();
    for (n, p) in [
        Port::DirectoryActor,
        Port::RfDispatcherActor,
        Port::FencesActor,
    ]
    .into_iter()
    .enumerate()
    {
        f.state
            .claim_test_session_term_five_port_v3(&first, p, UnixMillis::new(13_007 + n as u64))
            .unwrap();
    }
    let actor = f
        .state
        .claim_test_session_term_five_port_v3(
            &first,
            Port::NativeSessionsActor,
            UnixMillis::new(13_010),
        )
        .unwrap();
    let native = f
        .state
        .claim_test_session_term_five_port_v3(&first, Port::NativeHttp, UnixMillis::new(13_011))
        .unwrap();
    let key = ed25519_dalek::SigningKey::from_bytes(&[97; 32]);
    let ns = scope(&f.pin);
    let pin = |purpose| AccountMachineIssuerPinV2 {
        namespace: "account-fixture".into(),
        service_id: "account-service".into(),
        environment: "test".into(),
        key_id: "account-issuer".into(),
        public_key: key
            .verifying_key()
            .to_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        issuer_epoch: DecimalU64(1),
        config_generation: DecimalU64(1),
        not_before: DecimalU64(0),
        not_after: DecimalU64(100_000),
        native_audience_origin: ns.http_origin.into(),
        allowed_purpose: purpose,
    };
    let a = pin(AccountMachinePurposeV2::TermNativeSessionsCurrentV3);
    let n = pin(AccountMachinePurposeV2::NativeTermCurrentAttemptV3);
    let issuer = AccountMachineIssuerV2 {
        namespace: a.namespace.clone(),
        service_id: a.service_id.clone(),
        environment: a.environment.clone(),
        key_id: a.key_id.clone(),
        issuer_epoch: a.issuer_epoch,
        config_generation: a.config_generation,
        native_audience_origin: a.native_audience_origin.clone(),
    };
    let floor = AccountMachineIssuerFloorV2 {
        issuer_epoch: DecimalU64(1),
        config_generation: DecimalU64(1),
    };
    let prepared = f
        .state
        .prepare_test_session_term_direct_native_v3(
            actor,
            native,
            TermDirectIssuerV3 {
                issuer,
                actor_pin: &a,
                native_pin: &n,
                floor: &floor,
                actor_object_id: &"ab".repeat(32),
            },
            &ns,
            UnixMillis::new(13_012),
            &mut StrictCrypto,
        )
        .unwrap();
    let encode = |bytes: [u8; 64]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let actor_signature = encode(
        key.sign(&prepared.actor_signing_bytes().unwrap())
            .to_bytes(),
    );
    let native_signature = encode(
        key.sign(&prepared.native_signing_bytes(&actor_signature).unwrap())
            .to_bytes(),
    );
    let dispatch = f
        .state
        .record_test_session_term_direct_native_v3(
            prepared,
            actor_signature,
            native_signature,
            UnixMillis::new(13_013),
        )
        .unwrap();
    // Root service responses below are explicit signed component fixtures,
    // not an actual Native DB/Current observation or CF payment qualification.
    let frame = dispatch.frame().clone();
    let state_json = serde_json::to_value(&f.state).unwrap();
    let machine = &state_json["test_sessions_v2"][&f.session_id]["native_machine_registration"];
    let pending: SignedAccountConsumedPendingV2 =
        serde_json::from_value(machine["signed"].clone()).unwrap();
    let active: SignedAccountActiveThreeRootsV2 =
        serde_json::from_value(machine["active"]["signed"].clone()).unwrap();
    let registration: SignedNativeRegistrationAckV2 =
        serde_json::from_value(machine["registered"]["ack"].clone()).unwrap();
    let activation: SignedNativeActivationAckV2 =
        serde_json::from_value(machine["active"]["ready"]["ack"].clone()).unwrap();
    let mut lf = pending.certificate.native_lifetime.clone();
    lf.lifetime.status.nonce = frame.body.lifetime_nonce.clone();
    lf.lifetime.status.observed_at = DecimalU64(13_014);
    lf.lifetime.status.expires_at = DecimalU64(frame.actor.binding.expires_at.0);
    lf.signature = URL_SAFE_NO_PAD.encode(
        super::super::key()
            .sign(&lf.lifetime.signing_bytes().unwrap())
            .to_bytes(),
    );
    let original_admission = f.state.test_session_v2_original(&f.session_id).unwrap();
    assert_eq!(
        original_admission.native_verify_request().unwrap().approval,
        pending.certificate.input.approval
    );
    let mut origin = verification(original_admission, &f.context).verification;
    origin.key_id = ns.native_root_key_id.into();
    origin.purpose = oasis7_custody_core::approval::VerificationPurposeV1::InspectOrigin;
    origin.verify_nonce = frame.body.origin_nonce.clone();
    origin.observed_at = DecimalU64(13_014);
    let origin = sign_verification(origin);
    let fact = NativeTermDirectFactV3 {
        api_version: 3,
        frame_digest: frame.digest().unwrap(),
        local: NativeTermCurrentFactV2 {
            api_version: 2,
            native_key_id: ns.native_root_key_id.into(),
            identity_namespace: registration.ack.identity_namespace.clone(),
            identity_epoch: registration.ack.identity_epoch,
            custody_account_id: pending
                .certificate
                .input
                .request
                .context
                .custody_account_id
                .clone(),
            session_id: frame.body.session_id.clone(),
            approved_input_digest: pending.certificate.approved_input_digest.clone(),
            active_certificate_digest: active.digest().unwrap(),
            opaque_registration_ref: registration.ack.opaque_registration_ref.clone(),
            owned_invocation_id: frame.body.owned_invocation_id.clone(),
            stop_operation_id: frame.body.stop_operation_id.clone(),
            close_operation_id: frame.body.close_operation_id.clone(),
            source_operation_id: frame.body.source_operation_id.clone(),
            native_transport_attempt_id: frame.actor.binding.native_attempt_id.clone(),
            native_request_digest: frame.digest().unwrap(),
            nonce: frame.body.nonce.clone(),
            origin_nonce: frame.body.origin_nonce.clone(),
            lifetime_nonce: frame.body.lifetime_nonce.clone(),
            storage_head: activation.ack.storage_head.clone(),
            registry_cursor: registration.ack.registry_cursor,
            lifetime: lf,
            origin,
            observation_started_at: frame.actor.binding.observation_started_at,
            issued_at: DecimalU64(13_014),
            expires_at: frame.actor.binding.expires_at,
        },
    };
    fact.validate_frame(&frame).unwrap();
    fact.validate_approval(&pending, &active, &registration)
        .unwrap();
    let native = SignedNativeTermDirectFactV3 {
        signature_hex: encode(
            super::super::key()
                .sign(&fact.signing_bytes().unwrap())
                .to_bytes(),
        ),
        fact,
    };
    let reply = TermDirectPrivateCurrentReplyV3 {
        api_version: 3,
        frame_digest: frame.digest().unwrap(),
        actor_object_id: frame.actor.binding.actor_object_id.clone(),
        session_ref: frame.body.metadata_session.session_ref.clone(),
        private_sid_generation: frame.body.metadata_session.private_sid_generation,
        lifetime_digest: native.fact.local.lifetime.lifetime.digest().unwrap(),
        actor_capacity: DecimalU64(2),
        actor_claim_count: DecimalU64(1),
        native,
    };
    reply
        .verify_for(
            &frame,
            TermDirectFactContextV3 {
                pending: &pending,
                active: &active,
                registration: &registration,
                native_key_id: ns.native_root_key_id,
                native_public_key: ns.native_root_public_key,
                now: 13_015,
            },
        )
        .unwrap();
    f.state
        .record_test_session_term_direct_result_v3(dispatch, reply, UnixMillis::new(13_015))
        .unwrap();
    let result_state = f.state.clone();
    let raw = serde_json::to_value(&f.state).unwrap();
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(raw.clone()).unwrap(),
        f.state
    );
    for field in [
        "missing",
        "null",
        "root_signature",
        "private_generation",
        "frame_digest",
        "lf_digest",
        "round",
    ] {
        let mut changed = raw.clone();
        let p = &mut changed["test_sessions_v2"][&f.session_id]["termination_state"]["term_five_funding"]
            [0]["native_paid"];
        match field {
            "missing" => {
                p.as_object_mut().unwrap().remove("result");
            }
            "null" => p["result"] = serde_json::Value::Null,
            "root_signature" => {
                p["result"]["reply"]["native"]["signature_hex"] = "00".repeat(64).into()
            }
            "private_generation" => p["result"]["reply"]["private_sid_generation"] = "999".into(),
            "frame_digest" => p["result"]["reply"]["frame_digest"] = "ff".repeat(32).into(),
            "lf_digest" => p["result"]["reply"]["lifetime_digest"] = "ee".repeat(32).into(),
            "round" => p["frame"]["actor"]["binding"]["round_id"] = "other-round".into(),
            _ => unreachable!(),
        }
        assert!(
            serde_json::from_value::<AccountProvisionApprovalState>(changed).is_err(),
            "{field}"
        );
        assert_eq!(f.state, result_state);
    }

    let original_state = f.state.clone();
    let mut raw = serde_json::to_value(&f.state).unwrap();
    fn paid(value: &mut serde_json::Value) -> Option<&mut serde_json::Value> {
        match value {
            serde_json::Value::Object(map) => {
                if map.get("native_paid").is_some_and(|v| !v.is_null()) {
                    return map.get_mut("native_paid");
                }
                for v in map.values_mut() {
                    if let Some(p) = paid(v) {
                        return Some(p);
                    }
                }
                None
            }
            serde_json::Value::Array(items) => {
                for v in items {
                    if let Some(p) = paid(v) {
                        return Some(p);
                    }
                }
                None
            }
            _ => None,
        }
    }
    let actual = paid(&mut raw)
        .expect("actual newly committed paid original")
        .clone();
    let original: NativeTermDirectFrameV3 =
        serde_json::from_value(actual["frame"].clone()).unwrap();
    original
        .verify_issuer(&a, &n, &floor, &"ab".repeat(32), 13_013)
        .unwrap();
    assert!(
        original
            .verify_issuer(&a, &n, &floor, &"cd".repeat(32), 13_013)
            .is_err()
    );
    original
        .verify_at_native_recipient(&a, &n, &floor, 13_013)
        .unwrap();
    for actor_signature in [true, false] {
        let mut bad = original.clone();
        if actor_signature {
            bad.actor.signature_hex = "00".repeat(64);
        } else {
            bad.native.signature_hex = "00".repeat(64);
        }
        assert!(
            bad.verify_at_native_recipient(&a, &n, &floor, 13_013)
                .is_err()
        );
        assert!(
            bad.verify_issuer(&a, &n, &floor, &"ab".repeat(32), 13_013)
                .is_err()
        );
    }

    for field in ["signature", "metadata", "correlation"] {
        let mut changed = raw.clone();
        let record = paid(&mut changed).unwrap();
        match field {
            "signature" => record["frame"]["native"]["signature_hex"] = "00".repeat(64).into(),
            "metadata" => {
                record["frame"]["body"]["metadata_session"]["private_sid_generation"] = "999".into()
            }
            "correlation" => {
                record["frame"]["actor"]["binding"]["round_id"] = "foreign-round".into()
            }
            _ => unreachable!(),
        }
        assert!(
            serde_json::from_value::<AccountProvisionApprovalState>(changed).is_err(),
            "{field}"
        );
        assert_eq!(f.state, original_state);
    }
    // Same paid IDs and valid fixture signatures do not authorize another body.
    let mut foreign = original.clone();
    foreign.body.owned_invocation_id = "foreign-owned-invocation".into();
    foreign.actor.binding.body_digest = foreign.body.digest().unwrap();
    foreign.actor.signature_hex = encode(
        key.sign(&foreign.actor.binding.actor_signing_bytes().unwrap())
            .to_bytes(),
    );
    foreign.native.claim.binding = foreign.actor.binding.clone();
    foreign.native.claim.actor_proof_digest = foreign.actor.digest().unwrap();
    foreign.native.signature_hex = encode(
        key.sign(&foreign.native.claim.signing_bytes().unwrap())
            .to_bytes(),
    );
    foreign
        .verify_issuer(&a, &n, &floor, &"ab".repeat(32), 13_013)
        .unwrap();
    assert_eq!(
        foreign.actor.binding.actor_attempt_id,
        original.actor.binding.actor_attempt_id
    );
    assert_eq!(
        foreign.actor.binding.native_attempt_id,
        original.actor.binding.native_attempt_id
    );
    let mut changed = raw;
    paid(&mut changed).unwrap()["frame"] = serde_json::to_value(foreign).unwrap();
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(changed).is_err());
    assert_eq!(f.state, original_state);
    assert_eq!(spent(&f), 5);
    let raw = serde_json::to_value(&f.state).unwrap();
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(raw).unwrap(),
        f.state
    );
}
