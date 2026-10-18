use super::mock_http::{
    MockHttpResponse, provider_context_response, spawn_runtime_live_mock_http_server,
};
use super::*;
use std::sync::atomic::AtomicUsize;

#[test]
fn runtime_auto_play_uses_shared_server_gate_across_sessions() {
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal).with_auto_play_on_connect(true),
    )
    .expect("runtime server");
    let mut first = RuntimeLiveSession::new();
    let mut second = RuntimeLiveSession::new();

    server.enable_auto_play_for_session_if_available(&mut first);
    assert!(first.playing);
    assert!(server.should_advance_auto_play_step());
    assert!(
        !server.should_advance_auto_play_step(),
        "a second session should not advance the same server-level auto-play tick"
    );

    server.enable_auto_play_for_session_if_available(&mut second);
    assert!(second.playing);
    assert!(
        !server.should_advance_auto_play_step(),
        "joining sessions share the same background play gate instead of becoming owners"
    );

    server.next_auto_play_step_at = Some(Instant::now() - Duration::from_millis(1));
    assert!(server.should_advance_auto_play_step());
}

#[test]
fn runtime_auto_play_pause_and_resume_are_global() {
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal).with_auto_play_on_connect(true),
    )
    .expect("runtime server");
    let mut first = RuntimeLiveSession::new();
    let mut second = RuntimeLiveSession::new();

    server.enable_auto_play_for_session_if_available(&mut first);
    assert!(first.playing);
    server.pause_auto_play(&mut first);
    assert!(!first.playing);
    assert!(!server.should_advance_auto_play_step());

    server.enable_auto_play_for_session_if_available(&mut second);
    assert!(
        !second.playing,
        "new sessions should respect a global pause until the user resumes live play"
    );

    server.resume_auto_play(&mut second);
    assert!(second.playing);
    assert!(server.should_advance_auto_play_step());
}

#[test]
fn runtime_paused_start_play_polls_provider_and_pause_survives_reconnect() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
    let requests = Arc::new(AtomicUsize::new(0));
    let feedbacks = Arc::new(AtomicUsize::new(0));
    let url = spawn_runtime_live_mock_http_server(8, {
        let requests = Arc::clone(&requests);
        let feedbacks = Arc::clone(&feedbacks);
        move |request| {
            if request.path == "/v1/world-simulator/feedback-context" {
                feedbacks.fetch_add(1, Ordering::SeqCst);
                return MockHttpResponse {
                    status_code: 200,
                    body: "{\"ok\":true}".into(),
                };
            }
            requests.fetch_add(1, Ordering::SeqCst);
            let context = serde_json::from_slice(&request.body).expect("request context");
            let response = crate::simulator::DecisionResponse {
                decision: crate::simulator::ProviderDecision::Wait,
                module_command: None,
                provider_error: None,
                diagnostics: Default::default(),
                trace_payload: Default::default(),
                memory_write_intents: Vec::new(),
            };
            MockHttpResponse {
                status_code: 200,
                body: serde_json::to_string(&provider_context_response(&context, response))
                    .expect("response"),
            }
        }
    });
    // SAFETY: The canonical provider environment lock is held for this test.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_loopback_http");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, url);
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
        oasis7::env_mut::set_var(VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity");
    }
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm)
            .with_auto_play_on_connect(false)
            .with_test_cognition_runtime_binding(
                "paused-start",
                0,
                Some(crate::simulator::h_v1("paused-start", &0).to_string()),
                "verified",
                0,
            ),
    )
    .expect("server");
    provider_continuation_drains::install_cognition_scheduler(&mut server);
    server
        .world
        .install_test_provider_capability_fixture("agent-0")
        .expect("capability fixture");
    let (mut writer, _client) = test_writer_pair();
    let mut first = RuntimeLiveSession::new();
    let mut probe = RuntimeLiveSession::new();
    let initial_time = server.world.state().time;
    // Presence probes request snapshots before a player exists. They must not
    // turn the initially paused server into an automatic decision source.
    for session in [&mut first, &mut probe] {
        session.initial_snapshot_sent = true;
        server.enable_auto_play_for_session_if_available(session);
        server
            .drive_auto_play(session, &mut writer)
            .expect("paused probe");
        assert!(!session.playing);
    }
    assert_eq!(server.world.state().time, initial_time);
    assert_eq!(requests.load(Ordering::SeqCst), 0);
    assert!(!server.should_advance_auto_play_step());

    server
        .apply_control_mode(ViewerControl::Play, None, &mut first, &mut writer)
        .expect("explicit Play");
    let deadline = Instant::now() + Duration::from_secs(5);
    while feedbacks.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        server.next_auto_play_step_at = None;
        server
            .drive_auto_play(&mut first, &mut writer)
            .expect("poll async turn");
        thread::sleep(Duration::from_millis(2));
    }
    assert!(
        requests.load(Ordering::SeqCst) > 0,
        "Play must dispatch the async request"
    );
    assert!(
        feedbacks.load(Ordering::SeqCst) > 0,
        "later playback passes must consume the async response"
    );
    server
        .apply_control_mode(ViewerControl::Pause, None, &mut first, &mut writer)
        .expect("Pause");
    let paused_time = server.world.state().time;
    let paused_requests = requests.load(Ordering::SeqCst);
    let mut reconnect = RuntimeLiveSession::new();
    reconnect.initial_snapshot_sent = true;
    server.enable_auto_play_for_session_if_available(&mut reconnect);
    assert!(!reconnect.playing);
    for session in [&mut first, &mut probe, &mut reconnect] {
        server
            .drive_auto_play(session, &mut writer)
            .expect("globally paused");
    }
    assert_eq!(server.world.state().time, paused_time);
    assert_eq!(requests.load(Ordering::SeqCst), paused_requests);
    clear_runtime_provider_env();
}
