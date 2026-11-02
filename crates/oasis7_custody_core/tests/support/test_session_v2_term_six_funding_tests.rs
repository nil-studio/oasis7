//! Payer/once and fixture signatures only; no Current, actual Actor reply or physical send.
use super::*;
use oasis7_custody_core::native_cleanup_v2::{
    AccountMachineIssuerFloorV2, AccountMachineIssuerPinV2, AccountMachineIssuerV2,
    AccountMachinePurposeV2,
};
fn term_issuer_pin(origin: &str) -> AccountMachineIssuerPinV2 {
    AccountMachineIssuerPinV2 {
        namespace: "account-fixture".into(),
        service_id: "account-service".into(),
        environment: "test".into(),
        key_id: "account-issuer".into(),
        public_key: ed25519_dalek::SigningKey::from_bytes(&[97; 32])
            .verifying_key()
            .to_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        issuer_epoch: DecimalU64(1),
        config_generation: DecimalU64(1),
        not_before: DecimalU64(0),
        not_after: DecimalU64(100_000),
        native_audience_origin: origin.into(),
        allowed_purpose: AccountMachinePurposeV2::NativeTermCurrentAttempt,
    }
}

use oasis7_custody_core::provisioning::{
    TermObservationPortV2 as Port, TermObservationRoundV2 as Round,
};
#[test]
fn term_six_real_finish_pays_two_fixed_rounds_and_claims_each_port_once() {
    let mut f = super::super::machine_tests::ready_world3(9000);
    let mut q = request(&f, "term-owned-initial-claim", None);
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
        request_id: "term-owned-finish".into(),
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
    let ingress = "term-owned-finish-http";
    control_tests::control_paid(&mut f, &pop, ingress, 13_003);
    let proof = || SessionProofRequest {
        ingress_attempt_id: ingress,
        original_bytes: &bytes,
        possession: &pop,
    };
    let (ack, owned) = f
        .state
        .control_test_session_v2_and_begin_owned_term(
            proof(),
            UnixMillis::new(13_005),
            &mut NoNativeChecks,
        )
        .unwrap();
    assert_eq!(ack.kind, SessionControlKindV1::Finish);
    let owned = owned.expect("real first closing Control in Ready session owns local continuation");

    f.state
        .claim_test_session_term_owner_slot_read_v2(
            &owned,
            "term-six-qualification",
            UnixMillis::new(13_006),
        )
        .unwrap();
    let spent = |state: &AccountProvisionApprovalState| {
        serde_json::to_value(state).unwrap()["test_sessions_v2"][&f.session_id]["roots"]["termination_plan"]["root"]["budgets"]["transport"]["spent"].as_str().unwrap().parse::<u64>().unwrap()
    };
    assert_eq!(spent(&f.state), 1);
    let first = f
        .state
        .reserve_test_session_term_six_v2(&owned, Round::First, UnixMillis::new(13_007))
        .unwrap();
    assert_eq!(spent(&f.state), 7);
    let exact = f.state.clone();
    assert!(
        f.state
            .reserve_test_session_term_six_v2(&owned, Round::First, UnixMillis::new(13_008))
            .is_err()
    );
    assert!(
        f.state
            .reserve_test_session_term_six_v2(&owned, Round::Fresh, UnixMillis::new(13_008))
            .is_err()
    );
    assert_eq!(f.state, exact);
    let ports = [
        Port::Directory,
        Port::RfService,
        Port::RfActor,
        Port::IdentityService,
        Port::NativeSessionsActor,
        Port::Native,
    ];
    for (n, port) in ports.into_iter().enumerate() {
        if port == Port::Native {
            let mut branch = f.state.clone();
            let claimed = branch
                .claim_test_session_term_six_port_v2(&first, port, UnixMillis::new(13_015))
                .unwrap();
            let native = scope(&f.pin);
            let pin = term_issuer_pin(native.http_origin);
            let issuer = AccountMachineIssuerV2 {
                namespace: pin.namespace.clone(),
                service_id: pin.service_id.clone(),
                environment: pin.environment.clone(),
                key_id: pin.key_id.clone(),
                issuer_epoch: pin.issuer_epoch,
                config_generation: pin.config_generation,
                native_audience_origin: pin.native_audience_origin.clone(),
            };
            let floor = AccountMachineIssuerFloorV2 {
                issuer_epoch: DecimalU64(1),
                config_generation: DecimalU64(1),
            };
            let wrong_key = "00".repeat(32);
            let wrong = NativeStageScope {
                http_origin: native.http_origin,
                native_root_key_id: native.native_root_key_id,
                native_root_public_key: &wrong_key,
                owner_binding_pin: native.owner_binding_pin,
            };
            let exact = branch.clone();
            assert!(
                branch
                    .prepare_test_session_term_native_paid_v2(
                        claimed,
                        NativeMachinePendingIssuerV2 {
                            issuer,
                            pin: &pin,
                            floor: &floor
                        },
                        &wrong,
                        UnixMillis::new(13_016),
                        &mut StrictCrypto
                    )
                    .is_err()
            );
            assert_eq!(branch, exact);
        }
        let token = f
            .state
            .claim_test_session_term_six_port_v2(&first, port, UnixMillis::new(13_010 + n as u64))
            .unwrap();
        assert_eq!(token.kind(), port);
        assert_eq!(token.paid_index(), 1 + n as u32);
        assert_eq!(token.until(), first.until());
        if port == Port::Native {
            assert_eq!(
                token.native_request(&f.state).unwrap().session_id,
                f.session_id
            );
            let native = scope(&f.pin);
            let pin = term_issuer_pin(native.http_origin);
            let issuer = AccountMachineIssuerV2 {
                namespace: pin.namespace.clone(),
                service_id: pin.service_id.clone(),
                environment: pin.environment.clone(),
                key_id: pin.key_id.clone(),
                issuer_epoch: pin.issuer_epoch,
                config_generation: pin.config_generation,
                native_audience_origin: pin.native_audience_origin.clone(),
            };
            let floor = AccountMachineIssuerFloorV2 {
                issuer_epoch: DecimalU64(1),
                config_generation: DecimalU64(1),
            };
            let prepared = f
                .state
                .prepare_test_session_term_native_paid_v2(
                    token,
                    NativeMachinePendingIssuerV2 {
                        issuer,
                        pin: &pin,
                        floor: &floor,
                    },
                    &native,
                    UnixMillis::new(13_016),
                    &mut StrictCrypto,
                )
                .unwrap();
            let signature = ed25519_dalek::SigningKey::from_bytes(&[97; 32])
                .sign(&prepared.signing_bytes().unwrap())
                .to_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            let dispatch = f
                .state
                .record_test_session_term_native_paid_v2(
                    prepared,
                    signature,
                    UnixMillis::new(13_017),
                )
                .unwrap();
            assert_eq!(dispatch.envelope().body.session_id, f.session_id);
            assert_eq!(dispatch.envelope().paid.edge.native_paid_ordinal.0, 7);
            assert_eq!(spent(&f.state), 7);
            let raw = serde_json::to_value(&f.state).unwrap();
            assert!(
                raw["test_sessions_v2"][&f.session_id]["termination_state"]["term_six_funding"][0]
                    ["native_paid"]["envelope"]["paid"]["signature_hex"]
                    .as_str()
                    .is_some()
            );
            let mut bad = raw.clone();
            bad["test_sessions_v2"][&f.session_id]["termination_state"]["term_six_funding"][0]["native_paid"]
                ["envelope"]["paid"]["signature_hex"] = serde_json::Value::String("00".repeat(64));
            assert!(serde_json::from_value::<AccountProvisionApprovalState>(bad).is_err());
            let mut missing = raw;
            missing["test_sessions_v2"][&f.session_id]["termination_state"]["term_six_funding"][0]
                .as_object_mut()
                .unwrap()
                .remove("native_paid");
            assert!(serde_json::from_value::<AccountProvisionApprovalState>(missing).is_err());
        }
        let exact = f.state.clone();
        assert!(
            f.state
                .claim_test_session_term_six_port_v2(&first, port, UnixMillis::new(13_020))
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
    assert_eq!(spent(&f.state), 7);
    // The original qualification first token is not renewed or recreated here.
    // This is reservation/once metadata only, with no later permission implied.
    let fresh = f
        .state
        .reserve_test_session_term_six_v2(&owned, Round::Fresh, UnixMillis::new(18_008))
        .unwrap();
    assert_eq!(spent(&f.state), 13);
    assert!(fresh.started_at().get() > first.until().get());
    let exact = f.state.clone();
    assert!(
        f.state
            .claim_test_session_term_six_port_v2(&first, Port::Native, UnixMillis::new(18_009))
            .is_err()
    );
    assert!(
        f.state
            .reserve_test_session_term_six_v2(&owned, Round::Fresh, UnixMillis::new(18_009))
            .is_err()
    );
    assert_eq!(f.state, exact);
    let raw = serde_json::to_value(&f.state).unwrap();
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(raw.clone()).unwrap(),
        f.state
    );
    for path in ["input_digest", "invocation_id", "source_id", "nonce"] {
        let mut bad = raw.clone();
        bad["test_sessions_v2"][&f.session_id]["termination_state"]["term_six_funding"][0]["header"]
            [path] = serde_json::Value::String("wrong".into());
        assert!(
            serde_json::from_value::<AccountProvisionApprovalState>(bad).is_err(),
            "{path}"
        );
    }
}
