use super::*;

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn hosted_outcome_sync_checkpoint_failure_retries_original_before_exposing_decision() {
    let agent = "agent-sync-outcome";
    let mut world = lineage_recovery_tests::bound_provider_lease_test_world(&[agent]);
    let cognition = queued_wait_cognition(&mut world, agent);
    let original_lease = cognition.cognition_lease.clone();
    let root = std::env::temp_dir().join(format!(
        "hosted-outcome-sync-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("lineage.json");
    std::fs::create_dir(&path).unwrap(); // deterministic synchronous rename failure
    let mut owner = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    owner.configure_provider_lineage_store(&path);
    let provider = crate::simulator::MockDecisionProvider::new("sync-outcome");
    let calls = provider.shared_state();
    let mut runner = crate::simulator::AsyncAgentRunner::with_default_capacity();
    runner
        .register(
            crate::simulator::ProviderBackedAgentBehavior::new_legacy_compatibility(
                agent,
                provider,
                vec![crate::simulator::ActionCatalogEntry::new("wait", "wait")],
            ),
        )
        .unwrap();
    owner.runner = Some(RuntimeDecisionRunner::ProviderBacked(runner));
    owner.test_retain_hosted_outcome(async_support::RuntimeLlmDecision {
        agent_id: agent.into(),
        decision: AgentDecision::Wait,
        decision_trace: None,
        cognition: Some(cognition),
        memory_write_intents: Vec::new(),
        continuation_admitted: false,
    });
    let original =
        serde_json::to_value(owner.provider_completed_decisions.front().unwrap()).unwrap();
    for _ in 0..2 {
        assert!(
            owner.poll_hosted_provider_decision(&world).is_err(),
            "an empty subsequent poll must not expose the unconfirmed original"
        );
        assert!(owner.hosted_outcome_checkpoint_pending);
        assert_eq!(
            serde_json::to_value(owner.provider_completed_decisions.front().unwrap()).unwrap(),
            original
        );
        assert!(calls.lock().unwrap().recorded_requests.is_empty());
    }
    std::fs::remove_dir(&path).unwrap();
    let recovered = owner
        .poll_hosted_provider_decision(&world)
        .unwrap()
        .unwrap();
    assert_eq!(serde_json::to_value(&recovered).unwrap(), original);
    assert_eq!(recovered.cognition.unwrap().cognition_lease, original_lease);
    assert!(!owner.hosted_outcome_checkpoint_pending);
    let checkpoint: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(checkpoint["provider_completed_decisions"][0], original);
    assert!(calls.lock().unwrap().recorded_requests.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(not(target_arch = "wasm32"))]
pub(in super::super) fn queued_wait_cognition(
    world: &mut RuntimeWorld,
    agent_id: &str,
) -> crate::viewer::runtime_live::control_plane::llm_sidecar::RuntimeProviderActionContext {
    queued_wait_cognition_with_identity(world, agent_id, "turn-queued", "request-queued")
}

#[cfg(not(target_arch = "wasm32"))]
fn queued_wait_cognition_with_identity(
    world: &mut RuntimeWorld,
    agent_id: &str,
    turn_id: &str,
    request_id: &str,
) -> crate::viewer::runtime_live::control_plane::llm_sidecar::RuntimeProviderActionContext {
    let mut context =
        lineage_recovery_tests::valid_test_provider_context(world, agent_id, turn_id, request_id);
    let request = &mut context.request_context;
    request.observation_digest = crate::simulator::h_v1(
        "oasis7.cognition.observation.v1",
        &request.base_decision_request.observation,
    );
    request.memory_snapshot_digest = context.turn_context.memory_snapshot.digest.clone().into();
    request.goal_snapshot_digest = context.turn_context.goal_snapshot.digest.clone().into();
    request.continuation_digest = crate::simulator::h_v1(
        "oasis7.cognition.continuation.v1",
        &context.turn_context.continuation,
    );
    request.request_digest = request.request_digest();
    context.turn_context.request_digest = request.request_digest.clone();
    request
        .validate_production_lane()
        .expect("Runtime-bound Wait request");
    let lease = async_support::reserve_provider_cognition_lease(world, &context)
        .expect("reserve original Runtime Wait lease");
    let response = crate::simulator::DecisionResponse::wait("queued-completion-provider");
    crate::viewer::runtime_live::control_plane::llm_sidecar::RuntimeProviderActionContext {
        request: context.clone(),
        response: crate::simulator::ContinuousAgentResponseContextV1 {
            response_digest: crate::simulator::cognition_response_digest(&response),
            base_decision_response: response,
            context_discriminator: crate::simulator::CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR.into(),
            context_version: crate::simulator::CONTINUOUS_AGENT_CONTEXT_VERSION,
            agent_session_id: context.request_context.agent_session_id.clone(),
            agent_turn_id: context.request_context.agent_turn_id.clone(),
            decision_request_id: context.request_context.decision_request_id.clone(),
            retry_seq: context.request_context.retry_seq,
            transport_attempt: context.request_context.transport_attempt,
            request_digest: context.request_context.request_digest.clone(),
        },
        cognition_lease: Some(lease),
        memory_write_intents: Vec::new(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn assert_queued_provider_decision_survives_restore(decision: AgentDecision) {
    let decision_kind = match &decision {
        AgentDecision::Act(_) => "action",
        AgentDecision::Wait => "wait",
        _ => "other",
    };
    let path = std::env::temp_dir().join(format!(
        "oasis7-viewer-provider-lineage-queued-completion-{}-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos(),
        decision_kind
    ));
    let agent_id = "agent-queued";
    let mut recovery_world = lineage_recovery_tests::bound_provider_lease_test_world(&[agent_id]);
    let mut context = lineage_recovery_tests::valid_test_provider_context(
        &recovery_world,
        agent_id,
        "turn-queued",
        "request-queued",
    );
    let cognition = if matches!(decision, AgentDecision::Wait) {
        let cognition = queued_wait_cognition(&mut recovery_world, agent_id);
        context = cognition.request.clone();
        Some(cognition)
    } else {
        None
    };
    let mut first = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    first.configure_provider_lineage_store(path.clone());
    first.provider_agent_ids.insert(agent_id.to_string());
    first
        .provider_contexts
        .insert(agent_id.to_string(), context.clone());
    first
        .provider_active_turns
        .insert(agent_id.to_string(), context.clone());
    first
        .provider_completed_decisions
        .push_back(async_support::RuntimeLlmDecision {
            agent_id: agent_id.to_string(),
            decision: decision.clone(),
            decision_trace: None,
            cognition,
            memory_write_intents: Vec::new(),
            continuation_admitted: false,
        });
    first
        .persist_provider_lineage()
        .expect("persist queued completed provider decision");
    if matches!(decision, AgentDecision::Wait) {
        let original = std::fs::read(&path).expect("read original Wait checkpoint");
        let mut missing: serde_json::Value = serde_json::from_slice(&original).unwrap();
        missing["provider_completed_decisions"][0]["cognition"] = serde_json::Value::Null;
        std::fs::write(&path, serde_json::to_vec(&missing).unwrap()).unwrap();
        let mut invalid = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
        invalid.configure_provider_lineage_store(path.clone());
        assert!(
            invalid.restore_provider_lineage(&recovery_world).is_err(),
            "queued Wait without original cognition must fail closed"
        );
        std::fs::write(&path, original).expect("restore authentic Wait checkpoint");
    }

    let mut restored = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restored.configure_provider_lineage_store(path.clone());
    restored
        .restore_provider_lineage(&recovery_world)
        .expect("restore queued completed provider decision");
    assert!(
        !restored.provider_transport_exhausted.contains(agent_id),
        "queued completion must retain the active identity during recovery"
    );

    let provider = crate::simulator::MockDecisionProvider::new("queued-completion-provider");
    let provider_state = provider.shared_state();
    let behavior = crate::simulator::ProviderBackedAgentBehavior::new_legacy_compatibility(
        agent_id,
        provider,
        vec![crate::simulator::ActionCatalogEntry::new("wait", "wait")],
    );
    let mut runner = crate::simulator::AsyncAgentRunner::with_default_capacity();
    runner.register(behavior).expect("register provider actor");
    restored.runner = Some(RuntimeDecisionRunner::ProviderBacked(runner));
    let mut world = recovery_world;
    let mut kernel = WorldKernel::new();

    let delivered = restored
        .next_async_provider_decision(&mut world, &mut kernel, "world")
        .expect("queued completion must be delivered after restore");
    assert_eq!(delivered.agent_id, agent_id);
    assert_eq!(delivered.decision, decision);
    assert!(restored.provider_held_decisions.contains_key(agent_id));
    assert!(
        restored.provider_completed_decisions.is_empty(),
        "a restored completion must be removed from the queue after one delivery"
    );
    assert!(
        provider_state
            .lock()
            .expect("provider state lock")
            .recorded_requests
            .is_empty(),
        "replaying a queued completion must not readmit the provider"
    );
    let _ = std::fs::remove_file(path);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn provider_lineage_restore_delivers_queued_action_once_without_readmission() {
    assert_queued_provider_decision_survives_restore(AgentDecision::Act(
        oasis7_simulator::Action::MoveAgent {
            agent_id: "agent-queued".to_string(),
            to: "loc-queued".to_string(),
        },
    ));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn provider_lineage_restore_delivers_queued_wait_once_without_readmission() {
    assert_queued_provider_decision_survives_restore(AgentDecision::Wait);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn hosted_raw_completion_error_preserves_both_actual_actor_originals_without_readmission() {
    assert_hosted_raw_completion_escrow(false, false);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn hosted_raw_completion_checkpoint_failure_preserves_actual_batch_and_reopen() {
    assert_hosted_raw_completion_escrow(true, false);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn hosted_known_provider_failure_retains_original_and_uses_canonical_reconciliation() {
    assert_hosted_raw_completion_escrow(false, true);
}

#[cfg(not(target_arch = "wasm32"))]
fn assert_hosted_raw_completion_escrow(fail_checkpoint: bool, known_failure: bool) {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct Behavior {
        agent: String,
        response: Option<crate::simulator::ContinuousAgentResponseContextV1>,
        calls: Arc<AtomicUsize>,
        known_failure: bool,
    }
    impl crate::simulator::AgentBehavior for Behavior {
        fn agent_id(&self) -> &str {
            &self.agent
        }
        fn decide(&mut self, _: &crate::simulator::Observation) -> AgentDecision {
            self.calls.fetch_add(1, Ordering::SeqCst);
            AgentDecision::Wait
        }
        fn take_decision_trace(&mut self) -> Option<crate::simulator::AgentDecisionTrace> {
            self.known_failure.then(|| serde_json::from_value(serde_json::json!({
                "agent_id": self.agent, "time": 0, "decision": AgentDecision::Wait,
                "llm_input": null, "llm_output": null,
                "llm_error": "provider_unavailable: deterministic actor failure", "parse_error": null
            })).unwrap())
        }
        fn take_continuous_response_context(
            &mut self,
        ) -> Option<crate::simulator::ContinuousAgentResponseContextV1> {
            self.response.take()
        }
    }
    fn contains(value: &serde_json::Value, expected: &serde_json::Value) -> bool {
        value == expected
            || match value {
                serde_json::Value::Array(values) => values.iter().any(|v| contains(v, expected)),
                serde_json::Value::Object(values) => values.values().any(|v| contains(v, expected)),
                _ => false,
            }
    }
    let agents = ["raw-failed", "raw-valid"];
    let mut world = lineage_recovery_tests::bound_provider_lease_test_world(&agents);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut runner = crate::simulator::AsyncAgentRunner::with_default_capacity();
    let mut owner = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    static NEXT_ROOT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = loop {
        let sequence = NEXT_ROOT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let candidate = std::env::temp_dir().join(format!(
            "hosted-raw-completion-{}-{}-{sequence}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        match std::fs::create_dir(&candidate) {
            Ok(()) => break candidate,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("cannot allocate isolated raw completion fixture: {error}"),
        }
    };
    let path = root.join("lineage.json");
    owner.configure_provider_lineage_store(&path);
    for (index, agent) in agents.iter().enumerate() {
        let cognition = queued_wait_cognition_with_identity(
            &mut world,
            agent,
            &format!("turn-{agent}"),
            &format!("request-{agent}"),
        );
        runner
            .register(Behavior {
                agent: (*agent).into(),
                response: (index == 1).then(|| cognition.response.clone()),
                calls: calls.clone(),
                known_failure: index == 0 && known_failure,
            })
            .unwrap();
        crate::viewer::runtime_live::llm_sidecar_feedback_recovery::restore_agent_feedback_history(
            &world,
            &mut runner,
        )
        .expect("restore actual Runtime-owned feedback history before actor admission");
        let request = cognition.request.clone();
        owner
            .sync_shadow_kernel(&world, &WorldConfig::default())
            .unwrap();
        let observation = owner
            .shadow_kernel
            .as_mut()
            .unwrap()
            .observe(agent)
            .unwrap();
        // The production dispatcher synchronizes the runner clock before supplying
        // a Runtime-issued lease. Keep this fixture on the same actual world tick.
        runner.sync_logical_tick(world.state().time);
        runner
            .start_turn_with_request_context_and_observation_and_lease(
                agent,
                observation,
                request.turn_context.clone(),
                request.request_context.clone(),
                cognition.cognition_lease.clone().unwrap(),
            )
            .unwrap();
        owner.provider_agent_ids.insert((*agent).into());
        owner
            .provider_contexts
            .insert((*agent).into(), request.clone());
        owner.provider_active_turns.insert((*agent).into(), request);
    }
    owner.runner = Some(RuntimeDecisionRunner::ProviderBacked(runner));
    owner.persist_provider_lineage().unwrap();
    if fail_checkpoint {
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut saw_error = false;
    loop {
        saw_error |= owner.poll_hosted_provider_decision(&world).is_err();
        let runner = owner.runner.as_mut().unwrap().async_runner_mut().unwrap();
        if runner.awaiting_runtime_outcomes().count() == 2 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "both real actor completions must arrive"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        saw_error,
        "missing response must reject rather than invent Wait response"
    );
    if fail_checkpoint {
        assert!(owner.hosted_outcome_checkpoint_pending);
        std::fs::remove_dir(&path).unwrap();
    }

    let originals = owner
        .runner
        .as_mut()
        .unwrap()
        .async_runner_mut()
        .unwrap()
        .awaiting_runtime_outcomes()
        .map(|v| serde_json::to_value(v).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(originals.len(), 2);
    for _ in 0..2 {
        let _ = owner.poll_hosted_provider_decision(&world);
        let durable: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_raw_schema_compatibility(&durable);
        for original in &originals {
            assert!(
                contains(&durable, original),
                "every raw actor original must be durable before acknowledgement, including the failed original and later valid completion"
            );
        }
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "recovery cannot redispatch either model turn"
        );
    }
    let mut restored = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restored.configure_provider_lineage_store(&path);
    restored.restore_provider_lineage(&world).unwrap();
    assert_eq!(restored.hosted_raw_outcomes().unwrap().len(), 2);
    let recovered = restored
        .poll_hosted_provider_decision(&world)
        .unwrap()
        .unwrap();
    assert_eq!(recovered.agent_id, "raw-valid");
    let expected_reason = if known_failure {
        "executor_failed_after_dispatch:known_provider_failure"
    } else {
        "actor_outcome_validation_pending"
    };
    assert!(
        restored.provider_recovery_pending["raw-failed"]
            .reason
            .starts_with(expected_reason)
    );
    restored.begin_hosted_orphan_recovery();
    assert_eq!(
        restored.hosted_service_phase.is_some(),
        known_failure,
        "only a known provider failure may enter canonical failure/settle reconciliation; missing artifact alone remains pending"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "reopen cannot dispatch a new model call"
    );
    if known_failure {
        // This isolates the post-canonical feedback publication gate. The
        // canonical failure/settled-view checks stay in the service caller.
        use crate::viewer::runtime_live::control_plane::llm_sidecar::lineage_persistence::checkpoint_write::DeferredPublications;
        let capture = DeferredPublications::begin(&owner).unwrap();
        let feedback = owner
            .fail_provider_turn_with_feedback_after_publication("raw-failed")
            .unwrap()
            .unwrap();
        let ticket = owner.publication_ticket();
        let _ = capture.finish();
        assert_ne!(ticket, 0);
        assert_eq!(
            owner
                .runner
                .as_mut()
                .unwrap()
                .async_runner_mut()
                .unwrap()
                .awaiting_runtime_outcomes()
                .count(),
            2,
            "capturing a failed feedback cannot acknowledge its live actor"
        );
        for publication in owner.take_queued_publications() {
            publication.execute().unwrap();
        }
        assert!(
            !owner.publication_ticket_confirmed(ticket),
            "actual write without matching ACK is insufficient"
        );
        assert!(owner.confirm_lineage_publication(ticket + 1).is_err());
        owner.confirm_lineage_publication(ticket).unwrap();
        assert!(owner.publication_ticket_confirmed(ticket));
        let mut changed = feedback.clone();
        changed.reject_reason = Some("different_payload_same_id".into());
        assert!(!owner.feedback_publication_confirmed(&changed));
        let mut other_owner = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
        other_owner.hosted_terminal_feedback_publications =
            std::mem::take(&mut owner.hosted_terminal_feedback_publications);
        assert!(
            !other_owner.feedback_publication_confirmed(&feedback),
            "receipt cannot cross publication owner instances"
        );
        owner.hosted_terminal_feedback_publications =
            std::mem::take(&mut other_owner.hosted_terminal_feedback_publications);
        // A later unrelated ACK is not an exact receipt for this feedback.
        let capture = DeferredPublications::begin(&owner).unwrap();
        owner.persist_provider_lineage().unwrap();
        let later_ticket = owner.publication_ticket();
        let _ = capture.finish();
        for publication in owner.take_queued_publications() {
            publication.execute().unwrap();
        }
        owner.confirm_lineage_publication(later_ticket).unwrap();
        assert!(!owner.feedback_publication_confirmed(&feedback));
        let capture = DeferredPublications::begin(&owner).unwrap();
        let restaged = owner
            .fail_provider_turn_with_feedback_after_publication("raw-failed")
            .unwrap()
            .unwrap();
        assert_eq!(restaged, feedback);
        let exact_ticket = owner.publication_ticket();
        let _ = capture.finish();
        assert_eq!(
            owner
                .runner
                .as_mut()
                .unwrap()
                .async_runner_mut()
                .unwrap()
                .awaiting_runtime_outcomes()
                .count(),
            2
        );
        for publication in owner.take_queued_publications() {
            publication.execute().unwrap();
        }
        owner.confirm_lineage_publication(exact_ticket).unwrap();
        assert!(owner.feedback_publication_confirmed(&feedback));
        // Cut after the feedback became durable but before sidecar release can
        // publish. Its exact original remains on disk; SDK ACK may replay safely.
        let saved = root.join("published-feedback.json");
        std::fs::rename(&path, &saved).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(
            owner
                .fail_provider_turn_with_feedback_after_publication("raw-failed")
                .is_err()
        );
        assert_eq!(
            owner
                .runner
                .as_mut()
                .unwrap()
                .async_runner_mut()
                .unwrap()
                .awaiting_runtime_outcomes()
                .count(),
            1,
            "only the exact acknowledged failed actor is released"
        );
        std::fs::remove_dir(&path).unwrap();
        std::fs::rename(&saved, &path).unwrap();
        let mut after_cut = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
        after_cut.configure_provider_lineage_store(&path);
        after_cut.restore_provider_lineage(&world).unwrap();
        assert_eq!(
            after_cut.provider_terminal_states["raw-failed"]
                .feedback
                .as_ref(),
            Some(&feedback)
        );
        assert!(!after_cut.provider_active_turns.contains_key("raw-failed"));
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "published feedback recovery cannot dispatch another model call"
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

// Frozen legacy reader boundary: versions 1/2 only, serde ignores unknown fields.
// Keep this oracle independent of the current reader's constants and raw DTO.
fn assert_raw_schema_compatibility(raw: &serde_json::Value) {
    #[derive(serde::Deserialize)]
    struct FrozenLegacyHeader {
        schema_version: u16,
    }
    fn old_accepts(value: &serde_json::Value) -> bool {
        let header: FrozenLegacyHeader = serde_json::from_value(value.clone()).unwrap();
        matches!(header.schema_version, 1 | 2)
    }
    let decode = |value: &serde_json::Value| {
        lineage_persistence::validate_test_provider_lineage_checkpoint(
            &serde_json::to_vec(value).unwrap(),
        )
    };
    assert_eq!(raw["schema_version"], 3);
    assert!(
        !old_accepts(raw),
        "legacy reader must reject raw-bearing checkpoints"
    );
    assert_eq!(raw["provider_raw_outcomes"].as_array().unwrap().len(), 2);
    decode(raw).unwrap();
    for version in [1, 2] {
        let mut downgrade = raw.clone();
        downgrade["schema_version"] = serde_json::json!(version);
        assert!(
            old_accepts(&downgrade),
            "frozen old serde silently ignores raw"
        );
        assert!(
            decode(&downgrade).is_err(),
            "new reader fences raw downgrade"
        );
        let mut legacy = downgrade;
        legacy
            .as_object_mut()
            .unwrap()
            .remove("provider_raw_outcomes");
        assert!(old_accepts(&legacy));
        assert!(
            decode(&legacy).is_ok(),
            "legacy empty escrow remains readable"
        );
    }
    for malformed in [
        serde_json::Value::Null,
        serde_json::json!([]),
        serde_json::json!({}),
    ] {
        let mut value = raw.clone();
        value["provider_raw_outcomes"] = malformed;
        assert!(decode(&value).is_err());
    }
    let mut missing = raw.clone();
    missing
        .as_object_mut()
        .unwrap()
        .remove("provider_raw_outcomes");
    assert!(decode(&missing).is_err());
    let mut duplicate = raw.clone();
    let original = duplicate["provider_raw_outcomes"][0].clone();
    duplicate["provider_raw_outcomes"]
        .as_array_mut()
        .unwrap()
        .push(original);
    assert!(decode(&duplicate).is_err());
    let mut ownerless = raw.clone();
    ownerless["provider_active_turns"] = serde_json::json!({});
    assert!(decode(&ownerless).is_err());
    let mut unknown = raw.clone();
    unknown["schema_version"] = serde_json::json!(4);
    assert!(decode(&unknown).is_err());
}
