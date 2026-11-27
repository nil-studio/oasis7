use super::{LauncherConfig, build_launcher_args, validate_game_config};

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].clone())
}

#[test]
fn web_launcher_config_deserializes_agent_provider_fields() {
    let config: LauncherConfig = serde_json::from_value(serde_json::json!({
        "agent_decision_source": "agent_direct_connect",
        "agent_provider_backend": "provider_local_bridge",
        "agent_provider_contract": "worldsim_provider_v1",
        "agent_provider_transport": "remote_https",
        "agent_provider_url": "https://provider.example",
        "agent_provider_auth_token": "secret-token",
        "provider_auto_discover": false,
        "agent_provider_connect_timeout_ms": "2500",
        "agent_execution_lane": "headless",
        "agent_provider_profile": "custom-profile"
    }))
    .expect("agent provider fields deserialize");

    assert_eq!(config.agent_decision_source, "agent_direct_connect");
    assert_eq!(config.agent_provider_backend, "provider_local_bridge");
    assert_eq!(config.agent_provider_contract, "worldsim_provider_v1");
    assert_eq!(config.agent_provider_transport, "remote_https");
    assert_eq!(config.agent_provider_url, "https://provider.example");
    assert_eq!(config.agent_provider_auth_token, "secret-token");
    assert!(!config.provider_auto_discover);
    assert_eq!(config.agent_provider_connect_timeout_ms, "2500");
    assert_eq!(config.agent_execution_lane, "headless");
    assert_eq!(config.agent_provider_profile, "custom-profile");
}

#[test]
fn web_launcher_args_forward_agent_provider_fields() {
    let config = LauncherConfig {
        viewer_static_dir: ".".to_string(),
        agent_decision_source: "agent_direct_connect".to_string(),
        agent_provider_transport: "remote_https".to_string(),
        agent_provider_url: "https://provider.example".to_string(),
        agent_provider_auth_token: "secret-token".to_string(),
        provider_auto_discover: false,
        agent_provider_connect_timeout_ms: "2500".to_string(),
        agent_execution_lane: "headless".to_string(),
        agent_provider_profile: "custom-profile".to_string(),
        ..LauncherConfig::default()
    };
    let args = build_launcher_args(&config).expect("args");

    assert_eq!(
        arg_value(&args, "--agent-decision-source").as_deref(),
        Some("provider_backed")
    );
    assert_eq!(
        arg_value(&args, "--agent-provider-backend").as_deref(),
        Some("provider_local_bridge")
    );
    assert_eq!(
        arg_value(&args, "--agent-provider-contract").as_deref(),
        Some("worldsim_provider_v1")
    );
    assert_eq!(
        arg_value(&args, "--agent-provider-transport").as_deref(),
        Some("remote_https")
    );
    assert_eq!(
        arg_value(&args, "--agent-provider-url").as_deref(),
        Some("https://provider.example")
    );
    assert_eq!(
        arg_value(&args, "--agent-provider-auth-token").as_deref(),
        Some("secret-token")
    );
    assert_eq!(
        arg_value(&args, "--agent-provider-connect-timeout-ms").as_deref(),
        Some("2500")
    );
    assert_eq!(
        arg_value(&args, "--agent-execution-lane").as_deref(),
        Some("headless_agent")
    );
    assert_eq!(
        arg_value(&args, "--agent-provider-profile").as_deref(),
        Some("custom-profile")
    );
}

#[test]
fn web_launcher_rejects_invalid_provider_backed_subfields() {
    for (field, config) in [
        (
            "backend",
            LauncherConfig {
                viewer_static_dir: ".".to_string(),
                agent_decision_source: "provider_backed".to_string(),
                agent_provider_backend: "remote_provider".to_string(),
                ..LauncherConfig::default()
            },
        ),
        (
            "contract",
            LauncherConfig {
                viewer_static_dir: ".".to_string(),
                agent_decision_source: "provider_backed".to_string(),
                agent_provider_contract: "worldsim_provider_v2".to_string(),
                ..LauncherConfig::default()
            },
        ),
        (
            "transport",
            LauncherConfig {
                viewer_static_dir: ".".to_string(),
                agent_decision_source: "provider_backed".to_string(),
                agent_provider_transport: "websocket".to_string(),
                ..LauncherConfig::default()
            },
        ),
    ] {
        let issues = validate_game_config(&config);
        assert!(
            issues.iter().any(|issue| issue
                == "agent provider mode must use a supported backend/contract/transport"),
            "{field} should be rejected before launch: {issues:?}"
        );
        assert!(
            build_launcher_args(&config).is_err(),
            "{field} should not silently fall back to default provider args"
        );
    }
}

#[test]
fn web_launcher_rejects_invalid_agent_decision_source_without_fallback() {
    let config = LauncherConfig {
        viewer_static_dir: ".".to_string(),
        agent_decision_source: "unsupported_decision_source".to_string(),
        ..LauncherConfig::default()
    };

    let issues = validate_game_config(&config);
    assert!(
        issues
            .iter()
            .any(|issue| issue == "agent decision source must be builtin_llm or provider_backed"),
        "invalid decision source should be rejected before launch: {issues:?}"
    );
    assert!(
        build_launcher_args(&config).is_err(),
        "invalid decision source should not silently fall back to builtin_llm"
    );
}

#[test]
fn web_launcher_rejects_provider_url_that_violates_transport_policy() {
    for (case, config) in [
        (
            "loopback_http_non_loopback",
            LauncherConfig {
                viewer_static_dir: ".".to_string(),
                agent_decision_source: "provider_backed".to_string(),
                agent_provider_transport: "loopback_http".to_string(),
                agent_provider_url: "http://192.168.0.5:5841".to_string(),
                provider_auto_discover: false,
                ..LauncherConfig::default()
            },
        ),
        (
            "remote_https_loopback",
            LauncherConfig {
                viewer_static_dir: ".".to_string(),
                agent_decision_source: "provider_backed".to_string(),
                agent_provider_transport: "remote_https".to_string(),
                agent_provider_url: "https://127.0.0.1:5841".to_string(),
                provider_auto_discover: false,
                ..LauncherConfig::default()
            },
        ),
        (
            "remote_https_http",
            LauncherConfig {
                viewer_static_dir: ".".to_string(),
                agent_decision_source: "provider_backed".to_string(),
                agent_provider_transport: "remote_https".to_string(),
                agent_provider_url: "http://provider.example:5841".to_string(),
                provider_auto_discover: false,
                ..LauncherConfig::default()
            },
        ),
        (
            "remote_https_bad_hostname",
            LauncherConfig {
                viewer_static_dir: ".".to_string(),
                agent_decision_source: "provider_backed".to_string(),
                agent_provider_transport: "remote_https".to_string(),
                agent_provider_url: "https://bad host:5841".to_string(),
                provider_auto_discover: false,
                ..LauncherConfig::default()
            },
        ),
    ] {
        let issues = validate_game_config(&config);
        assert!(
            issues
                .iter()
                .any(|issue| issue == "agent provider URL is invalid for the selected transport"),
            "{case} should reject provider URL before launch: {issues:?}"
        );
        assert!(
            build_launcher_args(&config).is_err(),
            "{case} should not forward an invalid provider URL"
        );
    }
}

#[test]
fn agent_provider_fields_have_config_and_arg_contract() {
    let config = LauncherConfig {
        viewer_static_dir: ".".to_string(),
        ..LauncherConfig::default()
    };
    let args = build_launcher_args(&config).expect("default args");
    assert_eq!(
        arg_value(&args, "--agent-decision-source").as_deref(),
        Some("provider_backed")
    );
    assert_eq!(
        arg_value(&args, "--agent-provider-url").as_deref(),
        Some("http://127.0.0.1:5841")
    );
    assert_eq!(
        arg_value(&args, "--agent-execution-lane").as_deref(),
        Some("headless_agent")
    );
}
