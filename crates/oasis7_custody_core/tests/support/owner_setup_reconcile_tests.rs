use super::*;
fn reconcile_plan(s: &AccountProvisionApprovalState) -> OwnerSetupReconcilePlanV1 {
    reconcile_plan_for(s, OwnerSetupStageV1::CreateTask)
}
fn reconcile_plan_for(
    s: &AccountProvisionApprovalState,
    stage: OwnerSetupStageV1,
) -> OwnerSetupReconcilePlanV1 {
    let p = s.owner_setup_plan("setup-one").unwrap();
    let original = s
        .owner_setup_outbox("setup-one", stage)
        .unwrap()
        .original
        .as_ref()
        .unwrap();
    let correlation_digest = match original {
        OwnerSetupOriginalV1::Canonical { submit, .. } => {
            bytes_digest(&oasis7_wasm_abi::encode_canonical_cbor(&submit.correlation).unwrap())
        }
        _ => bytes_digest(&oasis7_wasm_abi::encode_canonical_cbor(original).unwrap()),
    };
    OwnerSetupReconcilePlanV1 {
        api_version: 1,
        reconcile_id: "reconcile-one".into(),
        setup_id: p.setup_id.clone(),
        provision_id: p.provision_id.clone(),
        stage,
        setup_plan_digest: p.digest().unwrap(),
        original_digest: bytes_digest(&oasis7_wasm_abi::encode_canonical_cbor(original).unwrap()),
        correlation_digest,
        world_identity: p.world_identity.clone(),
        owner_key_ref: p.owner_key_ref.clone(),
        executor_key_ref: p.executor_key_ref.clone(),
        original_deadline: p.deadline,
        deadline: p.deadline,
        attempt_limit: 1,
        transport_limit: DecimalU64(112),
    }
}
fn approved() -> (
    AccountProvisionApprovalState,
    ApprovalAdmissionContext,
    NativeBindingPinV1,
    OwnerSetupCandidateV1,
) {
    approved_with_limit(1)
}
fn approved_with_limit(
    attempt_limit: u8,
) -> (
    AccountProvisionApprovalState,
    ApprovalAdmissionContext,
    NativeBindingPinV1,
    OwnerSetupCandidateV1,
) {
    approve_reconcile(
        archive_tests::dispatched(),
        OwnerSetupStageV1::CreateTask,
        attempt_limit,
    )
}
fn approve_reconcile(
    input: (
        AccountProvisionApprovalState,
        ApprovalAdmissionContext,
        NativeBindingPinV1,
        OwnerSetupCandidateV1,
    ),
    stage: OwnerSetupStageV1,
    attempt_limit: u8,
) -> (
    AccountProvisionApprovalState,
    ApprovalAdmissionContext,
    NativeBindingPinV1,
    OwnerSetupCandidateV1,
) {
    let (mut s, mut c, pin, candidate) = input;

    let mut plan = reconcile_plan_for(&s, stage);
    plan.attempt_limit = attempt_limit;
    plan.transport_limit = DecimalU64(8 + 104 * u64::from(attempt_limit));
    c.handoff_until = plan.deadline;
    let mut input = s.owner_setup_original("setup-one").unwrap().input().clone();
    input.operation_id = plan.reconcile_id.clone();
    input.request.approval_request_id = "reconcile-request".into();
    input.request.intent = plan.intent().unwrap();
    input.approval.proof.approval_request_id = input.request.approval_request_id.clone();
    input.approval.proof.nonce = "new-reconcile-mfa-nonce".into();
    input.approval.proof.approval_id = "new-reconcile-mfa".into();
    input.approval.proof.intent_digest = input.request.intent_digest().unwrap();
    input.approval.proof.issued_at = DecimalU64(7000);
    input.approval.proof.expires_at = DecimalU64(37000);
    input.approval = sign_proof(input.approval.proof);
    let policy = Policy::new(s.draft());
    let record = s
        .begin_owner_setup_reconcile(
            OwnerSetupReconcileAdmissionV1 {
                input,
                plan,
                verify_nonce: "c1".repeat(32),
            },
            &policy.current(&c),
            7000,
            &mut StrictCrypto,
        )
        .unwrap();
    let mut fact = verification(&record, &c).verification;
    fact.observed_at = DecimalU64(7001);
    s.commit_owner_setup_reconcile(
        CommitOwnerSetupV1 {
            id: "reconcile-one".into(),
            signed: sign_verification(fact),
            source_nonce: "c2".repeat(32),
        },
        &scope(&pin),
        &policy.current(&c),
        7002,
        &mut StrictCrypto,
    )
    .unwrap();
    s.claim_owner_setup_reconcile_source("reconcile-one", None, &policy.current(&c), 7003)
        .unwrap();
    let mut fact = verification(
        s.owner_setup_reconcile_original("reconcile-one").unwrap(),
        &c,
    )
    .verification;
    fact.purpose = VerificationPurposeV1::InspectOrigin;
    fact.verify_nonce = "c2".repeat(32);
    fact.observed_at = DecimalU64(7004);
    s.complete_owner_setup_reconcile_source(
        "reconcile-one",
        None,
        sign_verification(fact),
        &policy.current(&c),
        7005,
        &mut StrictCrypto,
    )
    .unwrap();
    (s, c, pin, candidate)
}
fn published() -> (
    AccountProvisionApprovalState,
    ApprovalAdmissionContext,
    NativeBindingPinV1,
    OwnerSetupCandidateV1,
    OwnerSetupReconcilePermitV1,
) {
    publish_reconcile(approved_with_limit(2))
}
fn publish_reconcile(
    input: (
        AccountProvisionApprovalState,
        ApprovalAdmissionContext,
        NativeBindingPinV1,
        OwnerSetupCandidateV1,
    ),
) -> (
    AccountProvisionApprovalState,
    ApprovalAdmissionContext,
    NativeBindingPinV1,
    OwnerSetupCandidateV1,
    OwnerSetupReconcilePermitV1,
) {
    let (mut s, c, pin, candidate) = input;
    let stage = s.owner_setup_reconciles()["reconcile-one"].plan.stage;

    let policy = Policy::new(s.draft());
    let current = policy.current(&c);
    let ns = scope(&pin);
    let trusted = OwnerSetupCurrent {
        provision: &current,
        native_scope: &ns,
    };
    s.reserve_owner_setup_reconcile_attempt(
        "reconcile-one",
        "attempt-one",
        &"c3".repeat(32),
        &"c4".repeat(32),
        7010,
    )
    .unwrap();
    if s.owner_setup_reconciles()["reconcile-one"]
        .plan
        .attempt_limit
        > 1
    {
        s.reserve_owner_setup_reconcile_attempt(
            "reconcile-one",
            "other-attempt",
            &"e1".repeat(32),
            &"e2".repeat(32),
            7010,
        )
        .unwrap();
    }
    s.claim_owner_setup_reconcile_source("reconcile-one", Some("attempt-one"), &current, 7011)
        .unwrap();
    let mut fact = verification(
        s.owner_setup_reconcile_original("reconcile-one").unwrap(),
        &c,
    )
    .verification;
    fact.purpose = VerificationPurposeV1::InspectOrigin;
    fact.verify_nonce = "c4".repeat(32);
    fact.observed_at = DecimalU64(7012);
    s.complete_owner_setup_reconcile_source(
        "reconcile-one",
        Some("attempt-one"),
        sign_verification(fact),
        &current,
        7013,
        &mut StrictCrypto,
    )
    .unwrap();
    for n in 1..=2 {
        let read = s
            .reserve_owner_setup_reconcile_read(
                "reconcile-one",
                "attempt-one",
                OwnerSetupReadPurposeV1::Session,
                Role::Identity,
                7014,
            )
            .unwrap();
        s.authorize_owner_setup_reconcile_read(
            "reconcile-one",
            OwnerSetupReconcileRead {
                attempt_id: "attempt-one",
                read,
                role: Role::Identity,
            },
            &trusted,
            7014,
            &mut StrictCrypto,
        )
        .unwrap();
        if n == 2 {
            s.claim_owner_setup_reconcile_access("reconcile-one", "attempt-one", 7014)
                .unwrap();
        }
    }
    let nonces = s
        .owner_setup_reconcile_nonces("reconcile-one", "attempt-one")
        .unwrap();
    let mut access = fresh_access(&s, stage);
    access.policy.request = ws::sign_read_request(
        PARTICIPANT_POLICY_PATH,
        ParticipantPolicyRequestV1 {
            observation_nonce: nonces.policy,
            ..access.policy.request.request
        },
        &"40".repeat(32),
    )
    .unwrap();
    access.policy.response = ws::sign_service_response(
        PARTICIPANT_POLICY_PATH,
        ws::request_digest(PARTICIPANT_POLICY_PATH, &access.policy.request).unwrap(),
        access.policy.response.payload,
        &"3f".repeat(32),
    )
    .unwrap();
    access.policy_session = fixture::root_session(&s, nonces.policy_session, 7015);
    access.authorize_session = fixture::root_session(&s, nonces.authorize_session, 7015);
    let read = s
        .reserve_owner_setup_reconcile_read(
            "reconcile-one",
            "attempt-one",
            OwnerSetupReadPurposeV1::Sign,
            Role::Owner,
            7016,
        )
        .unwrap();
    s.record_owner_setup_reconcile_access(
        "reconcile-one",
        "attempt-one",
        access,
        &trusted,
        7016,
        &mut StrictCrypto,
    )
    .unwrap();
    s.authorize_owner_setup_reconcile_read(
        "reconcile-one",
        OwnerSetupReconcileRead {
            attempt_id: "attempt-one",
            read,
            role: Role::Owner,
        },
        &trusted,
        7016,
        &mut StrictCrypto,
    )
    .unwrap();
    let read = s
        .reserve_owner_setup_reconcile_read(
            "reconcile-one",
            "attempt-one",
            OwnerSetupReadPurposeV1::Sign,
            Role::Owner,
            7016,
        )
        .unwrap();
    let view = s
        .authorize_owner_setup_reconcile_read(
            "reconcile-one",
            OwnerSetupReconcileRead {
                attempt_id: "attempt-one",
                read,
                role: Role::Owner,
            },
            &trusted,
            7016,
            &mut StrictCrypto,
        )
        .unwrap();
    let mut observation = view.participant_access;
    observation.read =
        ws::sign_read_request(ws::VIEW_JSON_PATH, view.observation, &"3e".repeat(32)).unwrap();
    observation.response_profile = Some(ParticipantOwnerViewProfileV1::OwnerSetupV1);
    s.reserve_owner_setup_reconcile_read(
        "reconcile-one",
        "attempt-one",
        OwnerSetupReadPurposeV1::Sign,
        Role::Owner,
        7016,
    )
    .unwrap();
    let permit = s
        .publish_owner_setup_reconcile_reads(
            "reconcile-one",
            "attempt-one",
            OwnerSetupReconcileReadCandidateV1 {
                observation,
                goal: view.final_goal.map(|goal| {
                    ws::sign_read_request(
                        ws::private_goal_read::GOAL_CURRENT_PATH,
                        goal,
                        &"3e".repeat(32),
                    )
                    .unwrap()
                }),
            },
            &trusted,
            7016,
            &mut StrictCrypto,
        )
        .unwrap();
    (s, c, pin, candidate, permit)
}
#[test]
fn reconcile_budget_and_proof_are_separate_and_no_old_grant_renewal() {
    let (mut s, _, _, _) = approved();
    let original = s
        .owner_setup_outbox("setup-one", OwnerSetupStageV1::CreateTask)
        .unwrap()
        .clone();
    let old_remaining = s
        .owner_setup_progress("setup-one")
        .unwrap()
        .transport_remaining;
    s.reserve_owner_setup_reconcile_attempt(
        "reconcile-one",
        "a",
        &"d1".repeat(32),
        &"d2".repeat(32),
        7010,
    )
    .unwrap();
    assert_eq!(
        s.owner_setup_reconcile_progress("reconcile-one")
            .unwrap()
            .transport_remaining,
        DecimalU64(0)
    );
    assert_eq!(
        s.owner_setup_progress("setup-one")
            .unwrap()
            .transport_remaining,
        old_remaining
    );
    assert_eq!(
        s.owner_setup_outbox("setup-one", OwnerSetupStageV1::CreateTask)
            .unwrap(),
        &original
    );
    let before = s.clone();
    assert!(
        s.reserve_owner_setup_reconcile_attempt(
            "reconcile-one",
            "b",
            &"d3".repeat(32),
            &"d4".repeat(32),
            7011
        )
        .is_err()
    );
    assert_eq!(s, before);
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(serde_json::to_value(&s).unwrap())
            .unwrap(),
        s
    );
    let mut raw = serde_json::to_value(&s).unwrap();
    raw["owner_setup_reconciles"]["reconcile-one"]["attempts"]["a"]["original_lookup_bytes"] =
        "{}".into();
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(raw).is_err());
}
#[path = "owner_setup_reconcile_completion_tests.rs"]
mod completion_tests;
#[test]
fn reconcile_limits_expiry_fence_and_read_role_are_not_new_write_grants() {
    let (mut s, c, pin, _) = approved();
    let mut p = reconcile_plan(&s);
    for count in 1..=3 {
        p.attempt_limit = count;
        p.transport_limit = DecimalU64(8 + 104 * u64::from(count));
        p.validate().unwrap();
    }
    for count in [0, 4] {
        p.attempt_limit = count;
        assert!(p.validate().is_err());
    }
    p = reconcile_plan(&s);
    p.deadline = DecimalU64(p.original_deadline.0 + 1);
    assert!(p.validate().is_err());
    let before = s.clone();
    assert!(
        s.reserve_owner_setup_reconcile_attempt(
            "reconcile-one",
            "expired",
            &"d6".repeat(32),
            &"d7".repeat(32),
            80_000
        )
        .is_err()
    );
    assert_eq!(s, before);
    s.reserve_owner_setup_reconcile_attempt(
        "reconcile-one",
        "first",
        &"d6".repeat(32),
        &"d7".repeat(32),
        7010,
    )
    .unwrap();
    assert!(
        s.reserve_owner_setup_reconcile_read(
            "reconcile-one",
            "first",
            OwnerSetupReadPurposeV1::Sign,
            Role::Owner,
            7011
        )
        .is_err()
    );
    let policy = Policy::new(s.draft());
    let current = policy.current(&c);
    let ns = scope(&pin);
    let trusted = OwnerSetupCurrent {
        provision: &current,
        native_scope: &ns,
    };
    assert!(
        s.authorize_owner_setup_reconcile_read(
            "reconcile-one",
            OwnerSetupReconcileRead {
                attempt_id: "first",
                read: OwnerSetupReadTicket {
                    stage: OwnerSetupStageV1::CreateTask,
                    ticket: 1,
                    purpose: OwnerSetupReadPurposeV1::Session
                },
                role: Role::Identity
            },
            &trusted,
            7011,
            &mut StrictCrypto
        )
        .is_err()
    );
    s.freeze(2).unwrap();
    assert!(
        s.claim_owner_setup_reconcile_source("reconcile-one", Some("first"), &current, 7011)
            .is_err()
    );
    assert!(
        s.reserve_owner_setup_reconcile_read(
            "reconcile-one",
            "first",
            OwnerSetupReadPurposeV1::Session,
            Role::Identity,
            7011
        )
        .is_err()
    );
    assert_eq!(
        s.owner_setup_reconcile_progress("reconcile-one")
            .unwrap()
            .transport_remaining,
        DecimalU64(0)
    );
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(serde_json::to_value(&s).unwrap())
            .unwrap(),
        s
    );
}

#[path = "owner_setup_final_sequence.rs"]
mod final_sequence;
