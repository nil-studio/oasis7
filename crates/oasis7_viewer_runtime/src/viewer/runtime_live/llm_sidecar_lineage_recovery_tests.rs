use super::*;

#[test]
fn canonical_release_receipt_requires_original_identity_and_accounting() {
    let mut world = bound_provider_lease_test_world(&["agent-a"]);
    let context = valid_test_provider_context(&world, "agent-a", "release-turn", "release-request");
    let lease = reserve_test_provider_lease(&mut world, &context);
    let receipt = world.release_cognition_lease(&lease.lease_id).unwrap();
    RuntimeLlmSidecar::validate_service_release_receipt(
        &context.request_context,
        &lease,
        serde_json::to_value(&receipt).unwrap(),
    )
    .unwrap();
    let mut wrong_identity = receipt.clone();
    wrong_identity.agent_turn_id = "different-turn".into();
    wrong_identity.receipt_digest = wrong_identity.recompute_digest();
    assert!(
        RuntimeLlmSidecar::validate_service_release_receipt(
            &context.request_context,
            &lease,
            serde_json::to_value(wrong_identity).unwrap()
        )
        .is_err()
    );
    let mut wrong_accounting = receipt;
    wrong_accounting.released_amount = 0;
    wrong_accounting.receipt_digest = wrong_accounting.recompute_digest();
    assert!(
        RuntimeLlmSidecar::validate_service_release_receipt(
            &context.request_context,
            &lease,
            serde_json::to_value(wrong_accounting).unwrap()
        )
        .is_err()
    );
}

#[test]
fn settled_service_lease_cleanup_preserves_a_newer_mirror() {
    let mut world = bound_provider_lease_test_world(&["agent-a"]);
    let original_context =
        valid_test_provider_context(&world, "agent-a", "turn-old", "request-old");
    let original = reserve_test_provider_lease(&mut world, &original_context);
    let newer_context = valid_test_provider_context(&world, "agent-a", "turn-new", "request-new");
    let newer = reserve_test_provider_lease(&mut world, &newer_context);
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar.bind_provider_cognition_lease("agent-a", newer.clone());
    assert!(
        sidecar
            .clear_settled_service_lease(&original_context.request_context, &original)
            .is_err()
    );
    assert_eq!(sidecar.provider_cognition_lease("agent-a"), Some(newer));
    sidecar.bind_provider_cognition_lease("agent-a", original.clone());
    sidecar
        .clear_settled_service_lease(&original_context.request_context, &original)
        .unwrap();
    assert!(sidecar.provider_cognition_lease("agent-a").is_none());
    sidecar
        .clear_settled_service_lease(&original_context.request_context, &original)
        .unwrap();
}
use crate::viewer::runtime_live::{
    ViewerRuntimeLiveServer, ViewerRuntimeLiveServerConfig, WorldScenario,
};

#[path = "llm_sidecar_lineage_generation_recovery_tests.rs"]
mod generation_recovery_tests;

pub(super) fn bound_provider_lease_test_world(agent_ids: &[&str]) -> RuntimeWorld {
    bound_provider_lease_test_world_with_binding(agent_ids, "pending", None)
}

fn bound_provider_lease_test_world_with_binding(
    agent_ids: &[&str],
    finality_status: &str,
    finality_block_hash: Option<String>,
) -> RuntimeWorld {
    let mut world = crate::runtime_host::world::new();
    world
        .bind_cognition_runtime(
            "lease-recovery-world",
            "lease-recovery-branch",
            0,
            finality_block_hash,
            finality_status,
            0,
        )
        .expect("Runtime cognition binding");
    for agent_id in agent_ids {
        world.submit_action(RuntimeAction::RegisterAgent {
            agent_id: (*agent_id).to_string(),
            pos: GeoPos::new(0, 0, 0),
        });
    }
    world.step().expect("register Runtime agents");
    for agent_id in agent_ids {
        world
            .install_test_provider_capability_fixture_without_cognition_balance(agent_id)
            .expect("install Runtime provider capability fixture");
        world
            .provision_cognition_for_agent(
                agent_id,
                format!("runtime-test-provision:{agent_id}"),
                "runtime-test-authority",
                128,
            )
            .expect("provision Runtime cognition allowance");
    }
    world
}

fn reserve_test_provider_lease(
    world: &mut RuntimeWorld,
    context: &cognition_context::ProviderContextState,
) -> crate::runtime::CognitionLeaseV1 {
    crate::viewer::runtime_live::control_plane::llm_sidecar::async_support::reserve_provider_cognition_lease(
        world, context,
    )
    .expect("reserve Runtime cognition lease")
}

pub(super) fn valid_test_provider_context(
    world: &RuntimeWorld,
    agent_id: &str,
    agent_turn_id: &str,
    decision_request_id: &str,
) -> cognition_context::ProviderContextState {
    let mut context = test_provider_context(agent_id, agent_turn_id, decision_request_id, 1);
    let request = &mut context.request_context;
    request.runtime_binding.base_world_hash =
        crate::simulator::h_v1("oasis7.cognition.test.world.v1", &agent_id);
    request.runtime_binding.runtime_manifest_hash =
        crate::simulator::h_v1("oasis7.cognition.test.manifest.v1", &agent_id);
    request.observation_digest = crate::simulator::h_v1(
        "oasis7.cognition.test.observation.v1",
        &request.base_decision_request.observation,
    );
    let persisted_invocation = world
        .capability_invocation_contexts()
        .values()
        .find(|context| {
            matches!(
                &context.subject,
                oasis7_wasm_abi::CapabilitySubject::Agent { agent_id: subject_id, .. }
                    if subject_id == agent_id
            ) && context.presenter.presenter_kind == "provider"
        })
        .cloned()
        .expect("Runtime provider invocation fixture");
    let (catalog, invocation) = world
        .capability_context_for_agent(
            agent_id,
            persisted_invocation.presenter.clone(),
            persisted_invocation.response_nonce.clone(),
        )
        .expect("Runtime provider capability projection");
    request.base_decision_request.capability_catalog = Some(catalog.clone());
    request.base_decision_request.capability_invocation_context = Some(invocation.clone());
    request.capability_catalog_digest = crate::simulator::h_v1(
        crate::simulator::COGNITION_CAPABILITY_CATALOG_DOMAIN,
        &catalog,
    );
    request.capability_invocation_context_digest = crate::simulator::h_v1(
        crate::simulator::COGNITION_CAPABILITY_INVOCATION_CONTEXT_DOMAIN,
        &invocation,
    );
    request.memory_snapshot_digest =
        crate::simulator::h_v1("oasis7.cognition.test.memory.v1", &agent_id);
    request.goal_snapshot_digest =
        crate::simulator::h_v1("oasis7.cognition.test.goal.v1", &agent_id);
    request.continuation_digest =
        crate::simulator::h_v1("oasis7.cognition.test.continuation.v1", &agent_id);
    request.runtime_binding = world
        .current_cognition_runtime_binding()
        .expect("Runtime cognition binding");
    request.request_digest = request.request_digest();
    context.turn_context.request_digest = request.request_digest.clone();
    context
}

fn provider_context_with_owner_binding(
    mut context: cognition_context::ProviderContextState,
    owner_binding: &str,
) -> cognition_context::ProviderContextState {
    let request = &mut context.request_context;
    let invocation = request
        .base_decision_request
        .capability_invocation_context
        .as_mut()
        .expect("provider invocation context");
    if let oasis7_wasm_abi::CapabilitySubject::Agent {
        owner_binding: current,
        ..
    } = &mut invocation.subject
    {
        *current = owner_binding.to_string();
    }
    let catalog = request
        .base_decision_request
        .capability_catalog
        .as_mut()
        .expect("provider capability catalog");
    catalog.subject = invocation.subject.clone();
    request.capability_catalog_digest = crate::simulator::h_v1(
        crate::simulator::COGNITION_CAPABILITY_CATALOG_DOMAIN,
        catalog,
    );
    request.capability_invocation_context_digest = crate::simulator::h_v1(
        crate::simulator::COGNITION_CAPABILITY_INVOCATION_CONTEXT_DOMAIN,
        invocation,
    );
    request.request_digest = request.request_digest();
    context.turn_context.request_digest = request.request_digest.clone();
    context
}

#[test]
fn provider_lease_uses_runtime_owner_binding_and_rejects_cross_owner_reuse() {
    let mut world = bound_provider_lease_test_world(&["agent-a"]);
    let context_a =
        valid_test_provider_context(&world, "agent-a", "turn-owner-a", "request-owner-a");
    let payer_a = payer_support::provider_payer_id(&context_a.request_context)
        .expect("Runtime owner binding A");
    let lease_a = reserve_test_provider_lease(&mut world, &context_a);
    assert_eq!(payer_a, "runtime-test-owner:agent-a");
    assert_eq!(lease_a.account_id, payer_a);
    assert_eq!(lease_a.quote.payer_id, payer_a);
    assert_ne!(lease_a.account_id, context_a.request_context.agent_subject);

    let context_b = provider_context_with_owner_binding(context_a.clone(), "authorized-owner-b");
    let payer_b = payer_support::provider_payer_id(&context_b.request_context)
        .expect("Runtime owner binding B");
    assert_ne!(
        payer_a, payer_b,
        "owner bindings must partition lease identity"
    );
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar
        .provider_contexts
        .insert("agent-a".to_string(), context_b);
    sidecar
        .provider_cognition_leases
        .insert("agent-a".to_string(), lease_a);
    sidecar.provider_wait_until.insert("agent-a".to_string(), 0);
    let economy_before = world
        .cognition_economy()
        .expect("read economy before reuse");
    let error = sidecar
        .release_due_provider_waits(&mut world)
        .expect_err("cross-owner lease reuse must fail closed");
    assert!(error.contains("cognition lease identity mismatch"));
    assert_eq!(
        world.cognition_economy().expect("read economy after reuse"),
        economy_before,
        "cross-owner rejection must not emit an economic receipt"
    );
}

#[test]
fn provider_lineage_restore_fences_cross_request_lease_before_dispatch() {
    let mut world = bound_provider_lease_test_world(&["agent-a"]);
    let old_context = valid_test_provider_context(&world, "agent-a", "turn-old", "request-old");
    let new_context = valid_test_provider_context(&world, "agent-a", "turn-new", "request-new");
    let old_lease = reserve_test_provider_lease(&mut world, &old_context);
    let path = std::env::temp_dir().join(format!(
        "oasis7-viewer-provider-lineage-cross-lease-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let mut first = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    first.configure_provider_lineage_store(path.clone());
    first.provider_agent_ids.insert("agent-a".to_string());
    first
        .provider_contexts
        .insert("agent-a".to_string(), new_context.clone());
    first
        .provider_active_turns
        .insert("agent-a".to_string(), new_context);
    first
        .provider_cognition_leases
        .insert("agent-a".to_string(), old_lease);
    first
        .persist_provider_lineage()
        .expect("persist cross-request provider lease");
    let economy_before = world
        .cognition_economy()
        .expect("read economy before restore");

    let mut restarted = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restarted.configure_provider_lineage_store(path.clone());
    restarted.hydrate_provider_lineage(&world);
    assert!(
        restarted
            .provider_lineage_recovery_pending
            .as_deref()
            .is_some_and(|reason| reason.contains("provider cognition lease")),
        "cross-request lease must create a durable recovery fence"
    );
    assert!(
        restarted.provider_cognition_leases.is_empty(),
        "invalid lease must not become an installed provider authority"
    );
    let mut kernel = WorldKernel::new();
    let error = restarted
        .prepare_provider_request_contexts(&mut world, &mut kernel, "lease-recovery-world")
        .expect_err("recovery fence must block provider dispatch");
    assert!(error.contains("provider lineage recovery fenced"));
    assert_eq!(
        world
            .cognition_economy()
            .expect("read economy after restore"),
        economy_before,
        "restore fencing must not mutate Runtime economy"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn binding_changed_restore_releases_stale_lease_before_replan() {
    let mut old_world = bound_provider_lease_test_world(&["agent-a"]);
    old_world = old_world.with_cognition_scheduler(
        serde_json::from_value(serde_json::json!({
            "schema_version": "scheduler-policy.v1",
            "max_total_wakes_per_tick": 8,
            "max_wakes_per_agent_per_tick": 1,
            "aging_after_ticks": 2,
            "max_starvation_ticks": 4,
            "initial_priority": 0,
            "comparator": "deadline_due_desc,next_wake_tick_asc,effective_priority_desc,starvation_deadline_tick_asc,cursor_distance_asc,agent_id_asc,continuation_id_asc,wake_seq_asc",
            "service_order": "stable_round_robin"
        }))
        .expect("decode binding-change scheduler policy"),
        8,
    );
    let old_context = valid_test_provider_context(&old_world, "agent-a", "turn-old", "request-old");
    let old_lease = reserve_test_provider_lease(&mut old_world, &old_context);
    let old_binding = old_world
        .current_cognition_runtime_binding()
        .expect("old Runtime cognition binding");
    let path = std::env::temp_dir().join(format!(
        "oasis7-viewer-provider-lineage-binding-change-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let mut first = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    first.configure_provider_lineage_store(path.clone());
    first.provider_agent_ids.insert("agent-a".to_string());
    first
        .provider_contexts
        .insert("agent-a".to_string(), old_context);
    first
        .provider_cognition_leases
        .insert("agent-a".to_string(), old_lease.clone());
    first.provider_lineage_binding = Some(old_binding);
    first
        .persist_provider_lineage()
        .expect("persist stale binding checkpoint");

    let mut new_world = old_world.clone();
    new_world
        .invalidate_cognition_for_reorg(1)
        .expect("authorize binding change");
    new_world
        .bind_cognition_runtime(
            "lease-recovery-world",
            "lease-recovery-branch",
            0,
            None,
            "pending",
            1,
        )
        .expect("bind new Runtime cognition identity");

    let mut restarted = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restarted.configure_provider_lineage_store(path.clone());
    restarted
        .restore_provider_lineage(&new_world)
        .expect("restore binding-changed provider lineage");
    assert!(
        restarted.provider_stale_replans.contains_key("agent-a"),
        "binding change must queue the old request for a fresh identity"
    );
    assert_eq!(
        restarted.provider_cognition_leases.get("agent-a"),
        Some(&old_lease),
        "restore must retain the exact old lease until the mutable cleanup pass"
    );

    let mut kernel = WorldKernel::new();
    restarted
        .prepare_provider_request_contexts(&mut new_world, &mut kernel, "lease-recovery-world")
        .expect("stale lease cleanup must complete before provider dispatch");

    let economy = new_world
        .cognition_economy()
        .expect("read economy after stale lease cleanup");
    let released = economy
        .leases
        .get(old_lease.lease_id.as_str())
        .expect("restored lease remains durable")
        .clone();
    assert_eq!(
        released.status,
        crate::runtime::CognitionLeaseStatusV1::Released,
        "binding-changed stale lease must release its reservation"
    );
    assert!(
        restarted.provider_cognition_leases.is_empty(),
        "new identity planning must not retain the old lease mirror"
    );
    let checkpoint: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&path).expect("read cleaned binding-change checkpoint"),
    )
    .expect("decode cleaned binding-change checkpoint");
    assert!(
        checkpoint["provider_cognition_leases"]
            .as_object()
            .is_some_and(serde_json::Map::is_empty),
        "old lease cleanup must be durable before the next provider identity"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn binding_changed_stale_lease_checkpoint_retry_reconciles_terminal_release() {
    let mut old_world = bound_provider_lease_test_world(&["agent-a"]);
    old_world = old_world.with_cognition_scheduler(
        serde_json::from_value(serde_json::json!({
            "schema_version": "scheduler-policy.v1",
            "max_total_wakes_per_tick": 8,
            "max_wakes_per_agent_per_tick": 1,
            "aging_after_ticks": 2,
            "max_starvation_ticks": 4,
            "initial_priority": 0,
            "comparator": "deadline_due_desc,next_wake_tick_asc,effective_priority_desc,starvation_deadline_tick_asc,cursor_distance_asc,agent_id_asc,continuation_id_asc,wake_seq_asc",
            "service_order": "stable_round_robin"
        }))
        .expect("decode checkpoint-retry scheduler policy"),
        8,
    );
    let old_context = valid_test_provider_context(&old_world, "agent-a", "turn-old", "request-old");
    let old_lease = reserve_test_provider_lease(&mut old_world, &old_context);
    let old_binding = old_world
        .current_cognition_runtime_binding()
        .expect("old Runtime cognition binding");
    let path = std::env::temp_dir().join(format!(
        "oasis7-viewer-provider-lineage-binding-retry-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let blocked_backup = path.with_extension(format!("blocked-backup-{}", std::process::id()));
    let mut first = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    first.configure_provider_lineage_store(path.clone());
    first.provider_agent_ids.insert("agent-a".to_string());
    first
        .provider_contexts
        .insert("agent-a".to_string(), old_context);
    first
        .provider_cognition_leases
        .insert("agent-a".to_string(), old_lease.clone());
    first.provider_lineage_binding = Some(old_binding);
    first
        .persist_provider_lineage()
        .expect("persist checkpoint-retry stale binding fixture");

    let mut new_world = old_world.clone();
    new_world
        .invalidate_cognition_for_reorg(1)
        .expect("authorize checkpoint-retry binding change");
    new_world
        .bind_cognition_runtime(
            "lease-recovery-world",
            "lease-recovery-branch",
            0,
            None,
            "pending",
            1,
        )
        .expect("bind checkpoint-retry Runtime identity");

    let mut restarted = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restarted.configure_provider_lineage_store(path.clone());
    restarted
        .restore_provider_lineage(&new_world)
        .expect("restore checkpoint-retry provider lineage");
    restarted
        .install_test_provider_lineage_checkpoint_blocker()
        .expect("install checkpoint blocker");
    let mut kernel = WorldKernel::new();
    let error = restarted
        .prepare_provider_request_contexts(&mut new_world, &mut kernel, "lease-recovery-world")
        .expect_err("checkpoint failure must retain retryable stale identity");
    assert!(
        error.contains("stale provider cognition lease cleanup persistence failed"),
        "checkpoint failure must be surfaced: {error}"
    );
    assert_eq!(
        new_world
            .cognition_economy()
            .expect("read Runtime economy after failed persistence")
            .leases
            .get(old_lease.lease_id.as_str())
            .expect("released lease remains durable")
            .status,
        crate::runtime::CognitionLeaseStatusV1::Released,
        "Runtime release must commit before sidecar persistence retry"
    );
    assert!(
        restarted.provider_cognition_leases.contains_key("agent-a"),
        "failed checkpoint must retain the sidecar mirror for retry"
    );

    std::fs::remove_dir(&path).expect("remove checkpoint blocker");
    restarted
        .prepare_provider_request_contexts(&mut new_world, &mut kernel, "lease-recovery-world")
        .expect("same-server stale lease cleanup must retry after checkpoint recovery");
    assert!(
        restarted.provider_cognition_leases.is_empty(),
        "retry must clear the terminal lease mirror"
    );
    let checkpoint: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read retried checkpoint cleanup"))
            .expect("decode retried checkpoint cleanup");
    assert!(
        checkpoint["provider_cognition_leases"]
            .as_object()
            .is_some_and(serde_json::Map::is_empty),
        "retry must durably clear the old lease before replan"
    );
    assert_eq!(
        new_world
            .cognition_economy()
            .expect("read Runtime economy after retry")
            .receipts
            .values()
            .filter(|receipt| receipt.operation == "release")
            .count(),
        1,
        "retry must not emit a duplicate Runtime release"
    );
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(blocked_backup);
}

#[test]
fn stale_replan_exhaustion_cleans_restored_lease_before_terminal_return() {
    let mut world = bound_provider_lease_test_world(&["agent-a"]);
    let context =
        valid_test_provider_context(&world, "agent-a", "turn-stale-max", "request-stale-max");
    let lease = reserve_test_provider_lease(&mut world, &context);
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("Runtime live server");
    server.world = world;
    server
        .llm_sidecar
        .provider_agent_ids
        .insert("agent-a".to_string());
    server
        .llm_sidecar
        .provider_contexts
        .insert("agent-a".to_string(), context);
    server
        .llm_sidecar
        .provider_cognition_leases
        .insert("agent-a".to_string(), lease.clone());
    for count in 1..=3 {
        assert!(server.llm_sidecar.schedule_provider_stale_replan(
            "agent-a",
            format!("turn-stale-max-{count}").as_str(),
            format!("request-stale-max-{count}").as_str(),
        ));
    }
    assert!(!server.llm_sidecar.schedule_provider_stale_replan(
        "agent-a",
        "turn-stale-max-overflow",
        "request-stale-max-overflow",
    ));

    server
        .enqueue_llm_action_from_sidecar()
        .expect_err("stale replan exhaustion must remain a terminal result");
    assert!(
        server.llm_sidecar.provider_cognition_leases.is_empty(),
        "exhaustion must clean the restored stale lease before returning"
    );
    assert_eq!(
        server
            .world
            .cognition_economy()
            .expect("read economy after stale replan exhaustion")
            .leases
            .get(lease.lease_id.as_str())
            .expect("stale lease remains durable")
            .status,
        crate::runtime::CognitionLeaseStatusV1::Released,
        "stale replan exhaustion must release the old Runtime reservation"
    );
}

#[test]
fn committed_marker_recovery_settles_provider_lease_before_clearing_mirror() {
    // The provider fixture installs a finalized capability authority. Commit
    // validation requires the equivalent verified runtime finality binding,
    // so seed the fixture with a valid shared finality hash from the start.
    let mut world = bound_provider_lease_test_world_with_binding(
        &["agent-a"],
        "verified",
        Some(
            crate::simulator::h_v1("oasis7.test.committed-marker-finality.v1", &"finality")
                .to_string(),
        ),
    );
    let context =
        valid_test_provider_context(&world, "agent-a", "turn-committed", "request-committed");
    let lease = reserve_test_provider_lease(&mut world, &context);
    let binding = world
        .current_cognition_runtime_binding()
        .expect("Runtime cognition binding");
    let request = crate::runtime::RuntimeCognitionCommitRequestV1 {
        agent_id: context.request_context.agent_subject.clone(),
        agent_session_id: context.request_context.agent_session_id.clone(),
        agent_turn_id: context.request_context.agent_turn_id.clone(),
        decision_request_id: context.request_context.decision_request_id.clone(),
        retry_seq: context.request_context.retry_seq,
        transport_attempt: context.request_context.transport_attempt,
        request_digest: context.request_context.request_digest.to_string(),
        observation_digest: context.request_context.observation_digest.to_string(),
        context_digest:
            crate::viewer::runtime_live::control_plane::llm_sidecar::runtime_provider_context_digest(
                &context.request_context,
            ),
        capability_snapshot_hash: crate::simulator::h_v1(
            "oasis7.runtime.manifest.v1",
            &world.capability_authorization_root(),
        )
        .to_string(),
        authority_context_hash: crate::simulator::h_v1(
            "oasis7.runtime.authority-context.v1",
            &world.capability_authorization_root(),
        )
        .to_string(),
        captured_base_binding: crate::runtime::RuntimeCognitionBaseBindingV1 {
            world_id: binding.world_id,
            branch_id: binding.branch_id,
            finality_epoch: binding.finality_epoch,
            finality_block_hash: binding.finality_block_hash.map(|hash| hash.to_string()),
            finality_status: binding.finality_status,
            base_tick: binding.base_tick,
            base_world_hash: binding.base_world_hash.to_string(),
            reorg_epoch: binding.reorg_epoch,
            runtime_manifest_hash: binding.runtime_manifest_hash.to_string(),
        },
    };
    let mut response = crate::runtime::RuntimeCognitionResponseArtifactV1 {
        schema_version: 1,
        context_discriminator:
            crate::runtime::RuntimeCognitionResponseArtifactV1::CONTEXT_DISCRIMINATOR.to_string(),
        context_version: crate::runtime::RuntimeCognitionResponseArtifactV1::CONTEXT_VERSION,
        agent_session_id: request.agent_session_id.clone(),
        agent_turn_id: request.agent_turn_id.clone(),
        decision_request_id: request.decision_request_id.clone(),
        retry_seq: request.retry_seq,
        transport_attempt: request.transport_attempt,
        request_digest: request.request_digest.clone(),
        response_digest: crate::simulator::h_v1(
            "oasis7.test.committed-marker-response.v1",
            &"response",
        )
        .to_string(),
        artifact_digest: String::new(),
    };
    response.refresh_artifact_digest();
    world
        .commit_cognition_action(
            request,
            RuntimeAction::MoveAgent {
                agent_id: "agent-a".to_string(),
                to: GeoPos::new(1, 1, 0),
            },
            response,
        )
        .expect("commit Runtime provider response");

    let path = std::env::temp_dir().join(format!(
        "oasis7-provider-committed-lease-recovery-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let mut first = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    first.configure_provider_lineage_store(path.clone());
    first.provider_agent_ids.insert("agent-a".to_string());
    first
        .provider_active_turns
        .insert("agent-a".to_string(), context.clone());
    first
        .provider_contexts
        .insert("agent-a".to_string(), context.clone());
    first
        .provider_cognition_leases
        .insert("agent-a".to_string(), lease.clone());
    let provider_response = crate::simulator::DecisionResponse::wait("recovery-provider");
    let cognition =
        crate::viewer::runtime_live::control_plane::llm_sidecar::RuntimeProviderActionContext {
            request: context.clone(),
            response: crate::simulator::ContinuousAgentResponseContextV1 {
                response_digest: crate::simulator::cognition_response_digest(&provider_response),
                base_decision_response: provider_response,
                context_discriminator: crate::simulator::CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR
                    .to_string(),
                context_version: crate::simulator::CONTINUOUS_AGENT_CONTEXT_VERSION,
                agent_session_id: context.request_context.agent_session_id.clone(),
                agent_turn_id: context.request_context.agent_turn_id.clone(),
                decision_request_id: context.request_context.decision_request_id.clone(),
                retry_seq: context.request_context.retry_seq,
                transport_attempt: context.request_context.transport_attempt,
                request_digest: context.request_context.request_digest.clone(),
            },
            cognition_lease: Some(lease.clone()),
            memory_write_intents: vec![crate::simulator::MemoryWriteIntent {
                scope: "provider-recovery".to_string(),
                summary: "restore committed memory intent".to_string(),
                tags: vec!["crash".to_string()],
            }],
        };
    first.provider_held_decisions.insert(
        "agent-a".to_string(),
        async_support::RuntimeLlmDecision {
            agent_id: "agent-a".to_string(),
            decision: AgentDecision::Act(oasis7_simulator::Action::MoveAgent {
                agent_id: "agent-a".to_string(),
                to: "loc-recovery".to_string(),
            }),
            decision_trace: None,
            cognition: Some(cognition),
            memory_write_intents: vec![crate::simulator::MemoryWriteIntent {
                scope: "provider-recovery".to_string(),
                summary: "restore committed memory intent".to_string(),
                tags: vec!["crash".to_string()],
            }],
            continuation_admitted: false,
        },
    );
    first
        .persist_provider_lineage()
        .expect("persist committed lease recovery checkpoint");

    let mut restored = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restored.configure_provider_lineage_store(path.clone());
    restored
        .restore_provider_lineage(&world)
        .expect("restore committed lease recovery checkpoint");
    assert!(
        restored.provider_cognition_leases.contains_key("agent-a"),
        "commit-marker recovery must retain the lease until Runtime settlement"
    );
    let recovered_pending = restored
        .pending_actions
        .values()
        .find(|pending| pending.agent_id == "agent-a")
        .expect("commit-marker recovery must rebuild the pre-track action");
    assert_eq!(
        recovered_pending
            .cognition
            .as_ref()
            .expect("recovered cognition context")
            .memory_write_intents
            .first()
            .map(|intent| intent.summary.as_str()),
        Some("restore committed memory intent"),
        "pre-track recovery must retain receipt-gated memory intent"
    );
    assert_eq!(
        world
            .cognition_economy()
            .expect("read economy before recovery settlement")
            .leases
            .get(lease.lease_id.as_str())
            .expect("committed lease")
            .status,
        crate::runtime::CognitionLeaseStatusV1::Reserved
    );
    restored
        .settle_committed_provider_cognition_leases(&mut world)
        .expect("committed marker must settle the exact Runtime lease");
    assert!(
        restored.provider_cognition_leases.is_empty(),
        "lease mirror is cleared only after settlement succeeds"
    );
    assert!(
        restored
            .pending_actions
            .values()
            .any(|pending| { pending.agent_id == "agent-a" && pending.cognition.is_some() }),
        "settlement must retain the recovery bridge until feedback finalization"
    );
    let economy = world
        .cognition_economy()
        .expect("read economy after recovery settlement");
    assert_eq!(
        economy
            .leases
            .get(lease.lease_id.as_str())
            .expect("settled lease")
            .status,
        crate::runtime::CognitionLeaseStatusV1::Settled
    );
    assert_eq!(
        economy
            .receipts
            .values()
            .filter(|receipt| receipt.lease_id == lease.lease_id && receipt.operation == "settle")
            .count(),
        1,
        "recovery settlement must emit exactly one receipt"
    );
    let checkpoint: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&path).expect("read settled lease recovery checkpoint"),
    )
    .expect("decode settled lease recovery checkpoint");
    assert!(
        checkpoint["provider_cognition_leases"]
            .as_object()
            .is_some_and(serde_json::Map::is_empty),
        "settled recovery must durably clear the sidecar lease mirror"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn provider_lease_release_fences_cross_request_without_economic_mutation() {
    let mut world = bound_provider_lease_test_world(&["agent-a"]);
    let old_context = valid_test_provider_context(&world, "agent-a", "turn-old", "request-old");
    let new_context = valid_test_provider_context(&world, "agent-a", "turn-new", "request-new");
    let old_lease = reserve_test_provider_lease(&mut world, &old_context);
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar
        .provider_contexts
        .insert("agent-a".to_string(), new_context);
    sidecar
        .provider_cognition_leases
        .insert("agent-a".to_string(), old_lease);
    sidecar.provider_wait_until.insert("agent-a".to_string(), 0);
    let economy_before = world
        .cognition_economy()
        .expect("read economy before release");

    let error = sidecar
        .release_due_provider_waits(&mut world)
        .expect_err("cross-request lease must be rejected before release");
    assert!(error.contains("cognition lease identity mismatch"));
    assert_eq!(
        world
            .cognition_economy()
            .expect("read economy after release"),
        economy_before,
        "identity rejection must not emit a Runtime economic receipt"
    );
    assert!(
        sidecar.provider_cognition_leases.contains_key("agent-a"),
        "rejected lease remains available for durable recovery inspection"
    );
}

#[test]
fn pre_io_release_failure_retains_and_fences_lease_mirror() {
    let mut world = bound_provider_lease_test_world(&["agent-a"]);
    let context = valid_test_provider_context(&world, "agent-a", "turn-pre-io", "request-pre-io");
    let lease = reserve_test_provider_lease(&mut world, &context);
    world
        .release_cognition_lease(lease.lease_id.as_str())
        .expect("close Runtime lease to inject a release fault");
    let mut fault_lease = lease.clone();
    fault_lease.lease_id = "missing-pre-io-lease".to_string();

    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar
        .provider_contexts
        .insert("agent-a".to_string(), context.clone());
    sidecar
        .provider_cognition_leases
        .insert("agent-a".to_string(), fault_lease.clone());
    let error = sidecar
        .release_provider_lease_before_io_or_fence(&mut world, "agent-a", &context, &fault_lease)
        .expect_err("closed Runtime lease must exercise the pre-I/O release fault path");
    assert!(error.contains("release before provider I/O failed"));
    assert!(
        sidecar.provider_cognition_leases.contains_key("agent-a"),
        "failed pre-I/O release must retain the exact lease mirror"
    );
    assert!(
        sidecar.provider_recovery_pending.contains_key("agent-a"),
        "failed pre-I/O release must install a durable recovery fence"
    );
    assert!(sidecar.provider_transport_exhausted.contains("agent-a"));
    assert_eq!(
        world
            .cognition_economy()
            .expect("read economy after injected release fault")
            .receipts
            .values()
            .filter(|receipt| receipt.lease_id == lease.lease_id && receipt.operation == "release")
            .count(),
        1,
        "the failed second release must not add an economic receipt"
    );
}

#[test]
fn provider_lease_reserve_fences_stale_runtime_binding_without_economic_mutation() {
    let mut world = bound_provider_lease_test_world(&["agent-a"]);
    let mut context = valid_test_provider_context(&world, "agent-a", "turn-stale", "request-stale");
    context.request_context.runtime_binding.branch_id = "stale-branch".to_string();
    context.request_context.request_digest = context.request_context.request_digest();
    context.turn_context.request_digest = context.request_context.request_digest.clone();
    let economy_before = world
        .cognition_economy()
        .expect("read economy before stale reserve");

    let error = crate::viewer::runtime_live::control_plane::llm_sidecar::async_support::reserve_provider_cognition_lease(
        &mut world,
        &context,
    )
        .expect_err("stale Runtime binding must be rejected before reserve");
    assert!(error.contains("Runtime binding changed before lease reserve"));
    assert_eq!(
        world
            .cognition_economy()
            .expect("read economy after stale reserve"),
        economy_before,
        "stale binding rejection must not reserve balance or emit a receipt"
    );
}

#[test]
fn malformed_runtime_wake_projection_fences_provider_dispatch_and_persists_recovery() {
    let path = std::env::temp_dir().join(format!(
        "oasis7-viewer-provider-lineage-runtime-wake-fence-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let valid_world = RuntimeWorld::default();
    let mut snapshot = valid_world.snapshot();
    snapshot.cognition["scheduler_state"] = serde_json::json!({
        "schema_version": "scheduler.v1",
        "in_flight": "malformed"
    });
    let mut malformed_world =
        crate::runtime_host::world::from_snapshot(snapshot, valid_world.journal().clone())
            .expect("malformed scheduler projection remains loadable for adapter fault injection");

    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar.configure_provider_lineage_store(path.clone());
    sidecar.provider_agent_ids.insert("agent-0".to_string());
    sidecar.hydrate_provider_lineage(&malformed_world);

    let recovery_reason = sidecar
        .provider_lineage_recovery_pending
        .as_deref()
        .expect("unreadable Runtime wake projection must retain a recovery fence")
        .to_string();
    assert!(recovery_reason.contains("Runtime cognition wake read failed"));
    assert!(
        !sidecar.provider_lineage_hydrated,
        "wake projection failure must not be marked as successfully hydrated"
    );
    let checkpoint: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&path).expect("read persisted wake projection recovery fence"),
    )
    .expect("decode persisted wake projection recovery fence");
    assert_eq!(
        checkpoint["provider_lineage_recovery_pending"],
        serde_json::json!(recovery_reason)
    );

    let mut kernel = WorldKernel::new();
    let error = sidecar
        .prepare_provider_request_contexts(&mut malformed_world, &mut kernel, "fault-world")
        .expect_err("provider admission must stop behind the Runtime recovery fence");
    assert!(error.contains("provider lineage recovery fenced"));

    let mut restarted = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restarted.configure_provider_lineage_store(path.clone());
    restarted.hydrate_provider_lineage(&malformed_world);
    assert!(
        restarted
            .provider_lineage_recovery_pending
            .as_deref()
            .is_some_and(|reason| reason.contains("Runtime cognition wake read failed")),
        "restart must retain an actionable recovery reason"
    );
    assert!(
        restarted.provider_lineage_hydrated,
        "restart may mark the persisted recovery fence hydrated, but admission remains blocked"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn provider_lineage_restore_terminalizes_exhausted_orphan_without_retry_loop() {
    let path = std::env::temp_dir().join(format!(
        "oasis7-viewer-provider-lineage-exhausted-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let mut first = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    first.configure_provider_lineage_store(path.clone());
    let context = test_provider_context(
        "agent-0",
        "turn-exhausted",
        "request-exhausted",
        MAX_PROVIDER_TRANSPORT_ATTEMPTS,
    );
    first
        .provider_contexts
        .insert("agent-0".to_string(), context.clone());
    first
        .provider_active_turns
        .insert("agent-0".to_string(), context);
    first.provider_agent_ids.insert("agent-0".to_string());
    first
        .persist_provider_lineage()
        .expect("persist exhausted provider lineage");

    let mut restored = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restored.configure_provider_lineage_store(path.clone());
    restored
        .restore_provider_lineage(&RuntimeWorld::default())
        .expect("restore exhausted provider lineage");

    assert!(!restored.provider_active_turns.contains_key("agent-0"));
    assert!(!restored.provider_retry_contexts.contains_key("agent-0"));
    assert!(restored.provider_transport_exhausted.contains("agent-0"));
    let _ = std::fs::remove_file(path);
}

#[test]
fn provider_lineage_restore_quarantines_missing_or_mismatched_active_identity() {
    let path = std::env::temp_dir().join(format!(
        "oasis7-viewer-provider-lineage-quarantine-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let mut first = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    first.configure_provider_lineage_store(path.clone());
    let context = test_provider_context("agent-missing", "turn-missing", "request-missing", 1);
    first
        .provider_active_turns
        .insert("agent-missing".to_string(), context.clone());
    first.provider_agent_ids.insert("agent-missing".to_string());
    let mut mismatched = context.clone();
    mismatched.request_context.agent_turn_id = "turn-newer".to_string();
    mismatched.turn_context.agent_turn_id = "turn-newer".to_string();
    first
        .provider_contexts
        .insert("agent-mismatch".to_string(), mismatched.clone());
    first
        .provider_active_turns
        .insert("agent-mismatch".to_string(), context);
    first
        .provider_agent_ids
        .insert("agent-mismatch".to_string());
    first
        .persist_provider_lineage()
        .expect("persist quarantined provider lineage");

    let mut restored = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restored.configure_provider_lineage_store(path.clone());
    restored
        .restore_provider_lineage(&RuntimeWorld::default())
        .expect("restore quarantined provider lineage");

    assert!(!restored.provider_active_turns.contains_key("agent-missing"));
    assert!(
        !restored
            .provider_active_turns
            .contains_key("agent-mismatch")
    );
    assert_eq!(
        restored
            .provider_recovery_pending
            .get("agent-missing")
            .map(|record| record.reason.as_str()),
        Some("active_context_missing")
    );
    assert_eq!(
        restored
            .provider_recovery_pending
            .get("agent-mismatch")
            .map(|record| record.reason.as_str()),
        Some("active_context_identity_mismatch")
    );
    assert!(
        restored
            .provider_transport_exhausted
            .contains("agent-missing"),
        "missing active identity must enter the durable Runtime terminalization path"
    );
    assert!(
        restored
            .provider_transport_exhausted
            .contains("agent-mismatch"),
        "mismatched active identity must enter the durable Runtime terminalization path"
    );
    assert!(restored.provider_contexts.contains_key("agent-mismatch"));
    assert!(
        !restored
            .provider_retry_contexts
            .contains_key("agent-mismatch")
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn provider_lineage_hydrates_sequences_and_session_from_runtime_projection() {
    let mut world = RuntimeWorld::default();
    world
        .enqueue_runtime_feedback(oasis7_agent_api::FeedbackEnvelopeV1 {
            feedback_id: "runtime-feedback:agent-0:7".to_string(),
            feedback_seq: 7,
            agent_subject: "agent-0".to_string(),
            agent_session_id: "persisted-session".to_string(),
            agent_turn_id: "persisted-session-turn-12".to_string(),
            decision_request_id: "persisted-session-request-12".to_string(),
            candidate_action_id: None,
            runtime_receipt_id: None,
            status: "rejected".to_string(),
            request_digest: crate::simulator::h_v1(
                "oasis7.cognition.request.v1",
                &"persisted-session-request-12",
            ),
            reject_reason: Some("no_effect".to_string()),
            provenance: "runtime_authoritative".to_string(),
        })
        .expect("persist feedback lineage");

    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar.hydrate_provider_lineage(&world);

    assert_eq!(
        sidecar
            .provider_session_ids
            .get("agent-0")
            .map(String::as_str),
        Some("persisted-session")
    );
    assert_eq!(sidecar.provider_context_seq.get("agent-0"), Some(&2));
    assert_eq!(sidecar.provider_feedback_seq.get("agent-0"), Some(&8));
}
