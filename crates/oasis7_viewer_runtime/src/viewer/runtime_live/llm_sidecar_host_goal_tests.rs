use super::*;

#[test]
fn runtime_live_host_goal_default_is_bound_not_empty() {
    let snapshot = trusted_provider_goal_snapshot(None).expect("default host goal");
    assert_eq!(snapshot.revision, 1);
    assert_eq!(
        snapshot.short_term_summary,
        runtime_live_phase1_short_term_goal()
    );
    assert!(snapshot.long_term_summary.is_empty());
    assert_eq!(snapshot.provenance, "harness_projection");
    assert_ne!(
        snapshot.digest,
        crate::simulator::GoalSnapshotV1::empty().digest
    );
}

#[test]
fn runtime_live_host_goal_profile_changes_bind_normalized_text_and_revision() {
    let mut profile = AgentPromptProfile::for_agent("agent-1");
    profile.version = 7;
    profile.short_term_goal_override = Some("  Cafe\u{301}  ".to_string());
    profile.long_term_goal_override = Some("  grow factory  ".to_string());
    let first = trusted_provider_goal_snapshot(Some(&profile)).expect("host overrides");
    assert_eq!(first.revision, 7);
    assert_eq!(first.short_term_summary, "Café");
    assert_eq!(first.long_term_summary, "grow factory");

    profile.short_term_goal_override = Some("Café".to_string());
    assert_eq!(
        trusted_provider_goal_snapshot(Some(&profile))
            .unwrap()
            .digest,
        first.digest
    );
    profile.version += 1;
    let revised = trusted_provider_goal_snapshot(Some(&profile)).unwrap();
    assert_ne!(revised.digest, first.digest);
    profile.long_term_goal_override = Some("another goal".to_string());
    assert_ne!(
        trusted_provider_goal_snapshot(Some(&profile))
            .unwrap()
            .digest,
        revised.digest
    );
}

#[test]
fn runtime_live_host_goal_rejects_oversize_text_without_truncation() {
    for long_term in [false, true] {
        let mut profile = AgentPromptProfile::for_agent("agent-1");
        let target = if long_term {
            &mut profile.long_term_goal_override
        } else {
            &mut profile.short_term_goal_override
        };
        *target = Some("界".repeat(171)); // 513 UTF-8 bytes.
        let error = trusted_provider_goal_snapshot(Some(&profile)).expect_err("oversize host goal");
        assert!(error.contains("goal_snapshot_too_large"), "{error}");
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn canonical_goal_prompt_delivery_preserves_provider_capability_refusal() {
    use oasis7_proto::viewer::*;
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar
        .replace_provider_backed_runner_for_test("agent-1")
        .unwrap();
    let mut projection = crate::world_service::projection::WorldServiceProjection::from_world(
        &RuntimeWorld::default(),
        None,
    )
    .unwrap();
    projection.canonical_agent_chat = Some(CanonicalAgentChatViewV1 {
        agent_id: "agent-1".into(),
        player_id: "owner".into(),
        public_key: "fixture".into(),
        world_id: "world".into(),
        reorg_epoch: 0,
        authority_scope: "player_agent_chat".into(),
        canonical_authority: CanonicalAgentChatAuthorityV1 {
            branch_id: "main".into(),
            agent_identity_generation: 1,
        },
        current_intent_id: Some("intent".into()),
        goal: Some(CanonicalAgentGoalV1 {
            intent_id: "intent".into(),
            message: "committed owner goal".into(),
            status: "accepted".into(),
            event_seq: 1,
            logical_time: 1,
        }),
    });
    sidecar.provider_service_projection = Some(projection);
    let mut profile = AgentPromptProfile::for_agent("agent-1");
    profile.short_term_goal_override = Some("committed owner goal".into());
    sidecar.prompt_profiles.insert("agent-1".into(), profile);
    assert!(
        sidecar
            .sync_canonical_goal_prompt()
            .unwrap_err()
            .contains("unsupported for ProviderBacked")
    );
    assert!(sidecar.canonical_goal_prompt_applied.is_none());
    sidecar.hosted_local_mock_test_lane = true;
    assert!(sidecar.sync_canonical_goal_prompt().is_err()); // no admitted outer Runtime context
    assert!(sidecar.canonical_goal_prompt_applied.is_none());
    // Removing a previously delivered goal still invokes protected clearing;
    // a missing Runtime context must not turn revocation into cached success.
    sidecar.canonical_goal_prompt_applied = Some(("agent-1".into(), Some("intent".into())));
    sidecar
        .provider_service_projection
        .as_mut()
        .unwrap()
        .canonical_agent_chat = None;
    assert!(sidecar.sync_canonical_goal_prompt().is_err());
    assert_eq!(
        sidecar
            .canonical_goal_prompt_applied
            .as_ref()
            .unwrap()
            .1
            .as_deref(),
        Some("intent")
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn canonical_goal_metadata_without_active_goal_does_not_invoke_prompt_control() {
    use oasis7_proto::viewer::*;
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar
        .replace_provider_backed_runner_for_test("agent-1")
        .unwrap();
    let mut projection = crate::world_service::projection::WorldServiceProjection::from_world(
        &RuntimeWorld::default(),
        None,
    )
    .unwrap();
    projection.canonical_agent_chat = Some(CanonicalAgentChatViewV1 {
        agent_id: "agent-1".into(),
        player_id: "owner".into(),
        public_key: "fixture".into(),
        world_id: "world".into(),
        reorg_epoch: 0,
        authority_scope: "player_agent_chat".into(),
        canonical_authority: CanonicalAgentChatAuthorityV1 {
            branch_id: "main".into(),
            agent_identity_generation: 1,
        },
        current_intent_id: None,
        goal: None,
    });
    sidecar.provider_service_projection = Some(projection);
    // An authenticated owner view with no active goal is not a prompt change.
    // ProviderBacked has no Hosted grant here; ordinary feedback/Wait cleanup
    // must proceed without attempting to install a default prompt override.
    sidecar
        .sync_canonical_goal_prompt()
        .expect("empty goal metadata is a no-op");
    assert!(sidecar.canonical_goal_prompt_applied.is_none());
    // owner_view retains the original goal record after terminal transitions,
    // while current_intent_id becomes None. Neither form grants prompt control.
    for status in [
        "completed",
        "rejected",
        "expired",
        "cancelled",
        "superseded",
    ] {
        sidecar
            .provider_service_projection
            .as_mut()
            .unwrap()
            .canonical_agent_chat
            .as_mut()
            .unwrap()
            .goal = Some(CanonicalAgentGoalV1 {
            intent_id: "terminal-intent".into(),
            message: "original goal".into(),
            status: status.into(),
            event_seq: 1,
            logical_time: 1,
        });
        sidecar
            .sync_canonical_goal_prompt()
            .expect("terminal goal is a no-op without prior delivery");
        assert!(sidecar.canonical_goal_prompt_applied.is_none());
    }
}
