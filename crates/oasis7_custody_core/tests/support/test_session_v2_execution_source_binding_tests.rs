//! The Native strong Source original, not a mutable bookkeeping digest, binds payment.
use super::*;
#[test]
fn session_v2_execution_source_preserves_four_old_funding_wires_and_closes_new_shape() {
    for (funding, wire) in [
        (SessionSourceFundingV1::Control, "control"),
        (SessionSourceFundingV1::Execution, "execution"),
        (SessionSourceFundingV1::ReconcileRead, "reconcile_read"),
        (SessionSourceFundingV1::TerminationPlan, "termination_plan"),
    ] {
        assert_eq!(
            serde_json::to_value(funding).unwrap(),
            serde_json::json!(wire)
        );
        let bytes = oasis7_wasm_abi::encode_canonical_cbor(&funding).unwrap();
        assert_eq!(
            serde_cbor::from_slice::<SessionSourceFundingV1>(&bytes).unwrap(),
            funding
        );
    }
    let funding = SessionSourceFundingV1::ExecutionFactoryV2 {
        nonce_salt: [93; 32],
    };
    let mut wire = serde_json::to_value(funding).unwrap();
    assert_eq!(
        serde_json::from_value::<SessionSourceFundingV1>(wire.clone()).unwrap(),
        funding
    );
    wire["execution_factory_v2"]["verified"] = serde_json::json!(true);
    assert!(serde_json::from_value::<SessionSourceFundingV1>(wire).is_err());
    assert!(
        serde_json::from_value::<SessionSourceFundingV1>(
            serde_json::json!({"execution_factory_v2": {}})
        )
        .is_err()
    );
}
#[test]
fn session_v2_execution_source_actual_paid_factory_salt_is_immutable() {
    let mut f = claimed_execution();
    let (body, pop) = action_pop(&mut f, "source-salt-move", 13_000, false);
    charge(
        &mut f,
        &body,
        &pop,
        "source-salt-ingress",
        SessionPurposeV1::Execution,
        13_002,
    )
    .unwrap();
    let proof = || SessionProofRequest {
        ingress_attempt_id: "source-salt-ingress",
        original_bytes: &body,
        possession: &pop,
    };
    let source = f
        .state
        .reserve_test_session_v2_execution_source(
            &f.session_id,
            "source-salt-reservation",
            [93; 32],
            proof(),
            UnixMillis::new(14_000),
        )
        .unwrap();
    let before = f.state.clone();
    assert!(
        f.state
            .reserve_test_session_v2_execution_source(
                &f.session_id,
                "source-salt-reservation",
                [94; 32],
                proof(),
                UnixMillis::new(14_001)
            )
            .is_err()
    );
    assert_eq!(f.state, before);
    assert!(
        f.state
            .reserve_test_session_v2_source(
                &f.session_id,
                SessionSourceReservation {
                    operation_id: "generic-cannot-make-factory",
                    nonce: &source.source.verify_nonce,
                    funding: SessionSourceFundingV1::ExecutionFactoryV2 {
                        nonce_salt: [93; 32]
                    }
                },
                UnixMillis::new(14_001)
            )
            .is_err()
    );
    assert_eq!(f.state, before);
}
#[test]
fn session_v2_execution_source_ordinary_strong_source_cannot_upgrade_to_book() {
    let mut f = claimed_execution();
    let (body, pop) = action_pop(&mut f, "ordinary-source-move", 13_000, false);
    charge(
        &mut f,
        &body,
        &pop,
        "ordinary-source-ingress",
        SessionPurposeV1::Execution,
        13_002,
    )
    .unwrap();
    let source_id = "ordinary-paid-execution-source";
    let nonce =
        oasis7_custody_core::approval::bytes_digest(b"ordinary source independent public nonce");
    f.state
        .reserve_test_session_v2_source(
            &f.session_id,
            SessionSourceReservation {
                operation_id: source_id,
                nonce: &nonce,
                funding: SessionSourceFundingV1::Execution,
            },
            UnixMillis::new(14_000),
        )
        .unwrap();
    complete_execution_source(&mut f, source_id, nonce, 14_000);
    let cold: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&f.state).unwrap()).unwrap();
    assert_eq!(cold, f.state);
    let policy = Policy::new(f.state.draft());
    let current = policy.current(&f.context);
    let before = f.state.clone();
    assert!(
        f.state
            .claim_test_session_v2_execution_lifetime(
                &f.session_id,
                source_id,
                SessionProofRequest {
                    ingress_attempt_id: "ordinary-source-ingress",
                    original_bytes: &body,
                    possession: &pop
                },
                &current,
                UnixMillis::new(14_001),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, before);
}
