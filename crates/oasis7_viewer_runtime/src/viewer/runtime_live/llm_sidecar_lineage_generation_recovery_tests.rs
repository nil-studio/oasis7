use super::*;

#[test]
fn provider_lease_continuity_requires_exact_reserved_runtime_record() {
    let mut world = bound_provider_lease_test_world(&["agent-a"]);
    let context = valid_test_provider_context(&world, "agent-a", "turn-live", "request-live");
    let lease = reserve_test_provider_lease(&mut world, &context);
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar.bind_provider_cognition_lease("agent-a", lease.clone());

    assert!(
        sidecar.provider_cognition_lease_in_flight(&world),
        "continuity requires the sidecar lease and Runtime record to agree"
    );

    let mut closed_world = world.clone();
    closed_world
        .release_cognition_lease(lease.lease_id.as_str())
        .expect("release test lease");
    assert!(
        !sidecar.provider_cognition_lease_in_flight(&closed_world),
        "a terminal Runtime record must not defer chain binding checks"
    );
}

#[test]
fn generation_changed_restore_releases_active_reserved_lease_before_fresh_planning() {
    let mut old_world = bound_provider_lease_test_world(&["agent-a"]);
    let old_context = valid_test_provider_context(&old_world, "agent-a", "turn-old", "request-old");
    let old_lease = reserve_test_provider_lease(&mut old_world, &old_context);
    let old_binding = old_world
        .current_cognition_runtime_binding()
        .expect("old Runtime cognition binding");
    let path = std::env::temp_dir().join(format!(
        "oasis7-viewer-provider-lineage-generation-change-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));

    let mut first = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    first.configure_provider_lineage_store(path.clone());
    first.provider_agent_ids.insert("agent-a".to_string());
    // A process can stop after the provider lease is reserved but before the
    // active turn mirror is copied into provider_contexts. Keep only the
    // active marker here to exercise that restart prefix.
    first
        .provider_active_turns
        .insert("agent-a".to_string(), old_context);
    first
        .provider_cognition_leases
        .insert("agent-a".to_string(), old_lease.clone());
    first.provider_lineage_binding = Some(old_binding.clone());
    first
        .persist_provider_lineage()
        .expect("persist generation-change active lease checkpoint");

    let mut rotated_world = old_world.clone();
    rotated_world
        .install_capability_agent_identity("agent-a", "runtime-test-owner:agent-a:rotated", 2)
        .expect("rotate Runtime capability generation");
    assert_eq!(
        rotated_world
            .current_cognition_runtime_binding()
            .expect("rotated Runtime cognition binding"),
        old_binding,
        "capability generation rotation must exercise the identity-only change"
    );
    rotated_world
        .install_test_provider_additional_capability_grant("agent-a", "generation-2")
        .expect("install current-generation provider grant");
    let presenter = rotated_world
        .capability_invocation_contexts()
        .values()
        .find(|context| {
            matches!(
                &context.subject,
                oasis7_wasm_abi::CapabilitySubject::Agent { agent_id, .. }
                    if agent_id == "agent-a"
            ) && context.presenter.presenter_kind == "provider"
        })
        .map(|context| context.presenter.clone())
        .expect("provider presenter survives generation rotation");
    rotated_world
        .install_capability_invocation_context_for_agent(
            "agent-a",
            presenter,
            "runtime-test-response:agent-a:generation-2",
        )
        .expect("persist current-generation provider invocation");

    let mut restarted = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restarted.configure_provider_lineage_store(path.clone());
    restarted
        .restore_provider_lineage(&rotated_world)
        .expect("restore generation-changed provider lineage");
    assert!(
        restarted.provider_stale_replans.contains_key("agent-a"),
        "generation change must fence the old request for a fresh identity"
    );
    assert_eq!(
        restarted.provider_cognition_leases.get("agent-a"),
        Some(&old_lease),
        "restore must retain the exact old lease until mutable cleanup"
    );

    let _env_guard = crate::viewer::runtime_live::canonical_runtime_provider_env_lock()
        .lock()
        .expect("provider env lock");
    let _provider_env_snapshot = ProviderEnvSnapshot::capture(&[
        VIEWER_AGENT_DECISION_SOURCE_ENV,
        VIEWER_AGENT_PROVIDER_BACKEND_ENV,
        VIEWER_AGENT_PROVIDER_CONTRACT_ENV,
        VIEWER_AGENT_PROVIDER_TRANSPORT_ENV,
        VIEWER_AGENT_PROVIDER_URL_ENV,
        VIEWER_AGENT_PROVIDER_AUTH_TOKEN_ENV,
        VIEWER_AGENT_PROVIDER_CONNECT_TIMEOUT_MS_ENV,
        VIEWER_AGENT_PROVIDER_DECISION_TIMEOUT_MS_ENV,
        VIEWER_AGENT_PROVIDER_PROFILE_ENV,
        VIEWER_AGENT_EXECUTION_LANE_ENV,
        VIEWER_AGENT_PROVIDER_MODE_ENV,
        crate::simulator::ENV_LLM_MODEL,
        crate::simulator::ENV_LLM_BASE_URL,
        crate::simulator::ENV_LLM_API_KEY,
    ]);
    // SAFETY: the canonical provider lock serializes this test's environment setup.
    unsafe {
        oasis7::env_mut::set_var(crate::simulator::ENV_LLM_MODEL, "generation-change-test");
        oasis7::env_mut::set_var(
            crate::simulator::ENV_LLM_BASE_URL,
            "https://example.invalid/v1",
        );
        oasis7::env_mut::set_var(crate::simulator::ENV_LLM_API_KEY, "test-key");
    }
    let provider =
        crate::simulator::MockDecisionProvider::new("generation-change-recovery-provider");
    let behavior = crate::simulator::ProviderBackedAgentBehavior::new_legacy_compatibility(
        "agent-a",
        provider,
        vec![crate::simulator::ActionCatalogEntry::new("wait", "wait")],
    );
    let mut runner = crate::simulator::AsyncAgentRunner::with_default_capacity();
    runner
        .register(behavior)
        .expect("register generation-change provider actor");
    restarted.runner = Some(RuntimeDecisionRunner::ProviderBacked(runner));
    restarted
        .sync_shadow_kernel(&rotated_world, &WorldConfig::default())
        .expect("sync generation-change provider shadow kernel");
    let mut kernel = restarted
        .shadow_kernel
        .take()
        .expect("generation-change provider shadow kernel");
    restarted
        .prepare_provider_request_contexts(&mut rotated_world, &mut kernel, "lease-recovery-world")
        .expect("old lease cleanup must precede fresh planning");

    let released = rotated_world
        .cognition_economy()
        .expect("read economy after generation-change cleanup")
        .leases
        .get(old_lease.lease_id.as_str())
        .expect("old lease remains durable")
        .clone();
    assert_eq!(
        released.status,
        crate::runtime::CognitionLeaseStatusV1::Released,
        "generation-change recovery must release the old Runtime reservation"
    );
    assert_eq!(
        rotated_world
            .cognition_economy()
            .expect("read receipts after generation-change cleanup")
            .receipts
            .values()
            .filter(|receipt| receipt.operation == "release")
            .count(),
        1,
        "generation-change recovery must emit one durable release receipt"
    );
    assert!(
        restarted.provider_cognition_leases.is_empty(),
        "fresh planning must not retain the obsolete lease mirror"
    );
    let fresh = restarted
        .provider_contexts
        .get("agent-a")
        .expect("fresh provider context after generation recovery");
    assert_eq!(
        crate::viewer::runtime_live::control_plane::llm_sidecar::lineage_generation_recovery::provider_request_capability_identity(&fresh.request_context),
        Some(crate::runtime::CapabilityAgentIdentity {
            owner_binding: "runtime-test-owner:agent-a:rotated".to_string(),
            generation: 2,
        }),
        "fresh planning must use the current owner/generation identity"
    );
    assert_eq!(
        restarted.provider_capability_identities.get("agent-a"),
        Some(&crate::runtime::CapabilityAgentIdentity {
            owner_binding: "runtime-test-owner:agent-a:rotated".to_string(),
            generation: 2,
        })
    );

    let _ = std::fs::remove_file(path);
}
