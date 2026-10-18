use super::*;
#[cfg(any(test, feature = "test_tier_required"))]
use std::net::IpAddr;

pub(super) fn env_requests_provider_backend() -> bool {
    named_env_var_any(&[
        VIEWER_AGENT_DECISION_SOURCE_ENV,
        VIEWER_AGENT_PROVIDER_MODE_ENV,
    ])
    .map(|value| value.trim().to_string())
    .as_deref()
    .and_then(canonical_agent_decision_source)
    .is_some_and(|value| value == PROVIDER_BACKED_DECISION_SOURCE)
}

/// The in-crate local-mock capability path is deliberately narrower than the
/// normal provider-backed lane.  It is available only to a test-tier build,
/// an explicitly Hosted server, and a complete loopback provider configuration.
/// A backend name by itself must never widen PromptControl authority.
pub(in crate::viewer::runtime_live) fn hosted_local_mock_test_lane_enabled(
    hosted_public_join_mode: bool,
) -> bool {
    if !hosted_public_join_mode {
        return false;
    }

    #[cfg(any(test, feature = "test_tier_required"))]
    {
        let decision_source = named_env_var_any(&[VIEWER_AGENT_DECISION_SOURCE_ENV]);
        let backend = named_env_var_any(&[VIEWER_AGENT_PROVIDER_BACKEND_ENV]);
        let contract = named_env_var_any(&[VIEWER_AGENT_PROVIDER_CONTRACT_ENV]);
        let transport = named_env_var_any(&[VIEWER_AGENT_PROVIDER_TRANSPORT_ENV]);
        let profile = named_env_var_any(&[VIEWER_AGENT_PROVIDER_PROFILE_ENV]);
        let execution_lane = named_env_var_any(&[VIEWER_AGENT_EXECUTION_LANE_ENV]);
        let provider_url = named_env_var_any(&[VIEWER_AGENT_PROVIDER_URL_ENV]);

        decision_source
            .as_deref()
            .and_then(canonical_agent_decision_source)
            == Some(PROVIDER_BACKED_DECISION_SOURCE)
            && backend
                .as_deref()
                .and_then(canonical_agent_provider_backend)
                == Some(LOCAL_MOCK_PROVIDER_BACKEND)
            && contract
                .as_deref()
                .and_then(canonical_agent_provider_contract)
                == Some(WORLDSIM_PROVIDER_CONTRACT)
            && transport
                .as_deref()
                .and_then(canonical_agent_provider_transport)
                == Some(LOOPBACK_HTTP_PROVIDER_TRANSPORT)
            && profile.as_deref() == Some(DEFAULT_PROVIDER_AGENT_PROFILE)
            && execution_lane.as_deref() == Some("player_parity")
            && provider_url
                .as_deref()
                .is_some_and(is_loopback_http_provider_url)
    }

    #[cfg(not(any(test, feature = "test_tier_required")))]
    {
        false
    }
}

pub(in crate::viewer::runtime_live) fn install_hosted_local_mock_test_capability_fixtures(
    world: &mut RuntimeWorld,
    hosted_public_join_mode: bool,
) -> Result<bool, String> {
    if !hosted_local_mock_test_lane_enabled(hosted_public_join_mode) {
        return Ok(false);
    }

    #[cfg(any(test, feature = "test_tier_required"))]
    {
        let agent_ids = world.state().agents.keys().cloned().collect::<Vec<_>>();
        if agent_ids.is_empty() {
            return Err(
                "Hosted local-mock test lane requires a Runtime-seeded Agent for capability installation"
                    .to_string(),
            );
        }
        for agent_id in agent_ids {
            world
                .install_test_provider_capability_fixture(agent_id.as_str())
                .map_err(|error| {
                    format!(
                        "Hosted local-mock Runtime capability fixture installation failed for agent {agent_id}: {error:?}"
                    )
                })?;
        }
        Ok(true)
    }

    #[cfg(not(any(test, feature = "test_tier_required")))]
    {
        let _ = world;
        Ok(false)
    }
}

#[cfg(any(test, feature = "test_tier_required"))]
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
    let Some((host, port_text)) = parse_provider_host_port(authority) else {
        return false;
    };
    let Ok(port) = port_text.parse::<u16>() else {
        return false;
    };
    if port == 0 {
        return false;
    }
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<IpAddr>()
        .map(|address| address.is_loopback())
        .unwrap_or(false)
}

#[cfg(any(test, feature = "test_tier_required"))]
fn parse_provider_host_port(authority: &str) -> Option<(&str, &str)> {
    if let Some(rest) = authority.strip_prefix('[') {
        let (host, remainder) = rest.split_once(']')?;
        let port = remainder.strip_prefix(':')?;
        if host.is_empty() || port.is_empty() || port.contains(':') {
            return None;
        }
        return Some((host, port));
    }
    let (host, port) = authority.rsplit_once(':')?;
    if host.is_empty() || host.contains(':') || port.is_empty() {
        return None;
    }
    Some((host, port))
}

pub(in crate::viewer::runtime_live) fn provider_settings_from_env()
-> Result<Option<ProviderDecisionSettings>, String> {
    let decision_source = named_env_var_any(&[
        VIEWER_AGENT_DECISION_SOURCE_ENV,
        VIEWER_AGENT_PROVIDER_MODE_ENV,
    ])
    .unwrap_or_default();
    let decision_source = decision_source.trim();
    if decision_source.is_empty() || decision_source == BUILTIN_LLM_DECISION_SOURCE {
        return Ok(None);
    }
    let Some(_) = canonical_agent_decision_source(decision_source) else {
        return Err(format!(
            "unsupported agent decision source `{decision_source}`; expected builtin_llm or provider_backed"
        ));
    };

    let backend = named_env_var_any(&[
        VIEWER_AGENT_PROVIDER_BACKEND_ENV,
        VIEWER_AGENT_PROVIDER_MODE_ENV,
    ])
    .unwrap_or_else(|| LOCAL_BRIDGE_PROVIDER_BACKEND.to_string());
    let Some(_) = canonical_agent_provider_backend(backend.as_str()) else {
        return Err(format!(
            "unsupported agent provider backend `{backend}`; expected provider_local_bridge or provider_local_mock"
        ));
    };
    let contract = named_env_var_any(&[
        VIEWER_AGENT_PROVIDER_CONTRACT_ENV,
        VIEWER_AGENT_PROVIDER_MODE_ENV,
    ])
    .unwrap_or_else(|| WORLDSIM_PROVIDER_CONTRACT.to_string());
    let Some(_) = canonical_agent_provider_contract(contract.as_str()) else {
        return Err(format!(
            "unsupported agent provider contract `{contract}`; expected worldsim_provider_v1"
        ));
    };
    let transport = named_env_var_any(&[
        VIEWER_AGENT_PROVIDER_TRANSPORT_ENV,
        VIEWER_AGENT_PROVIDER_MODE_ENV,
    ])
    .unwrap_or_else(|| LOOPBACK_HTTP_PROVIDER_TRANSPORT.to_string());
    let Some(_) = canonical_agent_provider_transport(transport.as_str()) else {
        return Err(format!(
            "unsupported agent provider transport `{transport}`; expected loopback_http or remote_https"
        ));
    };

    let base_url = named_env_var_any(&[VIEWER_AGENT_PROVIDER_URL_ENV]).unwrap_or_default();
    let base_url = base_url.trim();
    if base_url.is_empty() {
        return Err(format!(
            "{VIEWER_AGENT_PROVIDER_URL_ENV} is required for provider_backed/provider_local_bridge"
        ));
    }

    let connect_timeout_ms = named_env_var_any(&[VIEWER_AGENT_PROVIDER_CONNECT_TIMEOUT_MS_ENV])
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| {
            value.parse::<u64>().map_err(|err| {
                format!(
                    "invalid {VIEWER_AGENT_PROVIDER_CONNECT_TIMEOUT_MS_ENV} value `{value}`: {err}"
                )
            })
        })
        .transpose()?
        .unwrap_or(DEFAULT_PROVIDER_CONNECT_TIMEOUT_MS);
    if connect_timeout_ms == 0 {
        return Err(format!(
            "{VIEWER_AGENT_PROVIDER_CONNECT_TIMEOUT_MS_ENV} must be greater than zero"
        ));
    }
    let decision_timeout_ms = named_env_var_any(&[VIEWER_AGENT_PROVIDER_DECISION_TIMEOUT_MS_ENV])
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| {
            value.parse::<u64>().map_err(|err| {
                format!(
                    "invalid {VIEWER_AGENT_PROVIDER_DECISION_TIMEOUT_MS_ENV} value `{value}`: {err}"
                )
            })
        })
        .transpose()?
        .unwrap_or(connect_timeout_ms);
    if decision_timeout_ms == 0 {
        return Err(format!(
            "{VIEWER_AGENT_PROVIDER_DECISION_TIMEOUT_MS_ENV} must be greater than zero"
        ));
    }

    let agent_profile = named_env_var_any(&[VIEWER_AGENT_PROVIDER_PROFILE_ENV])
        .unwrap_or_else(|| DEFAULT_PROVIDER_AGENT_PROFILE.to_string());
    let agent_profile = agent_profile.trim();
    if agent_profile.is_empty() {
        return Err(format!(
            "{VIEWER_AGENT_PROVIDER_PROFILE_ENV} cannot be empty for provider_backed/provider_local_bridge"
        ));
    }

    let auth_token = named_env_var_any(&[VIEWER_AGENT_PROVIDER_AUTH_TOKEN_ENV])
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let execution_mode = named_env_var_any(&[VIEWER_AGENT_EXECUTION_LANE_ENV])
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| {
            ProviderExecutionMode::parse(value.as_str()).ok_or_else(|| {
                format!(
                    "invalid {VIEWER_AGENT_EXECUTION_LANE_ENV} value `{value}`: expected player_parity or headless_agent"
                )
            })
        })
        .transpose()?
        .unwrap_or(ProviderExecutionMode::HeadlessAgent);

    Ok(Some(ProviderDecisionSettings {
        requested_provider_mode: decision_source.to_string(),
        provider_transport: canonical_agent_provider_transport(transport.as_str())
            .unwrap_or(LOOPBACK_HTTP_PROVIDER_TRANSPORT)
            .to_string(),
        base_url: base_url.to_string(),
        auth_token,
        connect_timeout_ms,
        decision_timeout_ms,
        agent_profile: agent_profile.to_string(),
        execution_mode,
        fallback_reason: provider_mode_fallback_reason(decision_source),
    }))
}

pub(super) fn provider_phase1_action_catalog() -> Vec<ActionCatalogEntry> {
    vec![
        ActionCatalogEntry::new("wait", "yield current turn without acting"),
        ActionCatalogEntry::new("wait_ticks", "sleep for a bounded number of ticks"),
        ActionCatalogEntry::new("move_agent", "move to a neighboring location"),
        ActionCatalogEntry::new(
            "harvest_radiation",
            "recover electricity before industrial expansion or recipe execution",
        ),
        ActionCatalogEntry::new(
            "mine_compound",
            "extract raw compound mass when recovery needs material inputs",
        ),
        ActionCatalogEntry::new(
            "refine_compound",
            "convert compound mass into hardware output for recovery",
        ),
        ActionCatalogEntry::new(
            "build_factory",
            "start the first compatible factory line for industrial progression",
        ),
        ActionCatalogEntry::new(
            "schedule_recipe",
            "run the next compatible recipe on an existing factory line",
        ),
        ActionCatalogEntry::new("speak_to_nearby", "emit a lightweight nearby speech event"),
        ActionCatalogEntry::new(
            "inspect_target",
            "emit a lightweight target inspection event",
        ),
        ActionCatalogEntry::new(
            "simple_interact",
            "emit a lightweight simple interaction event",
        ),
    ]
}

pub(super) fn provider_phase1_memory_summary() -> String {
    concat!(
        "goal=post_onboarding.establish_first_capability; ",
        "开局默认种子资源通常已足够首个 smelter（例如 electricity>=10 且 data>=5）；",
        "若当前没有 factory.smelter.mk1，不要先 harvest_radiation，优先 build_factory(factory.smelter.mk1)。",
        "只有在 electricity<10 时才先 harvest_radiation；只有在 data<5 时才先 mine_compound/refine_compound。 ",
        "build_factory 成功后立刻 schedule_recipe(",
        "recipe.smelter.iron_ingot|recipe.smelter.copper_wire|recipe.smelter.polymer_resin|recipe.smelter.alloy_plate",
        ")；不要长期停留在 wait/move/speak/inspect。"
    )
    .to_string()
}

fn canonical_agent_decision_source(raw: &str) -> Option<&'static str> {
    match raw.trim() {
        BUILTIN_LLM_DECISION_SOURCE => Some(BUILTIN_LLM_DECISION_SOURCE),
        PROVIDER_BACKED_DECISION_SOURCE
        | PROVIDER_LOOPBACK_HTTP_IMPLEMENTATION
        | AGENT_DIRECT_CONNECT_PROVIDER_MODE_ALIAS => Some(PROVIDER_BACKED_DECISION_SOURCE),
        _ => None,
    }
}

fn canonical_agent_provider_backend(raw: &str) -> Option<&'static str> {
    match raw.trim() {
        LOCAL_MOCK_PROVIDER_BACKEND => Some(LOCAL_MOCK_PROVIDER_BACKEND),
        LOCAL_BRIDGE_PROVIDER_BACKEND
        | PROVIDER_LOOPBACK_HTTP_IMPLEMENTATION
        | AGENT_DIRECT_CONNECT_PROVIDER_MODE_ALIAS => Some(LOCAL_BRIDGE_PROVIDER_BACKEND),
        _ => None,
    }
}

fn canonical_agent_provider_contract(raw: &str) -> Option<&'static str> {
    match raw.trim() {
        WORLDSIM_PROVIDER_CONTRACT
        | PROVIDER_LOOPBACK_HTTP_IMPLEMENTATION
        | AGENT_DIRECT_CONNECT_PROVIDER_MODE_ALIAS => Some(WORLDSIM_PROVIDER_CONTRACT),
        _ => None,
    }
}

fn canonical_agent_provider_transport(raw: &str) -> Option<&'static str> {
    match raw.trim() {
        LOOPBACK_HTTP_PROVIDER_TRANSPORT
        | PROVIDER_LOOPBACK_HTTP_IMPLEMENTATION
        | AGENT_DIRECT_CONNECT_PROVIDER_MODE_ALIAS => Some(LOOPBACK_HTTP_PROVIDER_TRANSPORT),
        REMOTE_HTTPS_PROVIDER_TRANSPORT => Some(REMOTE_HTTPS_PROVIDER_TRANSPORT),
        _ => None,
    }
}

fn named_env_var_any(env_names: &[&str]) -> Option<String> {
    env_names
        .iter()
        .find_map(|env_name| std::env::var(env_name).ok())
}

fn provider_mode_fallback_reason(provider_mode: &str) -> Option<String> {
    match provider_mode.trim() {
        AGENT_DIRECT_CONNECT_PROVIDER_MODE_ALIAS => {
            Some("provider_mode_alias:agent_direct_connect".to_string())
        }
        _ => None,
    }
}
