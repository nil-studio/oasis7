use super::*;

#[test]
fn replay_rejects_receipt_committed_for_a_different_effect_intent() {
    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    let mut world = World::new_with_state(state);
    let snapshot = world.snapshot();
    world
        .record_agent_chat_intent("player-receipt", AGENT_ID, 1, "Start recipe")
        .unwrap();
    let mut journal = world.journal().clone();
    let accepted_event = journal
        .events
        .iter_mut()
        .find(|event| {
            matches!(
                &event.body,
                WorldEventBody::Domain(DomainEvent::AgentIntentAccepted { .. })
            )
        })
        .expect("find accepted intent event");
    let WorldEventBody::Domain(DomainEvent::AgentIntentAccepted { intent }) =
        &mut accepted_event.body
    else {
        panic!("expected accepted intent");
    };
    intent.effect_intent_id = Some("effect-for-this-intent".to_string());
    let mut completed = intent.clone();
    completed.status = "completed".to_string();
    completed.event_seq = 5;
    completed.updated_at = completed.updated_at.saturating_add(1);
    completed.receipt_ref = Some("world-event:4".to_string());
    journal.events.push(WorldEvent {
        id: 4,
        time: 7,
        caused_by: None,
        body: WorldEventBody::ReceiptAppended(EffectReceipt {
            birth_era: None,
            world: None,
            intent_payload_digest: None,
            intent_id: "different-effect-intent".to_string(),
            status: "ok".to_string(),
            payload: serde_json::json!({}),
            cost_cents: None,
            signature: None,
        }),
    });
    journal.events.push(WorldEvent {
        id: 5,
        time: 8,
        caused_by: None,
        body: WorldEventBody::Domain(DomainEvent::AgentIntentTransitioned { intent: completed }),
    });

    assert!(World::from_snapshot(snapshot, journal).is_err());
}

#[test]
fn durable_record_result_identifies_exact_replay() {
    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    let mut world = World::new_with_state(state);
    let context = AgentIntentAuthorityContext {
        intent_tick: Some(7),
        world_id: Some("runtime-world".to_string()),
        reorg_epoch: Some(2),
        authority_scope: Some("player_agent_chat".to_string()),
        replaces_intent_id: None,
    };
    let first = world
        .record_agent_chat_intent_with_authority(
            "player-replay",
            AGENT_ID,
            1,
            "Start recipe",
            context.clone(),
        )
        .expect("first durable intent");
    assert!(matches!(first, AgentIntentRecordOutcome::Accepted { .. }));

    let replay = world
        .record_agent_chat_intent_with_authority(
            "player-replay",
            AGENT_ID,
            1,
            "Start recipe",
            context,
        )
        .expect("durable replay");
    assert!(matches!(replay, AgentIntentRecordOutcome::Replayed { .. }));
    assert_eq!(first.event_seq(), replay.event_seq());
    assert_eq!(world.journal().events.len(), 3);
    let intent = world.state().agents[AGENT_ID]
        .intent
        .as_ref()
        .expect("accepted runtime intent");
    assert_eq!(
        intent.summary,
        "Agent guidance accepted; the Agent will evaluate its next world action."
    );
    assert!(!intent.summary.contains("Start recipe"));
}

#[test]
fn player_chat_intent_journals_proposed_submitted_accepted_and_replays() {
    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    let mut world = World::new_with_state(state);
    let snapshot = world.snapshot();

    let outcome = world
        .record_agent_chat_intent("player-lifecycle", AGENT_ID, 1, "Start recipe")
        .expect("record lifecycle intent");
    assert_eq!(outcome, 3);
    assert_eq!(world.journal().events.len(), 3);
    assert_eq!(
        world
            .journal()
            .events
            .iter()
            .map(|event| event.id)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    let statuses = world
        .journal()
        .events
        .iter()
        .map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::AgentIntentProposed { intent })
            | WorldEventBody::Domain(DomainEvent::AgentIntentSubmitted { intent })
            | WorldEventBody::Domain(DomainEvent::AgentIntentAccepted { intent }) => {
                intent.status.as_str()
            }
            other => panic!("unexpected lifecycle event: {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(statuses, vec!["proposed", "submitted", "accepted"]);

    let restored =
        World::from_snapshot(snapshot, world.journal().clone()).expect("replay lifecycle journal");
    let intent = restored.state().agents[AGENT_ID]
        .intent
        .as_ref()
        .expect("accepted lifecycle intent");
    assert_eq!(intent.status, "accepted");
    assert_eq!(intent.event_seq, 3);
    assert_eq!(restored.state().agent_intent_ledger.len(), 1);
}

#[test]
fn provider_chat_response_does_not_synthesize_world_effect_or_completion() {
    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    let mut world = World::new_with_state(state);
    let authority = AgentIntentAuthorityContext {
        intent_tick: Some(7),
        world_id: Some("runtime-world".to_string()),
        reorg_epoch: Some(2),
        authority_scope: Some("player_agent_chat".to_string()),
        replaces_intent_id: None,
    };

    world
        .record_agent_chat_intent_with_authority(
            "player-provider-reply",
            AGENT_ID,
            1,
            "Start recipe",
            authority,
        )
        .expect("persist accepted chat intent");
    let accepted = world.state().agents[AGENT_ID]
        .intent
        .clone()
        .expect("accepted chat intent");
    let journal_len = world.journal().events.len();

    // A provider response is an observation until a separately authorized
    // world effect publishes a real receipt. It must not acquire a synthetic
    // effect identity or complete the player intent on its own.
    assert_eq!(accepted.status, "accepted");
    assert!(accepted.effect_intent_id.is_none());
    assert!(
        world
            .complete_agent_intent_with_receipt_exact(
                AGENT_ID,
                accepted.intent_id.as_str(),
                accepted.request_digest.as_str(),
                0,
            )
            .is_err()
    );
    assert_eq!(world.journal().events.len(), journal_len);
    assert_eq!(
        world.state().agents[AGENT_ID]
            .intent
            .as_ref()
            .expect("chat intent remains current")
            .status,
        "accepted"
    );
}

#[test]
fn durable_digest_binds_intent_tick_world_reorg_and_authority_scope() {
    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    let mut world = World::new_with_state(state);
    let original_context = AgentIntentAuthorityContext {
        intent_tick: Some(7),
        world_id: Some("runtime-world".to_string()),
        reorg_epoch: Some(2),
        authority_scope: Some("player_agent_chat".to_string()),
        replaces_intent_id: None,
    };
    world
        .record_agent_chat_intent_with_authority(
            "player-digest",
            AGENT_ID,
            1,
            "Start recipe",
            original_context,
        )
        .expect("first durable intent");

    for changed_context in [
        AgentIntentAuthorityContext {
            intent_tick: Some(8),
            world_id: Some("runtime-world".to_string()),
            reorg_epoch: Some(2),
            authority_scope: Some("player_agent_chat".to_string()),
            replaces_intent_id: None,
        },
        AgentIntentAuthorityContext {
            intent_tick: Some(7),
            world_id: Some("other-world".to_string()),
            reorg_epoch: Some(2),
            authority_scope: Some("player_agent_chat".to_string()),
            replaces_intent_id: None,
        },
        AgentIntentAuthorityContext {
            intent_tick: Some(7),
            world_id: Some("runtime-world".to_string()),
            reorg_epoch: Some(3),
            authority_scope: Some("player_agent_chat".to_string()),
            replaces_intent_id: None,
        },
        AgentIntentAuthorityContext {
            intent_tick: Some(7),
            world_id: Some("runtime-world".to_string()),
            reorg_epoch: Some(2),
            authority_scope: Some("other_scope".to_string()),
            replaces_intent_id: None,
        },
    ] {
        assert!(
            world
                .record_agent_chat_intent_with_authority(
                    "player-digest",
                    AGENT_ID,
                    1,
                    "Start recipe",
                    changed_context,
                )
                .is_err(),
            "same intent identity with changed authority binding must conflict"
        );
    }
}

#[test]
fn durable_replay_returns_latest_terminal_disposition_event_seq() {
    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    let mut world = World::new_with_state(state);
    let context = AgentIntentAuthorityContext {
        intent_tick: Some(7),
        world_id: Some("runtime-world".to_string()),
        reorg_epoch: Some(2),
        authority_scope: Some("player_agent_chat".to_string()),
        replaces_intent_id: None,
    };
    let accepted = world
        .record_agent_chat_intent_with_authority(
            "player-terminal-replay",
            AGENT_ID,
            1,
            "Start recipe",
            context.clone(),
        )
        .expect("accepted intent");
    let accepted_intent = world.state().agents[AGENT_ID]
        .intent
        .clone()
        .expect("accepted runtime intent");
    let mut rejected = accepted_intent.clone();
    rejected.status = "rejected".to_string();
    rejected.summary = lifecycle_summary("rejected").to_string();
    rejected.event_seq = accepted.event_seq().saturating_add(1);
    rejected.updated_at = 8;
    rejected.reason_code = Some("policy_denied".to_string());
    rejected.reason_summary = Some("This instruction is not permitted".to_string());

    let mut terminal_state = world.state().clone();
    terminal_state
        .apply_domain_event(
            &DomainEvent::AgentIntentTransitioned {
                intent: rejected.clone(),
            },
            8,
        )
        .expect("record terminal disposition");
    let mut world = World::new_with_state(terminal_state);
    let replay = world
        .record_agent_chat_intent_with_authority(
            "player-terminal-replay",
            AGENT_ID,
            1,
            "Start recipe",
            context,
        )
        .expect("durable terminal replay");

    assert!(matches!(
        replay,
        AgentIntentRecordOutcome::Replayed { event_seq } if event_seq == rejected.event_seq
    ));
}

#[test]
fn provider_failure_disposition_is_exact_idempotent_and_replayable() {
    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    let mut world = World::new_with_state(state);
    let context = AgentIntentAuthorityContext {
        intent_tick: Some(7),
        world_id: Some("runtime-world".to_string()),
        reorg_epoch: Some(2),
        authority_scope: Some("player_agent_chat".to_string()),
        replaces_intent_id: None,
    };
    world
        .record_agent_chat_intent_with_authority(
            "player-provider-failure",
            AGENT_ID,
            1,
            "Start recipe",
            context.clone(),
        )
        .expect("accepted intent");
    let accepted = world.state().agents[AGENT_ID]
        .intent
        .clone()
        .expect("accepted runtime intent");
    let blocked = world
        .transition_agent_chat_provider_failure_exact(
            AGENT_ID,
            accepted.intent_id.as_str(),
            accepted.request_digest.as_str(),
            AgentIntentProviderFailureDisposition::Blocked,
        )
        .expect("persist provider failure disposition");
    assert_eq!(blocked.intent_id, accepted.intent_id);
    assert_eq!(blocked.status, "blocked");
    assert_eq!(blocked.reason_code.as_deref(), Some("provider_unavailable"));
    assert_eq!(
        blocked.reason_summary.as_deref(),
        Some("Agent service is temporarily unavailable")
    );
    assert_eq!(world.journal().events.len(), 4);

    let replay = world
        .transition_agent_chat_provider_failure_exact(
            AGENT_ID,
            accepted.intent_id.as_str(),
            accepted.request_digest.as_str(),
            AgentIntentProviderFailureDisposition::Blocked,
        )
        .expect("replayed provider failure disposition");
    assert_eq!(replay, blocked);
    assert_eq!(world.journal().events.len(), 4);

    let durable = world
        .agent_chat_intent_replay_disposition(
            "player-provider-failure",
            AGENT_ID,
            1,
            "Start recipe",
            context,
        )
        .expect("lookup durable provider disposition")
        .expect("durable disposition");
    assert_eq!(durable, blocked);
}

#[test]
fn provider_failure_identity_mismatch_has_no_state_effect() {
    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    let mut world = World::new_with_state(state);
    world
        .record_agent_chat_intent("player-provider-identity", AGENT_ID, 1, "Start recipe")
        .expect("accepted intent");
    let accepted = world.state().agents[AGENT_ID]
        .intent
        .clone()
        .expect("accepted runtime intent");
    let before = serde_json::to_vec(world.state()).expect("encode state before mismatch");
    assert!(
        world
            .transition_agent_chat_provider_failure_exact(
                "other-agent",
                accepted.intent_id.as_str(),
                accepted.request_digest.as_str(),
                AgentIntentProviderFailureDisposition::Rejected,
            )
            .is_err()
    );
    assert!(
        world
            .transition_agent_chat_provider_failure_exact(
                AGENT_ID,
                accepted.intent_id.as_str(),
                "wrong-request-digest",
                AgentIntentProviderFailureDisposition::Rejected,
            )
            .is_err()
    );
    assert_eq!(
        serde_json::to_vec(world.state()).expect("encode state after mismatch"),
        before
    );
    assert_eq!(world.journal().events.len(), 3);
}

#[test]
fn provider_advisory_intent_cannot_become_runtime_accepted() {
    let mut advisory = canonical_intent("intent-advisory-1", "accepted", "Start recipe", None);
    advisory["source"] = serde_json::json!("provider_advisory");
    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    assert!(
        state
            .apply_domain_event(&intent_event("AgentIntentAccepted", advisory), 7)
            .is_err(),
        "provider advisory output must not enter the accepted runtime lifecycle"
    );
}

#[test]
fn accepted_domain_event_requires_submitted_predecessor_but_exact_replay_is_idempotent() {
    let accepted = canonical_intent("intent-accepted-boundary", "accepted", "Start recipe", None);
    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());

    assert!(
        state
            .apply_domain_event(&intent_event("AgentIntentAccepted", accepted.clone()), 7)
            .is_err(),
        "an accepted event cannot create or promote an intent without submitted state"
    );
    assert!(state.agent_intent_ledger.is_empty());

    apply_accepted_lifecycle(&mut state, accepted.clone());
    let snapshot = serde_json::to_vec(&state).expect("encode accepted state");
    state
        .apply_domain_event(&intent_event("AgentIntentAccepted", accepted), 99)
        .expect("exact historical accepted replay is a no-op");
    assert_eq!(
        serde_json::to_vec(&state).expect("encode replayed state"),
        snapshot
    );
}

#[test]
fn replacement_and_superseded_transition_require_explicit_accepted_or_blocked_source() {
    let proposed = phase_intent("intent-replace-proposed", "proposed");
    let mut superseded = proposed.clone();
    superseded["status"] = serde_json::json!("superseded");
    superseded["summary"] = serde_json::json!(lifecycle_summary("superseded"));
    superseded["replaced_by"] = serde_json::json!("intent-replacement");
    superseded["event_seq"] = serde_json::json!(12);
    superseded["updated_at"] = serde_json::json!(8);
    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    state
        .apply_domain_event(&intent_event("AgentIntentProposed", proposed), 7)
        .expect("persist proposed source");
    assert!(
        state
            .apply_domain_event(&intent_event("AgentIntentReplaced", superseded.clone()), 8)
            .is_err(),
        "proposed intent cannot be replaced"
    );

    let accepted = canonical_intent("intent-replace-accepted", "accepted", "Start recipe", None);
    let mut accepted_state = WorldState::default();
    accepted_state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    apply_accepted_lifecycle(&mut accepted_state, accepted.clone());
    let mut missing_link = accepted.clone();
    missing_link["status"] = serde_json::json!("superseded");
    missing_link["summary"] = serde_json::json!(lifecycle_summary("superseded"));
    missing_link["event_seq"] = serde_json::json!(20);
    missing_link["updated_at"] = serde_json::json!(9);
    assert!(
        accepted_state
            .apply_domain_event(&intent_event("AgentIntentTransitioned", missing_link), 9)
            .is_err(),
        "superseded transition requires a replacement identity"
    );
}

#[test]
fn provider_advisory_is_durable_without_occupying_authoritative_intent_slot() {
    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    let mut world = World::new_with_state(state);
    world
        .record_provider_advisory_proposed(
            AGENT_ID,
            "provider-advisory-slot",
            "start_recipe",
            Some("factory-1".to_string()),
        )
        .expect("persist advisory proposal");
    assert!(
        world.state().agents[AGENT_ID].intent.is_none(),
        "provider advisory must not block the current authoritative intent slot"
    );
    assert_eq!(
        world
            .state()
            .agent_intent_ledger
            .get("provider-advisory-slot")
            .map(|intent| intent.source.as_str()),
        Some("provider_advisory")
    );
    world
        .record_agent_chat_intent("player-after-advisory", AGENT_ID, 1, "Start recipe")
        .expect("authenticated player intent remains independently admissible");
    assert_eq!(
        world.state().agents[AGENT_ID]
            .intent
            .as_ref()
            .map(|intent| intent.source.as_str()),
        Some("player")
    );
}

#[test]
fn non_superseded_transition_rejects_forged_replacement_link() {
    let accepted = canonical_intent(
        "intent-forged-replacement-link",
        "accepted",
        "Start recipe",
        None,
    );
    let mut blocked = accepted.clone();
    blocked["status"] = serde_json::json!("blocked");
    blocked["summary"] = serde_json::json!(lifecycle_summary("blocked"));
    blocked["reason_code"] = serde_json::json!("provider_unavailable");
    blocked["reason_summary"] = serde_json::json!("Agent service is temporarily unavailable");
    blocked["replaced_by"] = serde_json::json!("intent-not-authoritatively-replaced");
    blocked["event_seq"] = serde_json::json!(12);
    blocked["updated_at"] = serde_json::json!(8);

    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    apply_accepted_lifecycle(&mut state, accepted);

    assert!(
        state
            .apply_domain_event(&intent_event("AgentIntentTransitioned", blocked), 8)
            .is_err(),
        "replaced_by is only valid on an explicit superseding disposition"
    );
}

#[test]
fn non_completed_transition_rejects_forged_receipt_reference() {
    let accepted = canonical_intent(
        "intent-forged-receipt-link",
        "accepted",
        "Start recipe",
        None,
    );
    let mut blocked = accepted.clone();
    blocked["status"] = serde_json::json!("blocked");
    blocked["summary"] = serde_json::json!(lifecycle_summary("blocked"));
    blocked["reason_code"] = serde_json::json!("provider_unavailable");
    blocked["reason_summary"] = serde_json::json!("Agent service is temporarily unavailable");
    blocked["receipt_ref"] = serde_json::json!("world-event:999");
    blocked["event_seq"] = serde_json::json!(12);
    blocked["updated_at"] = serde_json::json!(8);

    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    apply_accepted_lifecycle(&mut state, accepted);

    assert!(
        state
            .apply_domain_event(&intent_event("AgentIntentTransitioned", blocked), 8)
            .is_err(),
        "receipt_ref is only valid on a completed intent with a committed receipt"
    );
}

#[test]
fn intent_updated_at_cannot_rewind_during_transition() {
    let accepted = canonical_intent("intent-monotonic-time", "accepted", "Start recipe", None);
    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    apply_accepted_lifecycle(&mut state, accepted.clone());
    let mut rejected = accepted;
    rejected["status"] = serde_json::json!("rejected");
    rejected["summary"] = serde_json::json!(lifecycle_summary("rejected"));
    rejected["reason_code"] = serde_json::json!("policy_denied");
    rejected["reason_summary"] = serde_json::json!("This instruction is not permitted");
    rejected["event_seq"] = serde_json::json!(12);
    rejected["updated_at"] = serde_json::json!(6);
    assert!(
        state
            .apply_domain_event(&intent_event("AgentIntentTransitioned", rejected), 6)
            .is_err(),
        "a later lifecycle event cannot rewind updated_at"
    );
}

#[test]
fn replay_rejects_journal_events_with_decreasing_world_time() {
    let state = WorldState::default();
    let world = World::new_with_state(state);
    let snapshot = world.snapshot();
    let effect = |intent_id: &str| {
        WorldEventBody::EffectQueued(EffectIntent {
            birth_era: None,
            intent_id: intent_id.to_string(),
            kind: "test_effect".to_string(),
            params: serde_json::json!({}),
            cap_ref: "test".to_string(),
            origin: EffectOrigin::System,
        })
    };
    let journal = Journal {
        events: vec![
            WorldEvent {
                id: 1,
                time: 8,
                caused_by: None,
                body: effect("effect-time-8"),
            },
            WorldEvent {
                id: 2,
                time: 7,
                caused_by: None,
                body: effect("effect-time-7"),
            },
        ],
    };

    let error = World::from_snapshot(snapshot, journal)
        .expect_err("replay must reject a journal time rewind");
    assert!(
        matches!(error, WorldError::ResourceBalanceInvalid { ref reason } if reason.contains("journal event time 7 cannot precede prior event time 8")),
        "unexpected replay error: {error:?}"
    );
}
