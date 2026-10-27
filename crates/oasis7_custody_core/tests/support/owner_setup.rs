use super::*;
fn plan(s: &AccountProvisionApprovalState) -> OwnerSetupPlanV1 {
    OwnerSetupPlanV1 {
        api_version: 1,
        setup_id: "setup-one".into(),
        provision_id: s.draft().entry.provision_id.clone(),
        participant_receipt_digest: s
            .participant_provisions()
            .values()
            .find_map(|v| v.result.as_ref())
            .map_or_else(
                || "ab".repeat(32),
                |v| bytes_digest(&oasis7_wasm_abi::encode_canonical_cbor(v).unwrap()),
            ),
        world_identity: s.draft().world_identity.clone(),
        owner_key_ref: s
            .provision()
            .role_key(oasis7_custody_core::http::Role::Owner)
            .unwrap()
            .key_id
            .clone(),
        executor_key_ref: s
            .provision()
            .role_key(oasis7_custody_core::http::Role::Executor)
            .unwrap()
            .key_id
            .clone(),
        bootstrap_config_version: s.draft().entry.bootstrap_config_version,
        goal_content: oasis7_agent_api::game::GoalContentV1 {
            goal_text: "private setup goal never logged".into(),
            soft_constraints: vec!["fixed private constraint".into()],
        },
        create_operation_id: "setup-create".into(),
        prepare_operation_id: "setup-prepare".into(),
        prepare_nonce: "ac".repeat(32),
        publish_operation_id: "setup-publish".into(),
        delegation_operation_id: "setup-delegate".into(),
        bind_operation_id: "setup-bind".into(),
        executor_id: "executor-one".into(),
        executor_public_key: s
            .provision()
            .role_key(oasis7_custody_core::http::Role::Executor)
            .unwrap()
            .public_key
            .clone(),
        delegation_generation: DecimalU64(1),
        delegation_nonce: DecimalU64(1),
        resource_refs: vec![],
        deadline: DecimalU64(80_000),
        request_limit: DecimalU64(6),
        transport_limit: DecimalU64(584),
        write_limit: DecimalU64(4),
    }
}
#[test]
fn setup_plan_is_exact_closed_private_and_independently_bounded() {
    let (s, _, _) = participant_ready();
    let p = plan(&s);
    p.validate().unwrap();
    let digest = p.digest().unwrap();
    let mut changed = p.clone();
    changed.goal_content.goal_text.push('!');
    assert_ne!(digest, changed.digest().unwrap());
    let debug = format!("{p:?}");
    assert!(!debug.contains("private setup goal"));
    assert!(!debug.contains(&digest));
    let mut raw = serde_json::to_value(&p).unwrap();
    raw["raw_signature_domain"] = json!("arbitrary");
    assert!(serde_json::from_value::<OwnerSetupPlanV1>(raw).is_err());
    for amount in [0, 400, 583, 601] {
        let mut bad = p.clone();
        bad.transport_limit = DecimalU64(amount);
        assert!(bad.validate().is_err());
    }
    for amount in [584, 600] {
        let mut valid = p.clone();
        valid.transport_limit = DecimalU64(amount);
        valid.validate().unwrap();
    }
    let mut bad = p.clone();
    bad.bind_operation_id = bad.create_operation_id.clone();
    assert!(bad.validate().is_err());
    let mut bad = p.clone();
    bad.resource_refs = vec!["unverified-resource".into()];
    assert!(bad.validate().is_err());
    let mut bad = p;
    bad.goal_content.goal_text = "x".repeat(4097);
    assert!(bad.validate().is_err());
}
#[test]
fn setup_http_binds_signed_plan_and_stable_setup_operation() {
    let (s, _, _) = participant_ready();
    let p = plan(&s);
    let mut input = s.accepted_approval().unwrap().input().clone();
    input.operation_id = p.setup_id.clone();
    input.request.intent = p.intent().unwrap();
    let approved = OwnerSetupApproveRequestV1 {
        api_version: 1,
        input,
        plan: p,
    };
    approved.validate().unwrap();
    let mut wrong = approved.clone();
    wrong.input.operation_id = "new-http-operation".into();
    assert!(wrong.validate().is_err());
    let mut wrong = approved;
    wrong.plan.prepare_nonce = "ff".repeat(32);
    assert!(wrong.validate().is_err());
}
#[test]
fn setup_cannot_reconsume_provision_proof_or_spend_before_approval() {
    let (mut s, c, pin) = participant_ready();
    let p = plan(&s);
    let mut input = s.accepted_approval().unwrap().input().clone();
    input.operation_id = p.setup_id.clone();
    input.request.intent = p.intent().unwrap();
    input.approval.proof.intent_digest = input.request.intent_digest().unwrap();
    input.approval = sign_proof(input.approval.proof);
    let policy = Policy::new(s.draft());
    let before = s.clone();
    assert_eq!(
        s.begin_owner_setup_checks(
            &OwnerSetupAdmissionV1 {
                input,
                plan: p,
                verify_nonce: "ae".repeat(32)
            },
            &policy.current(&c),
            1500
        ),
        Err(ProvisionApprovalError::Admission(
            ApprovalAdmissionError::NonceReused
        ))
    );
    assert!(
        s.reserve_owner_setup_stage(
            "setup-one",
            OwnerSetupStageV1::CreateTask,
            &"af".repeat(32),
            1500
        )
        .is_err()
    );
    assert_eq!(s, before);
    assert!(s.owner_setup_progress("setup-one").is_err());
    let _ = pin;
}
#[test]
fn setup_legacy_snapshot_missing_lane_is_empty_and_cannot_infer_prepared() {
    let (s, _, _) = participant_ready();
    let mut raw = serde_json::to_value(&s).unwrap();
    raw.as_object_mut().unwrap().remove("owner_setups");
    let restored: AccountProvisionApprovalState = serde_json::from_value(raw.clone()).unwrap();
    assert!(restored.owner_setups().is_empty());
    assert_eq!(restored, s);
    raw["provision"]["stage"] = json!("prepared");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(raw).is_err());
}

#[test]
fn setup_ticket_matrix_separates_identity_session_from_owner_signing() {
    use oasis7_custody_core::http::Role;
    for ticket in 1..=5 {
        let purpose = if ticket <= 2 {
            OwnerSetupReadPurposeV1::Session
        } else {
            OwnerSetupReadPurposeV1::Sign
        };
        let read = OwnerSetupReadTicket {
            stage: OwnerSetupStageV1::CreateTask,
            ticket,
            purpose,
        };
        read.validate_role(if ticket <= 2 {
            Role::Identity
        } else {
            Role::Owner
        })
        .unwrap();
        assert!(
            read.validate_role(if ticket <= 2 {
                Role::Owner
            } else {
                Role::Identity
            })
            .is_err()
        );
        assert!(read.validate_role(Role::Executor).is_err());
        assert!(read.validate_role(Role::RecoveryFence).is_err());
    }
    for ticket in [0, 6, u8::MAX] {
        for purpose in [
            OwnerSetupReadPurposeV1::Session,
            OwnerSetupReadPurposeV1::Sign,
        ] {
            let read = OwnerSetupReadTicket {
                stage: OwnerSetupStageV1::CreateTask,
                ticket,
                purpose,
            };
            assert!(read.validate_role(Role::Owner).is_err());
            assert!(read.validate_role(Role::Identity).is_err());
        }
    }
    assert!(serde_json::from_value::<OwnerSetupReadPurposeV1>(json!("raw_sign")).is_err());
}

#[path = "owner_setup_admission_tests.rs"]
mod admission_tests;
#[path = "owner_setup_fixture.rs"]
mod fixture;

#[path = "owner_setup_basis_tests.rs"]
mod basis_tests;
