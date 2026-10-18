use super::*;

#[test]
fn runtime_live_run_accepts_probe_while_viewer_session_is_open() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("reserve port");
    let addr = listener.local_addr().expect("local addr");
    drop(listener);

    let server_addr = addr.to_string();
    thread::spawn(move || {
        let server = ViewerRuntimeLiveServer::new(
            ViewerRuntimeLiveServerConfig::new(WorldScenario::LlmBootstrap)
                .with_bind_addr(server_addr),
        )
        .expect("create server");
        server.run().expect("run server");
    });
    wait_for_runtime_live_server(addr.to_string().as_str());

    let (mut viewer_reader, mut viewer_writer) =
        connect_runtime_live_client(addr.to_string().as_str());
    send_runtime_live_request(
        &mut viewer_writer,
        &ViewerRequest::Subscribe {
            streams: vec![
                ViewerStream::Snapshot,
                ViewerStream::Events,
                ViewerStream::Metrics,
            ],
            event_kinds: Vec::new(),
        },
    );
    send_runtime_live_request(&mut viewer_writer, &ViewerRequest::RequestSnapshot);
    let viewer_snapshot = read_runtime_live_snapshot(&mut viewer_reader);
    assert!(
        !viewer_snapshot.model.agents.is_empty(),
        "expected seeded agents in runtime snapshot"
    );

    let (mut probe_reader, mut probe_writer) =
        connect_runtime_live_client(addr.to_string().as_str());
    send_runtime_live_request(&mut probe_writer, &ViewerRequest::RequestSnapshot);
    let probe_snapshot = read_runtime_live_snapshot(&mut probe_reader);
    assert_eq!(
        probe_snapshot.model.agents.len(),
        viewer_snapshot.model.agents.len()
    );
}

#[test]
fn runtime_live_default_snapshot_request_does_not_enable_ongoing_streams() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    let mut session = RuntimeLiveSession::new();
    let (mut writer, peer) = test_writer_pair();

    server
        .handle_request(ViewerRequest::RequestSnapshot, &mut session, &mut writer)
        .expect("handle snapshot request");
    let responses = read_available_runtime_live_responses(&peer, Duration::from_millis(25));

    assert_eq!(
        responses.len(),
        2,
        "default request should emit one snapshot and recovery metadata only: {responses:?}"
    );
    assert!(matches!(responses[0], ViewerResponse::Snapshot { .. }));
    assert!(matches!(
        responses[1],
        ViewerResponse::AuthoritativeRecoveryAck { .. }
    ));
    assert!(session.uses_default_subscription());

    server
        .emit_background_play_snapshot(&mut session, &mut writer)
        .expect("emit background snapshot");
    let follow_up = read_available_runtime_live_responses(&peer, Duration::from_millis(25));
    assert!(
        follow_up.is_empty(),
        "default subscription must not become ongoing snapshot/events/metrics streaming: {follow_up:?}"
    );
}

#[test]
fn runtime_live_hello_omits_governed_rollback_when_no_durable_sink_exists() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    let mut session = RuntimeLiveSession::new();
    let (mut writer, peer) = test_writer_pair();

    server
        .handle_request(
            ViewerRequest::Hello {
                version: VIEWER_PROTOCOL_VERSION,
                client: "red-capability-probe".to_string(),
            },
            &mut session,
            &mut writer,
        )
        .expect("handle hello");
    let responses = read_available_runtime_live_responses(&peer, Duration::from_millis(25));
    let capabilities = match responses.as_slice() {
        [ViewerResponse::HelloAck { capabilities, .. }] => capabilities,
        other => panic!("expected one hello ack, got {other:?}"),
    };
    assert!(
        !capabilities
            .iter()
            .any(|capability| capability
                == crate::viewer::protocol::GOVERNED_ROLLBACK_REPLAY_CAPABILITY),
        "server must not offer governed rollback when it cannot durably commit it"
    );
}

#[test]
fn hosted_local_mock_seeded_agent_negotiates_prompt_control_result_capability() {
    let _guard = runtime_provider_env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    clear_runtime_provider_env();
    // SAFETY: This test/setup code mutates process environment while holding
    // the canonical provider environment lock.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_DECISION_SOURCE_ENV, "provider_backed");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_backed");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_BACKEND_ENV, "provider_local_mock");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_CONTRACT_ENV, "worldsim_provider_v1");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_TRANSPORT_ENV, "loopback_http");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, "http://127.0.0.1:9");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
        oasis7::env_mut::set_var(VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity");
    }

    let test_world_id = "live-runtime-minimal".to_string();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm)
            .with_hosted_public_join_mode(true)
            .with_test_cognition_runtime_binding(
                "hosted-local-mock-branch",
                0,
                Some(
                    crate::simulator::h_v1("oasis7.viewer.test.finality-block.v1", &test_world_id)
                        .to_string(),
                ),
                "verified",
                0,
            ),
    )
    .expect("create Hosted local-mock runtime server");
    assert!(
        server.world.state().agents.contains_key("agent-0"),
        "Hosted local-mock negotiation must start with the seeded Agent"
    );
    server
        .world
        .install_test_provider_capability_fixture("agent-0")
        .expect("install proof-bearing Hosted local-mock capability context");

    let mut session = RuntimeLiveSession::new();
    let (mut writer, peer) = test_writer_pair();
    server
        .handle_request(
            ViewerRequest::HelloV2 {
                client: "hosted-local-mock-capability-probe".to_string(),
                version: VIEWER_PROTOCOL_VERSION,
                capabilities: vec![
                    crate::viewer::protocol::PROMPT_CONTROL_RESULT_CAPABILITY.to_string(),
                ],
            },
            &mut session,
            &mut writer,
        )
        .expect("handle Hosted local-mock v2 hello");
    let responses = read_available_runtime_live_responses(&peer, Duration::from_millis(25));
    let (capabilities, authority_epoch) = match responses.first() {
        Some(ViewerResponse::HelloAck {
            capabilities,
            authority_epoch,
            ..
        }) => (capabilities, authority_epoch),
        other => panic!("expected Hosted local-mock hello ack, got {other:?}"),
    };
    let negotiated = crate::viewer::protocol::viewer_protocol_supports_prompt_control_result(
        &session.negotiated_protocol,
    );
    clear_runtime_provider_env();

    assert!(
        capabilities
            .iter()
            .any(|capability| capability
                == crate::viewer::protocol::PROMPT_CONTROL_RESULT_CAPABILITY),
        "Hosted local-mock seeded Agent must negotiate prompt_control_result_v1: {capabilities:?}"
    );
    assert!(
        authority_epoch.is_some(),
        "Hosted local-mock prompt-control negotiation must carry the authority epoch"
    );
    assert!(
        negotiated,
        "session must record prompt_control_result_v1 after Hosted local-mock negotiation"
    );
}

#[test]
fn runtime_live_status_unknown_fence_rejects_mutating_requests() {
    let recovery_dir = std::env::temp_dir().join(format!(
        "oasis7-status-unknown-fence-{}-{}",
        std::process::id(),
        crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms()
    ));
    let generation_root = recovery_dir.join(".distfs-state/sidecar-generations");
    std::fs::create_dir_all(&generation_root).expect("generation root");
    std::fs::write(generation_root.join("index.json"), b"not-json").expect("corrupt index");
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    server.set_authoritative_recovery_dir_override(Some(recovery_dir.clone()));
    server.authoritative_recovery_write_fence = Some("expected-generation-hash".to_string());
    let world_before = server.world.snapshot();
    let mut session = RuntimeLiveSession::new();
    let (mut writer, _peer) = test_writer_pair();

    let error = server
        .handle_request(
            ViewerRequest::LiveControl {
                mode: crate::viewer::protocol::LiveControl::Step { count: 1 },
                request_id: Some(1),
            },
            &mut session,
            &mut writer,
        )
        .expect_err("status-unknown fence must reject writes");
    assert!(format!("{error:?}").contains("read-only"));
    assert_eq!(server.world.snapshot(), world_before);
    assert!(server.authoritative_recovery_write_fence.is_some());
    let _ = std::fs::remove_dir_all(recovery_dir);
}

#[test]
fn runtime_live_events_subscription_requests_recovery_metadata_without_initial_snapshot() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    let mut session = RuntimeLiveSession::new();
    let (mut writer, peer) = test_writer_pair();

    server
        .handle_request(
            ViewerRequest::Subscribe {
                streams: vec![ViewerStream::Events],
                event_kinds: Vec::new(),
            },
            &mut session,
            &mut writer,
        )
        .expect("handle subscribe");
    server
        .handle_request(ViewerRequest::RequestSnapshot, &mut session, &mut writer)
        .expect("handle snapshot request");
    let responses = read_available_runtime_live_responses(&peer, Duration::from_millis(25));

    assert!(
        responses
            .iter()
            .any(|response| matches!(response, ViewerResponse::AuthoritativeRecoveryAck { .. })),
        "events subscription should get recovery metadata: {responses:?}"
    );
    assert!(
        !responses
            .iter()
            .any(|response| matches!(response, ViewerResponse::Snapshot { .. })),
        "events-only subscription must not receive an initial snapshot: {responses:?}"
    );
    assert!(session.explicitly_subscribed_to(ViewerStream::Events));
    assert!(!session.explicitly_subscribed_to(ViewerStream::Snapshot));
}

#[test]
fn runtime_live_agent_chat_echo_flushes_virtual_event_immediately_over_socket() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
    unsafe {
        oasis7::env_mut::remove_var(RUNTIME_AGENT_CHAT_ECHO_ENV);
        oasis7::env_mut::remove_var(crate::simulator::ENV_LLM_MODEL);
        oasis7::env_mut::remove_var(crate::simulator::ENV_LLM_BASE_URL);
        oasis7::env_mut::remove_var(crate::simulator::ENV_LLM_API_KEY);
    }

    let listener = TcpListener::bind("127.0.0.1:0").expect("reserve port");
    let addr = listener.local_addr().expect("local addr");
    drop(listener);

    let server_addr = addr.to_string();
    thread::spawn(move || {
        let mut server = ViewerRuntimeLiveServer::new(
            ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
                .with_bind_addr(server_addr)
                .with_decision_mode(ViewerLiveDecisionMode::Llm)
                .with_agent_chat_echo_enabled(true),
        )
        .expect("create server");
        let agent_id = server
            .world
            .state()
            .agents
            .keys()
            .next()
            .cloned()
            .expect("seed agent");
        seed_agent_chat_oc(&mut server, agent_id.as_str());
        server.run().expect("run server");
    });
    wait_for_runtime_live_server(addr.to_string().as_str());

    let (mut reader, mut writer) = connect_runtime_live_client(addr.to_string().as_str());
    send_runtime_live_request(&mut writer, &ViewerRequest::RequestSnapshot);
    let snapshot = read_runtime_live_snapshot(&mut reader);
    let agent_id = snapshot
        .model
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    match read_runtime_live_response(&mut reader) {
        ViewerResponse::AuthoritativeRecoveryAck { ack } => {
            assert_eq!(ack.status, AuthoritativeRecoveryStatus::CatchUpReady);
        }
        other => panic!("expected recovery ack after snapshot request, got {other:?}"),
    }

    send_runtime_live_request(
        &mut writer,
        &ViewerRequest::Subscribe {
            streams: vec![ViewerStream::Events],
            event_kinds: Vec::new(),
        },
    );

    let (public_key, private_key) = test_signer(34);
    let register_request = signed_session_register_request(
        crate::viewer::AuthoritativeSessionRegisterRequest {
            player_id: "player-a".to_string(),
            public_key: None,
            registration_grant: None,
            auth: None,
            requested_agent_id: Some(agent_id.clone()),
            force_rebind: false,
        },
        34,
        public_key.as_str(),
        private_key.as_str(),
    );
    send_runtime_live_request(
        &mut writer,
        &ViewerRequest::AuthoritativeRecovery {
            command: AuthoritativeRecoveryCommand::RegisterSession {
                request: register_request,
            },
        },
    );
    match read_runtime_live_response(&mut reader) {
        ViewerResponse::AuthoritativeRecoveryAck { ack } => {
            assert_eq!(ack.status, AuthoritativeRecoveryStatus::SessionRegistered);
            assert_eq!(ack.player_id.as_deref(), Some("player-a"));
            assert_eq!(ack.agent_id.as_deref(), Some(agent_id.as_str()));
        }
        other => panic!("expected session register ack, got {other:?}"),
    }

    let chat_request = signed_agent_chat_request(
        crate::viewer::AgentChatRequest {
            agent_id: agent_id.clone(),
            player_id: Some("player-a".to_string()),
            public_key: None,
            auth: None,
            message: "hello runtime echo over socket".to_string(),
            intent_tick: Some(snapshot.time),
            intent_seq: Some(35),
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            replaces_intent_id: None,
            canonical_authority: None,
        },
        35,
        public_key.as_str(),
        private_key.as_str(),
    );
    send_runtime_live_request(
        &mut writer,
        &ViewerRequest::AgentChat {
            request: chat_request,
        },
    );

    match read_runtime_live_response(&mut reader) {
        ViewerResponse::AgentChatAck { ack } => {
            assert_eq!(ack.agent_id, agent_id);
            assert_eq!(ack.player_id.as_deref(), Some("player-a"));
        }
        other => panic!("expected agent chat ack, got {other:?}"),
    }
    let mut saw_echo_event = false;
    loop {
        match read_runtime_live_response(&mut reader) {
            ViewerResponse::Event { event } => {
                saw_echo_event |= matches!(
                    &event.kind,
                    oasis7_simulator::WorldEventKind::AgentSpoke {
                        agent_id: event_agent_id,
                        message,
                        ..
                    } if event_agent_id == &agent_id && message == "[local-mock-receipt] 已收到消息；当前本地 mock provider 不生成真实 Agent 回复：hello runtime echo over socket"
                );
            }
            ViewerResponse::AuthoritativeBatch { .. } => {
                assert!(
                    saw_echo_event,
                    "expected qa echo event before authoritative batch flush"
                );
                break;
            }
            other => {
                panic!("expected event stream or authoritative batch after chat ack, got {other:?}")
            }
        }
    }
}

#[test]
fn runtime_simulator_action_mapping_equivalence_covers_core_gameplay_and_economy() {
    let server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    let assert_mapped = |action: oasis7_simulator::Action, expected: RuntimeAction| {
        let mapped = control_plane::simulator_action_to_runtime(&action, &server.world)
            .expect("action should map to runtime");
        assert_eq!(mapped, expected);
    };

    let move_target = GeoPos::new(10, 20, 30);
    assert_mapped(
        oasis7_simulator::Action::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: location_id_for_pos(move_target),
        },
        RuntimeAction::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: move_target,
        },
    );
    assert_mapped(
        oasis7_simulator::Action::TransferResource {
            from: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            to: ResourceOwner::Agent {
                agent_id: "agent-2".to_string(),
            },
            kind: ResourceKind::Electricity,
            amount: 3,
        },
        RuntimeAction::TransferResource {
            from_agent_id: "agent-1".to_string(),
            to_agent_id: "agent-2".to_string(),
            kind: ResourceKind::Electricity,
            amount: 3,
        },
    );
    assert_mapped(
        oasis7_simulator::Action::DeclareWar {
            initiator_agent_id: "agent-1".to_string(),
            war_id: "war.alpha".to_string(),
            aggressor_alliance_id: "alliance.a".to_string(),
            defender_alliance_id: "alliance.b".to_string(),
            objective: "expand".to_string(),
            intensity: 2,
        },
        RuntimeAction::DeclareWar {
            initiator_agent_id: "agent-1".to_string(),
            war_id: "war.alpha".to_string(),
            aggressor_alliance_id: "alliance.a".to_string(),
            defender_alliance_id: "alliance.b".to_string(),
            objective: "expand".to_string(),
            intensity: 2,
        },
    );
    assert_mapped(
        oasis7_simulator::Action::OpenEconomicContract {
            creator_agent_id: "agent-1".to_string(),
            contract_id: "contract.alpha".to_string(),
            counterparty_agent_id: "agent-2".to_string(),
            settlement_kind: ResourceKind::Data,
            settlement_amount: 5,
            reputation_stake: 7,
            expires_at: 99,
            description: "trade".to_string(),
        },
        RuntimeAction::OpenEconomicContract {
            creator_agent_id: "agent-1".to_string(),
            contract_id: "contract.alpha".to_string(),
            counterparty_agent_id: "agent-2".to_string(),
            fulfillment_kind: crate::runtime::EconomicContractFulfillmentKind::AtomicExchange,
            settlement_kind: ResourceKind::Data,
            settlement_amount: 5,
            reputation_stake: 7,
            expires_at: 99,
            description: "trade".to_string(),
        },
    );
}

#[test]
fn runtime_background_play_throttles_full_snapshots() {
    let mut session = RuntimeLiveSession::new();
    session.playing = true;

    assert!(
        should_emit_runtime_advance_snapshot(&mut session, "play", false),
        "first background play step should emit a full snapshot"
    );
    assert!(
        !should_emit_runtime_advance_snapshot(&mut session, "play", false),
        "second immediate background play step should be snapshot-throttled"
    );
    assert!(
        should_emit_runtime_advance_snapshot(&mut session, "step", true),
        "manual step should still emit a full snapshot immediately"
    );
    session.playing = false;
    assert!(
        should_emit_runtime_advance_snapshot(&mut session, "play", false),
        "non-playing play control responses are not background auto-play and should not be throttled"
    );
}

#[test]
fn runtime_decision_failure_reason_includes_upstream_trace() {
    let trace = AgentDecisionTrace {
        agent_id: "agent-0".to_string(),
        time: 1,
        decision: AgentDecision::Wait,
        llm_input: None,
        llm_output: Some(
            serde_json::json!({
                "provider_error": {
                    "code": "provider_gateway_unreachable",
                    "retryable": true,
                },
                "upstream_trace": {
                    "stage": "decision_invocation",
                    "diagnostics": {
                        "status_code": 200,
                        "data_event_count": 2,
                    },
                },
            })
            .to_string(),
        ),
        llm_error: Some("provider_gateway_unreachable: upstream failed".to_string()),
        parse_error: None,
        llm_diagnostics: None,
        llm_effect_intents: Vec::new(),
        llm_effect_receipts: Vec::new(),
        llm_step_trace: Vec::new(),
        llm_prompt_section_trace: Vec::new(),
        llm_chat_messages: Vec::new(),
    };

    let reason = append_decision_upstream_trace(
        "provider_gateway_unreachable: upstream failed".to_string(),
        &trace,
    );

    assert!(reason.contains("upstream_trace="));
    assert!(reason.contains("\"data_event_count\":2"));
}

#[test]
fn runtime_decision_failure_reason_truncates_utf8_trace_safely() {
    let trace = AgentDecisionTrace {
        agent_id: "agent-0".to_string(),
        time: 1,
        decision: AgentDecision::Wait,
        llm_input: None,
        llm_output: Some(
            serde_json::json!({
                "provider_error": {
                    "code": "provider_gateway_unreachable",
                    "retryable": true,
                },
                "upstream_trace": {
                    "error_summary": "余额不足".repeat(500),
                },
            })
            .to_string(),
        ),
        llm_error: Some("provider_gateway_unreachable: upstream failed".to_string()),
        parse_error: None,
        llm_diagnostics: None,
        llm_effect_intents: Vec::new(),
        llm_effect_receipts: Vec::new(),
        llm_step_trace: Vec::new(),
        llm_prompt_section_trace: Vec::new(),
        llm_chat_messages: Vec::new(),
    };

    let reason = append_decision_upstream_trace(
        "provider_gateway_unreachable: upstream failed".to_string(),
        &trace,
    );

    assert!(reason.contains("upstream_trace="));
    assert!(reason.ends_with("..."));
}

#[test]
fn runtime_decision_trace_reads_provider_retryable_flag() {
    let trace = AgentDecisionTrace {
        agent_id: "agent-0".to_string(),
        time: 1,
        decision: AgentDecision::Wait,
        llm_input: None,
        llm_output: Some(
            serde_json::json!({
                "provider_error": {
                    "code": "provider_unauthorized",
                    "retryable": false,
                },
                "upstream_trace": {
                    "stage": "decision_invocation",
                },
            })
            .to_string(),
        ),
        llm_error: Some("provider_unauthorized: no token".to_string()),
        parse_error: None,
        llm_diagnostics: None,
        llm_effect_intents: Vec::new(),
        llm_effect_receipts: Vec::new(),
        llm_step_trace: Vec::new(),
        llm_prompt_section_trace: Vec::new(),
        llm_chat_messages: Vec::new(),
    };

    assert_eq!(decision_trace_provider_error_retryable(&trace), Some(false));
}

#[test]
fn runtime_simulator_action_mapping_covers_module_artifact_actions() {
    let server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    let mut source_files = std::collections::BTreeMap::new();
    source_files.insert("module.toml".to_string(), b"manifest".to_vec());
    source_files.insert("src/lib.rs".to_string(), b"pub fn run() {}".to_vec());

    let compile = oasis7_simulator::Action::CompileModuleArtifactFromSource {
        publisher_agent_id: "agent-1".to_string(),
        module_id: "module.alpha".to_string(),
        manifest_path: "module.toml".to_string(),
        source_files: source_files.clone(),
    };
    let compile_mapped = control_plane::simulator_action_to_runtime(&compile, &server.world)
        .expect("compile action should map");
    assert_eq!(
        compile_mapped,
        RuntimeAction::CompileModuleArtifactFromSource {
            publisher_agent_id: "agent-1".to_string(),
            module_id: "module.alpha".to_string(),
            source_package: crate::runtime::ModuleSourcePackage {
                manifest_path: "module.toml".to_string(),
                files: source_files,
            },
        }
    );

    let deploy = oasis7_simulator::Action::DeployModuleArtifact {
        publisher_agent_id: "agent-1".to_string(),
        wasm_hash: "hash.alpha".to_string(),
        wasm_bytes: vec![0xAA, 0xBB],
        module_id_hint: Some("module.alpha".to_string()),
    };
    let deploy_mapped = control_plane::simulator_action_to_runtime(&deploy, &server.world)
        .expect("deploy action should map");
    assert_eq!(
        deploy_mapped,
        RuntimeAction::DeployModuleArtifact {
            publisher_agent_id: "agent-1".to_string(),
            wasm_hash: "hash.alpha".to_string(),
            wasm_bytes: vec![0xAA, 0xBB],
        }
    );

    let list = oasis7_simulator::Action::ListModuleArtifactForSale {
        seller_agent_id: "agent-1".to_string(),
        wasm_hash: "hash.alpha".to_string(),
        price_kind: ResourceKind::Data,
        price_amount: 9,
    };
    let list_mapped = control_plane::simulator_action_to_runtime(&list, &server.world)
        .expect("list action should map");
    assert_eq!(
        list_mapped,
        RuntimeAction::ListModuleArtifactForSale {
            seller_agent_id: "agent-1".to_string(),
            wasm_hash: "hash.alpha".to_string(),
            price_kind: ResourceKind::Data,
            price_amount: 9,
        }
    );

    let buy = oasis7_simulator::Action::BuyModuleArtifact {
        buyer_agent_id: "agent-2".to_string(),
        wasm_hash: "hash.alpha".to_string(),
    };
    let buy_mapped = control_plane::simulator_action_to_runtime(&buy, &server.world)
        .expect("buy action should map");
    assert_eq!(
        buy_mapped,
        RuntimeAction::BuyModuleArtifact {
            buyer_agent_id: "agent-2".to_string(),
            wasm_hash: "hash.alpha".to_string(),
        }
    );
}

#[test]
fn runtime_simulator_action_mapping_includes_industrial_actions() {
    let server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");

    let build_factory = oasis7_simulator::Action::BuildFactory {
        owner: ResourceOwner::Agent {
            agent_id: "agent-1".to_string(),
        },
        location_id: "runtime:10:20:0".to_string(),
        factory_id: "factory.alpha".to_string(),
        factory_kind: "factory.assembler.mk1".to_string(),
    };
    let build_mapped = control_plane::simulator_action_to_runtime(&build_factory, &server.world)
        .expect("build factory action should map");
    assert_eq!(
        build_mapped,
        RuntimeAction::BuildFactory {
            builder_agent_id: "agent-1".to_string(),
            site_id: "runtime:10:20:0".to_string(),
            spec: crate::runtime::FactoryModuleSpec {
                factory_id: "factory.alpha".to_string(),
                display_name: "Assembler MK1".to_string(),
                tier: 3,
                tags: vec!["assembler".to_string(), "precision".to_string()],
                build_cost: vec![
                    crate::runtime::MaterialStack::new("structural_frame", 8),
                    crate::runtime::MaterialStack::new("iron_ingot", 10),
                    crate::runtime::MaterialStack::new("copper_wire", 8),
                ],
                build_time_ticks: 1,
                base_power_draw: 20,
                recipe_slots: 2,
                throughput_bps: 10_000,
                maintenance_per_tick: 1,
            },
        }
    );

    let schedule_recipe = oasis7_simulator::Action::ScheduleRecipe {
        owner: ResourceOwner::Agent {
            agent_id: "agent-1".to_string(),
        },
        factory_id: "factory.alpha".to_string(),
        recipe_id: "recipe.assembler.control_chip".to_string(),
        batches: 3,
    };
    let schedule_mapped =
        control_plane::simulator_action_to_runtime(&schedule_recipe, &server.world)
            .expect("schedule recipe action should map");
    assert_eq!(
        schedule_mapped,
        RuntimeAction::ScheduleRecipe {
            requester_agent_id: "agent-1".to_string(),
            factory_id: "factory.alpha".to_string(),
            recipe_id: "recipe.assembler.control_chip".to_string(),
            plan: crate::runtime::RecipeExecutionPlan::accepted(
                3,
                vec![
                    crate::runtime::MaterialStack::new("copper_wire", 12),
                    crate::runtime::MaterialStack::new("polymer_resin", 6),
                ],
                vec![crate::runtime::MaterialStack::new("control_chip", 3)],
                vec![crate::runtime::MaterialStack::new("waste_resin", 3)],
                18,
                1,
            ),
            logistics_route_ids: Vec::new(),
            logistics_path_ids: Vec::new(),
        }
    );

    let transfer_to_location = oasis7_simulator::Action::TransferResource {
        from: ResourceOwner::Agent {
            agent_id: "agent-1".to_string(),
        },
        to: ResourceOwner::Location {
            location_id: "loc-1".to_string(),
        },
        kind: ResourceKind::Electricity,
        amount: 1,
    };
    assert!(
        control_plane::simulator_action_to_runtime(&transfer_to_location, &server.world).is_none()
    );
}
