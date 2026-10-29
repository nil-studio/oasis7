use super::*;

/// This is the restore-classification boundary, not canonical recovery proof.
/// A local journal mirror cannot replace the missing signed prefix original.
#[test]
fn hosted_restore_exposes_orphan_instead_of_silently_polling_an_empty_runner() {
    let path = std::env::temp_dir().join(format!(
        "oasis7-hosted-orphan-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut world = bound_provider_lease_test_world(&["agent-a"]);
    let context = valid_test_provider_context(&world, "agent-a", "turn-orphan", "request-orphan");
    let lease = reserve_test_provider_lease(&mut world, &context);
    // The production ProviderPrefix operation appends all three events in one
    // transaction. Reuse its local implementation rather than inventing three
    // scheduler intents. This mirror deliberately supplies no signed Lookup key.
    crate::viewer::runtime_live::control_plane::llm_sidecar::async_support::runtime_provider_prefix(
        &mut world, &context,
    ).unwrap();
    let original = serde_json::to_value(&context).unwrap();
    let mut first = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    first.configure_provider_lineage_store(path.clone());
    first.provider_service_required = true;
    first
        .provider_contexts
        .insert("agent-a".into(), context.clone());
    first
        .provider_active_turns
        .insert("agent-a".into(), context);
    first.bind_provider_cognition_lease("agent-a", lease.clone());
    first.persist_provider_lineage().unwrap();

    let mut restored = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restored.configure_provider_lineage_store(path.clone());
    restored.provider_service_required = true;
    restored.restore_provider_lineage(&world).unwrap();
    // Save the result before cleanup so a RED assertion leaves no test artifact.
    let recovery = restored.provider_recovery_pending.get("agent-a").cloned();
    let retained = serde_json::to_value(&restored.provider_active_turns["agent-a"]).unwrap();
    let retained_lease = restored.provider_cognition_lease("agent-a");
    let _ = std::fs::remove_file(path);
    assert_eq!(
        retained, original,
        "restore must preserve the exact orphan identity"
    );
    assert_eq!(
        retained_lease,
        Some(lease),
        "unknown model outcome cannot release/refund its lease"
    );
    assert!(restored.provider_completed_decisions.is_empty());
    assert!(restored.provider_service_pending.is_empty());
    assert!(
        restored.provider_scheduler_pending.is_empty(),
        "do not manufacture a signed original from journal events"
    );
    let recovery = recovery.expect(
        "hosted restore must expose durable orphan recovery; an active marker with no worker must not remain silent Idle",
    );
    assert_eq!(serde_json::to_value(recovery.active).unwrap(), original);
    assert_eq!(
        recovery.reason,
        "executor_lost_after_dispatch:model_outcome_unknown"
    );
}

fn orphan_service_fixture() -> (
    RuntimeLlmSidecar,
    RuntimeWorld,
    cognition_context::ProviderContextState,
    crate::runtime::CognitionLeaseV1,
    std::path::PathBuf,
) {
    let mut world = bound_provider_lease_test_world(&["agent-a"]);
    let context = valid_test_provider_context(&world, "agent-a", "turn-orphan", "request-orphan");
    let lease = reserve_test_provider_lease(&mut world, &context);
    crate::viewer::runtime_live::control_plane::llm_sidecar::async_support::runtime_provider_prefix(&mut world, &context).unwrap();
    let path = std::env::temp_dir().join(format!(
        "orphan-service-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar.configure_provider_lineage_store(path.clone());
    sidecar.provider_service_lineage_store_explicit = true;
    sidecar.provider_service_required = true;
    let request = &context.request_context;
    let mut view = crate::world_service::projection::from_world(&world, Some("agent-a")).unwrap();
    view.runtime_binding = Some(request.runtime_binding.clone());
    view.cognition_leases = vec![lease.clone()];
    view.agent_context = Some(crate::world_service::projection::WorldServiceAgentContext {
        agent_id: "agent-a".into(),
        capability_authorization_root: "fixture".into(),
        capability_snapshot_hash: "fixture".into(),
        authority_context_hash: "fixture".into(),
        capability_catalog: request
            .base_decision_request
            .capability_catalog
            .clone()
            .unwrap(),
        capability_invocation_context: request
            .base_decision_request
            .capability_invocation_context
            .clone()
            .unwrap(),
        payer_binding: serde_json::Value::Null,
    });
    sidecar.provider_service_projection = Some(view);
    sidecar.provider_service_config =
        Some(crate::world_service::client::WorldServiceClientConfig {
            formal_read_trust: None,
            endpoint: "http://127.0.0.1:1".into(),
            trusted_service_public_key: "11".repeat(32),
            expected_world: oasis7_client_api::world_service::WorldIdentity {
                world_id: request.runtime_binding.world_id.clone(),
                genesis_digest: "0".repeat(64),
            },
            scope_id: "agent:agent-a".into(),
            read_private_key_hex: "11".repeat(32),
            timeout: std::time::Duration::from_secs(2),
            max_response_bytes: 4096,
        });
    sidecar.provider_service_signer = Some(
        crate::world_service::client::WorldServiceAgentSignerConfig {
            private_key_hex: "11".repeat(32),
            delegation_generation: 1,
        },
    );
    sidecar
        .provider_contexts
        .insert("agent-a".into(), context.clone());
    sidecar
        .provider_active_turns
        .insert("agent-a".into(), context.clone());
    sidecar.bind_provider_cognition_lease("agent-a", lease.clone());
    let operation = crate::world_service::wire::SchedulerOperationV1::ProviderPrefix {
        request: crate::runtime_host::feedback::request_context_to_api(request),
        context_digest: crate::viewer::runtime_live::control_plane::llm_sidecar::async_support::runtime_provider_context_digest(request),
    };
    sidecar
        .prepare_service_scheduler_checkpoint(request, "prefix:1", operation, None)
        .unwrap();
    sidecar.restore_provider_lineage(&world).unwrap();
    (sidecar, world, context, lease, path)
}

#[test]
fn hosted_orphan_original_lookup_checkpoint_survives_restart_without_resigning() {
    let (sidecar, world, context, lease, path) = orphan_service_fixture();
    let id = sidecar.test_original_orphan_dispatch("agent-a").unwrap();
    let original = serde_json::to_vec(&sidecar.provider_scheduler_pending[&id]).unwrap();
    let mut restored = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restored.provider_service_required = true;
    restored.configure_provider_lineage_store(path.clone());
    restored.restore_provider_lineage(&world).unwrap();
    assert_eq!(
        serde_json::to_vec(&restored.provider_scheduler_pending[&id]).unwrap(),
        original
    );
    assert_eq!(restored.provider_cognition_lease("agent-a"), Some(lease));
    assert_eq!(
        restored.provider_active_turns["agent-a"]
            .request_context
            .provider_invocation_key(),
        context.request_context.provider_invocation_key()
    );
    assert!(
        restored.runner.is_none(),
        "recovery must not start a model runner"
    );
    assert!(restored.provider_recovery_pending.contains_key("agent-a"));
    let _ = std::fs::remove_file(path);
}

#[test]
fn hosted_orphan_settlement_readback_retains_reserved_then_allows_next_turn() {
    let (mut sidecar, mut world, context, lease, path) = orphan_service_fixture();
    let id = sidecar.test_original_orphan_dispatch("agent-a").unwrap();
    let request = &context.request_context;
    // A truthful local execution failure. Its exact repeat is idempotent and
    // never asserts anything about external provider billing or response.
    for _ in 0..2 {
        world
            .fail_cognition_turn(
                "agent-a",
                &request.agent_session_id,
                &request.agent_turn_id,
                &request.decision_request_id,
                request.request_digest.as_str(),
                "cognition_failed",
                request.retry_seq,
                request.transport_attempt,
            )
            .unwrap();
    }
    let receipt = world.settle_cognition_lease(&lease.lease_id, 1).unwrap();
    let value = serde_json::to_value(&receipt).unwrap();
    assert!(
        sidecar
            .test_orphan_settled_readback("agent-a", &id, value.clone())
            .is_err(),
        "a receipt alone cannot clear the still-Reserved trusted readback"
    );
    assert!(sidecar.provider_active_turns.contains_key("agent-a"));
    let canonical = world.cognition_economy().unwrap().leases[&lease.lease_id].clone();
    sidecar
        .provider_service_projection
        .as_mut()
        .unwrap()
        .cognition_leases = vec![canonical];
    sidecar
        .test_orphan_settled_readback("agent-a", &id, value)
        .unwrap();
    assert!(!sidecar.provider_active_turns.contains_key("agent-a"));
    assert!(!sidecar.provider_contexts.contains_key("agent-a"));
    assert!(!sidecar.provider_recovery_pending.contains_key("agent-a"));
    assert!(!sidecar.provider_scheduler_pending.contains_key(&id));
    assert!(
        sidecar.runner.is_none(),
        "original model invocation is never restarted"
    );
    let next = valid_test_provider_context(&world, "agent-a", "turn-next", "request-next");
    let next_lease = reserve_test_provider_lease(&mut world, &next);
    assert_ne!(
        next_lease.lease_id, lease.lease_id,
        "normal new-turn admission can proceed after terminal/settled recovery"
    );
    assert_eq!(receipt.consumed_amount, 1);
    assert_eq!(receipt.released_amount, 0);
    assert_eq!(receipt.refunded_amount, 0);
    let _ = std::fs::remove_file(path);
}

#[test]
fn hosted_orphan_wrong_receipt_or_current_authority_preserves_original() {
    let (mut sidecar, mut world, _context, lease, path) = orphan_service_fixture();
    let id = sidecar.test_original_orphan_dispatch("agent-a").unwrap();
    let original = serde_json::to_vec(&sidecar.provider_scheduler_pending[&id]).unwrap();
    let mut receipt = world.settle_cognition_lease(&lease.lease_id, 1).unwrap();
    receipt.agent_turn_id = "another-turn".into();
    receipt.receipt_digest = receipt.recompute_digest();
    assert!(
        sidecar
            .test_orphan_settled_readback("agent-a", &id, serde_json::to_value(receipt).unwrap())
            .is_err()
    );
    sidecar
        .provider_service_signer
        .as_mut()
        .unwrap()
        .delegation_generation += 1;
    assert!(sidecar.test_original_orphan_dispatch("agent-a").is_err());
    assert_eq!(
        serde_json::to_vec(&sidecar.provider_scheduler_pending[&id]).unwrap(),
        original
    );
    assert!(sidecar.provider_active_turns.contains_key("agent-a"));
    let _ = std::fs::remove_file(path);
}

#[test]
fn hosted_orphan_unknown_terminal_replays_same_signed_original_and_rejection_fences() {
    use crate::viewer::runtime_live::agent_service_io::{
        AgentServiceIoOperation, AgentServiceIoResponse,
    };
    use oasis7_client_api::world_service::*;
    let (sidecar, world, context, _lease, path) = orphan_service_fixture();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .unwrap();
    server.world = world;
    *server.llm_sidecar = sidecar;
    let id = server
        .llm_sidecar
        .test_original_orphan_dispatch("agent-a")
        .unwrap();
    let phase = server
        .llm_sidecar
        .test_orphan_phase("agent-a", "failure", Some(id));
    let operation = server.llm_sidecar.hosted_orphan_operation(&phase).unwrap();
    let AgentServiceIoOperation::Lookup { original, .. } = operation else {
        panic!("first terminal recovery must Lookup durable original")
    };
    let key = format!(
        "{}:failure:1",
        context.request_context.provider_invocation_key()
    );
    let correlation = server.llm_sidecar.provider_scheduler_pending[&key]
        .correlation
        .clone();
    let response = |outcome| IntentResponse {
        formal_profile: None,
        execution_evidence_scope: None,
        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
        correlation: correlation.clone(),
        outcome,
    };
    server
        .apply_hosted_orphan_response(
            phase,
            AgentServiceIoResponse::Intent(response(IntentOutcome::Unknown)),
        )
        .unwrap();
    let crate::viewer::runtime_live::agent_service_phase::HostedServicePhase::Orphan(replay) =
        server.llm_sidecar.hosted_service_phase.clone().unwrap()
    else {
        unreachable!()
    };
    let AgentServiceIoOperation::Submit(submit) =
        server.llm_sidecar.hosted_orphan_operation(&replay).unwrap()
    else {
        panic!("verified original Unknown authorizes exact replay")
    };
    assert_eq!(submit.signed_payload, original);
    assert_eq!(submit.correlation, correlation);
    let original_bytes =
        serde_json::to_vec(&server.llm_sidecar.provider_scheduler_pending[&key]).unwrap();
    // Simulate worker dispatch reservation. Config/token ownership is tested by
    // the common hosted pump; phase identity must ignore this consumed bit.
    let mut consumed = replay.clone();
    consumed.submit = false;
    assert_eq!(consumed.identity().unwrap(), replay.identity().unwrap());
    server.llm_sidecar.persist_provider_lineage().unwrap();
    server
        .llm_sidecar
        .restore_provider_lineage(&server.world)
        .unwrap();
    assert_eq!(
        serde_json::to_vec(&server.llm_sidecar.provider_scheduler_pending[&key]).unwrap(),
        original_bytes
    );
    assert!(matches!(
        server
            .llm_sidecar
            .hosted_orphan_operation(&consumed)
            .unwrap(),
        AgentServiceIoOperation::Lookup { .. }
    ));
    // Use an actual bounded rejection enum from the protocol rather than
    // changing the operation's identity to obtain another terminal attempt.
    let rejected = IntentOutcome::Rejected {
        reason: WorldServiceErrorKind::Unauthorized,
    };
    assert!(
        server
            .apply_hosted_orphan_response(
                consumed,
                AgentServiceIoResponse::Intent(response(rejected))
            )
            .is_err()
    );
    assert_eq!(
        serde_json::to_vec(&server.llm_sidecar.provider_scheduler_pending[&key]).unwrap(),
        original_bytes
    );
    assert!(
        server
            .llm_sidecar
            .provider_active_turns
            .contains_key("agent-a")
    );
    assert!(
        server.llm_sidecar.provider_recovery_pending["agent-a"]
            .reason
            .contains("canonical_rejected")
    );
    assert!(server.llm_sidecar.runner.is_none());
    let _ = std::fs::remove_file(path);
}

#[test]
fn hosted_orphan_failed_checkpoint_write_never_authorizes_undurable_submit() {
    use crate::viewer::runtime_live::agent_service_io::{
        AgentServiceIoOperation, AgentServiceIoResponse,
    };
    use oasis7_client_api::world_service::*;
    for stage in ["failure", "settle"] {
        let (sidecar, world, _context, _lease, path) = orphan_service_fixture();
        let mut server = ViewerRuntimeLiveServer::new(
            ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
                .with_decision_mode(ViewerLiveDecisionMode::Llm),
        )
        .unwrap();
        server.world = world;
        *server.llm_sidecar = sidecar;
        let id = server
            .llm_sidecar
            .test_original_orphan_dispatch("agent-a")
            .unwrap();
        let phase = server
            .llm_sidecar
            .test_orphan_phase("agent-a", stage, Some(id));
        server
            .llm_sidecar
            .install_test_provider_lineage_checkpoint_blocker()
            .unwrap();
        assert!(
            server.llm_sidecar.hosted_orphan_operation(&phase).is_err(),
            "the first operation must expose failed durable preparation"
        );
        // Same process, unchanged failing storage. A verified Unknown cannot
        // turn an in-memory-only capsule into permission to send that capsule.
        let mut submitted_undurable = false;
        match server.llm_sidecar.hosted_orphan_operation(&phase) {
            Ok(AgentServiceIoOperation::Lookup { original, .. }) => {
                let pending = server
                    .llm_sidecar
                    .provider_scheduler_pending
                    .values()
                    .find(|p| p.payload == original)
                    .unwrap();
                let response = IntentResponse {
                    formal_profile: None,
                    execution_evidence_scope: None,
                    contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                    correlation: pending.correlation.clone(),
                    outcome: IntentOutcome::Unknown,
                };
                if server
                    .apply_hosted_orphan_response(phase, AgentServiceIoResponse::Intent(response))
                    .is_ok()
                {
                    let crate::viewer::runtime_live::agent_service_phase::HostedServicePhase::Orphan(retry) = server.llm_sidecar.hosted_service_phase.clone().unwrap() else { unreachable!() };
                    submitted_undurable = matches!(
                        server.llm_sidecar.hosted_orphan_operation(&retry),
                        Ok(AgentServiceIoOperation::Submit(_))
                    );
                }
            }
            Ok(AgentServiceIoOperation::Submit(_)) => submitted_undurable = true,
            _ => {}
        }
        let backup = path.with_extension(format!("blocked-backup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        let _ = std::fs::remove_file(backup);
        assert!(
            !submitted_undurable,
            "{stage}: Lookup Unknown must never authorize Submit while the exact signed original has not been persisted"
        );
    }
}

#[test]
fn hosted_orphan_classification_does_not_hide_agent_b_behind_agent_a_admission() {
    let world = bound_provider_lease_test_world(&["agent-a", "agent-b"]);
    let context_a = valid_test_provider_context(&world, "agent-a", "turn-a", "request-a");
    let context_b = valid_test_provider_context(&world, "agent-b", "turn-b", "request-b");
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar.provider_service_required = true;
    sidecar
        .provider_contexts
        .insert("agent-a".into(), context_a.clone());
    sidecar
        .provider_active_turns
        .insert("agent-a".into(), context_a.clone());
    sidecar
        .provider_contexts
        .insert("agent-b".into(), context_b.clone());
    sidecar
        .provider_active_turns
        .insert("agent-b".into(), context_b.clone());
    sidecar.hosted_admission = Some(
        serde_json::from_value(serde_json::json!({
            "context": context_a,
            "observation": crate::simulator::Observation {
                time: 0, agent_id: "agent-a".into(), pos: crate::geometry::GeoPos::new(0,0,0),
                self_resources: Default::default(), visibility_range_cm: 0,
                visible_agents: vec![], visible_locations: vec![],
                module_lifecycle: Default::default(), module_market: Default::default(),
                power_market: Default::default(), social_state: Default::default(),
            },
            "metadata_identity": "fixture-a", "stage": "dispatch", "commit": null, "lease": null,
        }))
        .unwrap(),
    );
    sidecar.classify_hosted_orphans();
    assert!(
        !sidecar.provider_recovery_pending.contains_key("agent-a"),
        "the exact durable admission still owns Agent A's turn"
    );
    let orphan_b = sidecar
        .provider_recovery_pending
        .get("agent-b")
        .expect("Agent A's durable phase must not suppress Agent B's lost worker recovery");
    assert_eq!(
        serde_json::to_value(&orphan_b.active).unwrap(),
        serde_json::to_value(&context_b).unwrap()
    );
    assert_eq!(
        orphan_b.reason,
        "executor_lost_after_dispatch:model_outcome_unknown"
    );
    assert!(sidecar.runner.is_none());
}
