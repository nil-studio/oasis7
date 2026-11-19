//! Bounded fresh decisions follow verified terminal stale originals only.
use super::*;

pub(super) fn retain_stale(
    execution: &Execution<'_>,
    context: &TaskContextIdentityV1,
    original: &SubmitIntentRequest<WorldServicePayloadV1>,
) {
    let current = pin(execution.root, execution.identity);
    let key = correlation::key_digest(&original.correlation.key).unwrap();
    let canonical: CanonicalIntentResultV1 = serde_json::from_value(
        current
            .world
            .capability_revocation_state()
            .world_service_results[&key]
            .clone(),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&canonical.request).unwrap(),
        serde_json::to_value(original).unwrap(),
    );
    let stale = format!(
        "{:?}",
        oasis7_runtime::WorldError::DistributedValidationFailed {
            reason: "stale_base".into(),
        }
    );
    assert_eq!(canonical.rejected.as_deref(), Some(stale.as_str()));
    assert!(canonical.receipt.is_null());
    assert!(
        current.world.state().time <= context.valid_until_tick.0,
        "terminal recovery must not infer closure from an expired lease",
    );
    let repeated = execution
        .client
        .lookup(
            LookupIntentRequest {
                contract_version: 1,
                key: original.correlation.key.clone(),
            },
            original.signed_payload.clone(),
        )
        .unwrap();
    assert!(matches!(repeated.outcome, IntentOutcome::Rejected { .. }));
    let WorldServicePayloadV1::Cognition(signed) = &original.signed_payload else {
        panic!("only a terminal cognition original permits fresh preparation");
    };
    let view = execution
        .client
        .read_view(ReadWorldViewRequest {
            contract_version: 1,
            world: execution.identity.clone(),
            scope_id: format!("agent:{}", context.fence.agent_id),
            observation_nonce: None,
            formal_anchor: None,
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        })
        .expect("verified scoped terminal feedback");
    assert_eq!(
        GameTaskFenceV1::from_control(view.projection().game_task_control.as_ref().unwrap()),
        context.fence,
        "the current task must retain the original owner, goal and executor fence",
    );
    assert!(view.projection().state.time <= context.valid_until_tick.0);
    let r = &signed.request.request;
    let matching = view
        .projection()
        .feedback_history
        .as_ref()
        .unwrap()
        .records
        .iter()
        .filter(|record| {
            let f = &record.feedback;
            f.agent_subject == r.agent_id
                && f.agent_session_id == r.agent_session_id
                && f.agent_turn_id == r.agent_turn_id
                && f.decision_request_id == r.decision_request_id
                && f.request_digest.to_string() == r.request_digest
        })
        .collect::<Vec<_>>();
    assert_eq!(
        matching.len(),
        1,
        "the exact original must have one terminal feedback"
    );
    let feedback = &matching[0].feedback;
    assert!(
        feedback.status == "rejected"
            && feedback.reject_reason.as_deref() == Some("stale_base")
            && feedback.provenance == "runtime_authoritative"
            && feedback.candidate_action_id.is_none()
            && feedback.runtime_receipt_id.is_none()
            && matching[0].receipt_lineage.is_none()
    );
    assert!(
        game_task::validate_task_context_capture(
            &current.world,
            context,
            &signed.subject_public_key,
        )
        .is_err(),
        "the old turn must be closed before a fresh Context is prepared"
    );
    let mut retained = execution.stale_originals.borrow_mut();
    retained.push(canonical);
    fs::write(
        execution.root.join("ordinary-terminal-attempts.json"),
        serde_json::to_vec_pretty(&*retained).unwrap(),
    )
    .unwrap();
    eprintln!(
        "ORDINARY_TERMINAL_STALE attempt={} original={}",
        retained.len(),
        original.correlation.key.request_id_or_nonce
    );
}
