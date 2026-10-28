//! Full public Core transitions with real Owner/World/Native signatures.
use super::*;
use oasis7_agent_api::game::*;
use oasis7_client_api::world_service::private_goal::{
    PreparedGoalReceiptV1, prepared_goal_commitment, sign_goal_prepare,
};
struct CanonicalState {
    task: Option<GameTaskControlV1>,
    delegation: Option<oasis7_client_api::world_service_wire::AgentSignerDelegationChangeV1>,
    commit: ws::CommitRef,
}
fn sign_candidate(
    s: &AccountProvisionApprovalState,
    bundle: OwnerSetupSigningBundleV1,
) -> OwnerSetupCandidateV1 {
    let mut observation = bundle.participant_access;
    observation.read =
        ws::sign_read_request(ws::VIEW_JSON_PATH, bundle.observation, &"3e".repeat(32)).unwrap();
    observation.response_profile = Some(ParticipantOwnerViewProfileV1::OwnerSetupV1);
    let payload = match &bundle.action {
        OwnerSetupSigningRequestV1::CreateTask(r) => Some(WorldServicePayloadV1::GameTaskCreate(
            ws::sign_read_request(GAME_TASK_CREATE_DOMAIN, r.clone(), &"3e".repeat(32)).unwrap(),
        )),
        OwnerSetupSigningRequestV1::PublishGoal(r) => Some(WorldServicePayloadV1::GameGoalPublish(
            ws::sign_read_request(GAME_GOAL_PUBLISH_DOMAIN, r.clone(), &"3e".repeat(32)).unwrap(),
        )),
        OwnerSetupSigningRequestV1::Delegation(r) => Some(WorldServicePayloadV1::Delegation(
            ws::sign_read_request("delegation", r.clone(), &"3e".repeat(32)).unwrap(),
        )),
        OwnerSetupSigningRequestV1::Bind(r) => Some(WorldServicePayloadV1::GameExecutorControl(
            ws::sign_read_request(GAME_EXECUTOR_CONTROL_DOMAIN, r.clone(), &"3e".repeat(32))
                .unwrap(),
        )),
        _ => None,
    };
    let original = if let Some(payload) = payload {
        let w = &s.draft().world_identity;
        let correlation = oasis7_client_api::world_service_correlation::derive_correlation(
            ws::WorldIdentity {
                world_id: w.world_id.clone(),
                genesis_digest: w.genesis_digest.clone(),
            },
            &payload,
        )
        .unwrap();
        OwnerSetupOriginalV1::Canonical {
            submit: Box::new(ws::SubmitIntentRequest {
                contract_version: 1,
                correlation: correlation.clone(),
                deadline_unix_ms: Some(80_000),
                signed_payload: payload.clone(),
            }),
            lookup: Box::new(AuthenticatedLookup {
                request: ws::LookupIntentRequest {
                    contract_version: 1,
                    key: correlation.key,
                },
                original: payload,
            }),
        }
    } else {
        match bundle.action {
            OwnerSetupSigningRequestV1::PrepareGoal(r) => OwnerSetupOriginalV1::PrepareGoal(
                Box::new(sign_goal_prepare(r, &"3e".repeat(32)).unwrap()),
            ),
            OwnerSetupSigningRequestV1::FinalObservation { goal, .. } => {
                OwnerSetupOriginalV1::FinalObservation {
                    owner: Box::new(observation.clone()),
                    goal: Box::new(
                        ws::sign_read_request(
                            ws::private_goal_read::GOAL_CURRENT_PATH,
                            goal,
                            &"3e".repeat(32),
                        )
                        .unwrap(),
                    ),
                }
            }
            _ => unreachable!(),
        }
    };
    OwnerSetupCandidateV1 {
        original,
        observation,
    }
}
fn world_evidence(
    s: &AccountProvisionApprovalState,
    candidate: &OwnerSetupCandidateV1,
    canonical: &CanonicalState,
    now: u64,
) -> OwnerSetupAdmissionEvidenceV1 {
    let mut e = admission(s, candidate, now);
    let control = &mut e.world.response.payload.control;
    control.view.game_task_control = canonical.task.clone();
    control.version.commit = canonical.commit.clone();
    control.version.projection_revision = ws::request_digest("projection", &control.view).unwrap();
    e.world.response.payload.signer_delegation = canonical.delegation.clone();
    e.world.response = ws::sign_service_response(
        PARTICIPANT_OWNER_VIEW_PATH,
        e.world.response.request_digest,
        e.world.response.payload,
        &"3f".repeat(32),
    )
    .unwrap();
    e
}
fn prepare_stage(
    s: &mut AccountProvisionApprovalState,
    c: &ApprovalAdmissionContext,
    pin: &NativeBindingPinV1,
    stage: OwnerSetupStageV1,
    canonical: &CanonicalState,
    now: u64,
) -> OwnerSetupCandidateV1 {
    let policy = Policy::new(s.draft());
    let current = policy.current(c);
    let ns = scope(pin);
    let trusted = OwnerSetupCurrent {
        provision: &current,
        native_scope: &ns,
    };
    s.reserve_owner_setup_stage("setup-one", stage, &format!("{now:064x}"), now)
        .unwrap();
    s.claim_owner_setup_stage_source("setup-one", stage, &current, now + 1)
        .unwrap();
    let mut origin = verification(s.owner_setup_original("setup-one").unwrap(), c).verification;
    origin.purpose = VerificationPurposeV1::InspectOrigin;
    origin.verify_nonce = s
        .owner_setup_outbox("setup-one", stage)
        .unwrap()
        .source
        .verify_nonce
        .clone();
    origin.observed_at = DecimalU64(now + 2);
    s.complete_owner_setup_stage_source(
        "setup-one",
        stage,
        sign_verification(origin),
        &current,
        now + 3,
        &mut StrictCrypto,
    )
    .unwrap();
    for ticket in 1..=2 {
        let read = s
            .reserve_owner_setup_read(
                "setup-one",
                stage,
                OwnerSetupReadPurposeV1::Session,
                Role::Identity,
                now + 4,
            )
            .unwrap();
        s.authorize_owner_setup_read(
            "setup-one",
            read,
            Role::Identity,
            &trusted,
            now + 4,
            &mut StrictCrypto,
        )
        .unwrap();
        if ticket == 2 {
            s.claim_owner_setup_access("setup-one", stage, now + 4)
                .unwrap();
        }
    }
    let mut access = fresh_access(s, stage);
    let n = s.owner_setup_access_nonces("setup-one", stage).unwrap();
    access.policy_session = fixture::root_session(s, n.policy_session, now + 5);
    access.authorize_session = fixture::root_session(s, n.authorize_session, now + 5);
    for ticket in 3..=4 {
        let read = s
            .reserve_owner_setup_read(
                "setup-one",
                stage,
                OwnerSetupReadPurposeV1::Sign,
                Role::Owner,
                now + 6,
            )
            .unwrap();
        if ticket == 3 {
            s.record_owner_setup_access(
                "setup-one",
                stage,
                access.clone(),
                &trusted,
                now + 6,
                &mut StrictCrypto,
            )
            .unwrap();
        }
        s.authorize_owner_setup_read(
            "setup-one",
            read,
            Role::Owner,
            &trusted,
            now + 6,
            &mut StrictCrypto,
        )
        .unwrap();
    }
    let bundle = s
        .owner_setup_signing_request("setup-one", stage, &trusted, now + 6)
        .unwrap();
    let candidate = sign_candidate(s, bundle);
    let read = s
        .reserve_owner_setup_read(
            "setup-one",
            stage,
            OwnerSetupReadPurposeV1::Sign,
            Role::Owner,
            now + 6,
        )
        .unwrap();
    s.publish_owner_setup_original(
        "setup-one",
        read,
        candidate.clone(),
        &trusted,
        now + 6,
        &mut StrictCrypto,
    )
    .unwrap();
    s.reserve_owner_setup_admission_callback("setup-one", stage, now + 8)
        .unwrap();
    let evidence = world_evidence(s, &candidate, canonical, now + 9);
    s.record_owner_setup_admission(
        "setup-one",
        stage,
        evidence,
        &trusted,
        now + 10,
        &mut StrictCrypto,
    )
    .unwrap();
    s.claim_owner_setup_send("setup-one", stage, &trusted, now + 11, &mut StrictCrypto)
        .unwrap();
    candidate
}
fn finish_stage(
    s: &mut AccountProvisionApprovalState,
    c: &ApprovalAdmissionContext,
    pin: &NativeBindingPinV1,
    stage: OwnerSetupStageV1,
    candidate: &OwnerSetupCandidateV1,
    canonical: &mut CanonicalState,
    now: u64,
) {
    let policy = Policy::new(s.draft());
    let current = policy.current(c);
    let ns = scope(pin);
    let trusted = OwnerSetupCurrent {
        provision: &current,
        native_scope: &ns,
    };
    let result = match &candidate.original {
        OwnerSetupOriginalV1::Canonical { submit, .. } => {
            canonical.commit.position += 10;
            let legacy = serde_json::from_value(serde_json::to_value(submit).unwrap()).unwrap();
            let receipt = match &submit.signed_payload {
                WorldServicePayloadV1::GameTaskCreate(_) => {
                    let task =
                        oasis7_client_api::legacy_task_results::create_control(&legacy).unwrap();
                    canonical.task = Some(task.clone());
                    serde_json::to_value(task).unwrap()
                }
                WorldServicePayloadV1::GameGoalPublish(_) => {
                    let receipt = oasis7_client_api::legacy_task_results::publish_receipt(
                        &legacy,
                        canonical.commit.position,
                    )
                    .unwrap();
                    let task = canonical.task.as_mut().unwrap();
                    task.task_revision = receipt.task_revision;
                    task.latest_goal_revision = receipt.goal_reference.goal_revision;
                    task.active_goal_revision = receipt.goal_reference.goal_revision;
                    task.active_goal_reference = Some(receipt.goal_reference.clone());
                    serde_json::to_value(receipt).unwrap()
                }
                WorldServicePayloadV1::Delegation(signed) => {
                    canonical.delegation = Some(signed.request.clone());
                    json!({"delegation":signed.request})
                }
                WorldServicePayloadV1::GameExecutorControl(_) => {
                    let receipt =
                        oasis7_client_api::legacy_task_results::executor_receipt(&legacy).unwrap();
                    let task = canonical.task.as_mut().unwrap();
                    task.task_revision = receipt.task_revision;
                    task.task_status = receipt.task_status.clone();
                    task.executor_epoch = receipt.executor_epoch;
                    task.executor_binding = receipt.executor_binding.clone();
                    task.executor_id = task
                        .executor_binding
                        .as_ref()
                        .map(|v| v.executor_id.clone());
                    serde_json::to_value(receipt).unwrap()
                }
                _ => unreachable!(),
            };
            let fact = archive_tests::fact(
                candidate,
                OwnerSetupCanonicalSourceV1::Submit,
                ws::IntentOutcome::Committed {
                    commit: Box::new(canonical.commit.clone()),
                    receipt,
                },
            );
            let key = s.draft().world_identity.service_public_key.clone();
            s.archive_owner_setup_canonical(
                "setup-one",
                stage,
                fact.clone(),
                &OwnerSetupArchiveContext {
                    world_public_key: &key,
                },
                &mut StrictCrypto,
            )
            .unwrap();
            OwnerSetupResultV1::Canonical {
                response: fact.response,
                from_lookup: false,
            }
        }
        OwnerSetupOriginalV1::PrepareGoal(original) => {
            let salt = [0xab; 32];
            let receipt = PreparedGoalReceiptV1 {
                schema_revision: 1,
                goal_ref: prepared_goal_commitment(&salt, original).unwrap(),
                reference_salt: "ab".repeat(32),
                owner_proof: original.as_ref().clone(),
            };
            OwnerSetupResultV1::PreparedGoal(
                ws::sign_service_response(
                    ws::private_goal_read::GOAL_PREPARE_PATH,
                    ws::request_digest(ws::private_goal_read::GOAL_PREPARE_PATH, original).unwrap(),
                    receipt,
                    &"3f".repeat(32),
                )
                .unwrap(),
            )
        }
        _ => unreachable!(),
    };
    s.reserve_owner_setup_callback("setup-one", stage, now + 12)
        .unwrap();
    let nonce = s
        .owner_setup_outbox("setup-one", stage)
        .unwrap()
        .session_nonce
        .clone();
    let session = fixture::root_session(s, nonce, now + 13);
    s.complete_owner_setup_stage(
        "setup-one",
        OwnerSetupCompletionV1 {
            stage,
            result,
            session,
        },
        &trusted,
        now + 13,
        &mut StrictCrypto,
    )
    .unwrap();
}
fn final_dispatched() -> (
    AccountProvisionApprovalState,
    ApprovalAdmissionContext,
    NativeBindingPinV1,
    OwnerSetupCandidateV1,
) {
    let (mut s, c, pin, created) = archive_tests::dispatched();
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
    finish_stage(
        &mut s,
        &c,
        &pin,
        OwnerSetupStageV1::CreateTask,
        &created,
        &mut canonical,
        1510,
    );
    for (n, stage) in [
        OwnerSetupStageV1::PrepareGoal,
        OwnerSetupStageV1::PublishGoal,
        OwnerSetupStageV1::Delegation,
        OwnerSetupStageV1::Bind,
    ]
    .into_iter()
    .enumerate()
    {
        let now = 1600 + n as u64 * 100;
        let candidate = prepare_stage(&mut s, &c, &pin, stage, &canonical, now);
        finish_stage(&mut s, &c, &pin, stage, &candidate, &mut canonical, now);
    }
    assert_eq!(
        canonical.task.as_ref().unwrap().task_status,
        GameTaskStatusV1::Active
    );
    let final_candidate = prepare_stage(
        &mut s,
        &c,
        &pin,
        OwnerSetupStageV1::FinalObservation,
        &canonical,
        2100,
    );
    assert_eq!(s.provision().stage(), ProvisionStage::ReadyForSetup);
    assert!(
        s.owner_setup_outbox("setup-one", OwnerSetupStageV1::FinalObservation)
            .unwrap()
            .result
            .is_none()
    );
    (s, c, pin, final_candidate)
}
#[path = "owner_setup_final_reconcile_tests.rs"]
mod tests;

#[path = "test_session_authority_tests.rs"]
mod test_session_authority_tests;

#[path = "owner_setup_restore_metrics_tests.rs"]
mod restore_metrics_tests;
