use super::*;

pub(super) fn build_oasis7_viewer_live_command(
    path: &Path,
    options: &CliOptions,
    parent_has_llm_timeout_ms: bool,
    repo_has_node_config_file: bool,
) -> Command {
    build_viewer_live_command_with_service_mode(
        path,
        options,
        parent_has_llm_timeout_ms,
        repo_has_node_config_file,
        std::env::var_os("OASIS7_WORLD_SERVICE_ENDPOINT").is_some(),
    )
}

fn build_viewer_live_command_with_service_mode(
    path: &Path,
    options: &CliOptions,
    parent_has_llm_timeout_ms: bool,
    repo_has_node_config_file: bool,
    service_mode: bool,
) -> Command {
    let mut command = Command::new(path);
    if !options.scenario.trim().is_empty() {
        command.arg(options.scenario.as_str());
    }
    command
        .arg("--bind")
        .arg(options.live_bind.as_str())
        .arg("--web-bind")
        .arg(options.web_bind.as_str())
        .arg("--deployment-mode")
        .arg(viewer_deployment_mode_from_options(options).as_str())
        .arg("--major-world-event-visibility")
        .arg(major_world_event_visibility_as_str(
            options.major_world_event_visibility,
        ));
    if !service_mode && !options.generated_world_dir.trim().is_empty() {
        command
            .arg("--generated-world-dir")
            .arg(options.generated_world_dir.as_str());
    }
    if !options.provider_lineage_store.trim().is_empty() {
        command
            .arg("--provider-lineage-store")
            .arg(options.provider_lineage_store.as_str());
    }
    for path in &options.provider_bootstrap_authority_paths {
        command
            .arg("--provider-bootstrap-authority")
            .arg(path.as_str());
    }
    // Hosted public join may deliberately run without a launcher-managed chain.
    // Only pass the viewer's chain client endpoint when the chain is enabled or
    // the operator explicitly supplied an external status endpoint. Passing the
    // launcher's default here makes a chain-disabled stack try an absent service
    // during the first snapshot and close the client after ConnectionRefused.
    if !service_mode && (options.chain_enabled || options.chain_status_bind_explicit) {
        command
            .arg("--chain-status-bind")
            .arg(options.chain_status_bind.as_str())
            .arg("--chain-execution-world-dir")
            .arg(resolved_chain_execution_world_dir(options))
            .arg("--chain-link-policy")
            .arg(options.chain_link_policy.as_str());
    }
    if viewer_deployment_mode_from_options(options) == DeploymentMode::HostedPublicJoin {
        command.env_remove("OASIS7_HOSTED_AUTHORIZATION_ISSUER_PRIVATE_KEY");
        command.env_remove("OASIS7_HOSTED_IDENTITY_SESSION_KEY");
        command.env_remove("OASIS7_HOSTED_IDENTITY_EMAIL_KEY");
        command.env_remove("OASIS7_HOSTED_LOGIN_SMTP_PASSWORD");
        if oasis7_client_api::viewer_auth::hosted_identity_v2_enabled() {
            if let Ok(private) = std::env::var("OASIS7_HOSTED_AUTHORIZATION_ISSUER_PRIVATE_KEY")
                && let Ok(public) =
                    oasis7_client_api::viewer_auth::derive_hosted_registration_issuer_public_key(
                        &private,
                    )
            {
                command.env(
                    oasis7_client_api::viewer_auth::HOSTED_AUTHORIZATION_ISSUER_PUBLIC_KEY_ENV,
                    public,
                );
            }
            if let (Ok(service), Ok(environment)) = (
                std::env::var("OASIS7_HOSTED_IDENTITY_SERVICE_ID"),
                std::env::var("OASIS7_HOSTED_IDENTITY_ENVIRONMENT"),
            ) {
                command.env(
                    "OASIS7_HOSTED_IDENTITY_NAMESPACE",
                    format!("{}:{}:{}", service.len(), service, environment),
                );
            }
        }
        command
            .env_remove(oasis7_client_api::viewer_auth::HOSTED_REGISTRATION_ISSUER_PRIVATE_KEY_ENV);
        if let Ok(issuer_private_key) = std::env::var(
            oasis7_client_api::viewer_auth::HOSTED_REGISTRATION_ISSUER_PRIVATE_KEY_ENV,
        ) && let Ok(issuer_public_key) =
            oasis7_client_api::viewer_auth::derive_hosted_registration_issuer_public_key(
                issuer_private_key.as_str(),
            )
        {
            command.env(
                oasis7_client_api::viewer_auth::HOSTED_REGISTRATION_ISSUER_PUBLIC_KEY_ENV,
                issuer_public_key,
            );
        }
    }
    if options.auto_play {
        command.arg("--auto-play");
    } else {
        command.arg("--no-auto-play");
    }
    if options.allow_debug_scenario {
        command.arg("--allow-debug-scenario");
    }
    if options.with_llm {
        command.arg("--llm");
        if oasis7_client_api::viewer_protocol::runtime_agent_chat_echo_enabled_from_env() {
            command.arg("--agent-chat-echo");
        }
        apply_viewer_live_env_overrides(
            &mut command,
            options,
            parent_has_llm_timeout_ms,
            repo_has_node_config_file,
        );
    } else {
        command.arg("--no-llm");
    }
    command
}

pub(super) fn apply_viewer_live_env_overrides(
    command: &mut Command,
    options: &CliOptions,
    parent_has_llm_timeout_ms: bool,
    repo_has_node_config_file: bool,
) {
    for env_name in [
        VIEWER_AGENT_DECISION_SOURCE_ENV,
        VIEWER_AGENT_PROVIDER_BACKEND_ENV,
        VIEWER_AGENT_PROVIDER_CONTRACT_ENV,
        VIEWER_AGENT_PROVIDER_TRANSPORT_ENV,
        VIEWER_AGENT_PROVIDER_URL_ENV,
        VIEWER_AGENT_PROVIDER_AUTH_TOKEN_ENV,
        VIEWER_AGENT_PROVIDER_CONNECT_TIMEOUT_MS_ENV,
        VIEWER_AGENT_PROVIDER_DECISION_TIMEOUT_MS_ENV,
        VIEWER_AGENT_PROVIDER_PROFILE_ENV,
        VIEWER_AGENT_EXECUTION_LANE_ENV,
        VIEWER_AGENT_PROVIDER_MODE_ENV,
    ] {
        command.env_remove(env_name);
    }

    if uses_provider_http_transport(options) {
        command.env(
            VIEWER_AGENT_DECISION_SOURCE_ENV,
            PROVIDER_BACKED_DECISION_SOURCE,
        );
        command.env(
            VIEWER_AGENT_PROVIDER_BACKEND_ENV,
            options.agent_provider_backend.as_str(),
        );
        command.env(
            VIEWER_AGENT_PROVIDER_CONTRACT_ENV,
            WORLDSIM_PROVIDER_CONTRACT,
        );
        command.env(
            VIEWER_AGENT_PROVIDER_TRANSPORT_ENV,
            options.agent_provider_transport.as_str(),
        );
        command.env(
            VIEWER_AGENT_PROVIDER_URL_ENV,
            options.agent_provider_url.as_str(),
        );
        if !options.agent_provider_auth_token.trim().is_empty() {
            command.env(
                VIEWER_AGENT_PROVIDER_AUTH_TOKEN_ENV,
                options.agent_provider_auth_token.as_str(),
            );
        }
        command.env(
            VIEWER_AGENT_PROVIDER_CONNECT_TIMEOUT_MS_ENV,
            options.agent_provider_connect_timeout_ms.to_string(),
        );
        command.env(
            VIEWER_AGENT_PROVIDER_DECISION_TIMEOUT_MS_ENV,
            options.agent_provider_connect_timeout_ms.to_string(),
        );
        command.env(
            VIEWER_AGENT_PROVIDER_PROFILE_ENV,
            options.agent_provider_profile.as_str(),
        );
        command.env(
            VIEWER_AGENT_EXECUTION_LANE_ENV,
            options.agent_execution_lane.as_str(),
        );
        return;
    }

    if !parent_has_llm_timeout_ms && !repo_has_node_config_file {
        command.env(
            LLM_TIMEOUT_MS_ENV,
            DEFAULT_INTERACTIVE_LLM_TIMEOUT_MS.to_string(),
        );
    }
}

#[cfg(test)]
mod service_assembly_tests {
    use super::*;

    #[test]
    fn service_viewer_omits_node_source_and_legacy_endpoint_without_changing_operator_options() {
        let options = CliOptions {
            generated_world_dir: "operator-generated-world".into(),
            chain_enabled: true,
            chain_status_bind_explicit: true,
            chain_status_bind: "127.0.0.1:5399".into(),
            ..CliOptions::default()
        };
        let command = build_viewer_live_command_with_service_mode(
            Path::new("viewer"),
            &options,
            false,
            false,
            true,
        );
        let args: Vec<_> = command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();
        assert!(!args.iter().any(|value| value == "--generated-world-dir"
            || value == "operator-generated-world"
            || value == "--chain-status-bind"
            || value == "--chain-link-policy"));
        assert_eq!(options.generated_world_dir, "operator-generated-world");
        assert!(options.chain_enabled);
        for key in [
            "OASIS7_WORLD_SERVICE_ENDPOINT",
            "OASIS7_WORLD_SERVICE_PUBLIC_KEY",
            "OASIS7_WORLD_SERVICE_WORLD_ID",
            "OASIS7_WORLD_SERVICE_GENESIS_DIGEST",
            "OASIS7_WORLD_SERVICE_SCOPE",
            "OASIS7_WORLD_SERVICE_TIMEOUT_MS",
            "OASIS7_WORLD_SERVICE_READ_PRIVATE_KEY",
            "OASIS7_WORLD_SERVICE_AGENT_PRIVATE_KEY",
            "OASIS7_WORLD_SERVICE_AGENT_DELEGATION_GENERATION",
        ] {
            assert!(
                !command
                    .get_envs()
                    .any(|(name, value)| name == key && value.is_none()),
                "service identity must remain inherited: {key}"
            );
        }
    }

    #[test]
    fn offline_viewer_keeps_explicit_local_source_and_legacy_fixture_endpoint() {
        let options = CliOptions {
            generated_world_dir: "offline-generated-world".into(),
            chain_enabled: true,
            ..CliOptions::default()
        };
        let command = build_viewer_live_command_with_service_mode(
            Path::new("viewer"),
            &options,
            false,
            false,
            false,
        );
        let args: Vec<_> = command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();
        assert!(args.iter().any(|value| value == "--generated-world-dir"));
        assert!(args.iter().any(|value| value == "--chain-status-bind"));
    }
}
