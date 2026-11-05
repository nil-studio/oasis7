//! Actual approval, credential, signed Tool proof and admitted Move fixture.
use super::*;
fn registered() -> SessionFixture {
    let (mut f, _, life) = action_fixture();
    let policy = Policy::new(f.state.draft());
    let context = f.context.clone();
    let pin = f.pin.clone();
    let provision = policy.current(&context);
    let ns = scope(&pin);
    let live = TestSessionLiveCurrent {
        provision: &provision,
        native_scope: &ns,
        lifetime: &life,
        private_sid_generation: DecimalU64(1),
        source_operation_id: "action-live-source",
        source_ticket: 2,
        observed_at: UnixMillis::new(12_002),
    };
    let original = f
        .state
        .register_attested_move_context_v1(
            ID,
            "actual-collision-action",
            "attested-prepare-operation",
            &"e4".repeat(32),
            &live,
            UnixMillis::new(12_007),
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(original.client_operation_id, "attested-prepare-operation");
    assert_eq!(
        original.fence,
        f.state.test_sessions()[ID].termination.initial_fence
    );
    f
}
#[test]
fn attested_v1_actual_approved_move_registration_restores_without_usable_legacy_context() {
    let f = registered();
    let wire = serde_json::to_value(&f.state).unwrap();
    let restored: AccountProvisionApprovalState = serde_json::from_value(wire).unwrap();
    assert_eq!(restored, f.state);
    let record = f.state.test_sessions()[ID].context.as_ref().unwrap();
    let SessionContextRecordV1::Attested(ledger) = record else {
        panic!("new profile must not become legacy");
    };
    assert!(ledger.public_facts.is_empty());
    assert_eq!(ledger.originals.len(), 1);
    assert!(ledger.originals.values().all(|v| v.send_claim.is_none()
        && v.archived_response_cbor_hex.is_none()
        && v.admitted_at.is_none()));
}
#[test]
fn attested_v1_registered_move_rejects_missing_tag_foreign_action_and_unearned_stage_restore() {
    let f = registered();
    let wire = serde_json::to_value(&f.state).unwrap();
    for field in ["profile", "paid_rows", "originals", "action_request_digest"] {
        let mut bad = wire.clone();
        bad["test_sessions"][ID]["context"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            serde_json::from_value::<AccountProvisionApprovalState>(bad).is_err(),
            "required closed field {field}"
        );
    }
    for (field, value) in [
        ("action_request_id", serde_json::json!("unadmitted-action")),
        ("approved_position_cm", serde_json::json!([44, 55, 66])),
        (
            "configured_world_public_key",
            serde_json::json!("00".repeat(32)),
        ),
    ] {
        let mut bad = wire.clone();
        bad["test_sessions"][ID]["context"][field] = value;
        assert!(
            serde_json::from_value::<AccountProvisionApprovalState>(bad).is_err(),
            "actual authority binding {field}"
        );
    }
    let mut bad = wire.clone();
    bad["test_sessions"][ID]["context"]["originals"]["prepare"]["send_claim"] =
        serde_json::json!("unpaid-send");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(bad).is_err());
}
