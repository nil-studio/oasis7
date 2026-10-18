use super::super::*;
use super::pos;
use crate::agent_state::AgentState;
const AGENT_ID: &str = "agent-intent-v2";

fn legacy_agent_cell() -> AgentCell {
    AgentCell::new(AgentState::new(AGENT_ID, pos(0, 0)), 7)
}

fn canonical_intent(
    intent_id: &str,
    status: &str,
    summary: &str,
    replaced_by: Option<&str>,
) -> serde_json::Value {
    let summary = if summary.chars().count() > 512 {
        summary.to_string()
    } else {
        lifecycle_summary(status).to_string()
    };
    serde_json::json!({
        "schema_version": 2,
        "agent_id": AGENT_ID,
        "intent_id": intent_id,
        "kind": "start_recipe",
        "summary": summary,
        "target_id": "factory-intent-v2",
        "status": status,
        "source": "player",
        "logical_time": 7,
        "event_seq": 11,
        "updated_at": 7,
        "receipt_ref": null,
        "reason_code": null,
        "reason_summary": null,
        "replaced_by": replaced_by,
    })
}

fn cell_with_intent(intent: serde_json::Value) -> AgentCell {
    let mut value = serde_json::to_value(legacy_agent_cell()).expect("encode legacy cell");
    value
        .as_object_mut()
        .expect("agent cell object")
        .insert("intent".to_string(), intent);
    serde_json::from_value(value).expect("decode AgentCell with AgentIntentV2")
}

fn intent_event(event_type: &str, intent: serde_json::Value) -> DomainEvent {
    serde_json::from_value(serde_json::json!({
        "type": event_type,
        "data": { "intent": intent },
    }))
    .expect("decode canonical AgentIntentV2 runtime event")
}

/// Build the canonical lifecycle that precedes an accepted reducer event.
///
/// Domain-event replay must preserve the production proposed -> submitted ->
/// accepted ordering; tests that exercise an accepted event directly use this
/// helper so they do not accidentally encode a bypass of that boundary.
fn apply_accepted_lifecycle(state: &mut WorldState, accepted: serde_json::Value) {
    let accepted_event_seq = accepted["event_seq"]
        .as_u64()
        .expect("accepted intent event_seq");
    let updated_at = accepted["updated_at"]
        .as_u64()
        .expect("accepted intent updated_at");
    let mut proposed = accepted.clone();
    proposed["status"] = serde_json::json!("proposed");
    proposed["summary"] = serde_json::json!(lifecycle_summary("proposed"));
    proposed["event_seq"] = serde_json::json!(accepted_event_seq.saturating_sub(2));
    state
        .apply_domain_event(&intent_event("AgentIntentProposed", proposed), updated_at)
        .expect("persist proposed intent before acceptance");
    let mut submitted = accepted.clone();
    submitted["status"] = serde_json::json!("submitted");
    submitted["summary"] = serde_json::json!(lifecycle_summary("submitted"));
    submitted["event_seq"] = serde_json::json!(accepted_event_seq.saturating_sub(1));
    state
        .apply_domain_event(&intent_event("AgentIntentSubmitted", submitted), updated_at)
        .expect("persist submitted intent before acceptance");
    state
        .apply_domain_event(&intent_event("AgentIntentAccepted", accepted), updated_at)
        .expect("accept submitted intent");
}

fn lifecycle_summary(status: &str) -> &'static str {
    match status {
        "proposed" => "Agent guidance is proposed and not yet accepted.",
        "submitted" => "Agent guidance was submitted and awaits runtime acceptance.",
        "accepted" => "Agent guidance accepted; the Agent will evaluate its next world action.",
        "blocked" => "Agent guidance is blocked pending a runtime recheck.",
        "completed" => "Agent guidance completed with a confirmed world receipt.",
        "rejected" => "Agent guidance was rejected by runtime authority.",
        "expired" => "Agent guidance expired before execution.",
        "cancelled" => "Agent guidance was cancelled before completion.",
        "superseded" => "Agent guidance was replaced by newer guidance.",
        other => panic!("unsupported lifecycle status {other}"),
    }
}

fn phase_intent(intent_id: &str, status: &str) -> serde_json::Value {
    let mut intent = canonical_intent(intent_id, status, "phase-safe copy", None);
    intent["summary"] = serde_json::json!(lifecycle_summary(status));
    intent
}

#[test]
fn legacy_agent_cell_without_intent_remains_explicitly_missing() {
    let cell = legacy_agent_cell();
    let encoded = serde_json::to_value(&cell).expect("encode legacy cell");
    assert!(
        !encoded
            .as_object()
            .expect("agent cell object")
            .contains_key("intent")
    );

    let restored: AgentCell = serde_json::from_value(encoded).expect("decode legacy cell");
    let restored_json = serde_json::to_value(restored).expect("re-encode legacy cell");
    assert!(
        !restored_json
            .as_object()
            .expect("restored agent cell object")
            .contains_key("intent")
    );
}

#[test]
fn accepted_intent_v2_roundtrips_authority_position_and_freshness() {
    let expected = canonical_intent(
        "intent-accepted-1",
        "accepted",
        "Start the iron recipe",
        None,
    );
    let cell = cell_with_intent(expected.clone());
    let encoded = serde_json::to_value(cell).expect("encode accepted AgentIntentV2");

    assert_eq!(encoded.get("intent"), Some(&expected));
    assert_eq!(encoded["intent"]["logical_time"], 7);
    assert_eq!(encoded["intent"]["event_seq"], 11);
    let intent = encoded["intent"]
        .as_object()
        .expect("canonical intent object");
    for projection_only_field in [
        "world_id",
        "reorg_epoch",
        "source_class",
        "freshness",
        "control_state",
    ] {
        assert!(
            !intent.contains_key(projection_only_field),
            "AgentCell intent must not persist projection-only field {projection_only_field}"
        );
    }
}

#[test]
fn replaced_and_terminal_intents_keep_explicit_lifecycle_dispositions() {
    for (status, replaced_by) in [
        ("blocked", None),
        ("completed", None),
        ("rejected", None),
        ("expired", None),
        ("cancelled", None),
        ("superseded", Some("intent-replacement-2")),
    ] {
        let expected = canonical_intent(
            "intent-lifecycle-1",
            status,
            "Keep this intent auditable",
            replaced_by,
        );
        let encoded = serde_json::to_value(cell_with_intent(expected.clone()))
            .expect("encode lifecycle AgentIntentV2");
        assert_eq!(encoded.get("intent"), Some(&expected));
        assert_eq!(encoded["intent"]["status"], status);
        assert_eq!(
            encoded["intent"]["replaced_by"],
            serde_json::json!(replaced_by)
        );
    }
}

#[test]
fn duplicate_intent_identity_is_idempotent_and_conflict_is_not_silent() {
    let accepted = canonical_intent("intent-idempotency-1", "accepted", "Start recipe", None);
    let mut conflict = canonical_intent(
        "intent-idempotency-1",
        "accepted",
        "Silently replace the original intent",
        None,
    );
    conflict["summary"] = serde_json::json!("Silently replace the original intent");

    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    let accepted_event = intent_event("AgentIntentAccepted", accepted.clone());
    apply_accepted_lifecycle(&mut state, accepted.clone());
    let after_accept = serde_json::to_vec(&state).expect("encode accepted state");

    state
        .apply_domain_event(&accepted_event, 8)
        .expect("replay identical intent");
    assert_eq!(
        serde_json::to_vec(&state).expect("encode replayed state"),
        after_accept,
        "replaying one intent_id must not create a second mutation"
    );

    let conflict_event = intent_event("AgentIntentAccepted", conflict);
    assert!(
        state.apply_domain_event(&conflict_event, 9).is_err(),
        "same intent_id with different content must be an explicit conflict"
    );
    assert_eq!(
        serde_json::to_vec(&state).expect("encode conflict state"),
        after_accept,
        "an intent identity conflict must not silently overwrite authority"
    );
}

#[test]
fn replaced_intent_event_is_replayable_without_promoting_activity_or_receipt() {
    let accepted = canonical_intent("intent-replay-1", "accepted", "Start recipe", None);
    let superseded = canonical_intent(
        "intent-replay-1",
        "superseded",
        "Start recipe",
        Some("intent-replacement-2"),
    );
    let mut superseded = superseded;
    superseded["event_seq"] = serde_json::json!(12);
    superseded["updated_at"] = serde_json::json!(8);
    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    apply_accepted_lifecycle(&mut state, accepted);
    state
        .apply_domain_event(&intent_event("AgentIntentReplaced", superseded), 8)
        .expect("replace original intent");

    let encoded = serde_json::to_value(&state).expect("encode replaced intent state");
    let decoded: WorldState =
        serde_json::from_value(encoded.clone()).expect("decode replaced state");
    assert_eq!(
        serde_json::to_value(decoded).expect("re-encode replaced state"),
        encoded,
        "replay/snapshot roundtrip must preserve the replacement identity"
    );
    let agent = &encoded["agents"][AGENT_ID];
    assert!(
        agent.get("activity").is_none(),
        "intent must not become activity"
    );
    assert!(
        agent.get("receipt").is_none(),
        "intent must not become a world receipt"
    );
}

#[test]
fn blocked_intent_cannot_resume_without_a_dedicated_authority_proof() {
    let mut accepted = canonical_intent("intent-lifecycle-2", "accepted", "Start recipe", None);
    accepted["effect_intent_id"] = serde_json::json!("effect-intent-lifecycle-2");
    let mut blocked = accepted.clone();
    blocked["status"] = serde_json::json!("blocked");
    blocked["summary"] = serde_json::json!(lifecycle_summary("blocked"));
    blocked["reason_code"] = serde_json::json!("insufficient_power");
    blocked["reason_summary"] = serde_json::json!("Restore power to continue");
    blocked["event_seq"] = serde_json::json!(12);
    let mut resumed = accepted.clone();
    resumed["event_seq"] = serde_json::json!(13);
    resumed["updated_at"] = serde_json::json!(8);
    let mut completed_without_receipt = resumed.clone();
    completed_without_receipt["status"] = serde_json::json!("completed");
    completed_without_receipt["summary"] = serde_json::json!(lifecycle_summary("completed"));
    completed_without_receipt["event_seq"] = serde_json::json!(14);
    let mut completed = completed_without_receipt.clone();
    completed["receipt_ref"] = serde_json::json!("world-event:14");

    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    apply_accepted_lifecycle(&mut state, accepted);
    state
        .apply_domain_event(&intent_event("AgentIntentTransitioned", blocked), 7)
        .unwrap();
    assert_eq!(
        state.agents[AGENT_ID].intent.as_ref().unwrap().status,
        "blocked"
    );
    let blocked_state = serde_json::to_vec(&state).expect("encode blocked state");
    assert!(
        state
            .apply_domain_event(&intent_event("AgentIntentTransitioned", resumed), 8)
            .is_err(),
        "blocked -> accepted requires a dedicated authority-proof API"
    );
    assert_eq!(
        state.agents[AGENT_ID].intent.as_ref().unwrap().status,
        "blocked"
    );
    assert_eq!(
        serde_json::to_vec(&state).expect("encode unchanged blocked state"),
        blocked_state
    );
    assert!(
        state
            .apply_domain_event(
                &intent_event("AgentIntentTransitioned", completed_without_receipt),
                9
            )
            .is_err()
    );
    assert!(
        state
            .apply_domain_event(
                &intent_event("AgentIntentTransitioned", completed.clone()),
                9,
            )
            .is_err(),
        "direct WorldState reducers must not accept a completion without a committed receipt witness"
    );
}

#[test]
fn proposed_and_submitted_intents_have_durable_terminal_paths() {
    let mut proposed_state = WorldState::default();
    proposed_state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    proposed_state
        .apply_domain_event(
            &intent_event(
                "AgentIntentProposed",
                phase_intent("intent-proposed-rejected", "proposed"),
            ),
            7,
        )
        .expect("persist proposed intent");
    let mut proposed_rejected = phase_intent("intent-proposed-rejected", "rejected");
    proposed_rejected["event_seq"] = serde_json::json!(12);
    proposed_rejected["updated_at"] = serde_json::json!(8);
    proposed_rejected["reason_code"] = serde_json::json!("policy_denied");
    proposed_rejected["reason_summary"] = serde_json::json!("This instruction is not permitted");
    proposed_state
        .apply_domain_event(
            &intent_event("AgentIntentTransitioned", proposed_rejected),
            8,
        )
        .expect("proposed intent can be rejected");
    assert_eq!(
        proposed_state.agents[AGENT_ID]
            .intent
            .as_ref()
            .expect("rejected intent")
            .status,
        "rejected"
    );

    let mut submitted_state = WorldState::default();
    submitted_state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    submitted_state
        .apply_domain_event(
            &intent_event(
                "AgentIntentProposed",
                phase_intent("intent-submitted-cancelled", "proposed"),
            ),
            7,
        )
        .expect("persist proposed cancellation candidate");
    let mut submitted = phase_intent("intent-submitted-cancelled", "submitted");
    submitted["event_seq"] = serde_json::json!(12);
    submitted["updated_at"] = serde_json::json!(8);
    submitted_state
        .apply_domain_event(&intent_event("AgentIntentSubmitted", submitted.clone()), 8)
        .expect("persist submitted intent");
    submitted["status"] = serde_json::json!("cancelled");
    submitted["summary"] = serde_json::json!(lifecycle_summary("cancelled"));
    submitted["event_seq"] = serde_json::json!(13);
    submitted["updated_at"] = serde_json::json!(9);
    submitted_state
        .apply_domain_event(&intent_event("AgentIntentTransitioned", submitted), 9)
        .expect("submitted intent can be cancelled");
    assert_eq!(
        submitted_state.agents[AGENT_ID]
            .intent
            .as_ref()
            .expect("cancelled intent")
            .status,
        "cancelled"
    );
}

#[test]
fn lifecycle_events_use_phase_specific_player_safe_copy() {
    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    let mut world = World::new_with_state(state);
    world
        .record_agent_chat_intent("player-phase-copy", AGENT_ID, 1, "Start recipe")
        .expect("record lifecycle intent");
    let summaries = world
        .journal()
        .events
        .iter()
        .map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::AgentIntentProposed { intent })
            | WorldEventBody::Domain(DomainEvent::AgentIntentSubmitted { intent })
            | WorldEventBody::Domain(DomainEvent::AgentIntentAccepted { intent }) => {
                intent.summary.as_str()
            }
            other => panic!("unexpected lifecycle event: {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        summaries,
        vec![
            lifecycle_summary("proposed"),
            lifecycle_summary("submitted"),
            lifecycle_summary("accepted")
        ]
    );
}

#[test]
fn provider_advisory_can_persist_only_as_a_proposed_suggestion() {
    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    let mut advisory = phase_intent("intent-advisory-proposed", "proposed");
    advisory["source"] = serde_json::json!("provider_advisory");
    let event = intent_event("AgentIntentProposed", advisory);
    state
        .apply_domain_event(&event, 7)
        .expect("provider advisory proposed intent is a safe suggestion");
    assert!(
        state.agents[AGENT_ID].intent.is_none(),
        "advisory must not occupy the authoritative current intent slot"
    );
    assert_eq!(
        state
            .agent_intent_ledger
            .get("intent-advisory-proposed")
            .expect("advisory ledger entry")
            .source,
        "provider_advisory"
    );
    assert!(
        state
            .apply_domain_event(
                &intent_event(
                    "AgentIntentAccepted",
                    phase_intent("intent-advisory-accepted", "accepted")
                ),
                8,
            )
            .is_err()
    );
}

#[test]
fn provider_advisory_runtime_api_persists_and_replays_a_proposed_suggestion() {
    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    let mut world = World::new_with_state(state);
    let first = world
        .record_provider_advisory_proposed(
            AGENT_ID,
            "provider-advisory-1",
            "start_recipe",
            Some("factory-1".to_string()),
        )
        .expect("persist provider advisory proposal");
    assert_eq!(first.status, "proposed");
    assert!(
        world.state().agents[AGENT_ID].intent.is_none(),
        "proposed advisory must remain separate from the authoritative slot"
    );
    assert_eq!(
        world
            .state()
            .agent_intent_ledger
            .get("provider-advisory-1")
            .expect("proposed advisory ledger entry")
            .source,
        "provider_advisory"
    );
    let replay = world
        .record_provider_advisory_proposed(
            AGENT_ID,
            "provider-advisory-1",
            "start_recipe",
            Some("factory-1".to_string()),
        )
        .expect("replay provider advisory proposal");
    assert_eq!(replay.event_seq, first.event_seq);
    assert_eq!(world.journal().events.len(), 1);
    assert!(matches!(
        world.journal().events[0].body,
        WorldEventBody::Domain(DomainEvent::AgentIntentProposed { .. })
    ));
}

#[test]
fn live_terminal_apis_persist_expiry_and_cancellation_for_exact_identity() {
    let authority = AgentIntentAuthorityContext {
        intent_tick: Some(7),
        world_id: Some("runtime-world".to_string()),
        reorg_epoch: Some(2),
        authority_scope: Some("player_agent_chat".to_string()),
        replaces_intent_id: None,
    };
    let mut expired_state = WorldState::default();
    expired_state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    let mut expired_world = World::new_with_state(expired_state);
    expired_world
        .record_agent_chat_intent_with_authority(
            "player-terminal",
            AGENT_ID,
            1,
            "Expire this instruction",
            authority.clone(),
        )
        .expect("persist accepted intent");
    let intent = expired_world.state().agents[AGENT_ID]
        .intent
        .as_ref()
        .expect("current intent")
        .clone();
    let expired = expired_world
        .expire_agent_intent_exact(
            AGENT_ID,
            intent.intent_id.as_str(),
            intent.request_digest.as_str(),
        )
        .expect("persist expiry");
    assert_eq!(expired.status, "expired");
    assert_eq!(
        expired_world.state().agents[AGENT_ID]
            .intent
            .as_ref()
            .unwrap()
            .summary,
        lifecycle_summary("expired")
    );
    assert_eq!(
        expired_world
            .expire_agent_intent_exact(
                AGENT_ID,
                intent.intent_id.as_str(),
                intent.request_digest.as_str(),
            )
            .expect("repeated expiry is idempotent")
            .event_seq,
        expired.event_seq
    );

    let mut cancelled_state = WorldState::default();
    cancelled_state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    let mut cancelled_world = World::new_with_state(cancelled_state);
    cancelled_world
        .record_agent_chat_intent_with_authority(
            "player-terminal",
            AGENT_ID,
            2,
            "Cancel this instruction",
            authority,
        )
        .expect("persist second accepted intent");
    let intent = cancelled_world.state().agents[AGENT_ID]
        .intent
        .as_ref()
        .expect("current cancellation candidate")
        .clone();
    let cancelled = cancelled_world
        .cancel_agent_intent_exact(
            AGENT_ID,
            intent.intent_id.as_str(),
            intent.request_digest.as_str(),
        )
        .expect("persist cancellation");
    assert_eq!(cancelled.status, "cancelled");
    assert_eq!(
        cancelled_world.state().agents[AGENT_ID]
            .intent
            .as_ref()
            .unwrap()
            .summary,
        lifecycle_summary("cancelled")
    );
}

#[test]
fn live_completion_api_requires_matching_committed_receipt_and_replays() {
    let authority = AgentIntentAuthorityContext {
        intent_tick: Some(7),
        world_id: Some("runtime-world".to_string()),
        reorg_epoch: Some(2),
        authority_scope: Some("player_agent_chat".to_string()),
        replaces_intent_id: None,
    };
    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    let mut world = World::new_with_state(state);
    let snapshot = world.snapshot();
    world
        .record_agent_chat_intent_with_authority(
            "player-complete",
            AGENT_ID,
            1,
            "Complete this instruction",
            authority,
        )
        .expect("persist accepted intent");
    let mut journal = world.journal().clone();
    let mut accepted_found = false;
    for event in &mut journal.events {
        match &mut event.body {
            WorldEventBody::Domain(DomainEvent::AgentIntentProposed { intent })
            | WorldEventBody::Domain(DomainEvent::AgentIntentSubmitted { intent })
            | WorldEventBody::Domain(DomainEvent::AgentIntentAccepted { intent }) => {
                intent.effect_intent_id = Some("effect-live-complete".to_string());
                accepted_found |= intent.status == "accepted";
            }
            _ => {}
        }
    }
    assert!(accepted_found, "find accepted lifecycle event");
    journal.events.push(WorldEvent {
        id: 4,
        time: 7,
        caused_by: None,
        body: WorldEventBody::EffectQueued(EffectIntent {
            birth_era: None,
            intent_id: "effect-live-complete".to_string(),
            kind: "test_effect".to_string(),
            params: serde_json::json!({}),
            cap_ref: "test".to_string(),
            origin: EffectOrigin::System,
        }),
    });
    journal.events.push(WorldEvent {
        id: 5,
        time: 7,
        caused_by: Some(CausedBy::Effect {
            intent_id: "effect-live-complete".to_string(),
        }),
        body: WorldEventBody::ReceiptAppended(EffectReceipt {
            birth_era: None,
            world: None,
            intent_payload_digest: None,
            intent_id: "effect-live-complete".to_string(),
            status: "ok".to_string(),
            payload: serde_json::json!({"ok": true}),
            cost_cents: None,
            signature: None,
        }),
    });
    let mut restored = World::from_snapshot(snapshot, journal).expect("replay live receipt");
    let accepted = restored.state().agents[AGENT_ID]
        .intent
        .as_ref()
        .expect("accepted intent after replay")
        .clone();
    let completed = restored
        .complete_agent_intent_with_receipt_exact(
            AGENT_ID,
            accepted.intent_id.as_str(),
            accepted.request_digest.as_str(),
            5,
        )
        .expect("persist receipt-bound completion");
    assert_eq!(completed.status, "completed");
    assert_eq!(completed.receipt_ref.as_deref(), Some("world-event:5"));
    assert_eq!(
        restored.state().agents[AGENT_ID]
            .intent
            .as_ref()
            .unwrap()
            .summary,
        lifecycle_summary("completed")
    );
    let replay = restored
        .complete_agent_intent_with_receipt_exact(
            AGENT_ID,
            accepted.intent_id.as_str(),
            accepted.request_digest.as_str(),
            5,
        )
        .expect("repeated completion is idempotent");
    assert_eq!(replay.event_seq, completed.event_seq);
}

#[test]
fn terminal_intent_is_immutable_and_transition_replay_is_idempotent() {
    let accepted = canonical_intent("intent-terminal-1", "accepted", "Start recipe", None);
    let mut rejected = accepted.clone();
    rejected["status"] = serde_json::json!("rejected");
    rejected["summary"] = serde_json::json!(lifecycle_summary("rejected"));
    rejected["reason_code"] = serde_json::json!("policy_denied");
    rejected["reason_summary"] = serde_json::json!("This instruction is not permitted");
    rejected["event_seq"] = serde_json::json!(12);
    rejected["updated_at"] = serde_json::json!(8);
    let mut cancelled = rejected.clone();
    cancelled["status"] = serde_json::json!("cancelled");
    cancelled["summary"] = serde_json::json!(lifecycle_summary("cancelled"));
    cancelled["event_seq"] = serde_json::json!(13);

    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    apply_accepted_lifecycle(&mut state, accepted);
    let event = intent_event("AgentIntentTransitioned", rejected);
    state.apply_domain_event(&event, 8).unwrap();
    let terminal = serde_json::to_vec(&state).unwrap();
    state.apply_domain_event(&event, 9).unwrap();
    assert_eq!(serde_json::to_vec(&state).unwrap(), terminal);
    assert!(
        state
            .apply_domain_event(&intent_event("AgentIntentTransitioned", cancelled), 10)
            .is_err()
    );
    assert_eq!(serde_json::to_vec(&state).unwrap(), terminal);
}

#[test]
fn transition_rejects_mutating_immutable_payload_or_rewinding_event_sequence() {
    let accepted = canonical_intent("intent-immutable-1", "accepted", "Start recipe", None);
    let mut mutated = accepted.clone();
    mutated["status"] = serde_json::json!("blocked");
    mutated["summary"] = serde_json::json!("Rewrite the original instruction");
    mutated["reason_code"] = serde_json::json!("insufficient_power");
    mutated["event_seq"] = serde_json::json!(10);

    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    apply_accepted_lifecycle(&mut state, accepted);
    let before = serde_json::to_vec(&state).unwrap();

    assert!(
        state
            .apply_domain_event(&intent_event("AgentIntentTransitioned", mutated), 8)
            .is_err()
    );
    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
}

#[test]
fn historical_duplicate_after_terminal_replacement_is_an_idempotent_noop() {
    let old = canonical_intent("intent-history-old", "accepted", "Start recipe", None);
    let mut superseded = old.clone();
    superseded["status"] = serde_json::json!("superseded");
    superseded["summary"] = serde_json::json!(lifecycle_summary("superseded"));
    superseded["replaced_by"] = serde_json::json!("intent-history-new");
    superseded["event_seq"] = serde_json::json!(12);
    superseded["updated_at"] = serde_json::json!(8);
    let mut new = canonical_intent("intent-history-new", "accepted", "Stop recipe", None);
    new["logical_time"] = serde_json::json!(9);
    new["updated_at"] = serde_json::json!(9);
    let mut terminal = new.clone();
    terminal["status"] = serde_json::json!("rejected");
    terminal["summary"] = serde_json::json!(lifecycle_summary("rejected"));
    terminal["reason_code"] = serde_json::json!("policy_denied");
    terminal["reason_summary"] = serde_json::json!("This instruction is not permitted");
    terminal["event_seq"] = serde_json::json!(13);
    terminal["updated_at"] = serde_json::json!(10);

    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    apply_accepted_lifecycle(&mut state, old.clone());
    state
        .apply_domain_event(&intent_event("AgentIntentReplaced", superseded), 8)
        .unwrap();
    apply_accepted_lifecycle(&mut state, new);
    state
        .apply_domain_event(&intent_event("AgentIntentTransitioned", terminal), 10)
        .unwrap();

    let before_retry = serde_json::to_vec(&state).unwrap();
    state
        .apply_domain_event(&intent_event("AgentIntentAccepted", old), 11)
        .unwrap();
    assert_eq!(serde_json::to_vec(&state).unwrap(), before_retry);
}

#[test]
fn completion_requires_a_nonzero_committed_world_event_reference() {
    let mut accepted = canonical_intent("intent-receipt-1", "accepted", "Start recipe", None);
    accepted["effect_intent_id"] = serde_json::json!("effect-intent-receipt-1");
    let mut completed = accepted.clone();
    completed["status"] = serde_json::json!("completed");
    completed["summary"] = serde_json::json!(lifecycle_summary("completed"));
    completed["event_seq"] = serde_json::json!(12);
    completed["receipt_ref"] = serde_json::json!("world-event:0");

    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    apply_accepted_lifecycle(&mut state, accepted);
    assert!(
        state
            .apply_domain_event(&intent_event("AgentIntentTransitioned", completed), 8)
            .is_err()
    );
}

#[test]
fn intent_summary_is_bounded_before_it_reaches_persistent_state() {
    let accepted = canonical_intent(
        "intent-summary-bound-1",
        "accepted",
        &"x".repeat(1024),
        None,
    );
    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    assert!(
        state
            .apply_domain_event(&intent_event("AgentIntentAccepted", accepted), 7)
            .is_err()
    );
}

#[test]
fn accepted_intent_rejects_noncanonical_player_visible_copy() {
    let mut accepted = canonical_intent(
        "intent-summary-canonical-1",
        "accepted",
        "Start recipe",
        None,
    );
    accepted["summary"] = serde_json::json!("raw prompt or provider rationale");
    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    assert!(
        state
            .apply_domain_event(&intent_event("AgentIntentAccepted", accepted), 7)
            .is_err()
    );
}

#[test]
fn replay_rejects_an_intent_payload_that_does_not_match_its_world_event_envelope() {
    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    let mut world = World::new_with_state(state);
    let snapshot = world.snapshot();
    let event_id = world
        .record_agent_chat_intent("player-envelope", AGENT_ID, 1, "Start recipe")
        .expect("record canonical intent");
    let mut tampered_journal = world.journal().clone();
    let accepted_event = tampered_journal
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
        panic!("expected canonical intent event");
    };
    intent.event_seq = event_id.saturating_add(1);

    let result = World::from_snapshot(snapshot, tampered_journal);
    assert!(result.is_err(), "tampered payload must fail replay");
}

#[test]
fn world_api_exact_historical_retry_returns_latest_terminal_without_reactivating_it() {
    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    let mut world = World::new_with_state(state);
    let original = world
        .record_agent_chat_intent("player-history", AGENT_ID, 1, "Start recipe")
        .unwrap();
    let replaces_intent_id = world.state().agents[AGENT_ID]
        .intent
        .as_ref()
        .map(|intent| intent.intent_id.clone());
    world
        .record_agent_chat_intent_with_authority(
            "player-history",
            AGENT_ID,
            2,
            "Stop recipe",
            AgentIntentAuthorityContext {
                replaces_intent_id,
                ..AgentIntentAuthorityContext::default()
            },
        )
        .unwrap();
    let before_retry = world.state().agents[AGENT_ID].intent.clone().unwrap();

    let replay = world
        .record_agent_chat_intent("player-history", AGENT_ID, 1, "Start recipe")
        .unwrap();

    let latest_terminal = world
        .state()
        .agent_intent_ledger
        .values()
        .find(|intent| {
            intent.actor_id == "player-history" && intent.intent_id != before_retry.intent_id
        })
        .expect("superseded intent disposition");
    assert_eq!(latest_terminal.status, "superseded");
    assert!(latest_terminal.event_seq > original);
    assert_eq!(replay, latest_terminal.event_seq);
    assert_eq!(
        world.state().agents[AGENT_ID].intent.as_ref(),
        Some(&before_retry)
    );
}

#[test]
fn canonical_identity_and_request_digest_do_not_collide_on_embedded_nul() {
    let mut first_state = WorldState::default();
    first_state.agents.insert(
        "c".to_string(),
        AgentCell::new(AgentState::new("c", pos(0, 0)), 7),
    );
    let mut first = World::new_with_state(first_state);
    first
        .record_agent_chat_intent("a\0agent=b", "c", 1, "hello")
        .expect("record first NUL-shaped identity");
    let first_intent = first.state().agents["c"]
        .intent
        .as_ref()
        .expect("first accepted intent");

    let mut second_state = WorldState::default();
    second_state.agents.insert(
        "b\0agent=c".to_string(),
        AgentCell::new(AgentState::new("b\0agent=c", pos(0, 0)), 7),
    );
    let mut second = World::new_with_state(second_state);
    second
        .record_agent_chat_intent("a", "b\0agent=c", 1, "hello")
        .expect("record second NUL-shaped identity");
    let second_intent = second.state().agents["b\0agent=c"]
        .intent
        .as_ref()
        .expect("second accepted intent");

    assert_ne!(first_intent.intent_id, second_intent.intent_id);
    assert_ne!(first_intent.request_digest, second_intent.request_digest);
}

#[test]
fn authority_context_rejects_partial_world_reorg_scope_tuple() {
    let partial_contexts = [
        AgentIntentAuthorityContext {
            intent_tick: Some(7),
            world_id: Some("runtime-world".to_string()),
            reorg_epoch: None,
            authority_scope: Some("player_agent_chat".to_string()),
            replaces_intent_id: None,
        },
        AgentIntentAuthorityContext {
            intent_tick: Some(7),
            world_id: Some("runtime-world".to_string()),
            reorg_epoch: Some(2),
            authority_scope: None,
            replaces_intent_id: None,
        },
        AgentIntentAuthorityContext {
            intent_tick: Some(7),
            world_id: None,
            reorg_epoch: Some(2),
            authority_scope: Some("player_agent_chat".to_string()),
            replaces_intent_id: None,
        },
        AgentIntentAuthorityContext {
            intent_tick: Some(7),
            world_id: Some(" ".to_string()),
            reorg_epoch: Some(2),
            authority_scope: Some("player_agent_chat".to_string()),
            replaces_intent_id: None,
        },
    ];

    for authority in partial_contexts {
        let mut state = WorldState::default();
        state
            .agents
            .insert(AGENT_ID.to_string(), legacy_agent_cell());
        let mut world = World::new_with_state(state);
        assert!(
            world
                .record_agent_chat_intent_with_authority(
                    "player-partial-authority",
                    AGENT_ID,
                    1,
                    "Start recipe",
                    authority,
                )
                .is_err(),
            "partial authority tuple must be rejected"
        );
        assert!(world.journal().events.is_empty());
    }
}

#[test]
fn proposed_or_submitted_intent_cannot_be_superseded() {
    for status in ["proposed", "submitted"] {
        let mut state = WorldState::default();
        state.agents.insert(
            AGENT_ID.to_string(),
            cell_with_intent(canonical_intent(
                "pending-intent",
                status,
                "Start recipe",
                None,
            )),
        );
        let mut world = World::new_with_state(state);
        let result = world.record_agent_chat_intent_with_authority(
            "player-replacement",
            AGENT_ID,
            2,
            "Stop recipe",
            AgentIntentAuthorityContext {
                replaces_intent_id: Some("pending-intent".to_string()),
                ..AgentIntentAuthorityContext::default()
            },
        );

        assert!(
            format!(
                "{:?}",
                result.expect_err("pre-acceptance replacement must be rejected")
            )
            .contains("cannot be superseded before acceptance")
        );
        assert!(world.journal().events.is_empty());
        assert_eq!(
            world.state().agents[AGENT_ID]
                .intent
                .as_ref()
                .unwrap()
                .status,
            status
        );
    }
}

#[test]
fn canonical_world_replay_accepts_completion_only_after_matching_receipt_commit() {
    let mut state = WorldState::default();
    state
        .agents
        .insert(AGENT_ID.to_string(), legacy_agent_cell());
    let world = World::new_with_state(state);
    let snapshot = world.snapshot();
    let mut accepted =
        canonical_intent("intent-receipt-committed", "accepted", "Start recipe", None);
    accepted["effect_intent_id"] = serde_json::json!("effect-receipt-committed");
    accepted["world_id"] = serde_json::json!("runtime-world");
    accepted["reorg_epoch"] = serde_json::json!(2);
    accepted["authority_scope"] = serde_json::json!("player_agent_chat");
    accepted["event_seq"] = serde_json::json!(3);
    let mut proposed = accepted.clone();
    proposed["status"] = serde_json::json!("proposed");
    proposed["summary"] = serde_json::json!(lifecycle_summary("proposed"));
    proposed["event_seq"] = serde_json::json!(1);
    let mut submitted = accepted.clone();
    submitted["status"] = serde_json::json!("submitted");
    submitted["summary"] = serde_json::json!(lifecycle_summary("submitted"));
    submitted["event_seq"] = serde_json::json!(2);
    let mut completed = accepted.clone();
    completed["status"] = serde_json::json!("completed");
    completed["summary"] = serde_json::json!(lifecycle_summary("completed"));
    completed["event_seq"] = serde_json::json!(6);
    completed["updated_at"] = serde_json::json!(8);
    completed["receipt_ref"] = serde_json::json!("world-event:5");
    let journal = Journal {
        events: vec![
            WorldEvent {
                id: 1,
                time: 7,
                caused_by: None,
                body: WorldEventBody::Domain(DomainEvent::AgentIntentProposed {
                    intent: serde_json::from_value(proposed).expect("proposed intent"),
                }),
            },
            WorldEvent {
                id: 2,
                time: 7,
                caused_by: None,
                body: WorldEventBody::Domain(DomainEvent::AgentIntentSubmitted {
                    intent: serde_json::from_value(submitted).expect("submitted intent"),
                }),
            },
            WorldEvent {
                id: 3,
                time: 7,
                caused_by: None,
                body: WorldEventBody::Domain(DomainEvent::AgentIntentAccepted {
                    intent: serde_json::from_value(accepted).expect("accepted intent"),
                }),
            },
            WorldEvent {
                id: 4,
                time: 7,
                caused_by: None,
                body: WorldEventBody::EffectQueued(EffectIntent {
                    birth_era: None,
                    intent_id: "effect-receipt-committed".to_string(),
                    kind: "test_effect".to_string(),
                    params: serde_json::json!({}),
                    cap_ref: "test".to_string(),
                    origin: EffectOrigin::System,
                }),
            },
            WorldEvent {
                id: 5,
                time: 7,
                caused_by: None,
                body: WorldEventBody::ReceiptAppended(EffectReceipt {
                    birth_era: None,
                    world: None,
                    intent_payload_digest: None,
                    intent_id: "effect-receipt-committed".to_string(),
                    status: "ok".to_string(),
                    payload: serde_json::json!({}),
                    cost_cents: None,
                    signature: None,
                }),
            },
            WorldEvent {
                id: 6,
                time: 8,
                caused_by: None,
                body: WorldEventBody::Domain(DomainEvent::AgentIntentTransitioned {
                    intent: serde_json::from_value(completed).expect("completed intent"),
                }),
            },
        ],
    };

    let restored = World::from_snapshot(snapshot, journal).expect("committed receipt replay");
    assert_eq!(
        restored.state().agents[AGENT_ID]
            .intent
            .as_ref()
            .expect("completed intent")
            .status,
        "completed"
    );
}

#[path = "agent_intent_v2/lifecycle.rs"]
mod lifecycle;
#[path = "agent_intent_v2/raw_publication_transaction_regressions.rs"]
mod raw_publication_transaction_regressions;

#[path = "agent_intent_v2/receipt_reconciliation.rs"]
mod receipt_reconciliation;
