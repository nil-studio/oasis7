use super::*;

#[test]
fn runtime_background_play_retries_transient_llm_access_failure_after_prior_progress() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::remove_var(crate::simulator::ENV_LLM_MODEL);
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::remove_var(crate::simulator::ENV_LLM_BASE_URL);
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::remove_var(crate::simulator::ENV_LLM_API_KEY);
    }

    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm)
            .with_auto_play_on_connect(true),
    )
    .expect("runtime server");
    server
        .world
        .step()
        .expect("advance world once before failure");

    let baseline_time = server.world.state().time;
    let (mut writer, _client) = test_writer_pair();
    let mut session = RuntimeLiveSession::new();
    session.playing = true;

    server
        .advance_runtime(&mut session, &mut writer, "play", 1, None, false)
        .expect("play loop handled");

    assert!(
        session.playing,
        "background play should keep retrying after the first transient LLM failure once progress exists"
    );
    assert_eq!(
        session.transient_play_failures, 1,
        "first transient failure should increment retry budget"
    );
    assert_eq!(
        server.world.state().time,
        baseline_time,
        "failed retry should not advance world time"
    );
    assert!(
        !server.should_advance_auto_play_step(),
        "transient background play failure should back off before the next retry"
    );
    let feedback = server
        .latest_player_gameplay_feedback
        .as_ref()
        .expect("blocked feedback recorded");
    assert_eq!(feedback.action, "play");
    assert_eq!(feedback.stage, "blocked");
    assert!(feedback.effect.contains("will retry on the next play tick"));
}

#[test]
fn runtime_background_play_stops_after_retry_budget_exhausted() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::remove_var(crate::simulator::ENV_LLM_MODEL);
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::remove_var(crate::simulator::ENV_LLM_BASE_URL);
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::remove_var(crate::simulator::ENV_LLM_API_KEY);
    }

    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    server
        .world
        .step()
        .expect("advance world once before failure");

    let (mut writer, _client) = test_writer_pair();
    let mut session = RuntimeLiveSession::new();
    session.playing = true;

    for expected_failures in 1..BACKGROUND_PLAY_TRANSIENT_FAILURE_BUDGET {
        server
            .advance_runtime(&mut session, &mut writer, "play", 1, None, false)
            .expect("play loop handled");
        assert!(
            session.playing,
            "background play should still retry before budget is exhausted"
        );
        assert_eq!(session.transient_play_failures, expected_failures);
    }

    server
        .advance_runtime(&mut session, &mut writer, "play", 1, None, false)
        .expect("play loop handled");
    assert!(
        !session.playing,
        "background play should stop once transient failure budget is exhausted"
    );
    assert_eq!(
        session.transient_play_failures,
        BACKGROUND_PLAY_TRANSIENT_FAILURE_BUDGET,
    );
}

#[test]
fn provider_bridge_5xx_gets_local_letai_recovery_hint() {
    let hint = ViewerRuntimeLiveServer::llm_gameplay_hint_for_reason(
        "provider_http_502: provider bridge returned HTTP 502",
    );

    assert!(
        hint.contains("local-letai-provider-bridge.log"),
        "provider 5xx hint should point at the local LetAI bridge log"
    );
    assert!(
        hint.contains("proxy/upstream LetAI reachability"),
        "provider 5xx hint should explain the likely operator checks"
    );
}

#[test]
fn provider_gateway_timeout_gets_local_letai_recovery_hint() {
    let hint = ViewerRuntimeLiveServer::llm_gameplay_hint_for_reason(
        "provider_gateway_unreachable: provider gateway call agent exited with status exit status: 1: stderr=The read operation timed out stdout=",
    );

    assert!(
        hint.contains("local-letai-provider-bridge.log"),
        "timeout hint should point at the local LetAI bridge log"
    );
    assert!(
        hint.contains("scripts/run-local-letai-game-test.sh"),
        "timeout hint should point at the canonical local LetAI wrapper"
    );
    assert!(
        hint.contains("proxy/upstream LetAI reachability"),
        "timeout hint should explain the likely operator checks"
    );
}
