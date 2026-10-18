use super::*;

#[test]
fn world_service_submission_does_not_hold_shared_viewer_mutex_and_preserves_unknown_identity() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let service_key = ed25519_dalek::SigningKey::from_bytes(&[9; 32]);
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .unwrap();
    server.config.world_service = Some(crate::world_service::client::WorldServiceClientConfig {
        formal_read_trust: None,
        endpoint,
        trusted_service_public_key: hex::encode(service_key.verifying_key().to_bytes()),
        expected_world: oasis7_client_api::world_service::WorldIdentity {
            world_id: "test-world".into(),
            genesis_digest: "genesis".into(),
        },
        scope_id: "public".into(),
        read_private_key_hex: hex::encode([7; 32]),
        timeout: Duration::from_secs(2),
        max_response_bytes: 4096,
    });
    let signer = ed25519_dalek::SigningKey::from_bytes(&[5; 32]);
    let request = signed_gameplay_action_request(
        crate::viewer::GameplayActionRequest {
            action_id: crate::viewer::ACTION_CLAIM_FIRST_AGENT.into(),
            target_agent_id: crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID.into(),
            actor_agent_id: None,
            player_id: "player-service".into(),
            public_key: None,
            auth: None,
        },
        19,
        &hex::encode(signer.verifying_key().to_bytes()),
        &hex::encode([5; 32]),
    );
    let baseline = serde_json::to_value(server.world.state()).unwrap();
    server
        .session_policy
        .register_session(
            "player-service",
            &hex::encode(signer.verifying_key().to_bytes()),
        )
        .unwrap();
    let shared = Arc::new(Mutex::new(server));
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let http = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut bytes = [0; 4096];
        assert!(stream.read(&mut bytes).unwrap() > 0);
        started_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        // Lost response: the accepted signed intent may still commit remotely.
    });
    let worker_shared = shared.clone();
    let worker_request = request.clone();
    let worker = thread::spawn(move || {
        ViewerRuntimeLiveServer::prepare_shared_world_service_submission(
            &worker_shared,
            &ViewerRequest::GameplayAction {
                request: worker_request,
            },
        )
        .unwrap()
        .unwrap()
    });
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(
        shared.try_lock().is_ok(),
        "network wait must not hold Viewer mutex"
    );
    release_tx.send(()).unwrap();
    http.join().unwrap();
    let prepared = worker.join().unwrap();
    let expected = prepared.correlation.clone();
    let mut server = shared.lock().unwrap();
    server.prepared_world_service_submission = Some(prepared);
    let error = server.submit_world_service_gameplay(&request).unwrap_err();
    assert_eq!(error.code, "world_service_outcome_unknown");
    assert_eq!(server.pending_world_service_gameplay.len(), 1);
    assert_eq!(server.pending_world_service_gameplay[0].0, expected);
    assert_eq!(
        serde_json::to_value(server.world.state()).unwrap(),
        baseline
    );
    assert!(server.runtime_action_players.is_empty());
    server.prepared_world_service_submission = Some(
        super::super::super::world_service_link::PreparedWorldServiceSubmission {
            correlation: expected,
            outcome: Err("must not be consumed as another request".into()),
            admission_error: None,
            feedback_origin: None,
        },
    );
    let mut different = request;
    different.target_agent_id = "different-agent".into();
    different = signed_gameplay_action_request(
        different,
        20,
        &hex::encode(signer.verifying_key().to_bytes()),
        &hex::encode([5; 32]),
    );
    assert_eq!(
        server
            .submit_world_service_gameplay(&different)
            .unwrap_err()
            .code,
        "world_service_invalid_request"
    );
}

/// A canonical publication height is not the runtime clock or event position.
/// Exercise the real status HTTP + persisted-world synchronization path rather
/// than the watermark helper so projection admission is covered as well.
#[test]
fn chain_linked_runtime_preserves_commit_height_separately_from_tick_and_events() {
    let execution_world_dir = runtime_live_temp_dir("chain_sync_distinct_version_axes");
    let mut execution_world = crate::runtime::World::new_production_hardened();
    execution_world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: "version-axis-agent".to_string(),
        pos: crate::geometry::GeoPos::new(1, 2, 0),
    });
    for _ in 0..5 {
        execution_world
            .step()
            .expect("advance execution world clock");
    }
    execution_world
        .save_to_dir(execution_world_dir.as_path())
        .expect("persist execution world");
    let chain_status = TestChainStatusServer::start_with_release_security_policy(
        execution_world_dir.clone(),
        ReleaseSecurityPolicy::default(),
    );
    chain_status.committed_height.store(1, Ordering::SeqCst);
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_chain_status_bind(chain_status.addr.clone())
            .with_chain_execution_world_dir(execution_world_dir.clone()),
    )
    .expect("runtime server");
    let mut session = RuntimeLiveSession::new();
    session.playing = false;
    let (mut writer, _peer) = test_writer_pair();
    assert!(
        server
            .sync_chain_linked_runtime(&mut session, &mut writer)
            .expect("sync publication")
    );
    assert_eq!(
        server.world.state().time,
        5,
        "the published runtime clock must remain intact"
    );
    assert!(
        server
            .world
            .state()
            .agents
            .contains_key("version-axis-agent")
    );
    assert_eq!(
        server.last_chain_committed_height, 1,
        "publication identity must retain the server commit height instead of max(height, tick, event position)"
    );
}

#[test]
fn chain_linked_runtime_observes_new_commit_without_advancing_runtime_clock() {
    let execution_world_dir = runtime_live_temp_dir("chain_sync_commit_only_publication");
    let mut execution_world = crate::runtime::World::new_production_hardened();
    execution_world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: "commit-only-agent".to_string(),
        pos: crate::geometry::GeoPos::new(1, 2, 0),
    });
    for _ in 0..5 {
        execution_world
            .step()
            .expect("advance execution world clock");
    }
    execution_world
        .save_to_dir(execution_world_dir.as_path())
        .expect("persist execution world");
    let chain_status = TestChainStatusServer::start_with_release_security_policy(
        execution_world_dir.clone(),
        ReleaseSecurityPolicy::default(),
    );
    chain_status.committed_height.store(1, Ordering::SeqCst);
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_chain_status_bind(chain_status.addr.clone())
            .with_chain_execution_world_dir(execution_world_dir.clone()),
    )
    .expect("runtime server");
    let mut session = RuntimeLiveSession::new();
    session.playing = false;
    let (mut writer, _peer) = test_writer_pair();
    server
        .sync_chain_linked_runtime(&mut session, &mut writer)
        .expect("first publication");
    let original_tick = server.world.state().time;
    let original_events = server.world.journal().events.clone();
    chain_status.committed_height.store(2, Ordering::SeqCst);
    server
        .sync_chain_linked_runtime(&mut session, &mut writer)
        .expect("commit-only publication");
    assert_eq!(
        server.world.state().time,
        original_tick,
        "a commit is not a gameplay tick"
    );
    assert_eq!(
        server.world.journal().events,
        original_events,
        "a commit must not invent world events"
    );
    assert_eq!(
        server.last_chain_committed_height, 2,
        "a newer canonical publication must be observable even when tick and event position are unchanged"
    );
}

fn read_raw_chain_sync_responses(
    peer: &std::net::TcpStream,
    timeout: Duration,
) -> Vec<serde_json::Value> {
    use std::io::{BufRead, BufReader};

    let stream = peer.try_clone().expect("clone chain sync peer");
    stream
        .set_read_timeout(Some(timeout))
        .expect("set chain sync read timeout");
    let mut reader = BufReader::new(stream);
    let mut responses = Vec::new();
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim_end();
                if !trimmed.is_empty() {
                    responses.push(
                        serde_json::from_str(trimmed).expect("decode raw chain sync response"),
                    );
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                break;
            }
            Err(error) => panic!("read raw chain sync response failed: {error}"),
        }
    }
    responses
}

#[test]
fn chain_linked_runtime_empty_poll_does_not_advance_world() {
    let execution_world_dir = runtime_live_temp_dir("chain_sync_idle");
    let execution_world = crate::runtime::World::new_production_hardened();
    execution_world
        .save_to_dir(execution_world_dir.as_path())
        .expect("persist empty execution world");

    let chain_status = TestChainStatusServer::start(execution_world_dir.clone());
    chain_status.committed_height.store(0, Ordering::SeqCst);

    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_chain_status_bind(chain_status.addr.clone())
            .with_chain_execution_world_dir(execution_world_dir.clone())
            .with_chain_poll_interval(Duration::from_millis(50)),
    )
    .expect("runtime server");
    server.latest_player_gameplay_feedback = Some(crate::simulator::PlayerGameplayRecentFeedback {
        action: "chain_sync".to_string(),
        stage: "blocked".to_string(),
        effect: "committed runtime sync failed before the viewer could observe new world state"
            .to_string(),
        intent_summary: None,
        target_agent_id: None,
        reason: Some("simulated missing persistence".to_string()),
        hint: Some("wait for execution world persistence".to_string()),
        delta_logical_time: 0,
        delta_event_seq: 0,
    });
    let mut session = RuntimeLiveSession::new();
    session.playing = false;
    session.subscribed.insert(ViewerStream::Events);
    session.subscribed.insert(ViewerStream::Snapshot);
    let initial_time = server.world.state().time;
    let (mut writer, peer) = test_writer_pair();

    let progressed = server
        .sync_chain_linked_runtime(&mut session, &mut writer)
        .expect("chain sync should succeed");

    assert!(!progressed, "idle chain poll should not report progress");
    assert_eq!(server.world.state().time, initial_time);
    assert!(read_response_line(&peer, Duration::from_millis(100)).is_none());
    assert_eq!(server.last_chain_committed_height, 0);
    assert!(
        server.latest_player_gameplay_feedback.is_none(),
        "successful zero-delta chain sync should clear stale chain_sync feedback"
    );
}

#[test]
fn chain_linked_runtime_zero_delta_accepts_commit_without_advancing_clock_or_events() {
    let execution_world_dir = runtime_live_temp_dir("chain_sync_zero_delta_height");
    let execution_world = crate::runtime::World::new_production_hardened();
    execution_world
        .save_to_dir(execution_world_dir.as_path())
        .expect("persist empty execution world");

    let chain_status = TestChainStatusServer::start(execution_world_dir.clone());
    chain_status.committed_height.store(1, Ordering::SeqCst);

    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_chain_status_bind(chain_status.addr.clone())
            .with_chain_execution_world_dir(execution_world_dir.clone())
            .with_chain_poll_interval(Duration::from_millis(50)),
    )
    .expect("runtime server");
    let mut session = RuntimeLiveSession::new();
    session.playing = false;
    session.subscribed.insert(ViewerStream::Events);
    session.subscribed.insert(ViewerStream::Snapshot);
    let published_time = execution_world.state().time;
    let published_events = execution_world.journal().events.clone();
    let (mut writer, peer) = test_writer_pair();

    let progressed = server
        .sync_chain_linked_runtime(&mut session, &mut writer)
        .expect("chain sync should succeed");

    assert!(
        progressed,
        "a new verified commit advances publication identity"
    );
    assert_eq!(server.world.state().time, published_time);
    assert_eq!(server.world.journal().events, published_events);
    assert_eq!(server.last_chain_committed_height, 1);
    let _ = read_raw_chain_sync_responses(&peer, Duration::from_millis(100));
    assert!(
        !server
            .sync_chain_linked_runtime(&mut session, &mut writer)
            .expect("same publication idle")
    );
    assert_eq!(server.world.state().time, published_time);
    assert_eq!(server.world.journal().events, published_events);
    assert_eq!(server.last_chain_committed_height, 1);
    assert!(read_response_line(&peer, Duration::from_millis(100)).is_none());
}

#[test]
fn chain_linked_runtime_committed_height_zero_consumes_persisted_execution_world() {
    let execution_world_dir = runtime_live_temp_dir("chain_sync_zero_committed_height");
    let mut execution_world = crate::runtime::World::new_production_hardened();
    execution_world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: "chain-agent".to_string(),
        pos: crate::geometry::GeoPos::new(1, 2, 0),
    });
    execution_world.step().expect("advance execution world");
    execution_world
        .save_to_dir(execution_world_dir.as_path())
        .expect("persist execution world");

    let chain_status = TestChainStatusServer::start_with_release_security_policy(
        execution_world_dir.clone(),
        ReleaseSecurityPolicy::default(),
    );
    chain_status.committed_height.store(0, Ordering::SeqCst);

    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_chain_status_bind(chain_status.addr.clone())
            .with_chain_execution_world_dir(execution_world_dir.clone())
            .with_chain_poll_interval(Duration::from_millis(50)),
    )
    .expect("runtime server");
    server.latest_player_gameplay_feedback = Some(crate::simulator::PlayerGameplayRecentFeedback {
        action: "chain_sync".to_string(),
        stage: "blocked".to_string(),
        effect: "stale bootstrap execution world should be ignored before the first commit"
            .to_string(),
        intent_summary: None,
        target_agent_id: None,
        reason: Some("bootstrap-only".to_string()),
        hint: Some("wait for first committed height".to_string()),
        delta_logical_time: 0,
        delta_event_seq: 0,
    });
    let mut session = RuntimeLiveSession::new();
    session.playing = false;
    session.subscribed.insert(ViewerStream::Events);
    session.subscribed.insert(ViewerStream::Snapshot);
    let initial_time = server.world.state().time;
    let (mut writer, peer) = test_writer_pair();

    let progressed = server
        .sync_chain_linked_runtime(&mut session, &mut writer)
        .expect("chain sync should consume persisted zero-height execution world");

    assert!(progressed);
    assert_eq!(server.world.state().time, execution_world.state().time);
    assert_ne!(server.world.state().time, initial_time);
    assert_eq!(
        server.last_chain_committed_height, 0,
        "zero committed height must not be fabricated from gameplay time"
    );
    assert!(server.latest_player_gameplay_feedback.is_none());
    let line = read_response_line(&peer, Duration::from_millis(200))
        .expect("expected zero-height execution-world sync response");
    assert!(!line.trim().is_empty());
}

#[test]
fn chain_linked_runtime_recipe_completion_is_delivered_once_across_replay() {
    let _guard = lock_test_llm_env();
    let mut source = super::super::setup_industrial_gameplay_with_completed_jobs(82, 1);
    let execution_world_dir = runtime_live_temp_dir("chain_sync_recipe_completion");
    source
        .world
        .save_to_dir(execution_world_dir.as_path())
        .expect("persist completed recipe execution world");

    let chain_status = TestChainStatusServer::start(execution_world_dir.clone());
    chain_status.committed_height.store(1, Ordering::SeqCst);
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_chain_status_bind(chain_status.addr.clone())
            .with_chain_execution_world_dir(execution_world_dir.clone()),
    )
    .expect("runtime server");
    let mut session = RuntimeLiveSession::new();
    session.playing = false;
    session.subscribed.insert(ViewerStream::Events);
    let (mut writer, peer) = test_writer_pair();

    assert!(
        server
            .sync_chain_linked_runtime(&mut session, &mut writer)
            .expect("initial recipe completion chain sync")
    );
    let first_responses = read_available_runtime_live_responses(&peer, Duration::from_millis(200));
    assert!(first_responses.iter().any(|response| matches!(
        response,
        ViewerResponse::Event { event }
            if matches!(
                &event.kind,
                WorldEventKind::RuntimeEvent { kind, .. }
                    if kind == "runtime.economy.recipe_completed"
            )
    )));

    source.world.submit_action(RuntimeAction::MoveAgent {
        agent_id: "starter-agent-0".to_string(),
        to: crate::geometry::GeoPos::new(2, 1, 0),
    });
    source
        .world
        .step()
        .expect("append replay-following runtime event");
    source
        .world
        .save_to_dir(execution_world_dir.as_path())
        .expect("persist replay-following execution world");
    chain_status.committed_height.store(2, Ordering::SeqCst);

    let (mut replay_writer, replay_peer) = test_writer_pair();
    assert!(
        server
            .sync_chain_linked_runtime(&mut session, &mut replay_writer)
            .expect("replay-following recipe completion chain sync")
    );
    let replay_responses =
        read_available_runtime_live_responses(&replay_peer, Duration::from_millis(200));
    assert!(!replay_responses.iter().any(|response| matches!(
        response,
        ViewerResponse::Event { event }
            if matches!(
                &event.kind,
                WorldEventKind::RuntimeEvent { kind, .. }
                    if kind == "runtime.economy.recipe_completed"
            )
    )));
}

#[test]
fn chain_linked_runtime_event_suffix_delivers_new_era_recipe_completion() {
    let recipe_completion = crate::runtime::WorldEvent {
        id: u64::MAX,
        time: 83,
        caused_by: None,
        body: crate::runtime::WorldEventBody::Domain(
            crate::runtime::DomainEvent::RecipeCompleted {
                committed_recipe_origin: None,
                job_id: 7,
                requester_agent_id: "agent-0".to_string(),
                factory_id: "factory-smelter".to_string(),
                recipe_id: "recipe.iron-ingot".to_string(),
                accepted_batches: 1,
                produce: Vec::new(),
                byproducts: Vec::new(),
                output_ledger: crate::runtime::MaterialLedgerId::world(),
                bottleneck_tags: Vec::new(),
                logistics_route_ids: Vec::new(),
                logistics_path_ids: Vec::new(),
            },
        ),
    };
    let mut rollover_recipe_completion = recipe_completion.clone();
    rollover_recipe_completion.id = 1;
    let baseline_events = vec![recipe_completion.clone()];
    let prepared_events = vec![recipe_completion, rollover_recipe_completion];

    let selected = super::super::super::chain_link::runtime_events_after_baseline(
        baseline_events.as_slice(),
        prepared_events.as_slice(),
        0,
        1,
    );

    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].id, 1);
    assert!(matches!(
        &selected[0].body,
        crate::runtime::WorldEventBody::Domain(crate::runtime::DomainEvent::RecipeCompleted { .. })
    ));

    let compacted_selected = super::super::super::chain_link::runtime_events_after_baseline(
        baseline_events.as_slice(),
        std::slice::from_ref(&selected[0]),
        0,
        1,
    );
    assert_eq!(compacted_selected.len(), 1);
    assert_eq!(compacted_selected[0].id, 1);
}

#[test]
fn chain_linked_runtime_revalidates_initial_snapshot_after_previous_session() {
    let execution_world_dir = runtime_live_temp_dir("chain_sync_second_session_revalidation");
    let mut execution_world = crate::runtime::World::new_production_hardened();
    execution_world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: "chain-agent".to_string(),
        pos: crate::geometry::GeoPos::new(1, 2, 0),
    });
    execution_world
        .step()
        .expect("advance initial execution world");
    execution_world
        .save_to_dir_with_chain_resource_context(
            execution_world_dir.as_path(),
            crate::runtime::ChainResourceDerivationContext {
                world_id: "testnet-world",
                chain_id: "testnet-chain",
                genesis_ref: Some("testnet-genesis"),
                created_at_height: 1,
                manifest_height: 1,
                commit_block_hash: Some("testnet-block-1"),
                tick: execution_world.state().time,
            },
            "testnet-world-config",
            "testnet-generation-algorithm",
        )
        .expect("persist initial execution world");

    let chain_status = TestChainStatusServer::start(execution_world_dir.clone());
    chain_status.committed_height.store(1, Ordering::SeqCst);

    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_chain_status_bind(chain_status.addr.clone())
            .with_chain_execution_world_dir(execution_world_dir.clone())
            .with_chain_poll_interval(Duration::from_millis(50)),
    )
    .expect("runtime server");
    let mut first_session = RuntimeLiveSession::new();
    let (mut first_writer, first_peer) = test_writer_pair();

    server
        .handle_request(
            ViewerRequest::RequestSnapshot,
            &mut first_session,
            &mut first_writer,
        )
        .expect("first session snapshot");
    first_writer.flush().expect("flush first session snapshot");
    let _ = read_available_runtime_live_responses(&first_peer, Duration::from_millis(200));
    assert_eq!(chain_status.status_requests(), 1);
    let first_snapshot_time = server.world.state().time;

    let mut updated_execution_world =
        crate::runtime::World::load_from_dir(execution_world_dir.as_path())
            .expect("reload initial execution world");
    updated_execution_world
        .step()
        .expect("advance execution world after first session");
    updated_execution_world
        .save_to_dir_with_chain_resource_context(
            execution_world_dir.as_path(),
            crate::runtime::ChainResourceDerivationContext {
                world_id: "testnet-world",
                chain_id: "testnet-chain",
                genesis_ref: Some("testnet-genesis"),
                created_at_height: 1,
                manifest_height: 2,
                commit_block_hash: Some("testnet-block-2"),
                tick: updated_execution_world.state().time,
            },
            "testnet-world-config",
            "testnet-generation-algorithm",
        )
        .expect("persist updated execution world");
    chain_status.committed_height.store(2, Ordering::SeqCst);

    let mut second_session = RuntimeLiveSession::new();
    let (mut second_writer, second_peer) = test_writer_pair();
    server
        .handle_request(
            ViewerRequest::RequestSnapshot,
            &mut second_session,
            &mut second_writer,
        )
        .expect("second session snapshot");
    second_writer
        .flush()
        .expect("flush second session snapshot");

    assert_eq!(
        chain_status.status_requests(),
        2,
        "a later session must revalidate the chain before its first snapshot"
    );
    assert!(
        server.world.state().time > first_snapshot_time,
        "second session must not receive the prior session's stale projection"
    );
    let responses = read_available_runtime_live_responses(&second_peer, Duration::from_millis(200));
    let snapshot = responses
        .iter()
        .find_map(|response| match response {
            ViewerResponse::Snapshot { snapshot } => Some(snapshot),
            _ => None,
        })
        .expect("second session snapshot response");
    assert_eq!(snapshot.time, server.world.state().time);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn legacy_operator_observer_blocks_hosted_prompt_and_reconnects_exact_head() {
    let _env_guard = runtime_provider_env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _provider_env_snapshot = HostedLocalMockProviderEnvSnapshot::capture();
    configure_hosted_local_mock_provider_env();
    let _strong_auth_guard = lock_test_hosted_strong_auth_env();
    let (backend_public_key, backend_private_key) = test_signer(117);
    unsafe {
        oasis7::env_mut::set_var(
            HOSTED_STRONG_AUTH_GRANT_PUBLIC_KEY_ENV,
            backend_public_key.as_str(),
        );
    }

    let execution_world_dir = runtime_live_temp_dir("chain_sync_prompt_cold_start");
    let agent_id = "hosted-chain-recovery-agent";
    let player_id = "hosted-chain-recovery-player";
    let mut execution_world = crate::runtime::World::new_production_hardened();
    execution_world
        .bind_cognition_runtime(
            VIEWER_FORMAL_RELEASE_DEFAULT_WORLD_ID,
            "hosted-chain-recovery-branch",
            0,
            Some(
                "blake3:1111111111111111111111111111111111111111111111111111111111111111"
                    .to_string(),
            ),
            "verified",
            0,
        )
        .expect("bind authoritative chain cognition runtime");
    execution_world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: agent_id.to_string(),
        pos: crate::geometry::GeoPos::new(2, 3, 0),
    });
    execution_world
        .step()
        .expect("register authoritative chain Agent");
    let chain_world_height = execution_world.state().time;
    execution_world
        .save_to_dir_with_chain_resource_context(
            execution_world_dir.as_path(),
            crate::runtime::ChainResourceDerivationContext {
                world_id: VIEWER_FORMAL_RELEASE_DEFAULT_WORLD_ID,
                chain_id: "hosted-chain-recovery-chain",
                genesis_ref: Some("hosted-chain-recovery-genesis"),
                created_at_height: 1,
                manifest_height: chain_world_height,
                commit_block_hash: Some("hosted-chain-recovery-block-1"),
                tick: chain_world_height,
            },
            "hosted-chain-recovery-world-config",
            "hosted-chain-recovery-generation",
        )
        .expect("persist exact chain world");

    let persisted_world = crate::runtime::World::load_from_dir(execution_world_dir.as_path())
        .expect("reload persisted chain world");
    assert_eq!(persisted_world.state().time, execution_world.state().time);
    assert_eq!(
        persisted_world.state().agents,
        execution_world.state().agents
    );
    assert_eq!(
        persisted_world.journal().events,
        execution_world.journal().events
    );
    let persisted_manifest = persisted_world.chain_resource_manifest();
    assert_eq!(
        persisted_manifest.world_id,
        VIEWER_FORMAL_RELEASE_DEFAULT_WORLD_ID
    );
    assert_eq!(persisted_manifest.chain_id, "hosted-chain-recovery-chain");
    assert_eq!(persisted_manifest.created_at_height, 1);
    assert_eq!(persisted_manifest.manifest_height, chain_world_height);
    assert_eq!(
        persisted_manifest.created_at_block_hash.as_deref(),
        Some("hosted-chain-recovery-block-1")
    );
    let persisted_world_hash = super::super::super::authoritative::compute_runtime_snapshot_hash(
        &persisted_world.snapshot(),
    )
    .expect("hash persisted chain world");
    assert_eq!(
        persisted_world_hash,
        super::super::super::authoritative::compute_runtime_snapshot_hash(
            &persisted_world.snapshot(),
        )
        .expect("recompute persisted chain world hash")
    );
    let assert_persisted_journal_prefix = |actual: &[crate::runtime::WorldEvent]| {
        let persisted_events = persisted_world.journal().events.as_slice();
        assert!(actual.len() >= persisted_events.len());
        assert_eq!(&actual[..persisted_events.len()], persisted_events);
        let hosted_init_events = &actual[persisted_events.len()..];
        assert!(!hosted_init_events.is_empty());
        assert!(hosted_init_events.iter().all(|event| matches!(
            &event.body,
            crate::runtime::WorldEventBody::CapabilityAuthorization(_)
                | crate::runtime::WorldEventBody::Governance(_)
                | crate::runtime::WorldEventBody::ModuleEvent(_)
                | crate::runtime::WorldEventBody::ManifestUpdated(_)
        )));
    };

    let chain_status = TestChainStatusServer::start(execution_world_dir.clone());
    chain_status
        .committed_height
        .store(chain_world_height, Ordering::SeqCst);
    let viewer_config = || {
        ViewerRuntimeLiveServerConfig::formal_release_default()
            .with_hosted_public_join_mode(true)
            .with_decision_mode(ViewerLiveDecisionMode::Llm)
            .with_chain_status_bind(chain_status.addr.clone())
            .with_chain_execution_world_dir(execution_world_dir.clone())
            .with_chain_poll_interval(Duration::from_millis(50))
    };

    let mut first_viewer =
        ViewerRuntimeLiveServer::new(viewer_config()).expect("start first chain viewer");
    let mut first_sync_session = RuntimeLiveSession::new();
    first_sync_session.playing = false;
    first_sync_session.subscribed.insert(ViewerStream::Events);
    first_sync_session.subscribed.insert(ViewerStream::Snapshot);
    let (mut first_sync_writer, first_sync_peer) = test_writer_pair();
    assert!(
        first_viewer
            .sync_chain_linked_runtime(&mut first_sync_session, &mut first_sync_writer)
            .expect("first authoritative chain sync")
    );
    let first_sync_responses =
        read_raw_chain_sync_responses(&first_sync_peer, Duration::from_millis(200));
    assert!(
        first_sync_responses
            .iter()
            .any(|response| response.to_string().contains("snapshot"))
    );
    assert_eq!(
        first_viewer.world.state().time,
        persisted_world.state().time
    );
    assert_eq!(
        first_viewer.world.state().agents,
        persisted_world.state().agents
    );
    assert_persisted_journal_prefix(first_viewer.world.journal().events.as_slice());
    assert_eq!(
        first_viewer.world.chain_resource_manifest(),
        persisted_world.chain_resource_manifest()
    );

    let (player_public_key, player_private_key) = test_signer(118);
    let registration = register_runtime_session(
        &mut first_viewer,
        player_id,
        Some(agent_id),
        1,
        player_public_key.as_str(),
        player_private_key.as_str(),
    );
    let authority_epoch = first_viewer
        .prompt_control_authority
        .authority_epoch
        .clone();
    let issued_at_unix_ms = test_now_unix_ms().saturating_sub(1_000);
    let strong_auth_grant = |operation: &str| {
        crate::viewer::sign_hosted_prompt_control_strong_auth_grant(
            operation,
            player_id,
            player_public_key.as_str(),
            agent_id,
            issued_at_unix_ms,
            issued_at_unix_ms.saturating_add(60_000),
            backend_public_key.as_str(),
            backend_private_key.as_str(),
        )
        .expect("sign Hosted local-mock strong-auth grant")
    };
    let negotiated = crate::viewer::protocol::NegotiatedViewerProtocol {
        version: crate::viewer::VIEWER_PROTOCOL_VERSION,
        capabilities: vec![crate::viewer::protocol::PROMPT_CONTROL_RESULT_CAPABILITY.to_string()],
    };

    let mut preview_request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.to_string(),
            player_id: player_id.to_string(),
            expected_version: Some(0),
            updated_by: Some(player_id.to_string()),
            system_prompt_override: Some(Some("chain-preview-only".to_string())),
            request_id: Some("chain-recovery-preview".to_string()),
            session_epoch: registration.session_epoch,
            binding_epoch: registration.binding_epoch,
            expected_authority_epoch: Some(authority_epoch.clone()),
            ..Default::default()
        },
        crate::viewer::PromptControlAuthIntent::Preview,
        2,
        player_public_key.as_str(),
        player_private_key.as_str(),
    );
    preview_request.strong_auth_grant = Some(strong_auth_grant("prompt_control_preview"));
    // The directory-backed operator observer is read-only. It has no
    // authenticated Agent service projection and cannot open a Hosted model lane.
    let before_prompt_world = serde_json::to_value(first_viewer.world.snapshot()).unwrap();
    let before_prompt_journal = first_viewer.world.journal().events.clone();
    let before_prompt_summary = first_viewer.test_canonical_provider_summary();
    let preview = first_viewer
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Preview {
                request: preview_request,
            },
            &negotiated,
        )
        .expect_err("operator observer cannot supply canonical Hosted prompt context");
    assert_eq!(preview.code, "prompt_control_runtime_context_unavailable");
    assert!(
        preview
            .message
            .contains("canonical Agent observation unavailable")
    );
    assert_eq!(
        preview.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Blocked)
    );
    assert_eq!(
        serde_json::to_value(first_viewer.world.snapshot()).unwrap(),
        before_prompt_world
    );
    assert_eq!(first_viewer.world.journal().events, before_prompt_journal);
    assert_eq!(
        first_viewer.test_canonical_provider_summary(),
        before_prompt_summary
    );

    let mut apply_request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.to_string(),
            player_id: player_id.to_string(),
            expected_version: Some(0),
            updated_by: Some(player_id.to_string()),
            short_term_goal_override: Some(Some("chain-apply-runtime-only".to_string())),
            request_id: Some("chain-recovery-apply".to_string()),
            session_epoch: registration.session_epoch,
            binding_epoch: registration.binding_epoch,
            expected_authority_epoch: Some(authority_epoch.clone()),
            ..Default::default()
        },
        crate::viewer::PromptControlAuthIntent::Apply,
        3,
        player_public_key.as_str(),
        player_private_key.as_str(),
    );
    apply_request.strong_auth_grant = Some(strong_auth_grant("prompt_control_apply"));
    let old_apply_request = apply_request.clone();
    let apply = first_viewer
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Apply {
                request: apply_request,
            },
            &negotiated,
        )
        .expect_err("operator observer must also refuse Hosted prompt mutation");
    assert_eq!(apply.code, "prompt_control_runtime_context_unavailable");
    assert!(
        apply
            .message
            .contains("canonical Agent observation unavailable")
    );
    assert_eq!(
        apply.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Blocked)
    );
    assert_eq!(
        serde_json::to_value(first_viewer.world.snapshot()).unwrap(),
        before_prompt_world
    );
    assert_eq!(first_viewer.world.journal().events, before_prompt_journal);
    assert_eq!(
        first_viewer.test_canonical_provider_summary(),
        before_prompt_summary
    );

    let mut cold_viewer =
        ViewerRuntimeLiveServer::new(viewer_config()).expect("cold-start second chain viewer");
    assert_ne!(
        cold_viewer.prompt_control_authority.authority_epoch,
        authority_epoch
    );
    let mut cold_sync_session = RuntimeLiveSession::new();
    cold_sync_session.playing = false;
    cold_sync_session.subscribed.insert(ViewerStream::Events);
    cold_sync_session.subscribed.insert(ViewerStream::Snapshot);
    let (mut cold_sync_writer, cold_sync_peer) = test_writer_pair();
    assert!(
        cold_viewer
            .sync_chain_linked_runtime(&mut cold_sync_session, &mut cold_sync_writer)
            .expect("cold-start chain sync")
    );
    let _ = read_raw_chain_sync_responses(&cold_sync_peer, Duration::from_millis(200));
    assert_eq!(cold_viewer.world.state().time, chain_world_height);
    assert_eq!(
        cold_viewer.world.state().agents,
        persisted_world.state().agents
    );
    assert_persisted_journal_prefix(cold_viewer.world.journal().events.as_slice());
    let stale_error = cold_viewer
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Apply {
                request: old_apply_request,
            },
            &negotiated,
        )
        .expect_err("old PromptControl result retry must be fenced after cold start");
    assert_eq!(stale_error.code, "prompt_control_result_unknown");
    assert_eq!(stale_error.reason_code.as_deref(), Some("result_unknown"));
    assert_eq!(
        stale_error.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Blocked)
    );
    assert_eq!(
        stale_error.value_visibility,
        Some(crate::viewer::protocol::PromptControlValueVisibility::Hidden)
    );
    assert_eq!(cold_viewer.prompt_control_authority.result_ledger.len(), 0);

    let reconnect_registration = register_runtime_session(
        &mut cold_viewer,
        player_id,
        Some(agent_id),
        1,
        player_public_key.as_str(),
        player_private_key.as_str(),
    );
    let expected_snapshot_hash = super::super::super::authoritative::compute_runtime_snapshot_hash(
        &cold_viewer.world.snapshot(),
    )
    .expect("hash cold-start chain snapshot");
    let (reconnect_ack, emit_snapshot_after_ack) = cold_viewer
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ReconnectSync {
            request: AuthoritativeReconnectSyncRequest {
                player_id: player_id.to_string(),
                session_pubkey: Some(player_public_key.clone()),
                last_known_log_cursor: None,
                expected_reorg_epoch: Some(cold_viewer.reorg_epoch),
            },
        })
        .expect("current session reconnect");
    assert!(!emit_snapshot_after_ack);
    assert_eq!(
        reconnect_ack.status,
        AuthoritativeRecoveryStatus::CatchUpReady
    );
    assert_eq!(reconnect_ack.snapshot_height, chain_world_height);
    assert_eq!(reconnect_ack.snapshot_hash, expected_snapshot_hash);
    assert_eq!(reconnect_ack.player_id.as_deref(), Some(player_id));
    assert_eq!(reconnect_ack.agent_id.as_deref(), Some(agent_id));
    assert_eq!(
        reconnect_ack.session_epoch,
        reconnect_registration.session_epoch
    );
    assert_eq!(
        reconnect_ack.binding_epoch,
        reconnect_registration.binding_epoch
    );

    cold_viewer
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RevokeSession {
            request: AuthoritativeSessionRevokeRequest {
                player_id: player_id.to_string(),
                session_pubkey: Some(player_public_key.clone()),
                revoke_reason: "chain-recovery-revoked".to_string(),
                revoked_by: Some("runtime-test".to_string()),
            },
        })
        .expect("revoke current session");
    let revoked_reconnect = cold_viewer
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ReconnectSync {
            request: AuthoritativeReconnectSyncRequest {
                player_id: player_id.to_string(),
                session_pubkey: Some(player_public_key),
                last_known_log_cursor: None,
                expected_reorg_epoch: Some(cold_viewer.reorg_epoch),
            },
        })
        .expect_err("revoked session must be denied on reconnect");
    assert_eq!(revoked_reconnect.code, "session_revoked");

    let mut next_chain_world = crate::runtime::World::load_from_dir(execution_world_dir.as_path())
        .expect("reload exact chain world for subsequent sync");
    next_chain_world.step().expect("advance chain world once");
    let next_chain_height = next_chain_world.state().time;
    next_chain_world
        .save_to_dir_with_chain_resource_context(
            execution_world_dir.as_path(),
            crate::runtime::ChainResourceDerivationContext {
                world_id: VIEWER_FORMAL_RELEASE_DEFAULT_WORLD_ID,
                chain_id: "hosted-chain-recovery-chain",
                genesis_ref: Some("hosted-chain-recovery-genesis"),
                created_at_height: 1,
                manifest_height: next_chain_height,
                commit_block_hash: Some("hosted-chain-recovery-block-2"),
                tick: next_chain_height,
            },
            "hosted-chain-recovery-world-config",
            "hosted-chain-recovery-generation",
        )
        .expect("persist subsequent exact chain world");
    chain_status
        .committed_height
        .store(next_chain_height, Ordering::SeqCst);
    cold_viewer.latest_player_gameplay_feedback =
        Some(crate::simulator::PlayerGameplayRecentFeedback {
            action: "chain_sync".to_string(),
            stage: "blocked".to_string(),
            effect: "stale chain sync feedback".to_string(),
            intent_summary: None,
            target_agent_id: None,
            reason: Some("test-only stale feedback".to_string()),
            hint: None,
            delta_logical_time: 0,
            delta_event_seq: 0,
        });
    let mut followup_session = RuntimeLiveSession::new();
    followup_session.playing = false;
    followup_session.subscribed.insert(ViewerStream::Events);
    followup_session.subscribed.insert(ViewerStream::Snapshot);
    let (mut followup_writer, followup_peer) = test_writer_pair();
    assert!(
        cold_viewer
            .sync_chain_linked_runtime(&mut followup_session, &mut followup_writer)
            .expect("one subsequent chain sync")
    );
    let followup_responses =
        read_raw_chain_sync_responses(&followup_peer, Duration::from_millis(200));
    assert_eq!(
        followup_responses
            .iter()
            .filter(|response| response.to_string().contains("snapshot"))
            .count(),
        1
    );
    assert!(cold_viewer.latest_player_gameplay_feedback.is_none());

    let (mut duplicate_writer, duplicate_peer) = test_writer_pair();
    assert!(
        !cold_viewer
            .sync_chain_linked_runtime(&mut followup_session, &mut duplicate_writer)
            .expect("repeat same chain head sync")
    );
    assert!(read_raw_chain_sync_responses(&duplicate_peer, Duration::from_millis(100)).is_empty());
    assert!(cold_viewer.latest_player_gameplay_feedback.is_none());
}
