//! Developer measurement and signed-result corruption regression. No tuple material is printed.
use super::*;
fn normal_completed() -> AccountProvisionApprovalState {
    let (mut state, context, pin, candidate) = final_dispatched();
    let stage = OwnerSetupStageV1::FinalObservation;
    let owner = state
        .owner_setup_outbox("setup-one", stage)
        .unwrap()
        .world
        .as_ref()
        .unwrap()
        .response
        .clone();
    let OwnerSetupOriginalV1::FinalObservation { goal: request, .. } = &candidate.original else {
        unreachable!()
    };
    let Some(OwnerSetupResultV1::PreparedGoal(prepared)) = state
        .owner_setup_outbox("setup-one", OwnerSetupStageV1::PrepareGoal)
        .unwrap()
        .result
        .as_ref()
    else {
        unreachable!()
    };
    let Some(OwnerSetupResultV1::Canonical {
        response: published,
        ..
    }) = state
        .owner_setup_outbox("setup-one", OwnerSetupStageV1::PublishGoal)
        .unwrap()
        .result
        .as_ref()
    else {
        unreachable!()
    };
    let ws::IntentOutcome::Committed {
        commit: activation, ..
    } = &published.payload.outcome
    else {
        unreachable!()
    };
    let goal = ws::sign_service_response(
        ws::private_goal_read::GOAL_CURRENT_PATH,
        ws::request_digest(ws::private_goal_read::GOAL_CURRENT_PATH, request).unwrap(),
        ws::private_goal_read::CurrentGoalResponse {
            schema_revision: 1,
            control: owner
                .payload
                .control
                .view
                .game_task_control
                .clone()
                .unwrap(),
            read_commit: owner.payload.control.version.commit.clone(),
            activation_commit: Some((**activation).clone()),
            active_goal: Some(prepared.payload.clone()),
        },
        &"3f".repeat(32),
    )
    .unwrap();
    state
        .reserve_owner_setup_callback("setup-one", stage, 2112)
        .unwrap();
    let nonce = state
        .owner_setup_outbox("setup-one", stage)
        .unwrap()
        .session_nonce
        .clone();
    let session = fixture::root_session(&state, nonce, 2113);
    let policy = Policy::new(state.draft());
    let current = policy.current(&context);
    let ns = scope(&pin);
    state
        .complete_owner_setup_stage(
            "setup-one",
            OwnerSetupCompletionV1 {
                stage,
                result: OwnerSetupResultV1::FinalObservation {
                    owner: Box::new(owner),
                    goal: Box::new(goal),
                },
                session,
            },
            &OwnerSetupCurrent {
                provision: &current,
                native_scope: &ns,
            },
            2113,
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(state.provision().stage(), ProvisionStage::Prepared);
    state
}
#[cfg(feature = "signature-metrics")]
#[test]
fn setup_normal_completed_restore_developer_signature_metrics() {
    // All construction, signing and serialization finish before the measured scope.
    let state = normal_completed();
    let bytes = serde_json::to_vec(&state).unwrap();
    let stages = [
        OwnerSetupStageV1::CreateTask,
        OwnerSetupStageV1::PrepareGoal,
        OwnerSetupStageV1::PublishGoal,
        OwnerSetupStageV1::Delegation,
        OwnerSetupStageV1::Bind,
        OwnerSetupStageV1::FinalObservation,
    ];
    let mut counts = std::collections::BTreeMap::new();
    for stage in stages {
        let outbox = state.owner_setup_outbox("setup-one", stage).unwrap();
        assert!(matches!(
            outbox.result_provenance,
            None | Some(OwnerSetupResultProvenanceV1::Normal)
        ));
        assert!(outbox.result.as_ref().unwrap().succeeded());
        counts.insert(stage, 1u64);
    }
    oasis7_custody_core::signature_metrics::begin_scope();
    let restored = serde_json::from_slice::<AccountProvisionApprovalState>(&bytes);
    let summary = oasis7_custody_core::signature_metrics::take_summary();
    assert_eq!(restored.unwrap(), state);
    assert!(summary.strict_attempts > 0);
    assert_eq!(summary.failures, 0);
    assert_eq!(summary.successes, summary.strict_attempts);
    assert_eq!(
        summary.overflow_tuples, 0,
        "fixture must fit the bounded uniqueness collector"
    );
    assert_eq!(
        summary.distinct_tuples + summary.duplicate_tuples,
        summary.strict_attempts
    );
    eprintln!(
        "{}",
        json!({"normal_completed_restore_signature_metrics":{"stage_counts":counts,"bytes":bytes.len(),"summary":summary}})
    );
    // This is a baseline measurement, not a claim all duplicates are redundant:
    // archived facts and nested authority evidence can legitimately repeat tuples.
}
fn flip_signature(value: &mut Value) {
    let old = value.as_str().unwrap();
    assert_eq!(old.len(), 128);
    let replacement = if old.starts_with('0') { '1' } else { '0' };
    let mut changed = old.to_owned();
    changed.replace_range(0..1, &replacement.to_string());
    *value = changed.into();
}
#[test]
fn setup_normal_completed_restore_rejects_corrupt_signatures_in_all_result_variants() {
    let state = normal_completed();
    let original = serde_json::to_value(&state).unwrap();
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(original.clone()).unwrap(),
        state
    );
    for case in 0..4 {
        let mut changed = original.clone();
        let stages = &mut changed["owner_setups"]["setup-one"]["stages"];
        match case {
            0 => {
                flip_signature(
                    &mut stages["create_task"]["result"]["result"]["response"]["signature_hex"],
                );
                let signature =
                    stages["create_task"]["result"]["result"]["response"]["signature_hex"].clone();
                // Preserve result/archive equality so immutable-slot mismatch is
                // not the only reason a corrupted canonical signature rejects.
                stages["create_task"]["canonical_history"]["submit"]["response"]["signature_hex"] =
                    signature;
            }
            1 => flip_signature(&mut stages["prepare_goal"]["result"]["result"]["signature_hex"]),
            2 => {
                flip_signature(
                    &mut stages["final_observation"]["result"]["result"]["owner"]["signature_hex"],
                );
                let signature =
                    stages["final_observation"]["result"]["result"]["owner"]["signature_hex"]
                        .clone();
                // Preserve the exact admitted World/result equality as well.
                stages["final_observation"]["world"]["response"]["signature_hex"] = signature;
            }
            _ => flip_signature(
                &mut stages["final_observation"]["result"]["result"]["goal"]["signature_hex"],
            ),
        }
        let stage = match case {
            0 => "create_task",
            1 => "prepare_goal",
            _ => "final_observation",
        };
        // The persisted Operation and final Provision phase bind the COMPLETE
        // typed result, including its signature. Mirror those bookkeeping
        // digests so restore must reach cryptographic verification rather than
        // reject a stale operation/phase receipt digest first.
        let typed: OwnerSetupResultV1 =
            serde_json::from_value(stages[stage]["result"].clone()).unwrap();
        let receipt_digest = oasis7_custody_core::approval::bytes_digest(
            &oasis7_wasm_abi::encode_canonical_cbor(&typed).unwrap(),
        );
        stages[stage]["operation"]["receipt_digest"] = receipt_digest.clone().into();
        if stage == "final_observation" {
            changed["provision"]["originals"]["setup"]["receipt_digest"] = receipt_digest.into();
        }
        #[cfg(feature = "signature-metrics")]
        oasis7_custody_core::signature_metrics::begin_scope();
        let restored = serde_json::from_value::<AccountProvisionApprovalState>(changed);
        #[cfg(feature = "signature-metrics")]
        {
            let summary = oasis7_custody_core::signature_metrics::take_summary();
            assert!(
                summary.strict_attempts > 0,
                "corruption case {case} must reach actual strict crypto"
            );
            assert_eq!(
                summary.failures, 1,
                "only the changed strong signature must fail in case {case}"
            );
        }
        assert!(
            restored.is_err(),
            "normal completed result signature corruption case {case}"
        );
    }
}
