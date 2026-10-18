use super::*;
use sha2::{Digest, Sha256};

const ENV_VIEWER_LIVE_LLM_TIMEOUT_MS: &str = "OASIS7_VIEWER_LIVE_LLM_TIMEOUT_MS";
const DEFAULT_VIEWER_LIVE_LLM_TIMEOUT_MS: u64 = 30_000;

pub(super) fn resolve_viewer_live_llm_timeout_ms(configured_timeout_ms: u64) -> u64 {
    let configured_timeout_ms = configured_timeout_ms.max(1);
    let live_timeout_ceiling_ms = std::env::var(ENV_VIEWER_LIVE_LLM_TIMEOUT_MS)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(|value| value.max(1))
        .unwrap_or(DEFAULT_VIEWER_LIVE_LLM_TIMEOUT_MS);
    configured_timeout_ms.min(live_timeout_ceiling_ms)
}

#[expect(
    clippy::result_large_err,
    reason = "Prompt-control protocol errors preserve the stable typed error envelope"
)]
pub(super) fn normalize_required_player_id(
    player_id: &str,
    agent_id: &str,
) -> Result<String, PromptControlError> {
    let normalized = player_id.trim();
    if normalized.is_empty() {
        return Err(PromptControlError {
            code: "player_id_required".to_string(),
            message: format!(
                "prompt_control for {} requires non-empty player_id",
                agent_id
            ),
            agent_id: Some(agent_id.to_string()),
            current_version: None,
            ..PromptControlError::default_legacy()
        });
    }
    Ok(normalized.to_string())
}

pub(super) fn normalize_optional_public_key(public_key: Option<&str>) -> Option<String> {
    public_key
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[expect(
    clippy::result_large_err,
    reason = "Prompt-control protocol errors preserve the stable typed error envelope"
)]
pub(super) fn ensure_updated_by_matches_player(
    updated_by: Option<&str>,
    player_id: &str,
    agent_id: &str,
) -> Result<(), PromptControlError> {
    let Some(updated_by) = updated_by.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    if updated_by == player_id {
        return Ok(());
    }
    Err(PromptControlError {
        code: "updated_by_mismatch".to_string(),
        message: format!(
            "updated_by ({}) must match player_id ({}) for {}",
            updated_by, player_id, agent_id
        ),
        agent_id: Some(agent_id.to_string()),
        current_version: None,
        ..PromptControlError::default_legacy()
    })
}

#[expect(
    clippy::result_large_err,
    reason = "Prompt-control protocol errors preserve the stable typed error envelope"
)]
pub(super) fn ensure_agent_player_access(
    kernel: &WorldKernel,
    agent_id: &str,
    player_id: &str,
    public_key: Option<&str>,
) -> Result<(), PromptControlError> {
    if !kernel.model().agents.contains_key(agent_id) {
        return Err(PromptControlError {
            code: "agent_not_found".to_string(),
            message: format!("agent not found: {agent_id}"),
            agent_id: Some(agent_id.to_string()),
            current_version: None,
            ..PromptControlError::default_legacy()
        });
    }
    let Some(bound_player_id) = kernel.player_binding_for_agent(agent_id) else {
        return Ok(());
    };
    if bound_player_id == player_id {
        let Some(bound_public_key) = kernel.public_key_binding_for_agent(agent_id) else {
            return Ok(());
        };
        let requested_public_key = normalize_optional_public_key(public_key);
        if requested_public_key.as_deref() == Some(bound_public_key) {
            return Ok(());
        }
        let message = if requested_public_key.is_none() {
            format!(
                "agent {} is bound to player {} with public_key {}, public_key is required",
                agent_id, bound_player_id, bound_public_key
            )
        } else {
            format!(
                "agent {} is bound to player {} with different public_key",
                agent_id, bound_player_id
            )
        };
        return Err(PromptControlError {
            code: "agent_control_forbidden".to_string(),
            message,
            agent_id: Some(agent_id.to_string()),
            current_version: kernel
                .model()
                .agent_prompt_profiles
                .get(agent_id)
                .map(|profile| profile.version),
            ..PromptControlError::default_legacy()
        });
    }
    Err(PromptControlError {
        code: "agent_control_forbidden".to_string(),
        message: format!(
            "agent {} is bound to player {}, not {}",
            agent_id, bound_player_id, player_id
        ),
        agent_id: Some(agent_id.to_string()),
        current_version: kernel
            .model()
            .agent_prompt_profiles
            .get(agent_id)
            .map(|profile| profile.version),
        ..PromptControlError::default_legacy()
    })
}

pub(super) fn sanitize_patch_string(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
}

pub(super) fn apply_prompt_patch(
    profile: &mut AgentPromptProfile,
    request: &PromptControlApplyRequest,
) {
    if let Some(next) = &request.system_prompt_override {
        profile.system_prompt_override = sanitize_patch_string(next.clone());
    }
    if let Some(next) = &request.short_term_goal_override {
        profile.short_term_goal_override = sanitize_patch_string(next.clone());
    }
    if let Some(next) = &request.long_term_goal_override {
        profile.long_term_goal_override = sanitize_patch_string(next.clone());
    }
}

pub(super) fn changed_prompt_fields(
    current: &AgentPromptProfile,
    candidate: &AgentPromptProfile,
) -> Vec<String> {
    let mut fields = Vec::new();
    if current.system_prompt_override != candidate.system_prompt_override {
        fields.push("system_prompt_override".to_string());
    }
    if current.short_term_goal_override != candidate.short_term_goal_override {
        fields.push("short_term_goal_override".to_string());
    }
    if current.long_term_goal_override != candidate.long_term_goal_override {
        fields.push("long_term_goal_override".to_string());
    }
    fields
}

pub(super) fn prompt_profile_digest(profile: &AgentPromptProfile) -> String {
    let payload = serde_json::json!({
        "agent_id": profile.agent_id,
        "system_prompt_override": profile.system_prompt_override,
        "short_term_goal_override": profile.short_term_goal_override,
        "long_term_goal_override": profile.long_term_goal_override,
    });
    let bytes = serde_json::to_vec(&payload).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[expect(
    clippy::result_large_err,
    reason = "Prompt-control protocol errors preserve the stable typed error envelope"
)]
pub(super) fn ensure_expected_prompt_version(
    agent_id: &str,
    current_version: u64,
    expected_version: Option<u64>,
) -> Result<(), PromptControlError> {
    if let Some(expected) = expected_version
        && expected != current_version
    {
        return Err(PromptControlError {
            code: "version_conflict".to_string(),
            message: format!(
                "prompt profile version conflict for {}: expected {}, current {}",
                agent_id, expected, current_version
            ),
            agent_id: Some(agent_id.to_string()),
            current_version: Some(current_version),
            ..PromptControlError::default_legacy()
        });
    }
    Ok(())
}

pub(super) fn build_driver(
    kernel: &WorldKernel,
    decision_mode: ViewerLiveDecisionMode,
) -> Result<LiveDriver, ViewerLiveServerError> {
    match decision_mode {
        ViewerLiveDecisionMode::Script => Ok(LiveDriver::Script(LiveScript::new(kernel))),
        ViewerLiveDecisionMode::Llm => {
            let mut runner = AgentRunner::new();
            let mut agent_ids: Vec<String> = kernel.model().agents.keys().cloned().collect();
            agent_ids.sort();
            for agent_id in agent_ids {
                let mut config = LlmAgentConfig::from_default_sources_for_agent(agent_id.as_str())
                    .map_err(LlmAgentBuildError::Config)?;
                config.timeout_ms = resolve_viewer_live_llm_timeout_ms(config.timeout_ms);
                let client = OpenAiChatCompletionClient::from_config(&config)
                    .map_err(LlmAgentBuildError::Client)?;
                let mut behavior = LlmAgentBehavior::new(agent_id.clone(), config, client);
                if let Some(profile) = kernel.model().agent_prompt_profiles.get(&agent_id) {
                    behavior.apply_prompt_overrides(
                        profile.system_prompt_override.clone(),
                        profile.short_term_goal_override.clone(),
                        profile.long_term_goal_override.clone(),
                    );
                }
                restore_behavior_long_term_memory_from_model(&mut behavior, kernel, &agent_id);
                runner.register(behavior);
            }
            Ok(LiveDriver::Llm(runner))
        }
    }
}

pub(super) fn restore_behavior_long_term_memory_from_model(
    behavior: &mut LlmAgentBehavior<OpenAiChatCompletionClient>,
    kernel: &WorldKernel,
    agent_id: &str,
) {
    if let Some(entries) = kernel.long_term_memory_for_agent(agent_id) {
        behavior.restore_long_term_memory_entries(entries);
    } else {
        behavior.restore_long_term_memory_entries(&[]);
    }
}

pub(super) fn sync_llm_runner_long_term_memory(
    kernel: &mut WorldKernel,
    runner: &AgentRunner<LlmAgentBehavior<OpenAiChatCompletionClient>>,
) {
    for agent_id in runner.agent_ids() {
        let Some(agent) = runner.get(agent_id.as_str()) else {
            continue;
        };
        let entries = agent.behavior.export_long_term_memory_entries();
        if let Err(message) = kernel.set_agent_long_term_memory(agent_id.as_str(), entries) {
            crate::observability::emit_stderr_or_event(
                tracing::Level::WARN,
                format!(
                    "viewer live: skip long-term memory sync for {}: {}",
                    agent_id, message
                )
                .as_str(),
                "viewer live skipped long-term memory sync",
            );
        }
    }
}
