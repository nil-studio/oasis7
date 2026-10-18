use super::*;

#[test]
fn missing_authenticated_feedback_view_cannot_create_fresh_economic_checkpoint() {
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    assert!(
        sidecar
            .check_scoped_feedback_fresh_admission("agent-a", "session-a")
            .is_err()
    );
    assert!(sidecar.provider_scheduler_pending.is_empty());
    assert!(sidecar.provider_active_turns.is_empty());
    assert!(sidecar.hosted_admission.is_none());
}

fn actual_world_history(
    sequences: &[u64],
) -> (
    RuntimeWorld,
    crate::world_service::projection::WorldServiceProjection,
    String,
) {
    let mut world = crate::runtime_host::world::new();
    world
        .bind_cognition_runtime("feedback-guard-world", "main", 0, None, "pending", 0)
        .unwrap();
    world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: "agent-a".into(),
        pos: crate::geometry::GeoPos::new(0, 0, 0),
    });
    world.step().unwrap();
    world
        .install_test_provider_capability_fixture("agent-a")
        .unwrap();
    let authority =
        crate::viewer::ViewerRuntimeLiveServer::canonical_agent_service_context(&world, "agent-a")
            .unwrap();
    let session = authority
        .capability_invocation_context
        .presenter
        .session_id
        .clone()
        .unwrap();
    for seq in sequences {
        let feedback = crate::simulator::FeedbackEnvelopeV1 {
            feedback_id: format!("feedback-{seq}"),
            feedback_seq: *seq,
            agent_subject: "agent-a".into(),
            agent_session_id: session.clone(),
            agent_turn_id: format!("turn-{seq}"),
            decision_request_id: format!("request-{seq}"),
            candidate_action_id: None,
            runtime_receipt_id: None,
            status: "rejected".into(),
            request_digest: crate::simulator::h_v1("guard-feedback-request", seq),
            reject_reason: Some("stale_base".into()),
            provenance: "runtime_authoritative".into(),
        };
        world
            .enqueue_runtime_feedback(crate::simulator::agent_api_compat::feedback_to_api(
                &feedback,
            ))
            .unwrap();
        world.claim_runtime_feedback(&feedback.feedback_id).unwrap();
        world.ack_runtime_feedback(&feedback.feedback_id).unwrap();
    }
    let mut projection = crate::world_service::projection::WorldServiceProjection::from_world(
        &world,
        Some("agent-a"),
    )
    .unwrap();
    projection.agent_context = Some(authority);
    (world, projection, session)
}
fn registered_sidecar(
    projection: crate::world_service::projection::WorldServiceProjection,
) -> (
    RuntimeLlmSidecar,
    std::sync::Arc<std::sync::Mutex<crate::simulator::MockDecisionProviderState>>,
) {
    let provider = crate::simulator::MockDecisionProvider::new("feedback-fence-provider");
    let state = provider.shared_state();
    let behavior = crate::simulator::ProviderBackedAgentBehavior::new_legacy_compatibility(
        "agent-a",
        provider,
        vec![crate::simulator::ActionCatalogEntry::new("wait", "wait")],
    );
    let mut runner = crate::simulator::AsyncAgentRunner::with_default_capacity();
    runner.register(behavior).unwrap();
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar.runner = Some(RuntimeDecisionRunner::ProviderBacked(runner));
    sidecar.provider_service_projection = Some(projection);
    (sidecar, state)
}
#[test]
fn actual_world_gap_blocks_capture_and_dispatch_before_model_or_economic_checkpoint() {
    let (_world, projection, session) = actual_world_history(&[1, 3]);
    let (mut sidecar, calls) = registered_sidecar(projection);
    let error = sidecar.capture_hosted_admission().unwrap_err();
    assert!(error.contains("feedback history fences"), "{error}");
    let mut context =
        super::super::super::tests::test_provider_context("agent-a", "turn-4", "request-4", 1);
    context.request_context.agent_session_id = session.clone();
    context.turn_context.agent_session_id = session;
    sidecar.hosted_admission = Some(HostedAdmission {
        context,
        observation: Observation {
            time: 0,
            agent_id: "agent-a".into(),
            pos: crate::geometry::GeoPos::new(0, 0, 0),
            self_resources: Default::default(),
            visibility_range_cm: 0,
            visible_agents: vec![],
            visible_locations: vec![],
            module_lifecycle: Default::default(),
            module_market: Default::default(),
            power_market: Default::default(),
            social_state: Default::default(),
        },
        metadata_identity: "unused-before-feedback-fence".into(),
        stage: "dispatch".into(),
        commit: None,
        lease: None,
    });
    assert!(
        sidecar
            .dispatch_hosted_admission()
            .unwrap_err()
            .contains("feedback history fences")
    );
    assert!(
        calls.lock().unwrap().recorded_requests.is_empty(),
        "actual registered provider must not be invoked"
    );
    assert!(sidecar.provider_scheduler_pending.is_empty());
    assert!(sidecar.provider_active_turns.is_empty());
    assert!(sidecar.provider_cognition_leases.is_empty());
}

#[test]
fn service_test_queue_blocks_restored_session_fence_before_actor_or_lease_registration() {
    let (mut world, projection, session) = actual_world_history(&[1, 3]);
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar.provider_service_projection = Some(projection);
    sidecar
        .sync_shadow_kernel(&world, &WorldConfig::default())
        .expect("sync authoritative test observation");
    let action = SimulatorAction::MoveAgent {
        agent_id: "agent-a".into(),
        to: "fixture-location".into(),
    };
    let mut request =
        super::super::super::tests::test_provider_context("agent-a", "turn-4", "request-4", 1);
    request.request_context.agent_session_id = session.clone();
    request.turn_context.agent_session_id = session.clone();
    let base_decision_response =
        crate::simulator::DecisionResponse::wait("canonical-test-provider");
    let response = ContinuousAgentResponseContextV1 {
        response_digest: crate::simulator::cognition_response_digest(&base_decision_response),
        base_decision_response,
        context_discriminator: CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR.into(),
        context_version: CONTINUOUS_AGENT_CONTEXT_VERSION,
        agent_session_id: session.clone(),
        agent_turn_id: request.request_context.agent_turn_id.clone(),
        decision_request_id: request.request_context.decision_request_id.clone(),
        retry_seq: request.request_context.retry_seq,
        transport_attempt: request.request_context.transport_attempt,
        request_digest: request.request_context.request_digest.clone(),
    };
    let cognition = RuntimeProviderActionContext {
        request,
        response,
        cognition_lease: None,
        memory_write_intents: Vec::new(),
    };
    assert_eq!(
        cognition.request.request_context.agent_session_id, session,
        "the request must target the identity-bearing fenced session"
    );

    let error = sidecar
        .queue_service_test_response(&mut world, cognition, action)
        .expect_err("a successfully restored history can still fence this session");
    assert!(error.contains("feedback history fences"), "{error}");

    let runner = sidecar
        .runner
        .as_ref()
        .and_then(RuntimeDecisionRunner::async_runner)
        .expect("retain the restored verifier and its fence");
    assert!(runner.runtime_feedback_history_loaded());
    assert!(crate::viewer::runtime_live::llm_sidecar_feedback_recovery::
        feedback_history_blocks_dispatch(runner, "agent-a", &session));
    assert!(
        !runner.has_agent("agent-a"),
        "the fenced service test actor must not be registered"
    );
    assert!(!sidecar.service_test_actor_agents.contains("agent-a"));
    assert!(!sidecar.service_test_model_calls.contains_key("agent-a"));
    assert!(sidecar.provider_agent_ids.is_empty());
    assert!(sidecar.provider_active_turns.is_empty());
    assert!(sidecar.provider_contexts.is_empty());
    assert!(sidecar.provider_cognition_leases.is_empty());
    assert!(sidecar.provider_scheduler_pending.is_empty());
    assert_eq!(sidecar.llm_decision_mailbox, 0);
    assert!(world.cognition_economy().unwrap().leases.is_empty());
}

#[test]
fn authenticated_history_cannot_reset_initialized_native_verifier_ahead_of_runtime() {
    let (world, projection, session) = actual_world_history(&[1, 2]);
    let records = world.runtime_feedback_outbox().unwrap();
    let (mut sidecar, calls) = registered_sidecar(projection);
    sidecar
        .runner
        .as_mut()
        .unwrap()
        .async_runner_mut()
        .unwrap()
        .restore_runtime_feedback_outbox(&records)
        .unwrap();
    let (_older, projection, _) = actual_world_history(&[1]);
    sidecar.provider_service_projection = Some(projection);
    let error = sidecar
        .check_scoped_feedback_fresh_admission("agent-a", &session)
        .unwrap_err();
    assert!(
        error.contains("live feedback verifier conflicts"),
        "{error}"
    );
    assert!(calls.lock().unwrap().recorded_requests.is_empty());
}

#[test]
fn derived_session_suffix_without_signed_published_resume_is_rejected() {
    let (_world, projection, session) = actual_world_history(&[]);
    let (mut sidecar, calls) = registered_sidecar(projection);
    let error = sidecar
        .check_scoped_feedback_fresh_admission("agent-a", &format!("{session}-resume-1"))
        .unwrap_err();
    assert!(error.contains("original admission"), "{error}");
    assert!(sidecar.provider_scheduler_pending.is_empty());
    assert!(sidecar.provider_active_turns.is_empty());
    assert!(calls.lock().unwrap().recorded_requests.is_empty());
}

#[test]
fn resumed_prepared_turn_cannot_disagree_with_signed_request_identity() {
    let original =
        super::super::super::tests::test_provider_context("agent-a", "turn-4", "request-4", 1);
    RuntimeLlmSidecar::validate_resumed_turn_identity(&original).unwrap();
    for field in 0..3 {
        let mut changed = original.clone();
        match field {
            0 => {
                changed.turn_context.request_digest =
                    crate::simulator::h_v1("tampered-turn", &field)
            }
            1 => changed.turn_context.agent_turn_id.push_str("-tampered"),
            _ => changed
                .turn_context
                .decision_request_id
                .push_str("-tampered"),
        }
        let error = RuntimeLlmSidecar::validate_resumed_turn_identity(&changed).unwrap_err();
        assert!(error.contains("turn/request identity mismatch"), "{error}");
        // Copying a changed unsigned context to a second checkpoint cannot
        // make its identity agree with the immutable signed request.
        assert!(RuntimeLlmSidecar::validate_resumed_turn_identity(&changed.clone()).is_err());
    }
}
