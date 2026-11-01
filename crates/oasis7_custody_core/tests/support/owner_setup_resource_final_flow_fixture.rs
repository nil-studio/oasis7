//! Actual signed Core prefix; no copied V1 Prepared or fixture authority Boolean.
use super::*;
pub(super) fn resource_prefix() -> (
    AccountProvisionApprovalState,
    ApprovalAdmissionContext,
    NativeBindingPinV1,
    CanonicalState,
) {
    let selected = resource::ResourceBasisSelectionV1::Transfer {
        from: resource::ResourceLedgerV1::Agent {
            agent_id: "agent".into(),
        },
        to: resource::ResourceLedgerV1::Site {
            site_id: "site".into(),
        },
        material: "iron_ore".into(),
        amount: resource::ResourceDecimalI64V1(5),
        distance_km: resource::ResourceDecimalI64V1(0),
        priority: oasis7_agent_api::world_action::MaterialTransitPriority::Standard,
        routing: resource::ResourceRoutingV1::Direct,
    };
    let (mut s, c, pin) = consumed_selection(|s, _| {
        let mut selected = selected;
        if let resource::ResourceBasisSelectionV1::Transfer { from, .. } = &mut selected {
            *from = resource::ResourceLedgerV1::Agent {
                agent_id: s
                    .participant_original("participant-one")
                    .unwrap()
                    .result
                    .as_ref()
                    .unwrap()
                    .owner_response
                    .payload
                    .view
                    .canonical_agent_owner
                    .agent_id
                    .clone(),
            };
        }
        serde_json::to_value(selected).unwrap()
    });
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
    ]
    .into_iter()
    .enumerate()
    {
        let now = 1600 + n as u64 * 100;
        let candidate = prepare_stage(&mut s, &c, &pin, stage, &canonical, now);
        finish_stage(&mut s, &c, &pin, stage, &candidate, &mut canonical, now);
    }
    (s, c, pin, canonical)
}
pub(super) fn prepare_resource_access(
    s: &mut AccountProvisionApprovalState,
    c: &ApprovalAdmissionContext,
    pin: &NativeBindingPinV1,
    now: u64,
) {
    let stage = OwnerSetupStageV1::FinalObservation;
    let policy = Policy::new(s.draft());
    let current = policy.current(c);
    let ns = scope(pin);
    let trusted = OwnerSetupCurrent {
        provision: &current,
        native_scope: &ns,
    };
    s.reserve_owner_setup_resource_final_v2("setup-one", &format!("{now:064x}"), now)
        .unwrap();
    s.claim_owner_setup_stage_source("setup-one", stage, &current, now + 1)
        .unwrap();
    let mut origin =
        verification(s.owner_setup_phase_original("setup-one").unwrap(), c).verification;
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
}
pub(super) fn resource_published() -> (
    AccountProvisionApprovalState,
    ApprovalAdmissionContext,
    NativeBindingPinV1,
    CanonicalState,
    OwnerSetupResourceCandidateV2,
) {
    let (mut s, c, pin, canonical) = resource_prefix();
    prepare_resource_access(&mut s, &c, &pin, 2100);
    let policy = Policy::new(s.draft());
    let current = policy.current(&c);
    let ns = scope(&pin);
    let trusted = OwnerSetupCurrent {
        provision: &current,
        native_scope: &ns,
    };
    let claim = s
        .claim_owner_setup_resource_read_v2("setup-one", &trusted, 2106, &mut StrictCrypto)
        .unwrap();
    let mut observation = claim.participant_access().clone();
    observation.read = ws::sign_read_request(
        ws::VIEW_JSON_PATH,
        claim.observation().clone(),
        &"3e".repeat(32),
    )
    .unwrap();
    observation.response_profile = Some(ParticipantOwnerViewProfileV1::OwnerSetupV1);
    let candidate = OwnerSetupResourceCandidateV2 {
        original: ws::sign_read_request(
            resource::RESOURCE_GOAL_PATH_V2,
            claim.request().clone(),
            &"3e".repeat(32),
        )
        .unwrap()
        .into(),
        observation,
    };
    let read = s
        .reserve_owner_setup_read(
            "setup-one",
            OwnerSetupStageV1::FinalObservation,
            OwnerSetupReadPurposeV1::Sign,
            Role::Owner,
            2106,
        )
        .unwrap();
    s.authorize_owner_setup_read(
        "setup-one",
        read,
        Role::Owner,
        &trusted,
        2106,
        &mut StrictCrypto,
    )
    .unwrap();
    s.publish_owner_setup_resource_read_v2(
        "setup-one",
        candidate.clone(),
        &trusted,
        2106,
        &mut StrictCrypto,
    )
    .unwrap();
    s.reserve_owner_setup_resource_admission_callback_v2("setup-one", 2108)
        .unwrap();
    let mut control = s
        .participant_original("participant-one")
        .unwrap()
        .result
        .as_ref()
        .unwrap()
        .owner_response
        .payload
        .clone();
    control.logical_tick = 111;
    control.view.game_task_control = canonical.task.clone();
    control.version.commit = canonical.commit.clone();
    control.version.projection_revision = ws::request_digest("projection", &control.view).unwrap();
    let response = ws::sign_service_response(
        PARTICIPANT_OWNER_VIEW_PATH,
        ws::request_digest(PARTICIPANT_OWNER_VIEW_PATH, &candidate.observation).unwrap(),
        OwnerSetupObservationV1 {
            api_version: 1,
            control,
            signer_delegation: canonical.delegation.clone(),
        },
        &"3f".repeat(32),
    )
    .unwrap();
    let evidence = OwnerSetupAdmissionEvidenceV1 {
        world: OwnerSetupWorldEvidenceV1 {
            request: candidate.observation.clone(),
            response,
        },
        session: fixture::root_session(
            &s,
            candidate
                .observation
                .read
                .request
                .observation_nonce
                .clone()
                .unwrap(),
            2109,
        ),
    };
    s.record_owner_setup_admission(
        "setup-one",
        OwnerSetupStageV1::FinalObservation,
        evidence,
        &trusted,
        2110,
        &mut StrictCrypto,
    )
    .unwrap();
    (s, c, pin, canonical, candidate)
}
