//! New V2 approval is consumed before the shared SDK factory produces any write.
use super::*;
fn consumed_selection(
    selection: impl FnOnce(&AccountProvisionApprovalState, &OwnerSetupPlanV1) -> serde_json::Value,
) -> (
    AccountProvisionApprovalState,
    ApprovalAdmissionContext,
    NativeBindingPinV1,
) {
    let (mut s, mut c, pin) = fixture::participant_bound();
    let legacy_plan = plan(&s);
    let selection = selection(&s, &legacy_plan);
    let mut raw = serde_json::to_value(legacy_plan).unwrap();
    raw.as_object_mut().unwrap().remove("resource_refs");
    raw["api_version"] = json!(2);
    raw["model_budget"] = serde_json::Value::Null;
    raw["selection"] = selection;
    let header: OwnerSetupPlanV2 = serde_json::from_value(raw).unwrap();
    header.validate().unwrap();
    c.handoff_until = header.deadline;
    let mut input = s.accepted_approval().unwrap().input().clone();
    input.operation_id = header.setup_id.clone();
    input.request.context = c.authority.clone();
    input.request.approval_request_id = "v2-phase-approval".into();
    input.request.intent = header.intent().unwrap();
    input.approval.proof.context = input.request.context.clone();
    input.approval.proof.intent_digest = input.request.intent_digest().unwrap();
    input.approval.proof.nonce = "v2-phase-new-mfa".into();
    input.approval.proof.approval_id = "v2-phase-proof".into();
    input.approval.proof.approval_request_id = input.request.approval_request_id.clone();
    input.approval.proof.issued_at = DecimalU64(1500);
    input.approval.proof.expires_at = DecimalU64(31500);
    input.approval = sign_proof(input.approval.proof);
    let policy = Policy::new(s.draft());
    let record = s
        .begin_owner_setup_v2(
            input,
            header,
            "b3".repeat(32),
            &policy.current(&c),
            1500,
            &mut StrictCrypto,
        )
        .unwrap();
    let mut fact = verification(&record, &c).verification;
    fact.observed_at = DecimalU64(1501);
    s.commit_owner_setup_v2(
        CommitOwnerSetupV2 {
            id: "setup-one".into(),
            signed: sign_verification(fact),
            source_nonce: "b4".repeat(32),
        },
        &scope(&pin),
        &policy.current(&c),
        1502,
        &mut StrictCrypto,
    )
    .unwrap();
    s.claim_owner_setup_source("setup-one", &policy.current(&c), 1503)
        .unwrap();
    let mut origin =
        verification(s.owner_setup_phase_original("setup-one").unwrap(), &c).verification;
    origin.purpose = VerificationPurposeV1::InspectOrigin;
    origin.verify_nonce = s
        .owner_setup_phase_source("setup-one")
        .unwrap()
        .verify_nonce
        .clone();
    origin.observed_at = DecimalU64(1504);
    s.complete_owner_setup_source(
        "setup-one",
        sign_verification(origin),
        &policy.current(&c),
        1505,
        &mut StrictCrypto,
    )
    .unwrap();
    (s, c, pin)
}
fn consumed_move0() -> (
    AccountProvisionApprovalState,
    ApprovalAdmissionContext,
    NativeBindingPinV1,
) {
    consumed_selection(|_, _| json!({"kind":"move","position_cm":["1","2","3"]}))
}
#[test]
fn owner_v2_actual_four_writes_and_final_read_replay_under_full_header() {
    let (mut s, c, pin) = consumed_move0();
    assert!(s.owner_setups().is_empty());
    assert!(s.owner_setup_plan("setup-one").is_none());
    let commit = s
        .participant_original("participant-one")
        .unwrap()
        .result
        .as_ref()
        .unwrap()
        .owner_response
        .payload
        .version
        .commit
        .clone();
    let mut canonical = CanonicalState {
        task: None,
        delegation: None,
        commit,
    };
    for (n, stage) in [
        OwnerSetupStageV1::CreateTask,
        OwnerSetupStageV1::PrepareGoal,
        OwnerSetupStageV1::PublishGoal,
        OwnerSetupStageV1::Delegation,
        OwnerSetupStageV1::Bind,
        OwnerSetupStageV1::FinalObservation,
    ]
    .into_iter()
    .enumerate()
    {
        let now = 1600 + n as u64 * 100;
        let candidate = prepare_stage(&mut s, &c, &pin, stage, &canonical, now);
        finish_stage(&mut s, &c, &pin, stage, &candidate, &mut canonical, now);
    }
    assert_eq!(s.provision().stage(), ProvisionStage::Prepared);
    assert_eq!(s.owner_setup_progress("setup-one").unwrap().api_version, 2);
    assert!(s.owner_setups().is_empty());
    let writes = [
        OwnerSetupStageV1::CreateTask,
        OwnerSetupStageV1::PublishGoal,
        OwnerSetupStageV1::Delegation,
        OwnerSetupStageV1::Bind,
    ];
    for stage in writes {
        assert!(
            s.owner_setup_outbox("setup-one", stage)
                .unwrap()
                .result
                .is_some()
        );
    }
    let bytes = serde_json::to_value(&s).unwrap();
    let restored: AccountProvisionApprovalState = serde_json::from_value(bytes.clone()).unwrap();
    assert_eq!(restored, s);
    let mut tampered = bytes;
    tampered["owner_setup_v2"]["plan"]["selection"]["position_cm"][0] = json!("9");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(tampered).is_err());
}
#[test]
fn owner_v2_cannot_select_v1_or_another_setup_and_rejection_is_atomic() {
    let (mut s, _, _) = consumed_move0();
    let before = s.clone();
    assert!(
        s.reserve_owner_setup_stage(
            "wrong-setup",
            OwnerSetupStageV1::CreateTask,
            &"c1".repeat(32),
            1510
        )
        .is_err()
    );
    assert_eq!(s, before);
    assert!(s.owner_setup_plan("setup-one").is_none());
    assert!(s.owner_setup_original("setup-one").is_none());
    assert_eq!(s.owner_setup_engine_version("setup-one").unwrap(), 2);
    assert!(s.owner_setup_engine_version("wrong-setup").is_err());
    assert_eq!(s, before);
    let mut raw = serde_json::to_value(&s).unwrap();
    raw["owner_setup_v2"]["phase"]["record"]["plan"]["goal_content"]["goal_text"] =
        json!("other goal");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(raw).is_err());
}

pub(super) fn prepared_move0_v2_fixture() -> (
    AccountProvisionApprovalState,
    ApprovalAdmissionContext,
    NativeBindingPinV1,
) {
    let (mut s, c, pin) = consumed_move0();
    let commit = s
        .participant_original("participant-one")
        .unwrap()
        .result
        .as_ref()
        .unwrap()
        .owner_response
        .payload
        .version
        .commit
        .clone();
    let mut canonical = CanonicalState {
        task: None,
        delegation: None,
        commit,
    };
    for (n, stage) in [
        OwnerSetupStageV1::CreateTask,
        OwnerSetupStageV1::PrepareGoal,
        OwnerSetupStageV1::PublishGoal,
        OwnerSetupStageV1::Delegation,
        OwnerSetupStageV1::Bind,
        OwnerSetupStageV1::FinalObservation,
    ]
    .into_iter()
    .enumerate()
    {
        let now = 1600 + n as u64 * 100;
        let candidate = prepare_stage(&mut s, &c, &pin, stage, &canonical, now);
        finish_stage(&mut s, &c, &pin, stage, &candidate, &mut canonical, now);
    }
    assert_eq!(s.provision().stage(), ProvisionStage::Prepared);
    assert!(s.owner_setups().is_empty());
    let prepared = s.owner_setup_prepared_v2("setup-one").unwrap();
    assert_eq!(prepared.plan.api_version, 2);
    (s, c, pin)
}

#[path = "test_session_v2_authority_fixture.rs"]
mod session_v2_tests;

#[path = "owner_setup_resource_final_tests.rs"]
mod resource_final;
