use super::*;
use oasis7_custody_core::operations::OperationState;
pub(super) fn dispatched() -> (
    AccountProvisionApprovalState,
    ApprovalAdmissionContext,
    NativeBindingPinV1,
    OwnerSetupCandidateV1,
) {
    let (mut s, c, pin, candidate) = candidate_ready();
    let p = Policy::new(s.draft());
    let current = p.current(&c);
    let native_scope = scope(&pin);
    let trusted = OwnerSetupCurrent {
        provision: &current,
        native_scope: &native_scope,
    };
    let stage = OwnerSetupStageV1::CreateTask;
    s.reserve_owner_setup_admission_callback("setup-one", stage, 1518)
        .unwrap();
    let fact = admission(&s, &candidate, 1519);
    s.record_owner_setup_admission("setup-one", stage, fact, &trusted, 1520, &mut StrictCrypto)
        .unwrap();
    s.claim_owner_setup_send("setup-one", stage, &trusted, 1521, &mut StrictCrypto)
        .unwrap();
    (s, c, pin, candidate)
}
pub(super) fn fact(
    candidate: &OwnerSetupCandidateV1,
    source: OwnerSetupCanonicalSourceV1,
    outcome: ws::IntentOutcome<serde_json::Value>,
) -> OwnerSetupCanonicalFactV1 {
    let OwnerSetupOriginalV1::Canonical { submit, lookup } = &candidate.original else {
        unreachable!()
    };
    let domain = source.response_domain();
    let digest = match source {
        OwnerSetupCanonicalSourceV1::Submit => ws::request_digest(domain, submit),
        OwnerSetupCanonicalSourceV1::Lookup => ws::request_digest(domain, lookup),
    }
    .unwrap();
    OwnerSetupCanonicalFactV1 {
        source,
        response: ws::sign_service_response(
            domain,
            digest,
            ws::IntentResponse {
                formal_profile: None,
                execution_evidence_scope: None,
                contract_version: 1,
                correlation: submit.correlation.clone(),
                outcome,
            },
            &"3f".repeat(32),
        )
        .unwrap(),
    }
}
#[test]
fn setup_archive_is_fixed_pin_fact_only_after_freeze_and_exact_retry() {
    let (mut s, _, _, candidate) = dispatched();
    let stage = OwnerSetupStageV1::CreateTask;
    s.freeze(2).unwrap();
    let pin = s.draft().world_identity.service_public_key.clone();
    let context = OwnerSetupArchiveContext {
        world_public_key: &pin,
    };
    let fact = fact(
        &candidate,
        OwnerSetupCanonicalSourceV1::Submit,
        ws::IntentOutcome::Rejected {
            reason: ws::WorldServiceErrorKind::Conflict,
        },
    );
    let before = s.provision().clone();
    let balance = s
        .owner_setup_progress("setup-one")
        .unwrap()
        .transport_remaining;
    s.archive_owner_setup_canonical(
        "setup-one",
        stage,
        fact.clone(),
        &context,
        &mut StrictCrypto,
    )
    .unwrap();
    assert_eq!(s.provision(), &before);
    assert_eq!(
        s.owner_setup_progress("setup-one")
            .unwrap()
            .transport_remaining,
        balance
    );
    let o = s.owner_setup_outbox("setup-one", stage).unwrap();
    assert_eq!(o.operation.state(), OperationState::Dispatching);
    assert!(o.result.is_none());
    assert_eq!(o.canonical_history.len(), 1);
    let saved = s.clone();
    s.archive_owner_setup_canonical(
        "setup-one",
        stage,
        fact.clone(),
        &context,
        &mut StrictCrypto,
    )
    .unwrap();
    assert_eq!(s, saved);
    let alternate = self::fact(
        &candidate,
        OwnerSetupCanonicalSourceV1::Submit,
        ws::IntentOutcome::HistoryUnavailable,
    );
    assert!(
        s.archive_owner_setup_canonical("setup-one", stage, alternate, &context, &mut StrictCrypto)
            .is_err()
    );
    assert_eq!(s, saved);
    let wrong = "ee".repeat(32);
    assert!(
        s.owner_setup_archive_checks(
            "setup-one",
            stage,
            &fact,
            &OwnerSetupArchiveContext {
                world_public_key: &wrong
            }
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
fn setup_known_rejection_is_retained_but_never_unlocks_next_phase() {
    let (mut s, c, pin, candidate) = dispatched();
    let stage = OwnerSetupStageV1::CreateTask;
    let world = s.draft().world_identity.service_public_key.clone();
    let context = OwnerSetupArchiveContext {
        world_public_key: &world,
    };
    let fact = fact(
        &candidate,
        OwnerSetupCanonicalSourceV1::Submit,
        ws::IntentOutcome::Rejected {
            reason: ws::WorldServiceErrorKind::Conflict,
        },
    );
    s.archive_owner_setup_canonical(
        "setup-one",
        stage,
        fact.clone(),
        &context,
        &mut StrictCrypto,
    )
    .unwrap();
    let policy = Policy::new(s.draft());
    let current = policy.current(&c);
    let native_scope = scope(&pin);
    let trusted = OwnerSetupCurrent {
        provision: &current,
        native_scope: &native_scope,
    };
    s.reserve_owner_setup_callback("setup-one", stage, 1524)
        .unwrap();
    let nonce = s
        .owner_setup_outbox("setup-one", stage)
        .unwrap()
        .session_nonce
        .clone();
    let session = fixture::root_session(&s, nonce, 1525);
    s.complete_owner_setup_stage(
        "setup-one",
        OwnerSetupCompletionV1 {
            stage,
            result: OwnerSetupResultV1::Canonical {
                response: fact.response,
                from_lookup: false,
            },
            session,
        },
        &trusted,
        1526,
        &mut StrictCrypto,
    )
    .unwrap();
    let o = s.owner_setup_outbox("setup-one", stage).unwrap();
    assert_eq!(o.operation.state(), OperationState::Confirmed);
    assert!(o.result.as_ref().unwrap().terminal());
    assert!(!o.result.as_ref().unwrap().succeeded());
    assert_eq!(s.provision().stage(), ProvisionStage::ReadyForSetup);
    let before = s.clone();
    assert!(
        s.reserve_owner_setup_stage(
            "setup-one",
            OwnerSetupStageV1::PrepareGoal,
            &"c1".repeat(32),
            1527
        )
        .is_err()
    );
    assert_eq!(s, before);
    let progress = s.owner_setup_progress("setup-one").unwrap();
    assert_eq!(
        progress.blocked_operations[0].code,
        OwnerSetupBlockedCodeV1::Rejected
    );
    assert!(!progress.pending_operations.contains(&stage.name().into()));
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(serde_json::to_value(&s).unwrap())
            .unwrap(),
        s
    );
}
#[test]
fn setup_nonterminal_signed_observations_keep_unknown_and_all_reservations() {
    let (base, c, pin, candidate) = dispatched();
    let stage = OwnerSetupStageV1::CreateTask;
    for outcome in [
        ws::IntentOutcome::Expired,
        ws::IntentOutcome::HistoryUnavailable,
        ws::IntentOutcome::Unknown,
        ws::IntentOutcome::Pending,
        ws::IntentOutcome::Received {
            durability: ws::AdmissionDurability::Durable,
        },
    ] {
        let mut s = base.clone();
        let public = s.draft().world_identity.service_public_key.clone();
        let fact = fact(&candidate, OwnerSetupCanonicalSourceV1::Submit, outcome);
        s.archive_owner_setup_canonical(
            "setup-one",
            stage,
            fact.clone(),
            &OwnerSetupArchiveContext {
                world_public_key: &public,
            },
            &mut StrictCrypto,
        )
        .unwrap();
        let balance = s
            .owner_setup_progress("setup-one")
            .unwrap()
            .transport_remaining;
        let p = Policy::new(s.draft());
        let current = p.current(&c);
        let native_scope = scope(&pin);
        let trusted = OwnerSetupCurrent {
            provision: &current,
            native_scope: &native_scope,
        };
        s.reserve_owner_setup_callback("setup-one", stage, 1524)
            .unwrap();
        let nonce = s
            .owner_setup_outbox("setup-one", stage)
            .unwrap()
            .session_nonce
            .clone();
        let session = fixture::root_session(&s, nonce, 1525);
        s.complete_owner_setup_stage(
            "setup-one",
            OwnerSetupCompletionV1 {
                stage,
                result: OwnerSetupResultV1::Canonical {
                    response: fact.response,
                    from_lookup: false,
                },
                session,
            },
            &trusted,
            1526,
            &mut StrictCrypto,
        )
        .unwrap();
        let o = s.owner_setup_outbox("setup-one", stage).unwrap();
        assert_eq!(o.operation.state(), OperationState::Unknown);
        assert!(o.result.is_none());
        assert!(o.last_observation.is_some());
        assert_eq!(
            s.owner_setup_progress("setup-one")
                .unwrap()
                .transport_remaining,
            balance
        );
        assert!(
            s.reserve_owner_setup_stage(
                "setup-one",
                OwnerSetupStageV1::PrepareGoal,
                &"c2".repeat(32),
                1527
            )
            .is_err()
        );
        assert_eq!(
            serde_json::from_value::<AccountProvisionApprovalState>(
                serde_json::to_value(&s).unwrap()
            )
            .unwrap(),
            s
        );
    }
}
#[test]
fn setup_archive_domain_and_lookup_claim_are_not_caller_assertions() {
    let (mut s, c, pin, candidate) = dispatched();
    let stage = OwnerSetupStageV1::CreateTask;
    let world = s.draft().world_identity.service_public_key.clone();
    let context = OwnerSetupArchiveContext {
        world_public_key: &world,
    };
    let lookup = fact(
        &candidate,
        OwnerSetupCanonicalSourceV1::Lookup,
        ws::IntentOutcome::Pending,
    );
    assert!(
        s.owner_setup_archive_checks("setup-one", stage, &lookup, &context)
            .is_err()
    );
    let before = s.clone();
    let wrong = OwnerSetupCanonicalFactV1 {
        source: OwnerSetupCanonicalSourceV1::Submit,
        response: lookup.response.clone(),
    };
    assert!(
        s.archive_owner_setup_canonical("setup-one", stage, wrong, &context, &mut StrictCrypto)
            .is_err()
    );
    assert_eq!(s, before);
    let policy = Policy::new(s.draft());
    let current = policy.current(&c);
    let native_scope = scope(&pin);
    let trusted = OwnerSetupCurrent {
        provision: &current,
        native_scope: &native_scope,
    };
    s.reserve_owner_setup_lookup("setup-one", stage, 1522)
        .unwrap();
    s.claim_owner_setup_lookup("setup-one", stage, &trusted, 1523, &mut StrictCrypto)
        .unwrap();
    let submit = fact(
        &candidate,
        OwnerSetupCanonicalSourceV1::Submit,
        ws::IntentOutcome::Pending,
    );
    s.archive_owner_setup_canonical("setup-one", stage, submit, &context, &mut StrictCrypto)
        .unwrap();
    s.archive_owner_setup_canonical("setup-one", stage, lookup, &context, &mut StrictCrypto)
        .unwrap();
    assert_eq!(
        s.owner_setup_outbox("setup-one", stage)
            .unwrap()
            .canonical_history
            .len(),
        2
    );
    assert!(
        s.reserve_owner_setup_lookup("setup-one", stage, 1524)
            .is_err()
    );
    assert!(
        s.claim_owner_setup_lookup("setup-one", stage, &trusted, 1524, &mut StrictCrypto)
            .is_err()
    );
}
