use super::*;

#[test]
fn viewer_playback_control_request_round_trip() {
    let request = ViewerRequest::PlaybackControl {
        mode: PlaybackControl::Seek { tick: 24 },
        request_id: Some(11),
    };
    let json = serde_json::to_string(&request).expect("serialize request");
    let parsed: ViewerRequest = serde_json::from_str(&json).expect("deserialize request");
    assert_eq!(parsed, request);
}

#[test]
fn viewer_live_control_request_round_trip() {
    let request = ViewerRequest::LiveControl {
        mode: LiveControl::Step { count: 3 },
        request_id: Some(13),
    };
    let json = serde_json::to_string(&request).expect("serialize request");
    let parsed: ViewerRequest = serde_json::from_str(&json).expect("deserialize request");
    assert_eq!(parsed, request);
}

#[test]
fn viewer_control_request_defaults_request_id_to_none_for_compat_payload() {
    let request = ViewerRequest::Control {
        mode: ViewerControl::Step { count: 2 },
        request_id: None,
    };
    let json = serde_json::to_string(&request).expect("serialize request");
    assert!(!json.contains("request_id"));
    let parsed: ViewerRequest = serde_json::from_str(&json).expect("deserialize request");
    let ViewerRequest::Control { request_id, .. } = parsed else {
        panic!("expected control request");
    };
    assert_eq!(request_id, None);
}

#[test]
fn viewer_subscribe_round_trip_with_filters() {
    let request = ViewerRequest::Subscribe {
        streams: vec![ViewerStream::Events],
        event_kinds: vec![ViewerEventKind::AgentMoved, ViewerEventKind::Power],
    };
    let json = serde_json::to_string(&request).expect("serialize subscribe");
    let parsed: ViewerRequest = serde_json::from_str(&json).expect("deserialize subscribe");
    assert_eq!(parsed, request);
}

#[test]
fn viewer_prompt_control_request_round_trip() {
    let request = ViewerRequest::PromptControl {
        command: Box::new(PromptControlCommand::Apply {
            request: PromptControlApplyRequest {
                agent_id: "agent-0".to_string(),
                player_id: "player-1".to_string(),
                request_id: None,
                session_epoch: None,
                binding_epoch: None,
                expected_authority_epoch: None,
                public_key: Some("pk-1".to_string()),
                auth: Some(PlayerAuthProof {
                    scheme: PlayerAuthScheme::Ed25519,
                    player_id: "player-1".to_string(),
                    public_key: "pk-1".to_string(),
                    nonce: 7,
                    signature: "awviewauth:v1:deadbeef".to_string(),
                }),
                strong_auth_grant: None,
                expected_version: Some(3),
                updated_by: Some("tester".to_string()),
                system_prompt_override: Some(Some("system".to_string())),
                short_term_goal_override: Some(None),
                long_term_goal_override: None,
            },
        }),
    };
    let json = serde_json::to_string(&request).expect("serialize request");
    let value: serde_json::Value =
        serde_json::from_str(&json).expect("serialized request should be json");
    assert_eq!(value["type"], "prompt_control");
    assert_eq!(value["command"]["mode"], "apply");
    assert_eq!(value["command"]["request"]["agent_id"], "agent-0");
    let parsed: ViewerRequest = serde_json::from_str(&json).expect("deserialize request");
    assert_eq!(parsed, request);
}

#[test]
fn viewer_agent_chat_request_round_trip() {
    let request = ViewerRequest::AgentChat {
        request: AgentChatRequest {
            agent_id: "agent-0".to_string(),
            message: "go to loc-2".to_string(),
            player_id: Some("player-1".to_string()),
            public_key: Some("pk-1".to_string()),
            auth: Some(PlayerAuthProof {
                scheme: PlayerAuthScheme::Ed25519,
                player_id: "player-1".to_string(),
                public_key: "pk-1".to_string(),
                nonce: 9,
                signature: "awviewauth:v1:deadbeef".to_string(),
            }),
            intent_tick: Some(42),
            intent_seq: Some(9),
            world_id: Some("world-1".to_string()),
            reorg_epoch: Some(2),
            authority_scope: Some("player_agent_chat".to_string()),
            replaces_intent_id: Some("agent-intent-v2:prior".to_string()),
            canonical_authority: None,
        },
    };
    let json = serde_json::to_string(&request).expect("serialize request");
    let parsed: ViewerRequest = serde_json::from_str(&json).expect("deserialize request");
    assert_eq!(parsed, request);
}

#[test]
fn viewer_gameplay_action_request_round_trip() {
    let request = ViewerRequest::GameplayAction {
        request: GameplayActionRequest {
            action_id: "build_factory_smelter_mk1".to_string(),
            target_agent_id: "agent-0".to_string(),
            actor_agent_id: None,
            player_id: "player-1".to_string(),
            public_key: Some("pk-1".to_string()),
            auth: Some(PlayerAuthProof {
                scheme: PlayerAuthScheme::Ed25519,
                player_id: "player-1".to_string(),
                public_key: "pk-1".to_string(),
                nonce: 11,
                signature: "awviewauth:v1:deadbeef".to_string(),
            }),
        },
    };
    let json = serde_json::to_string(&request).expect("serialize request");
    let parsed: ViewerRequest = serde_json::from_str(&json).expect("deserialize request");
    assert_eq!(parsed, request);
}

#[test]
fn viewer_authoritative_challenge_submit_request_round_trip() {
    let request = ViewerRequest::AuthoritativeChallenge {
        command: AuthoritativeChallengeCommand::Submit {
            request: AuthoritativeChallengeSubmitRequest {
                batch_id: "batch-1".to_string(),
                watcher_id: "watcher-1".to_string(),
                recomputed_state_root: "a".repeat(64),
                recomputed_data_root: "b".repeat(64),
                challenge_id: Some("challenge-1".to_string()),
            },
        },
    };
    let json = serde_json::to_string(&request).expect("serialize request");
    let parsed: ViewerRequest = serde_json::from_str(&json).expect("deserialize request");
    assert_eq!(parsed, request);
}

#[test]
fn viewer_authoritative_recovery_rotate_session_request_round_trip() {
    let request = ViewerRequest::AuthoritativeRecovery {
        command: AuthoritativeRecoveryCommand::RotateSession {
            request: AuthoritativeSessionRotateRequest {
                player_id: "player-1".to_string(),
                old_session_pubkey: "old-key".to_string(),
                new_session_pubkey: "new-key".to_string(),
                rotate_reason: "security_rotation".to_string(),
                rotated_by: Some("ops".to_string()),
            },
        },
    };
    let json = serde_json::to_string(&request).expect("serialize request");
    let parsed: ViewerRequest = serde_json::from_str(&json).expect("deserialize request");
    assert_eq!(parsed, request);
}

#[test]
fn viewer_prompt_control_request_legacy_without_public_key_is_accepted() {
    let json = r#"{
            "type":"prompt_control",
            "command":{
                "mode":"apply",
                "request":{
                    "agent_id":"agent-0",
                    "player_id":"player-1"
                }
            }
        }"#;
    let parsed: ViewerRequest = serde_json::from_str(json).expect("deserialize legacy request");
    let ViewerRequest::PromptControl { command } = parsed else {
        panic!("expected prompt_control request");
    };
    let PromptControlCommand::Apply { request } = *command else {
        panic!("expected apply command");
    };
    assert_eq!(request.public_key, None);
    assert_eq!(request.auth, None);
}

#[test]
fn viewer_prompt_control_enhanced_request_fields_survive_legacy_decode() {
    let json = serde_json::json!({
        "type": "prompt_control",
        "command": {
            "mode": "apply",
            "request": {
                "agent_id": "agent-0",
                "player_id": "player-1",
                "request_id": "request-1",
                "session_epoch": 3,
                "binding_epoch": 4,
                "expected_authority_epoch": "authority-1",
                "expected_version": 7,
                "updated_by": "player-1",
                "system_prompt_override": "system"
            }
        }
    });
    let parsed: ViewerRequest =
        serde_json::from_value(json.clone()).expect("decode enhanced prompt request");
    let encoded = serde_json::to_value(parsed).expect("encode enhanced prompt request");
    for field in [
        "request_id",
        "session_epoch",
        "binding_epoch",
        "expected_authority_epoch",
    ] {
        assert_eq!(
            encoded["command"]["request"][field],
            json["command"]["request"][field]
        );
    }
}

#[test]
fn viewer_agent_chat_request_legacy_without_auth_is_accepted() {
    let json = r#"{
            "type":"agent_chat",
            "request":{
                "agent_id":"agent-0",
                "message":"hello",
                "player_id":"player-1",
                "public_key":"pk-1"
            }
        }"#;
    let parsed: ViewerRequest = serde_json::from_str(json).expect("deserialize legacy request");
    let ViewerRequest::AgentChat { request } = parsed else {
        panic!("expected agent_chat request");
    };
    assert_eq!(request.auth, None);
    assert_eq!(request.intent_tick, None);
    assert_eq!(request.intent_seq, None);
}

#[test]
fn viewer_response_round_trip_prompt_ack() {
    let response = ViewerResponse::<
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
        u64,
    >::PromptControlAck {
        ack: PromptControlAck {
            request_id: None,
            authority_epoch: None,
            agent_id: "agent-0".to_string(),
            operation: PromptControlOperation::Rollback,
            preview: false,
            status: None,
            player_id: None,
            session_epoch: None,
            binding_epoch: None,
            expected_version: None,
            version: 7,
            updated_at_tick: 42,
            applied_fields: vec![
                "system_prompt_override".to_string(),
                "short_term_goal_override".to_string(),
            ],
            digest: "abc".to_string(),
            value_visibility: None,
            applied_scope: None,
            persistence_scope: None,
            sync_scope: None,
            reason_code: None,
            next_step: None,
            operation_digest: None,
            idempotent_replay: false,
            mutation_count: None,
            rolled_back_to_version: Some(5),
        },
    };
    let json = serde_json::to_string(&response).expect("serialize response");
    let parsed: ViewerResponse<
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
        u64,
    > = serde_json::from_str(&json).expect("deserialize response");
    assert_eq!(parsed, response);
}

#[test]
fn viewer_prompt_control_enhanced_result_fields_survive_legacy_decode() {
    let json = serde_json::json!({
        "type": "prompt_control_ack",
        "ack": {
            "request_id": "request-1",
            "authority_epoch": "authority-1",
            "operation": "apply",
            "preview": false,
            "status": "applied",
            "agent_id": "agent-0",
            "player_id": "player-1",
            "session_epoch": 3,
            "binding_epoch": 4,
            "expected_version": 7,
            "version": 8,
            "updated_at_tick": 42,
            "digest": "profile-digest",
            "applied_fields": ["system_prompt_override"],
            "value_visibility": "latest_allowed",
            "applied_scope": "runtime_instance",
            "persistence_scope": "none",
            "sync_scope": "none",
            "reason_code": null,
            "next_step": null,
            "operation_digest": "operation-digest",
            "idempotent_replay": false,
            "mutation_count": 1
        }
    });
    let parsed: ViewerResponse<(), (), (), (), u64> =
        serde_json::from_value(json.clone()).expect("decode enhanced prompt result");
    let encoded = serde_json::to_value(parsed).expect("encode enhanced prompt result");
    for field in [
        "request_id",
        "authority_epoch",
        "status",
        "player_id",
        "session_epoch",
        "binding_epoch",
        "expected_version",
        "value_visibility",
        "applied_scope",
        "persistence_scope",
        "sync_scope",
        "operation_digest",
        "mutation_count",
    ] {
        assert_eq!(encoded["ack"][field], json["ack"][field]);
    }
}

#[test]
fn viewer_response_round_trip_control_completion_ack() {
    let response = ViewerResponse::<
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
        u64,
    >::ControlCompletionAck {
        ack: ControlCompletionAck {
            request_id: 42,
            status: ControlCompletionStatus::TimeoutNoProgress,
            delta_logical_time: 0,
            delta_event_seq: 0,
            error_code: None,
            error_message: None,
        },
    };
    let json = serde_json::to_string(&response).expect("serialize response");
    let parsed: ViewerResponse<
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
        u64,
    > = serde_json::from_str(&json).expect("deserialize response");
    assert_eq!(parsed, response);
}

#[test]
fn viewer_response_round_trip_agent_chat_ack() {
    let response = ViewerResponse::<
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
        u64,
    >::AgentChatAck {
        ack: AgentChatAck {
            auth_nonce: None,
            agent_id: "agent-0".to_string(),
            accepted_at_tick: 42,
            message_len: 11,
            player_id: Some("player-1".to_string()),
            intent_tick: Some(42),
            intent_seq: Some(17),
            idempotent_replay: true,
            intent_id: None,
            accepted_event_seq: None,
            status: None,
            receipt_ref: None,
            replaced_by: None,
        },
    };
    let json = serde_json::to_string(&response).expect("serialize response");
    let parsed: ViewerResponse<
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
        u64,
    > = serde_json::from_str(&json).expect("deserialize response");
    assert_eq!(parsed, response);
}

#[test]
fn viewer_response_round_trip_gameplay_action_ack() {
    let response = ViewerResponse::<
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
        u64,
    >::GameplayActionAck {
        ack: GameplayActionAck {
            submission_status: None,
            consensus_action_payload_hash: None,
            action_id: "build_factory_smelter_mk1".to_string(),
            target_agent_id: "agent-0".to_string(),
            player_id: "player-1".to_string(),
            runtime_action_id: 41,
            accepted_at_tick: 42,
            message: Some("advance 1-2 steps to apply the queued industrial action".to_string()),
        },
    };
    let json = serde_json::to_string(&response).expect("serialize response");
    let parsed: ViewerResponse<
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
        u64,
    > = serde_json::from_str(&json).expect("deserialize response");
    assert_eq!(parsed, response);
}

#[test]
fn world_feed_v1_round_trip_preserves_cursor_and_gap_contract() {
    let request_json = serde_json::json!({
        "type": "request_world_feed",
        "cursor": "opaque-cursor",
        "limit": 25
    });
    let request: ViewerRequest =
        serde_json::from_value(request_json.clone()).expect("decode world feed request");
    assert_eq!(
        serde_json::to_value(request).expect("encode request"),
        request_json
    );

    let legacy_response_json = serde_json::json!({
        "type": "world_feed",
        "feed": {
            "schema_version": "world_feed/v1",
            "world_id": "world-1",
            "reorg_epoch": 3,
            "cursor": "next-cursor",
            "events": [],
            "status": "gap",
            "gap_reason": "reorg_epoch_changed",
            "snapshot_reload_required": true
        }
    });
    let response: ViewerResponse<(), (), (), (), u64> =
        serde_json::from_value(legacy_response_json).expect("decode world feed response");
    let encoded = serde_json::to_value(response).expect("encode response");
    assert_eq!(encoded["feed"]["reorg_epoch"], serde_json::json!("3"));
}

#[test]
fn world_feed_v1_event_keeps_explicit_receipt_reference_nullable() {
    let legacy_response_json = serde_json::json!({
        "type": "world_feed",
        "feed": {
            "schema_version": "world_feed/v1",
            "world_id": "world-1",
            "reorg_epoch": 0,
            "cursor": "cursor-7",
            "events": [{
                "event_seq": 7,
                "kind": "domain",
                "summary": "Domain event",
                "detail": "{}",
                "receipt_ref": null
            }],
            "status": "ready",
            "snapshot_reload_required": false
        }
    });
    let response: ViewerResponse<(), (), (), (), u64> =
        serde_json::from_value(legacy_response_json).expect("decode world feed response");
    let encoded = serde_json::to_value(response).expect("encode response");
    assert_eq!(encoded["feed"]["reorg_epoch"], serde_json::json!("0"));
    assert_eq!(
        encoded["feed"]["events"][0]["event_seq"],
        serde_json::json!("7")
    );
    assert!(encoded["feed"]["events"][0]["receipt_ref"].is_null());
    assert!(encoded["feed"]["events"][0].get("major_event").is_none());
}

#[test]
fn world_feed_v1_major_event_round_trip_preserves_opaque_subtype() {
    let response_json = serde_json::json!({
        "type": "world_feed",
        "feed": {
            "schema_version": "world_feed/v1",
            "world_id": "world-1",
            "reorg_epoch": "3",
            "cursor": "cursor-7",
            "events": [{
                "event_seq": "7",
                "kind": "domain",
                "summary": "Crisis active",
                "detail": "{}",
                "receipt_ref": null,
                "major_event": {
                    "schema_version": "major_world_event/v1",
                    "identity": { "world_id": "world-1", "reorg_epoch": "3", "event_seq": "7" },
                    "category": "crisis",
                    "subtype": "power_shortage",
                    "severity": 4,
                    "lifecycle": "active",
                    "source": { "authority": "runtime_journal", "event_kind": "crisis_spawned" },
                    "freshness": "current",
                    "visibility": "public",
                    "logical_time": 10,
                    "world_anchor": { "scope": "world", "entity_id": "crisis-1" }
                }
            }],
            "status": "ready",
            "snapshot_reload_required": false
        }
    });
    let response: ViewerResponse<(), (), (), (), u64> =
        serde_json::from_value(response_json).expect("decode major world event");
    let encoded = serde_json::to_value(response).expect("encode major world event");
    assert_eq!(
        encoded["feed"]["events"][0]["major_event"]["subtype"],
        serde_json::json!("power_shortage")
    );
    assert!(encoded["feed"]["events"][0]["receipt_ref"].is_null());
}

#[test]
fn world_feed_v1_u64_identifiers_serialize_as_exact_decimal_strings() {
    let response = ViewerResponse::<(), (), (), (), u64>::WorldFeed {
        feed: WorldFeedEnvelope {
            schema_version: WORLD_FEED_SCHEMA_VERSION.to_string(),
            world_id: "world-max".to_string(),
            reorg_epoch: u64::MAX,
            cursor: "cursor-max".to_string(),
            events: vec![WorldFeedEvent {
                event_seq: u64::MAX,
                kind: "domain".to_string(),
                summary: "Max event sequence".to_string(),
                detail: "{}".to_string(),
                receipt_ref: None,
                module_visual_entity_id: None,
                major_event: None,
            }],
            status: WorldFeedStatus::Ready,
            gap_reason: None,
            unavailable_reason: None,
            snapshot_reload_required: false,
        },
    };

    let encoded = serde_json::to_value(&response).expect("encode max u64 feed");
    assert_eq!(
        encoded["feed"]["reorg_epoch"],
        serde_json::json!(u64::MAX.to_string())
    );
    assert_eq!(
        encoded["feed"]["events"][0]["event_seq"],
        serde_json::json!(u64::MAX.to_string())
    );

    let parsed: ViewerResponse<(), (), (), (), u64> =
        serde_json::from_value(encoded).expect("decode max u64 feed");
    assert_eq!(parsed, response);
}

#[test]
fn director_capability_grant_round_trip_preserves_signed_scope() {
    let grant_json = serde_json::json!({
        "version": 1,
        "action": "director_open",
        "audience": "viewer_director",
        "scope": "diagnostics_read",
        "player_id": "player-1",
        "player_public_key": "11".repeat(32),
        "server": "viewer-live-1",
        "session_epoch": 4,
        "nonce": "director-nonce-1",
        "issued_at_unix_ms": 1000,
        "expires_at_unix_ms": 2000,
        "signer_public_key": "22".repeat(32),
        "signature": "awdirectorgrant:v1:33".to_string() + &"33".repeat(63),
    });
    let grant: DirectorCapabilityGrant =
        serde_json::from_value(grant_json.clone()).expect("decode director grant");
    assert_eq!(
        serde_json::to_value(grant).expect("encode director grant"),
        grant_json
    );
}
