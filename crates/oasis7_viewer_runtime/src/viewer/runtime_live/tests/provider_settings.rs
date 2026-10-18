use super::*;

#[test]
fn provider_settings_from_env_defaults_to_none() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
    let settings =
        super::control_plane::runtime_provider_settings_from_env().expect("settings parse");
    assert_eq!(settings, None);
}

#[test]
fn runtime_live_llm_timeout_defaults_to_configured_budget() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();

    assert_eq!(
        super::control_plane::resolve_runtime_live_llm_timeout_ms(180_000),
        30_000
    );
    assert_eq!(
        super::control_plane::resolve_runtime_live_llm_timeout_ms(8_000),
        8_000
    );
}

#[test]
fn runtime_live_llm_timeout_respects_env_ceiling_without_expanding_budget() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(RUNTIME_LIVE_LLM_TIMEOUT_ENV, "9000");
    }

    assert_eq!(
        super::control_plane::resolve_runtime_live_llm_timeout_ms(180_000),
        9_000
    );
    assert_eq!(
        super::control_plane::resolve_runtime_live_llm_timeout_ms(4_000),
        4_000
    );

    clear_runtime_provider_env();
}

#[test]
fn provider_settings_from_env_parses_profile_and_timeout() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_DECISION_SOURCE_ENV, "provider_backed");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_BACKEND_ENV, "provider_local_bridge");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_CONTRACT_ENV, "worldsim_provider_v1");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_TRANSPORT_ENV, "loopback_http");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, "http://127.0.0.1:5841");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_CONNECT_TIMEOUT_MS_ENV, "4200");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_DECISION_TIMEOUT_MS_ENV, "6100");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_AUTH_TOKEN_ENV, "secret-token");
    }
    let settings = super::control_plane::runtime_provider_settings_from_env()
        .expect("settings parse")
        .expect("provider settings");
    assert_eq!(settings.requested_provider_mode, "provider_backed");
    assert_eq!(settings.provider_transport, "loopback_http");
    assert_eq!(settings.base_url, "http://127.0.0.1:5841");
    assert_eq!(settings.connect_timeout_ms, 4200);
    assert_eq!(settings.decision_timeout_ms, 6100);
    assert_eq!(settings.agent_profile, "oasis7_p0_low_freq_npc");
    assert_eq!(settings.execution_mode, ProviderExecutionMode::PlayerParity);
    assert_eq!(settings.auth_token.as_deref(), Some("secret-token"));
    assert_eq!(settings.fallback_reason, None);
    clear_runtime_provider_env();
}

#[test]
fn provider_settings_from_env_accepts_remote_https_transport() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_DECISION_SOURCE_ENV, "provider_backed");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_BACKEND_ENV, "provider_local_bridge");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_CONTRACT_ENV, "worldsim_provider_v1");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_TRANSPORT_ENV, "remote_https");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, "https://provider.example");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_CONNECT_TIMEOUT_MS_ENV, "4200");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_AUTH_TOKEN_ENV, "secret-token");
    }
    let settings = super::control_plane::runtime_provider_settings_from_env()
        .expect("settings parse")
        .expect("provider settings");
    assert_eq!(settings.provider_transport, "remote_https");
    assert_eq!(settings.base_url, "https://provider.example");
    assert_eq!(settings.auth_token.as_deref(), Some("secret-token"));
    clear_runtime_provider_env();
}

#[test]
fn provider_settings_from_env_accepts_local_mock_backend() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_DECISION_SOURCE_ENV, "provider_backed");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_BACKEND_ENV, "provider_local_mock");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_CONTRACT_ENV, "worldsim_provider_v1");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_TRANSPORT_ENV, "loopback_http");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, "http://127.0.0.1:5841");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
    }
    let settings = super::control_plane::runtime_provider_settings_from_env()
        .expect("settings parse")
        .expect("provider settings");
    assert_eq!(settings.provider_transport, "loopback_http");
    assert_eq!(settings.base_url, "http://127.0.0.1:5841");
    assert_eq!(settings.agent_profile, "oasis7_p0_low_freq_npc");
    assert_eq!(settings.decision_timeout_ms, settings.connect_timeout_ms);
    clear_runtime_provider_env();
}

#[test]
fn provider_settings_from_env_rejects_removed_old_brand_prefix() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(
            removed_old_brand_runtime_live_env("AGENT_PROVIDER_MODE"),
            "provider_loopback_http",
        );
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(
            removed_old_brand_runtime_live_env("AGENT_PROVIDER_URL"),
            "http://127.0.0.1:5842",
        );
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(
            removed_old_brand_runtime_live_env("AGENT_PROVIDER_CONNECT_TIMEOUT_MS"),
            "4300",
        );
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(
            removed_old_brand_runtime_live_env("AGENT_PROVIDER_PROFILE"),
            "oasis7_p0_low_freq_npc",
        );
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(
            removed_old_brand_runtime_live_env("AGENT_EXECUTION_LANE"),
            "player_parity",
        );
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(
            removed_old_brand_runtime_live_env("AGENT_PROVIDER_AUTH_TOKEN"),
            "removed-old-brand-token",
        );
    }

    let settings =
        super::control_plane::runtime_provider_settings_from_env().expect("settings parse");
    assert_eq!(settings, None);
    clear_runtime_provider_env();
}
