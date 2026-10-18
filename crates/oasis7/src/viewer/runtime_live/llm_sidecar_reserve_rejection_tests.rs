//! Admission callbacks operate only on responses already authenticated by the client.
use super::*;

#[test]
fn verified_pre_effect_reserve_rejection_retires_original_without_replacing_checkpoint() {
    let config =
        crate::viewer::ViewerRuntimeLiveServerConfig::new(oasis7_simulator::WorldScenario::Minimal);
    let mut server = crate::viewer::ViewerRuntimeLiveServer::new(config).unwrap();
    let context =
        crate::viewer::runtime_live::control_plane::llm_sidecar::tests::test_provider_context(
            "agent-a",
            "turn-a",
            "request-a",
            1,
        );
    let mut context = context;
    let request = &mut context.request_context;
    request.observation_digest = crate::simulator::h_v1(
        "oasis7.cognition.test.observation.v1",
        &request.base_decision_request.observation,
    );
    request.capability_catalog_digest =
        crate::simulator::h_v1("oasis7.cognition.test.catalog.v1", &Option::<()>::None);
    request.capability_invocation_context_digest =
        crate::simulator::h_v1("oasis7.cognition.test.invocation.v1", &Option::<()>::None);
    request.memory_snapshot_digest = crate::simulator::h_v1(
        "oasis7.cognition.test.memory.v1",
        &context.turn_context.memory_snapshot,
    );
    request.goal_snapshot_digest = crate::simulator::h_v1(
        "oasis7.cognition.test.goal.v1",
        &context.turn_context.goal_snapshot,
    );
    request.continuation_digest =
        crate::simulator::h_v1("oasis7.cognition.continuation.v1", &serde_json::Value::Null);
    request.runtime_binding.base_world_hash =
        crate::simulator::h_v1("oasis7.cognition.test.world.v1", &"checkpoint");
    request.runtime_binding.runtime_manifest_hash =
        crate::simulator::h_v1("oasis7.cognition.test.manifest.v1", &"checkpoint");
    request.request_digest = request.request_digest();
    context.turn_context.request_digest = request.request_digest.clone();
    let request = &context.request_context;
    let world_identity = WorldIdentity {
        world_id: request.runtime_binding.world_id.clone(),
        genesis_digest: "fixture-genesis".into(),
    };
    let service_config = crate::world_service::client::WorldServiceClientConfig {
        formal_read_trust: None,
        endpoint: "http://127.0.0.1:1".into(),
        trusted_service_public_key: "11".repeat(32),
        expected_world: world_identity.clone(),
        scope_id: "agent:agent-a".into(),
        read_private_key_hex: "11".repeat(32),
        timeout: std::time::Duration::from_secs(2),
        max_response_bytes: 4096,
    };
    let signer = crate::world_service::client::WorldServiceAgentSignerConfig {
        private_key_hex: "11".repeat(32),
        delegation_generation: 1,
    };
    server.config.world_service = Some(service_config.clone());
    server.config.world_service_agent_signer = Some(signer.clone());
    server.llm_sidecar.provider_service_config = Some(service_config);
    server.llm_sidecar.provider_service_signer = Some(signer);
    let path = std::env::temp_dir().join(format!(
        "pre2-reserve-rejection-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    server.llm_sidecar.provider_lineage_store = Some(path.clone());
    let operation = crate::world_service::wire::SchedulerOperationV1::ReserveLease(
        crate::runtime::CognitionLeaseRequestV1::new(
            request.provider_invocation_key().to_string(),
            "owner-a",
            "agent-a",
            request.agent_session_id.clone(),
            request.agent_turn_id.clone(),
            request.decision_request_id.clone(),
            request.request_digest.to_string(),
            crate::runtime::CognitionLeaseQuoteV1::new("reserve", "electricity", 1),
        ),
    );
    let (pending, _) = server
        .llm_sidecar
        .prepare_service_scheduler_checkpoint(request, "reserve", operation, None)
        .unwrap();
    let original_bytes = serde_json::to_vec(&pending).unwrap();
    server.llm_sidecar.hosted_admission = Some(HostedAdmission {
        context: context.clone(),
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
        metadata_identity: "unchanged".into(),
        stage: "reserve".into(),
        commit: None,
        lease: None,
    });
    let token = AgentServiceIoToken {
        generation: 1,
        config_digest: server.hosted_service_config_digest().unwrap(),
        phase_digest: "admission:reserve".into(),
    };
    server.llm_sidecar.hosted_service_inflight = Some(token.clone());
    let response = IntentResponse {
        formal_profile: None,
        execution_evidence_scope: None,
        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
        correlation: pending.correlation.clone(),
        outcome: IntentOutcome::Rejected {
            reason: WorldServiceErrorKind::Conflict,
        },
    };
    // Deny mismatched authenticated results and any evidence of dispatched effects.
    let before = serde_json::to_vec(&server.llm_sidecar.hosted_admission).unwrap();
    let mut wrong = response.clone();
    wrong.correlation.payload_digest.push_str("-changed");
    assert!(server.llm_sidecar.retire_rejected_reserve(&wrong).is_err());
    server
        .llm_sidecar
        .provider_active_turns
        .insert("agent-a".into(), context.clone());
    assert!(
        server
            .llm_sidecar
            .retire_rejected_reserve(&response)
            .is_err()
    );
    server.llm_sidecar.provider_active_turns.remove("agent-a");
    assert_eq!(
        serde_json::to_vec(&server.llm_sidecar.hosted_admission).unwrap(),
        before
    );
    // Even an untrusted continuation marker is sufficient to deny this
    // fresh-only recovery; it cannot stand in for canonical Resume cleanup.
    let proposal: crate::simulator::ContinuationProposalV1 = serde_json::from_value(serde_json::json!({
        "schema_version": 1, "continuation_proposal_id": "retained-resume", "world_id": request.runtime_binding.world_id,
        "agent_id": "agent-a", "agent_session_id": request.agent_session_id, "agent_turn_id": request.agent_turn_id,
        "decision_request_id": request.decision_request_id, "origin_turn_id": request.agent_turn_id,
        "origin_request_digest": request.request_digest.to_string(), "action_or_plan_kind": "wait",
        "remaining_budget": {"unit": "ticks", "value": 1}, "baseline_observation_digest": request.observation_digest.to_string(),
        "goal_digest": request.goal_snapshot_digest.to_string(), "policy_digest": request.goal_snapshot_digest.to_string(),
        "policy_revision": 1, "precondition_summary": "retained Resume context", "precondition_digest": request.observation_digest.to_string(),
        "wake_conditions": [], "source": "provider_wait", "proposal_digest": request.continuation_digest.to_string()
    })).unwrap();
    server
        .llm_sidecar
        .hosted_admission
        .as_mut()
        .unwrap()
        .context
        .turn_context
        .continuation = Some(proposal);
    assert!(
        server
            .llm_sidecar
            .retire_rejected_reserve(&response)
            .is_err()
    );
    server
        .llm_sidecar
        .hosted_admission
        .as_mut()
        .unwrap()
        .context
        .turn_context
        .continuation = None;
    assert_eq!(
        serde_json::to_vec(&server.llm_sidecar.hosted_admission).unwrap(),
        before
    );
    // Deleting unsigned continuation does not make a signed resumed request
    // fresh: the complete request digest and canonical None digest still bind it.
    server
        .llm_sidecar
        .hosted_admission
        .as_mut()
        .unwrap()
        .context
        .request_context
        .continuation_digest = crate::simulator::h_v1("retained-resume", &1);
    {
        let retained = &mut server
            .llm_sidecar
            .hosted_admission
            .as_mut()
            .unwrap()
            .context;
        retained.request_context.request_digest = retained.request_context.request_digest();
        retained.turn_context.request_digest = retained.request_context.request_digest.clone();
        retained.request_context.validate().unwrap();
    }
    assert!(
        server
            .llm_sidecar
            .retire_rejected_reserve(&response)
            .unwrap_err()
            .contains("authenticated fresh turn")
    );
    server
        .llm_sidecar
        .hosted_admission
        .as_mut()
        .unwrap()
        .context = context.clone();
    for changed in 0..2 {
        let turn = &mut server
            .llm_sidecar
            .hosted_admission
            .as_mut()
            .unwrap()
            .context
            .turn_context;
        if changed == 0 {
            turn.agent_id.push_str("-changed");
        } else {
            turn.agent_session_id.push_str("-changed");
        }
        assert!(
            server
                .llm_sidecar
                .retire_rejected_reserve(&response)
                .is_err()
        );
        server
            .llm_sidecar
            .hosted_admission
            .as_mut()
            .unwrap()
            .context = context.clone();
    }
    assert_eq!(
        serde_json::to_vec(&server.llm_sidecar.hosted_admission).unwrap(),
        before
    );
    // An actual obstructed persistence target must restore all staged local fields.
    let blocked = path.with_extension("blocked");
    std::fs::create_dir(&blocked).unwrap();
    server.llm_sidecar.provider_lineage_store = Some(blocked.clone());
    server.llm_sidecar.hosted_fresh_view_ready = true;
    assert!(
        server
            .llm_sidecar
            .retire_rejected_reserve(&response)
            .is_err()
    );
    assert_eq!(
        serde_json::to_vec(&server.llm_sidecar.hosted_admission).unwrap(),
        before
    );
    assert!(server.llm_sidecar.hosted_fresh_view_ready);
    assert!(server.llm_sidecar.provider_terminal_states.is_empty());
    assert_eq!(
        serde_json::to_vec(
            &server.llm_sidecar.provider_scheduler_pending
                [&format!("{}:reserve", request.provider_invocation_key())]
        )
        .unwrap(),
        original_bytes
    );
    std::fs::remove_dir(blocked).unwrap();
    server.llm_sidecar.provider_lineage_store = Some(path.clone());
    assert!(
        server
            .apply_fresh_service_io(AgentServiceIoResult {
                token,
                response: Ok(AgentServiceIoResponse::Intent(response))
            })
            .is_ok()
    );
    assert!(server.llm_sidecar.hosted_admission.is_none());
    let terminal = &server.llm_sidecar.provider_terminal_states["agent-a"];
    assert_eq!(terminal.agent_turn_id, request.agent_turn_id);
    assert_eq!(terminal.request_digest, request.request_digest.to_string());
    assert_eq!(terminal.status, "rejected");
    let retained = &server.llm_sidecar.provider_scheduler_pending
        [&format!("{}:reserve", request.provider_invocation_key())];
    assert_eq!(serde_json::to_vec(retained).unwrap(), original_bytes);
    let persisted: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(persisted["hosted_admission"].is_null());
    assert_eq!(
        persisted["provider_terminal_states"]["agent-a"]["request_digest"],
        request.request_digest.to_string()
    );
    // Pause/restart restoration may retain the rejected original but must
    // never scan it into a fresh admission or make a network/model request.
    let mut restored = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restored.provider_lineage_store = Some(path.clone());
    restored.restore_provider_lineage(&server.world).unwrap();
    assert!(restored.hosted_admission.is_none());
    assert!(restored.provider_active_turns.is_empty());
    assert_eq!(
        restored.provider_terminal_states["agent-a"].request_digest,
        request.request_digest.to_string()
    );
    assert_eq!(
        serde_json::to_vec(
            &restored.provider_scheduler_pending
                [&format!("{}:reserve", request.provider_invocation_key())]
        )
        .unwrap(),
        original_bytes
    );
    std::fs::remove_file(path).unwrap();
}
