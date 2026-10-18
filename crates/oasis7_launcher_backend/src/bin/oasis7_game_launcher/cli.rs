use super::*;
use oasis7_launcher_config::parse_chain_replication_bootstrap_peer;
use std::net::IpAddr;

pub(super) fn validate_chain_node_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.trim() != id
        || matches!(id, "." | "..")
        || id.contains('/')
        || id.contains('\\')
        || !std::path::Path::new(id)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
        || id.contains(':')
        || id.chars().any(char::is_control)
    {
        return Err("--chain-node-id requires a safe single path component".into());
    }
    Ok(())
}

pub(super) fn parse_options<'a>(args: impl Iterator<Item = &'a str>) -> Result<CliOptions, String> {
    let mut options = CliOptions::default();
    let mut iter = args.peekable();
    let mut explicit_chain_replication_bootstrap_peers = false;

    while let Some(arg) = iter.next() {
        match arg {
            "--deployment-mode" => {
                options.deployment_mode = parse_required_value(&mut iter, "--deployment-mode")?;
            }
            "--allow-trusted-local-playtest" => {
                options.allow_trusted_local_playtest = true;
            }
            "--scenario" => {
                options.scenario = parse_required_value(&mut iter, "--scenario")?;
            }
            "--generated-world-dir" => {
                options.generated_world_dir =
                    parse_required_value(&mut iter, "--generated-world-dir")?;
            }
            "--provider-lineage-store" => {
                options.provider_lineage_store =
                    parse_required_value(&mut iter, "--provider-lineage-store")?;
            }
            "--live-bind" => {
                options.live_bind = parse_required_value(&mut iter, "--live-bind")?;
            }
            "--web-bind" => {
                options.web_bind = parse_required_value(&mut iter, "--web-bind")?;
            }
            "--viewer-host" => {
                options.viewer_host = parse_required_value(&mut iter, "--viewer-host")?;
            }
            "--viewer-port" => {
                let raw = parse_required_value(&mut iter, "--viewer-port")?;
                options.viewer_port = raw.parse::<u16>().map_err(|_| {
                    format!("--viewer-port must be an integer in 1..=65535, got `{raw}`")
                })?;
                if options.viewer_port == 0 {
                    return Err("--viewer-port must be in 1..=65535".to_string());
                }
            }
            "--viewer-static-dir" => {
                options.viewer_static_dir = parse_required_value(&mut iter, "--viewer-static-dir")?;
            }
            "--with-llm" => {
                options.with_llm = true;
            }
            "--auto-play" => {
                options.auto_play = true;
            }
            "--no-auto-play" => {
                options.auto_play = false;
            }
            "--allow-debug-scenario" => {
                options.allow_debug_scenario = true;
            }
            "--agent-decision-source" => {
                options.agent_decision_source =
                    parse_required_value(&mut iter, "--agent-decision-source")?;
            }
            "--agent-provider-backend" => {
                options.agent_provider_backend =
                    parse_required_value(&mut iter, "--agent-provider-backend")?;
            }
            "--agent-provider-contract" => {
                options.agent_provider_contract =
                    parse_required_value(&mut iter, "--agent-provider-contract")?;
            }
            "--agent-provider-transport" => {
                options.agent_provider_transport =
                    parse_required_value(&mut iter, "--agent-provider-transport")?;
            }
            "--agent-provider-url" => {
                options.agent_provider_url =
                    parse_required_value(&mut iter, "--agent-provider-url")?;
            }
            "--agent-provider-auth-token" => {
                options.agent_provider_auth_token =
                    parse_required_value(&mut iter, "--agent-provider-auth-token")?;
            }
            "--agent-provider-connect-timeout-ms" => {
                let raw = parse_required_value(&mut iter, "--agent-provider-connect-timeout-ms")?;
                options.agent_provider_connect_timeout_ms = raw.parse::<u64>().map_err(|_| {
                    format!("--agent-provider-connect-timeout-ms must be a positive integer, got `{raw}`")
                })?;
                if options.agent_provider_connect_timeout_ms == 0 {
                    return Err(
                        "--agent-provider-connect-timeout-ms must be a positive integer"
                            .to_string(),
                    );
                }
            }
            "--agent-provider-profile" => {
                options.agent_provider_profile =
                    parse_required_value(&mut iter, "--agent-provider-profile")?;
            }
            "--agent-execution-lane" => {
                let raw = parse_required_value(&mut iter, "--agent-execution-lane")?;
                options.agent_execution_lane = ProviderExecutionMode::parse(raw.as_str())
                    .ok_or_else(|| {
                        format!(
                            "--agent-execution-lane must be one of player_parity or headless_agent, got `{raw}`"
                        )
                    })?;
            }
            "--agent-provider-mode" => {
                options.agent_decision_source =
                    parse_required_value(&mut iter, "--agent-provider-mode")?;
            }
            "--no-open-browser" => {
                options.open_browser = false;
            }
            "--chain-enable" => {
                options.chain_enabled = true;
            }
            "--chain-disable" => {
                options.chain_enabled = false;
            }
            "--chain-execution-world-dir" => {
                options.chain_execution_world_dir = Some(PathBuf::from(parse_required_value(
                    &mut iter,
                    "--chain-execution-world-dir",
                )?));
            }
            "--chain-status-bind" => {
                options.chain_status_bind = parse_required_value(&mut iter, "--chain-status-bind")?;
                options.chain_status_bind_explicit = true;
            }
            "--chain-link-policy" => {
                let raw = parse_required_value(&mut iter, "--chain-link-policy")?;
                options.chain_link_policy = parse_chain_link_policy(raw.as_str())?.to_string();
            }
            "--major-world-event-visibility" => {
                let raw = parse_required_value(&mut iter, "--major-world-event-visibility")?;
                options.major_world_event_visibility =
                    parse_major_world_event_visibility(raw.as_str())?;
            }
            "--chain-node-id" => {
                let raw = iter.next().ok_or("--chain-node-id requires a value")?;
                if raw.starts_with("--") {
                    return Err("--chain-node-id requires a value".into());
                }
                validate_chain_node_id(raw)?;
                options.chain_node_id = raw.to_string();
            }
            "--chain-network-tier-manifest" => {
                options.chain_network_tier_manifest =
                    parse_required_value(&mut iter, "--chain-network-tier-manifest")?;
            }
            "--chain-storage-profile" => {
                options.chain_storage_profile =
                    parse_required_value(&mut iter, "--chain-storage-profile")?
                        .parse::<StorageProfile>()?;
            }
            "--chain-world-id" => {
                options.chain_world_id = Some(parse_required_value(&mut iter, "--chain-world-id")?);
            }
            "--chain-node-role" => {
                let raw = parse_required_value(&mut iter, "--chain-node-role")?;
                options.chain_node_role = parse_chain_node_role(raw.as_str())?;
            }
            "--chain-p2p-user-mode" => {
                let raw = parse_required_value(&mut iter, "--chain-p2p-user-mode")?;
                options.chain_p2p_user_mode = parse_chain_p2p_user_mode(raw.as_str())?;
            }
            "--chain-p2p-accept-public-entry" => {
                options.chain_p2p_accept_public_entry = true;
            }
            "--chain-p2p-reject-public-entry" => {
                options.chain_p2p_accept_public_entry = false;
            }
            "--chain-replication-network-peer" => {
                let value = parse_required_value(&mut iter, "--chain-replication-network-peer")?;
                validate_chain_replication_network_peer(value.as_str())?;
                if !explicit_chain_replication_bootstrap_peers {
                    // The first explicit bootstrap peer switches CLI behavior from
                    // "use bundled defaults" to "use only the caller-provided list".
                    options.chain_replication_bootstrap_peers.clear();
                    explicit_chain_replication_bootstrap_peers = true;
                }
                options.chain_replication_bootstrap_peers.push(value);
            }
            "--provider-bootstrap-authority" => {
                options
                    .provider_bootstrap_authority_paths
                    .push(parse_required_value(
                        &mut iter,
                        "--provider-bootstrap-authority",
                    )?);
            }
            "--local-test-provider-authority" => {
                options.local_test_provider_authority_path = Some(parse_required_value(
                    &mut iter,
                    "--local-test-provider-authority",
                )?);
            }
            "--local-test-provider-wasm" => {
                options.local_test_provider_wasm_path = Some(parse_required_value(
                    &mut iter,
                    "--local-test-provider-wasm",
                )?);
            }
            "--local-test-provider-metadata" => {
                options.local_test_provider_metadata_path = Some(parse_required_value(
                    &mut iter,
                    "--local-test-provider-metadata",
                )?);
            }
            "--local-test-provider-agent-id" => {
                options.local_test_provider_agent_id =
                    parse_required_value(&mut iter, "--local-test-provider-agent-id")?;
            }
            "--local-test-provider-owner-binding" => {
                options.local_test_provider_owner_binding =
                    parse_required_value(&mut iter, "--local-test-provider-owner-binding")?;
            }
            "--local-test-provider-finality-block-hash" => {
                options.local_test_provider_finality_block_hash = Some(parse_required_value(
                    &mut iter,
                    "--local-test-provider-finality-block-hash",
                )?);
            }
            "--local-test-provider-session-mode" => {
                options.local_test_provider_session_mode =
                    parse_required_value(&mut iter, "--local-test-provider-session-mode")?;
            }
            "--chain-local-standalone-test" => {
                options.chain_local_standalone_test = true;
                options.chain_p2p_user_mode = "private_safe".to_string();
                options.chain_p2p_accept_public_entry = false;
                options.chain_replication_bootstrap_peers.clear();
                explicit_chain_replication_bootstrap_peers = true;
                options.chain_node_auto_attest_all_validators = true;
                options.chain_pos_slot_duration_ms = 1_000;
                options.chain_pos_ticks_per_slot = 1;
                options.chain_pos_proposal_tick_phase = 0;
            }
            "--chain-node-tick-ms" => {
                let raw = parse_required_value(&mut iter, "--chain-node-tick-ms")?;
                options.chain_node_tick_ms = raw.parse::<u64>().map_err(|_| {
                    format!("--chain-node-tick-ms must be a positive integer, got `{raw}`")
                })?;
                if options.chain_node_tick_ms == 0 {
                    return Err("--chain-node-tick-ms must be a positive integer".to_string());
                }
            }
            "--chain-pos-slot-duration-ms" => {
                let raw = parse_required_value(&mut iter, "--chain-pos-slot-duration-ms")?;
                options.chain_pos_slot_duration_ms = raw.parse::<u64>().map_err(|_| {
                    format!("--chain-pos-slot-duration-ms must be a positive integer, got `{raw}`")
                })?;
                if options.chain_pos_slot_duration_ms == 0 {
                    return Err(
                        "--chain-pos-slot-duration-ms must be a positive integer".to_string()
                    );
                }
            }
            "--chain-pos-ticks-per-slot" => {
                let raw = parse_required_value(&mut iter, "--chain-pos-ticks-per-slot")?;
                options.chain_pos_ticks_per_slot = raw.parse::<u64>().map_err(|_| {
                    format!("--chain-pos-ticks-per-slot must be a positive integer, got `{raw}`")
                })?;
                if options.chain_pos_ticks_per_slot == 0 {
                    return Err("--chain-pos-ticks-per-slot must be a positive integer".to_string());
                }
            }
            "--chain-pos-proposal-tick-phase" => {
                let raw = parse_required_value(&mut iter, "--chain-pos-proposal-tick-phase")?;
                options.chain_pos_proposal_tick_phase = raw.parse::<u64>().map_err(|_| {
                    format!("--chain-pos-proposal-tick-phase must be a non-negative integer, got `{raw}`")
                })?;
            }
            "--chain-pos-adaptive-tick-scheduler" => {
                options.chain_pos_adaptive_tick_scheduler_enabled = true;
            }
            "--chain-pos-no-adaptive-tick-scheduler" => {
                options.chain_pos_adaptive_tick_scheduler_enabled = false;
            }
            "--chain-pos-slot-clock-genesis-unix-ms" => {
                let raw =
                    parse_required_value(&mut iter, "--chain-pos-slot-clock-genesis-unix-ms")?;
                options.chain_pos_slot_clock_genesis_unix_ms =
                    Some(raw.parse::<i64>().map_err(|_| {
                        format!(
                            "--chain-pos-slot-clock-genesis-unix-ms must be an integer, got `{raw}`"
                        )
                    })?);
            }
            "--chain-pos-max-past-slot-lag" => {
                let raw = parse_required_value(&mut iter, "--chain-pos-max-past-slot-lag")?;
                options.chain_pos_max_past_slot_lag = raw.parse::<u64>().map_err(|_| {
                    format!(
                        "--chain-pos-max-past-slot-lag must be a non-negative integer, got `{raw}`"
                    )
                })?;
            }
            "--chain-node-validator" => {
                let value = parse_required_value(&mut iter, "--chain-node-validator")?;
                validate_chain_node_validator(value.as_str())?;
                options.chain_node_validators.push(value);
            }
            "--chain-node-auto-attest-all" => {
                options.chain_node_auto_attest_all_validators = true;
            }
            "--chain-node-no-auto-attest-all" => {
                options.chain_node_auto_attest_all_validators = false;
            }
            _ => return Err(format!("unknown option: {arg}")),
        }
    }

    let _ = parse_host_port(options.live_bind.as_str(), "--live-bind")?;
    let _ = parse_host_port(options.web_bind.as_str(), "--web-bind")?;
    validate_generated_world_options(&options)?;
    validate_debug_scenario_guardrail(&options)?;
    let deployment_mode = if options.allow_trusted_local_playtest {
        DeploymentMode::parse(options.deployment_mode.as_str(), "--deployment-mode")?
    } else {
        DeploymentMode::parse_user_facing(options.deployment_mode.as_str(), "--deployment-mode")?
    };
    if !deployment_mode.allows_local_chain_runtime()
        && !local_test_provider_setup_requested(&options)
    {
        options.chain_enabled = false;
    }
    validate_agent_decision_source(options.agent_decision_source.as_str())?;
    options.agent_decision_source =
        canonical_agent_decision_source(options.agent_decision_source.as_str()).to_string();
    if options.agent_decision_source == PROVIDER_BACKED_DECISION_SOURCE {
        validate_agent_provider_backend(options.agent_provider_backend.as_str())?;
        validate_agent_provider_contract(options.agent_provider_contract.as_str())?;
        validate_agent_provider_transport(options.agent_provider_transport.as_str())?;
        options.agent_provider_backend =
            canonical_agent_provider_backend(options.agent_provider_backend.as_str()).to_string();
        options.agent_provider_contract =
            canonical_agent_provider_contract(options.agent_provider_contract.as_str()).to_string();
        options.agent_provider_transport =
            canonical_agent_provider_transport(options.agent_provider_transport.as_str())
                .to_string();
        if options.agent_provider_url.trim().is_empty() {
            return Err("--agent-provider-url requires a non-empty value".to_string());
        }
        if options.agent_provider_profile.trim().is_empty() {
            return Err("--agent-provider-profile requires a non-empty value".to_string());
        }
    }
    normalize_http_target(
        options.viewer_host.as_str(),
        options.viewer_port,
        "viewer host/port",
    )?;
    if options.chain_status_bind_explicit
        && !options.chain_enabled
        && options.chain_execution_world_dir.is_none()
    {
        return Err(
            "external chain status requires --chain-execution-world-dir configured by the operator"
                .to_string(),
        );
    }
    validate_chain_node_id(&options.chain_node_id)?;
    if options.chain_enabled {
        let _ = parse_host_port(options.chain_status_bind.as_str(), "--chain-status-bind")?;
        let _ = parse_chain_link_policy(options.chain_link_policy.as_str())?;
        if options.chain_local_standalone_test && options.chain_node_validators.is_empty() {
            options
                .chain_node_validators
                .push(format!("{}:100", options.chain_node_id.trim()));
        }
        parse_chain_node_role(options.chain_node_role.as_str())?;
        if options.chain_network_tier_manifest.trim().is_empty()
            && options.chain_storage_profile.as_str().trim().is_empty()
        {
            return Err("--chain-storage-profile requires a non-empty value".to_string());
        }
        parse_chain_p2p_user_mode(options.chain_p2p_user_mode.as_str())?;
        if options.chain_node_tick_ms == 0 {
            return Err("--chain-node-tick-ms must be a positive integer".to_string());
        }
        if options.chain_pos_slot_duration_ms == 0 {
            return Err("--chain-pos-slot-duration-ms must be a positive integer".to_string());
        }
        if options.chain_pos_ticks_per_slot == 0 {
            return Err("--chain-pos-ticks-per-slot must be a positive integer".to_string());
        }
        if options.chain_pos_proposal_tick_phase >= options.chain_pos_ticks_per_slot {
            return Err(format!(
                "--chain-pos-proposal-tick-phase={} must be less than --chain-pos-ticks-per-slot={}",
                options.chain_pos_proposal_tick_phase, options.chain_pos_ticks_per_slot
            ));
        }
        for validator in &options.chain_node_validators {
            validate_chain_node_validator(validator.as_str())?;
        }
        for peer in &options.chain_replication_bootstrap_peers {
            validate_chain_replication_network_peer(peer.as_str())?;
        }
    }

    validate_local_test_provider_options(&options)?;

    Ok(options)
}

fn validate_local_test_provider_options(options: &CliOptions) -> Result<(), String> {
    let any_setup_option = options.local_test_provider_authority_path.is_some()
        || options.local_test_provider_wasm_path.is_some()
        || options.local_test_provider_metadata_path.is_some()
        || options.local_test_provider_finality_block_hash.is_some()
        || options.local_test_provider_agent_id != "starter-agent-0"
        || options.local_test_provider_owner_binding != "local-test-owner-0"
        || options.local_test_provider_session_mode != "hosted_public_join";
    if !any_setup_option {
        return Ok(());
    }
    if !options.chain_enabled {
        return Err("local test authority setup requires --chain-enable".to_string());
    }
    if !options.chain_local_standalone_test {
        return Err(
            "local test authority setup requires --chain-local-standalone-test".to_string(),
        );
    }
    if options.chain_storage_profile != StorageProfile::DevLocal {
        return Err(
            "local test authority setup requires --chain-storage-profile dev_local".to_string(),
        );
    }
    if !options.chain_network_tier_manifest.trim().is_empty() {
        return Err(
            "local test authority setup cannot use --chain-network-tier-manifest".to_string(),
        );
    }
    for (flag, present) in [
        (
            "--local-test-provider-authority",
            options.local_test_provider_authority_path.is_some(),
        ),
        (
            "--local-test-provider-wasm",
            options.local_test_provider_wasm_path.is_some(),
        ),
        (
            "--local-test-provider-metadata",
            options.local_test_provider_metadata_path.is_some(),
        ),
        (
            "--local-test-provider-finality-block-hash",
            options.local_test_provider_finality_block_hash.is_some(),
        ),
    ] {
        if !present {
            return Err(format!(
                "{flag} is required when local test authority setup is enabled"
            ));
        }
    }
    if options.local_test_provider_agent_id.trim().is_empty() {
        return Err("--local-test-provider-agent-id requires a non-empty value".to_string());
    }
    if options.local_test_provider_owner_binding.trim().is_empty() {
        return Err("--local-test-provider-owner-binding requires a non-empty value".to_string());
    }
    if !matches!(
        options.local_test_provider_session_mode.as_str(),
        "hosted_public_join" | "loopback"
    ) {
        return Err(
            "--local-test-provider-session-mode must be hosted_public_join or loopback".to_string(),
        );
    }
    let finality = options
        .local_test_provider_finality_block_hash
        .as_deref()
        .unwrap_or_default();
    let Some(digest) = finality.strip_prefix("blake3:") else {
        return Err(
            "--local-test-provider-finality-block-hash must be blake3:<64 lowercase hex characters>"
                .to_string(),
        );
    };
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(
            "--local-test-provider-finality-block-hash must be blake3:<64 lowercase hex characters>"
                .to_string(),
        );
    }
    if options.agent_decision_source == BUILTIN_LLM_DECISION_SOURCE {
        if options.deployment_mode != "trusted_local_only" {
            return Err(
                "local test provider authority setup requires trusted_local_only builtin_llm lane"
                    .to_string(),
            );
        }
        return Ok(());
    }
    if !hosted_local_mock_provider_tuple(options) {
        if options.deployment_mode != "hosted_public_join"
            || options.agent_decision_source != PROVIDER_BACKED_DECISION_SOURCE
        {
            return Err(
                "local test provider authority setup requires --agent-decision-source builtin_llm"
                    .to_string(),
            );
        }
        return Err(
            "Hosted local-mock funding requires the exact HostedPublicJoin provider_backed/provider_local_mock/worldsim_provider_v1/loopback_http/player_parity tuple"
                .to_string(),
        );
    }
    if !cfg!(feature = "test_tier_required") {
        return Err(
            "Hosted local-mock funding requires a binary built with feature test_tier_required"
                .to_string(),
        );
    }
    Ok(())
}

fn local_test_provider_setup_requested(options: &CliOptions) -> bool {
    options.local_test_provider_authority_path.is_some()
        || options.local_test_provider_wasm_path.is_some()
        || options.local_test_provider_metadata_path.is_some()
        || options.local_test_provider_finality_block_hash.is_some()
        || options.local_test_provider_agent_id != "starter-agent-0"
        || options.local_test_provider_owner_binding != "local-test-owner-0"
        || options.local_test_provider_session_mode != "hosted_public_join"
}

fn hosted_local_mock_provider_tuple(options: &CliOptions) -> bool {
    options.deployment_mode == "hosted_public_join"
        && options.agent_decision_source == PROVIDER_BACKED_DECISION_SOURCE
        && options.agent_provider_backend == LOCAL_MOCK_PROVIDER_BACKEND
        && options.agent_provider_contract == WORLDSIM_PROVIDER_CONTRACT
        && options.agent_provider_transport == LOOPBACK_HTTP_PROVIDER_TRANSPORT
        && options.agent_execution_lane == ProviderExecutionMode::PlayerParity
        && options.local_test_provider_session_mode == "hosted_public_join"
        && is_loopback_http_provider_url(options.agent_provider_url.as_str())
}

fn is_loopback_http_provider_url(raw: &str) -> bool {
    let value = raw.trim();
    let Some(scheme) = value.get(.."http://".len()) else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case("http://")
        || value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
    {
        return false;
    }

    let authority_and_path = &value["http://".len()..];
    let authority_end = authority_and_path
        .find(['/', '?', '#'])
        .unwrap_or(authority_and_path.len());
    let authority = &authority_and_path[..authority_end];
    if authority.is_empty() || authority.contains('@') {
        return false;
    }
    let Ok((host, _port)) = parse_host_port(authority, "provider URL") else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<IpAddr>()
        .map(|address| address.is_loopback())
        .unwrap_or(false)
}

fn validate_generated_world_options(options: &CliOptions) -> Result<(), String> {
    let generated_world_dir = options.generated_world_dir.trim();
    if generated_world_dir.is_empty() {
        return Ok(());
    }
    if !options.scenario.trim().is_empty() {
        return Err(
            "`--generated-world-dir` cannot be combined with `--scenario`; generated map sidecar is the viewer world source"
                .to_string(),
        );
    }
    let root = Path::new(generated_world_dir);
    for required in [
        root.join("generated-scenario-world").join("snapshot.json"),
        root.join("generated-scenario-world").join("journal.json"),
        root.join("world-generation-provenance.json"),
    ] {
        if !required.is_file() {
            return Err(format!(
                "`--generated-world-dir` is missing required file {}",
                required.display()
            ));
        }
    }
    Ok(())
}

fn validate_debug_scenario_guardrail(options: &CliOptions) -> Result<(), String> {
    let scenario = options.scenario.trim();
    if scenario.is_empty() || options.allow_debug_scenario {
        return Ok(());
    }
    if matches!(
        WorldScenario::parse(scenario),
        Some(WorldScenario::LlmBootstrap)
    ) {
        return Err(
            "`llm_bootstrap` is a seeded debug/LLM scenario, not a normal playtest or testnet entry; \
rerun with `--allow-debug-scenario` only for targeted diagnostics, or omit `--scenario` for the formal release default world"
                .to_string(),
        );
    }
    Ok(())
}

pub(super) fn deployment_mode_from_options(options: &CliOptions) -> DeploymentMode {
    DeploymentMode::parse(options.deployment_mode.as_str(), "deployment_mode")
        .unwrap_or(DeploymentMode::HostedPublicJoin)
}

/// The local chain writer remains explicitly trusted-local, while an opted-in
/// hosted session still needs the HostedPublicJoin player-plane contract for
/// the Viewer state and launcher HTTP issuer routes.
pub(super) fn viewer_deployment_mode_from_options(options: &CliOptions) -> DeploymentMode {
    if options.local_test_provider_authority_path.is_some()
        && options.local_test_provider_session_mode == "hosted_public_join"
    {
        DeploymentMode::HostedPublicJoin
    } else {
        deployment_mode_from_options(options)
    }
}

pub(super) fn uses_provider_http_transport(options: &CliOptions) -> bool {
    options.agent_decision_source == PROVIDER_BACKED_DECISION_SOURCE
        && matches!(
            options.agent_provider_backend.as_str(),
            LOCAL_BRIDGE_PROVIDER_BACKEND | LOCAL_MOCK_PROVIDER_BACKEND
        )
        && options.agent_provider_contract == WORLDSIM_PROVIDER_CONTRACT
        && matches!(
            options.agent_provider_transport.as_str(),
            LOOPBACK_HTTP_PROVIDER_TRANSPORT | REMOTE_HTTPS_PROVIDER_TRANSPORT
        )
}

fn parse_required_value<'a, I>(
    iter: &mut std::iter::Peekable<I>,
    flag: &str,
) -> Result<String, String>
where
    I: Iterator<Item = &'a str>,
{
    let Some(value) = iter.next() else {
        return Err(format!("{flag} requires a value"));
    };
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{flag} requires a non-empty value"));
    }
    Ok(value.to_string())
}

pub(super) fn parse_host_port(raw: &str, label: &str) -> Result<(String, u16), String> {
    let trimmed = raw.trim();
    let (host_raw, port_text) = if let Some(rest) = trimmed.strip_prefix('[') {
        let (host, remainder) = rest
            .split_once(']')
            .ok_or_else(|| format!("{label} IPv6 host must be in [addr]:port format"))?;
        let port_text = remainder
            .strip_prefix(':')
            .ok_or_else(|| format!("{label} must be in <host:port> format"))?;
        (host, port_text)
    } else {
        let (host, port_text) = trimmed
            .rsplit_once(':')
            .ok_or_else(|| format!("{label} must be in <host:port> format"))?;
        if host.contains(':') {
            return Err(format!("{label} IPv6 host must be wrapped in []"));
        }
        (host, port_text)
    };
    let host = host_raw.trim();
    if host.trim().is_empty() {
        return Err(format!("{label} host cannot be empty"));
    }
    let port = port_text
        .parse::<u16>()
        .map_err(|_| format!("{label} port must be an integer in 1..=65535"))?;
    if port == 0 {
        return Err(format!("{label} port must be in 1..=65535"));
    }
    Ok((host.trim().to_string(), port))
}

fn parse_chain_node_role(raw: &str) -> Result<String, String> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "sequencer" | "storage" | "observer" => Ok(normalized),
        _ => Err("--chain-node-role must be one of: sequencer, storage, observer".to_string()),
    }
}

fn parse_chain_p2p_user_mode(raw: &str) -> Result<String, String> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "auto_join" | "private_safe" | "public_entry" => Ok(normalized),
        _ => Err(
            "--chain-p2p-user-mode must be one of: auto_join, private_safe, public_entry"
                .to_string(),
        ),
    }
}

fn validate_chain_node_validator(raw: &str) -> Result<(), String> {
    let (validator_id, stake) = raw.rsplit_once(':').ok_or_else(|| {
        "--chain-node-validator must be in <validator_id:stake> format".to_string()
    })?;
    if validator_id.trim().is_empty() {
        return Err("--chain-node-validator validator_id cannot be empty".to_string());
    }
    let stake = stake
        .parse::<u64>()
        .map_err(|_| "--chain-node-validator stake must be a positive integer".to_string())?;
    if stake == 0 {
        return Err("--chain-node-validator stake must be a positive integer".to_string());
    }
    Ok(())
}

fn validate_chain_replication_network_peer(raw: &str) -> Result<(), String> {
    parse_chain_replication_bootstrap_peer(raw)
        .map(|_| ())
        .map_err(|err| format!("--chain-replication-network-peer invalid: {err}"))
}

fn parse_chain_link_policy(raw: &str) -> Result<&'static str, String> {
    ChainLinkPolicy::parse(raw)
        .map(ChainLinkPolicy::as_str)
        .ok_or_else(|| {
            format!(
                "--chain-link-policy must be one of enforcing|shadow, got `{}`",
                raw.trim()
            )
        })
}

fn parse_major_world_event_visibility(
    raw: &str,
) -> Result<oasis7_runtime::MajorWorldEventVisibilityPermission, String> {
    match raw.trim() {
        "unknown" => Ok(oasis7_runtime::MajorWorldEventVisibilityPermission::Unknown),
        "public" => Ok(oasis7_runtime::MajorWorldEventVisibilityPermission::Public),
        "restricted" => Ok(oasis7_runtime::MajorWorldEventVisibilityPermission::Restricted),
        "denied" => Ok(oasis7_runtime::MajorWorldEventVisibilityPermission::Denied),
        value => Err(format!(
            "--major-world-event-visibility must be one of unknown|public|restricted|denied, got `{value}`"
        )),
    }
}

pub(super) fn major_world_event_visibility_as_str(
    visibility: oasis7_runtime::MajorWorldEventVisibilityPermission,
) -> &'static str {
    match visibility {
        oasis7_runtime::MajorWorldEventVisibilityPermission::Unknown => "unknown",
        oasis7_runtime::MajorWorldEventVisibilityPermission::Public => "public",
        oasis7_runtime::MajorWorldEventVisibilityPermission::Restricted => "restricted",
        oasis7_runtime::MajorWorldEventVisibilityPermission::Denied => "denied",
    }
}

pub(super) fn print_help() {
    let pos_defaults = oasis7::chain_pos_defaults::defaults();
    println!(
        "Usage: oasis7_game_launcher [options]\n\n\
Start player stack with one command:\n\
- start oasis7_chain_runtime (default)\n\
- start oasis7_viewer_live\n\
- start built-in static web server\n\
- print URL and optionally open browser\n\n\
Options:\n\
  --deployment-mode <mode>    hosted_public_join (default: {DEFAULT_DEPLOYMENT_MODE})\n\
  --allow-trusted-local-playtest
                              Internal-only: allow --deployment-mode trusted_local_only for local playtest stacks\n\
  --scenario <name>            optional debug scenario; default uses formal release fixed world\n\
  --generated-world-dir <dir>  generated-world root with generated-scenario-world sidecar\n\
  --live-bind <host:port>      oasis7_viewer_live bind (default: {DEFAULT_LIVE_BIND})\n\
  --web-bind <host:port>       oasis7_viewer_live web bridge bind (default: {DEFAULT_WEB_BIND})\n\
  --viewer-host <host>         web viewer host (default: {DEFAULT_VIEWER_HOST})\n\
  --viewer-port <port>         web viewer port (default: {DEFAULT_VIEWER_PORT})\n\
  --viewer-static-dir <path>   prebuilt web asset dir (default: {DEFAULT_VIEWER_STATIC_DIR})\n\
  --chain-enable               enable oasis7_chain_runtime (default)\n\
  --chain-disable              disable oasis7_chain_runtime\n\
  --chain-execution-world-dir <path> shared local writer/observer checkpoint root\n\
  --chain-status-bind <addr>   oasis7_chain_runtime status bind (default: {DEFAULT_CHAIN_STATUS_BIND})\n\
  --chain-link-policy <mode>   viewer chain sync policy: enforcing|shadow (default: {DEFAULT_CHAIN_LINK_POLICY})\n\
  --major-world-event-visibility <policy>\n\
                               explicit audience policy: unknown|public|restricted|denied (default: unknown)\n\
  --chain-node-id <id>         oasis7_chain_runtime node id (default: {DEFAULT_CHAIN_NODE_ID})\n\
  --chain-network-tier-manifest <path>\n\
                               formal network tier manifest json; when set, chain bootstrap peers/status tier metadata load from manifest and explicit storage profile becomes optional\n\
  --chain-storage-profile <name> oasis7_chain_runtime storage profile (default: dev_local)\n\
  --chain-world-id <id>        oasis7_chain_runtime world id (default: live-<scenario>)\n\
  --chain-node-role <role>     oasis7_chain_runtime role (default: {DEFAULT_CHAIN_NODE_ROLE})\n\
  --chain-p2p-user-mode <mode> oasis7_chain_runtime user mode: auto_join|private_safe|public_entry (default: {DEFAULT_CHAIN_P2P_USER_MODE})\n\
  --chain-p2p-accept-public-entry\n\
                               accept auto-detected public-entry recommendation\n\
  --chain-p2p-reject-public-entry\n\
                               keep conservative fallback when auto mode suggests public entry (default)\n\
  --chain-replication-network-peer <multiaddr>\n\
                               oasis7_chain_runtime replication bootstrap peer multiaddr (repeatable; first explicit value replaces bundled defaults)\n\
  --provider-bootstrap-authority <path>\n\
                               explicit provider authority bundle for chain runtime/viewer (repeatable; no default)\n\
  --local-test-provider-authority <path>\n\
                               explicit DevLocal authority output bundle (local standalone + builtin_llm)\n\
  --local-test-provider-wasm <path>\n\
                               real WASM artifact for the explicit DevLocal provider setup\n\
  --local-test-provider-metadata <path>\n\
                               canonical build-suite metadata JSON for the WASM artifact\n\
  --local-test-provider-agent-id <id>\n\
                               starter agent to provision (default: starter-agent-0)\n\
  --local-test-provider-owner-binding <id>\n\
                               stable local session owner binding (default: local-test-owner-0)\n\
  --local-test-provider-finality-block-hash <hash>\n\
                               explicit local finality marker, blake3:<64 lowercase hex>\n\
  --local-test-provider-session-mode <mode>\n\
                               hosted_public_join|loopback (default: hosted_public_join)\n\
  --chain-local-standalone-test\n\
                               do not join external testnet peers; configure a self-validating single-node local commit loop\n\
  --chain-node-tick-ms <n>     oasis7_chain_runtime worker poll/fallback interval ms (default: {DEFAULT_CHAIN_NODE_TICK_MS})\n\
  --chain-pos-slot-duration-ms <n>\n\
                               oasis7_chain_runtime PoS slot duration ms (default: {slot_duration_ms})\n\
  --chain-pos-ticks-per-slot <n>\n\
                               oasis7_chain_runtime PoS logical ticks per slot (default: {ticks_per_slot})\n\
  --chain-pos-proposal-tick-phase <n>\n\
                               oasis7_chain_runtime proposal phase in slot tick window (default: {proposal_tick_phase})\n\
  --chain-pos-adaptive-tick-scheduler\n\
                               enable oasis7_chain_runtime adaptive tick scheduler\n\
  --chain-pos-no-adaptive-tick-scheduler\n\
                               disable oasis7_chain_runtime adaptive scheduler (default)\n\
  --chain-pos-slot-clock-genesis-unix-ms <n>\n\
                               oasis7_chain_runtime fixed slot clock genesis unix ms (default: auto)\n\
  --chain-pos-max-past-slot-lag <n>\n\
                               oasis7_chain_runtime max accepted stale slot lag (default: {max_past_slot_lag})\n\
  --chain-node-validator <v:s> oasis7_chain_runtime validator (repeatable)\n\
  --chain-node-auto-attest-all enable local auto-attestation for configured validators\n\
  --chain-node-no-auto-attest-all\n\
                               disable local auto-attestation (default)\n\
  --with-llm                   enable llm mode (default; required for gameplay)\n\
  --auto-play                  start oasis7_viewer_live gameplay/world progression on connect (default)\n\
  --no-auto-play               keep oasis7_viewer_live gameplay/world paused until explicit Play actions\n\
  --allow-debug-scenario       allow seeded debug scenarios such as llm_bootstrap\n\
  --provider-lineage-store <path>\n\
                               explicit durable provider lineage checkpoint for formal/synthetic restart recovery\n\
  --agent-decision-source <src>\n\
                               agent decision source: builtin_llm|provider_backed\n\
  --agent-provider-backend <id>\n\
                               provider backend: provider_local_bridge (default when provider_backed)\n\
  --agent-provider-contract <id>\n\
                               provider contract: worldsim_provider_v1 (default when provider_backed)\n\
  --agent-provider-transport <id>\n\
                               provider transport: loopback_http (default when provider_backed)\n\
  --agent-provider-url <url>   provider URL (default: {DEFAULT_AGENT_PROVIDER_URL})\n\
  --agent-provider-auth-token <tok>\n\
                               provider bearer token\n\
  --agent-provider-connect-timeout-ms <ms>\n\
                               provider connect timeout ms (default: {DEFAULT_AGENT_PROVIDER_CONNECT_TIMEOUT_MS})\n\
  --agent-provider-profile <id>\n\
                               provider profile (default: {DEFAULT_AGENT_PROVIDER_PROFILE})\n\
  --agent-execution-lane <mode>\n\
                               execution lane: player_parity|headless_agent (default: headless_agent)\n\
  --agent-provider-mode <mode> legacy alias for --agent-decision-source; accepts agent_direct_connect/provider_loopback_http\n\\
  --no-open-browser            do not auto open browser\n\
  -h, --help                   show help\n\n\
Env:\n\
  OASIS7_VIEWER_LIVE_BIN              explicit path of oasis7_viewer_live binary\n\
  OASIS7_CHAIN_RUNTIME_BIN            explicit path of oasis7_chain_runtime binary\n\
  OASIS7_GAME_STATIC_DIR              override default viewer static dir when --viewer-static-dir is omitted",
        slot_duration_ms = pos_defaults.slot_duration_ms,
        ticks_per_slot = pos_defaults.ticks_per_slot,
        proposal_tick_phase = pos_defaults.proposal_tick_phase,
        max_past_slot_lag = pos_defaults.max_past_slot_lag,
    );
}
