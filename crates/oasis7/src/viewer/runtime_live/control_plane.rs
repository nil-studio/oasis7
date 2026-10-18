use super::*;

use super::super::auth::{
    AGENT_CHAT_AUTHORITY_SCOPE, PromptControlAuthIntent, VerifiedPlayerAuth,
    has_enhanced_prompt_identity_apply, has_enhanced_prompt_identity_rollback,
    normalize_prompt_control_operation_identity, prompt_control_operation_digest,
    verify_agent_chat_auth_proof_with_authority,
    verify_hosted_prompt_control_apply_strong_auth_grant,
    verify_hosted_prompt_control_rollback_strong_auth_grant,
    verify_prompt_control_apply_auth_proof, verify_prompt_control_rollback_auth_proof,
};
use super::super::protocol::{
    AgentChatAck, AgentChatError, AgentChatRequest, PromptControlAck,
    PromptControlApplicationScope, PromptControlApplyRequest, PromptControlCommand,
    PromptControlError, PromptControlOperation, PromptControlResultStatus,
    PromptControlRollbackRequest, PromptControlValueVisibility,
};
use crate::runtime::{
    AgentIntentAuthorityContext, AgentIntentProviderFailureDisposition, AgentIntentV2,
    World as RuntimeWorld,
};
use oasis7_simulator::{AgentPromptProfile, PromptUpdateOperation, WorldEventKind};
use sha2::{Digest, Sha256};

#[path = "control_plane/agent_chat.rs"]
mod agent_chat;
mod agent_chat_intent;
#[path = "control_plane/agent_memory_correction.rs"]
mod agent_memory_correction;
#[path = "control_plane/auth_helpers.rs"]
mod auth_helpers;
#[path = "control_plane/canonical_agent_chat.rs"]
mod canonical_agent_chat;
pub(in crate::viewer::runtime_live) mod llm_sidecar;
#[path = "control_plane/prompt_control_enhanced.rs"]
mod prompt_control_enhanced;
#[path = "control_plane/prompt_control_legacy.rs"]
mod prompt_control_legacy;
#[path = "control_plane/prompt_control_terminal.rs"]
mod prompt_control_terminal;
#[path = "control_plane/prompt_profile.rs"]
mod prompt_profile;
#[path = "control_plane/provider_action.rs"]
mod provider_action;
use super::prompt_control_result::{
    PromptControlLedgerInsertError, PromptControlLedgerLookup, PromptControlLedgerReceipt,
};
#[path = "control_plane/provider_action_commit.rs"]
pub(in crate::viewer::runtime_live) mod provider_action_commit;
pub(in crate::viewer::runtime_live) use agent_chat_intent::RuntimePrimaryIntent;
use agent_chat_intent::{apply_accepted_primary_intent, resolve_agent_chat_intent};
pub(super) use auth_helpers::map_auth_verify_error_code;
use auth_helpers::{hosted_strong_auth_grant_public_key_from_env, hosted_strong_auth_now_unix_ms};
pub(in crate::viewer::runtime_live) use llm_sidecar::{
    RuntimeChatIntentAckRecord, RuntimeLlmSidecar, RuntimePlayerBindingPlan,
    hosted_local_mock_test_lane_enabled, install_hosted_local_mock_test_capability_fixtures,
    simulator_action_label, simulator_action_to_runtime,
};
const RUNTIME_AGENT_CHAT_ECHO_ENV: &str = "OASIS7_RUNTIME_AGENT_CHAT_ECHO";
const RUNTIME_AGENT_CHAT_ECHO_PREFIX: &str = "[local-mock-receipt]";
const RUNTIME_AGENT_CHAT_ECHO_NOTICE: &str =
    "已收到消息；当前本地 mock provider 不生成真实 Agent 回复：";
const HOSTED_STRONG_AUTH_GRANT_PUBLIC_KEY_ENV: &str = "OASIS7_HOSTED_STRONG_AUTH_PUBLIC_KEY";

#[allow(dead_code)]
pub(in crate::viewer::runtime_live) fn runtime_provider_settings_from_env()
-> Result<Option<llm_sidecar::ProviderDecisionSettings>, String> {
    llm_sidecar::provider_settings_from_env()
}

#[allow(dead_code)]
pub(in crate::viewer::runtime_live) fn resolve_runtime_live_llm_timeout_ms(
    configured_timeout_ms: u64,
) -> u64 {
    llm_sidecar::resolve_runtime_live_llm_timeout_ms(configured_timeout_ms)
}

pub fn runtime_agent_chat_echo_enabled_from_env() -> bool {
    std::env::var(RUNTIME_AGENT_CHAT_ECHO_ENV)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn provider_reply_matches_current_intent(
    current: Option<&AgentIntentV2>,
    pending_intent_id: Option<&str>,
    pending_request_digest: Option<&str>,
) -> bool {
    current.is_some_and(|current| {
        pending_intent_id == Some(current.intent_id.as_str())
            && pending_request_digest == Some(current.request_digest.as_str())
            && matches!(current.status.as_str(), "accepted" | "blocked")
    })
}

impl ViewerRuntimeLiveServer {
    fn verify_agent_chat_auth(
        &mut self,
        request: &AgentChatRequest,
    ) -> Result<VerifiedPlayerAuth, AgentChatError> {
        let Some(auth) = request.auth.as_ref() else {
            return Err(AgentChatError {
                code: "auth_proof_required".to_string(),
                message: "agent_chat requires auth proof".to_string(),
                agent_id: Some(request.agent_id.clone()),
            });
        };
        let verified = verify_agent_chat_auth_proof_with_authority(
            request,
            auth,
            self.config.world_id.as_str(),
            self.reorg_epoch,
            AGENT_CHAT_AUTHORITY_SCOPE,
        )
        .map_err(|message| AgentChatError {
            code: map_auth_verify_error_code(message.as_str()).to_string(),
            message,
            agent_id: Some(request.agent_id.clone()),
        })?;
        Ok(verified)
    }

    #[expect(
        clippy::result_large_err,
        reason = "Prompt-control protocol errors preserve the stable typed error envelope"
    )]
    fn bind_agent_player_access(
        &mut self,
        agent_id: &str,
        player_id: &str,
        public_key: Option<&str>,
    ) -> Result<(), PromptControlError> {
        ensure_agent_player_access_runtime(
            &self.world,
            &self.llm_sidecar,
            agent_id,
            player_id,
            public_key,
        )?;
        let events = self
            .llm_sidecar
            .bind_agent_player(agent_id, player_id, public_key, false)
            .map_err(|message| PromptControlError {
                code: "player_bind_failed".to_string(),
                message,
                agent_id: Some(agent_id.to_string()),
                current_version: self.current_prompt_version(agent_id),
                ..PromptControlError::default_legacy()
            })?;
        for event in events {
            if matches!(
                &event,
                WorldEventKind::AgentPlayerBound { .. } | WorldEventKind::AgentPlayerUnbound { .. }
            ) {
                self.prompt_control_authority
                    .advance_binding_epoch(agent_id);
            }
            self.enqueue_virtual_event(event);
        }
        Ok(())
    }

    fn bind_agent_player_access_for_chat(
        &mut self,
        agent_id: &str,
        player_id: &str,
        public_key: Option<&str>,
    ) -> Result<(), AgentChatError> {
        let mapped = ensure_agent_player_access_runtime(
            &self.world,
            &self.llm_sidecar,
            agent_id,
            player_id,
            public_key,
        )
        .map_err(|err| AgentChatError {
            code: "agent_control_forbidden".to_string(),
            message: err.message,
            agent_id: err.agent_id,
        });
        mapped?;
        let events = self
            .llm_sidecar
            .bind_agent_player(agent_id, player_id, public_key, false)
            .map_err(|message| AgentChatError {
                code: "player_bind_failed".to_string(),
                message,
                agent_id: Some(agent_id.to_string()),
            })?;
        for event in events {
            self.enqueue_virtual_event(event);
        }
        Ok(())
    }

    pub(super) fn enqueue_virtual_event(&mut self, kind: WorldEventKind) {
        let id = self.next_virtual_event_id();
        self.pending_virtual_events.push_back(WorldEvent {
            id,
            time: self.world.state().time,
            kind,
            runtime_event: None,
        });
    }

    fn enqueue_agent_chat_echo_event_if_enabled(&mut self, agent_id: &str, message: &str) {
        if self.chain_link_enabled() || !self.config.agent_chat_echo_enabled {
            return;
        }
        let Some(agent) = self.world.state().agents.get(agent_id) else {
            return;
        };
        self.enqueue_virtual_event(WorldEventKind::AgentSpoke {
            agent_id: agent_id.to_string(),
            location_id: location_id_for_pos(agent.state.pos),
            message: format!(
                "{RUNTIME_AGENT_CHAT_ECHO_PREFIX} {RUNTIME_AGENT_CHAT_ECHO_NOTICE}{message}"
            ),
            target_agent_id: None,
        });
    }

    fn enqueue_agent_chat_reply_event(&mut self, agent_id: &str, message: &str) {
        if self.chain_link_enabled() {
            return;
        }
        let Some(agent) = self.world.state().agents.get(agent_id) else {
            return;
        };
        self.enqueue_virtual_event(WorldEventKind::AgentSpoke {
            agent_id: agent_id.to_string(),
            location_id: location_id_for_pos(agent.state.pos),
            message: message.to_string(),
            target_agent_id: None,
        });
    }

    /// Complete a chat Intent only when the runtime has already published a
    /// receipt for the exact bound effect. Provider replies and local echoes
    /// are observations, not effects; this hook keeps their terminal path
    /// receipt-bound without inventing an effect or receipt for chat itself.
    pub(super) fn complete_agent_chat_if_receipt_bound(
        &mut self,
        agent_id: &str,
        intent_id: &str,
        request_digest: &str,
    ) {
        if self.chain_link_enabled() {
            return;
        }
        if let Err(error) =
            self.world
                .reconcile_agent_intent_receipt(crate::runtime::AgentIntentReference {
                    agent_id,
                    intent_id,
                    request_digest,
                })
        {
            tracing::warn!(agent_id, intent_id, error = ?error,
                "receipt-bound Agent Chat completion was not applied");
        }
    }

    pub(super) fn enqueue_pending_provider_agent_chat_replies(&mut self) -> Vec<AgentChatError> {
        if self.chain_link_enabled() {
            return vec![AgentChatError {
                code: "canonical_chat_unsupported".into(),
                message: "world service has no registered Agent chat capability".into(),
                agent_id: None,
            }];
        }
        let (replies, failures) = self
            .llm_sidecar
            .drain_provider_agent_chat_replies_with_identity(&self.world);
        for (pending, message) in replies {
            let current = self
                .world
                .state()
                .agents
                .get(pending.agent_id.as_str())
                .and_then(|cell| cell.intent.as_ref());
            if provider_reply_matches_current_intent(
                current,
                pending.intent_id.as_deref(),
                pending.request_digest.as_deref(),
            ) {
                self.enqueue_agent_chat_reply_event(pending.agent_id.as_str(), message.as_str());
                if let (Some(intent_id), Some(request_digest)) = (
                    pending.intent_id.as_deref(),
                    pending.request_digest.as_deref(),
                ) {
                    self.complete_agent_chat_if_receipt_bound(
                        pending.agent_id.as_str(),
                        intent_id,
                        request_digest,
                    );
                }
            } else {
                tracing::debug!(
                    agent_id = pending.agent_id.as_str(),
                    intent_id = pending.intent_id.as_deref().unwrap_or(""),
                    "discarding stale provider agent-chat reply"
                );
            }
        }
        let mut errors = Vec::with_capacity(failures.len());
        for failure in failures {
            let disposition = if failure.retryable {
                AgentIntentProviderFailureDisposition::Blocked
            } else {
                AgentIntentProviderFailureDisposition::Rejected
            };
            let disposition_error = if let (Some(intent_id), Some(request_digest)) = (
                failure.pending.intent_id.as_deref(),
                failure.pending.request_digest.as_deref(),
            ) {
                self.world
                    .record_agent_provider_failure(
                        crate::runtime::AgentIntentReference {
                            agent_id: failure.pending.agent_id.as_str(),
                            intent_id,
                            request_digest,
                        },
                        disposition,
                    )
                    .err()
            } else {
                Some(crate::runtime::WorldError::ResourceBalanceInvalid {
                    reason: "provider chat failure has no durable intent identity".to_string(),
                })
            };
            if let Some(error) = disposition_error {
                let failure_agent_id = failure.pending.agent_id.clone();
                tracing::error!(
                    agent_id = failure_agent_id.as_str(),
                    error = ?error,
                    "failed to persist exact provider chat disposition"
                );
                errors.push(AgentChatError {
                    code: "intent_disposition_failed".to_string(),
                    message: format!(
                        "provider chat failed but its durable intent disposition could not be persisted: {error:?}"
                    ),
                    agent_id: Some(failure_agent_id),
                });
            } else {
                errors.push(failure.error);
            }
        }
        errors
    }

    fn next_virtual_event_id(&mut self) -> u64 {
        let floor = latest_runtime_event_seq(&self.world)
            .saturating_add(1)
            .max(1);
        if self.next_virtual_event_id < floor {
            self.next_virtual_event_id = floor;
        }
        let id = self.next_virtual_event_id;
        self.next_virtual_event_id = self.next_virtual_event_id.saturating_add(1);
        id
    }
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

#[expect(
    clippy::too_many_arguments,
    reason = "Prompt-control error construction preserves the stable protocol context"
)]
fn prompt_control_enhanced_error(
    code: &str,
    message: &str,
    request_id: Option<String>,
    operation: PromptControlOperation,
    preview: bool,
    agent_id: Option<String>,
    player_id: Option<String>,
    status: PromptControlResultStatus,
) -> PromptControlError {
    let mut error = PromptControlError::default_legacy();
    error.code = code.to_string();
    error.message = message.to_string();
    error.request_id = request_id;
    error.operation = Some(operation);
    error.preview = Some(preview);
    error.status = Some(status);
    if status == PromptControlResultStatus::Blocked {
        error.value_visibility = Some(PromptControlValueVisibility::Hidden);
        error.next_step = Some("reauthenticate_and_retry".to_string());
    }
    error.agent_id = agent_id;
    error.player_id = player_id;
    error.reason_code = Some(code.to_string());
    error
}

fn prompt_control_result_cache_full_error(
    request_id: Option<String>,
    operation: PromptControlOperation,
    preview: bool,
) -> PromptControlError {
    let mut error = prompt_control_enhanced_error(
        "result_cache_full",
        "prompt control result cache is full",
        request_id,
        operation,
        preview,
        None,
        None,
        PromptControlResultStatus::Blocked,
    );
    error.next_step = Some("refresh_authority_and_retry_with_new_request_id".to_string());
    error
}

fn prompt_control_control_lost_error(
    request_id: &str,
    operation: PromptControlOperation,
    preview: bool,
    agent_id: Option<String>,
) -> PromptControlError {
    let mut error = prompt_control_enhanced_error(
        "control_lost",
        "prompt control authorization is no longer current",
        Some(request_id.to_string()),
        operation,
        preview,
        agent_id,
        None,
        PromptControlResultStatus::Blocked,
    );
    error.next_step = Some("reauthenticate_and_refresh_binding".to_string());
    error
}

fn prompt_control_access_error(
    error: PromptControlError,
    request_id: &str,
    operation: PromptControlOperation,
    preview: bool,
    agent_id: &str,
    player_id: &str,
) -> PromptControlError {
    if error.code == "agent_not_found" {
        return prompt_control_enhanced_error(
            "agent_not_found",
            "prompt control target Agent was not found",
            Some(request_id.to_string()),
            operation,
            preview,
            Some(agent_id.to_string()),
            Some(player_id.to_string()),
            PromptControlResultStatus::Rejected,
        );
    }
    prompt_control_control_lost_error(request_id, operation, preview, Some(agent_id.to_string()))
}

fn prompt_control_result_unknown_error(request_id: &str) -> PromptControlError {
    let mut error = PromptControlError::default_legacy();
    error.code = "prompt_control_result_unknown".to_string();
    error.message = "prompt control result is unknown for the current authority epoch".to_string();
    error.request_id = Some(request_id.to_string());
    error.status = Some(PromptControlResultStatus::Blocked);
    error.value_visibility = Some(PromptControlValueVisibility::Hidden);
    error.reason_code = Some("result_unknown".to_string());
    error.next_step = Some("refresh_authority_and_retry_with_new_request_id".to_string());
    error
}

fn prompt_control_ledger_error(
    error: PromptControlLedgerInsertError,
    request_id: &str,
) -> PromptControlError {
    let (code, message, status) = match error {
        PromptControlLedgerInsertError::Full => (
            "result_cache_full".to_string(),
            "prompt control result cache is full".to_string(),
            PromptControlResultStatus::Blocked,
        ),
        PromptControlLedgerInsertError::ReceiptTooLarge { actual, limit } => (
            "result_receipt_too_large".to_string(),
            format!("prompt control result receipt is {actual} bytes; limit is {limit}"),
            PromptControlResultStatus::Rejected,
        ),
        PromptControlLedgerInsertError::Serialize(message) => (
            "result_receipt_encode_failed".to_string(),
            message,
            PromptControlResultStatus::Blocked,
        ),
    };
    let mut result = PromptControlError::default_legacy();
    result.code = code.clone();
    result.message = message;
    result.request_id = Some(request_id.to_string());
    result.status = Some(status);
    if status == PromptControlResultStatus::Blocked {
        result.value_visibility = Some(PromptControlValueVisibility::Hidden);
    }
    result.reason_code = Some(code);
    result
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
pub(super) fn ensure_updated_by_matches_player_runtime(
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
pub(super) fn ensure_agent_player_access_runtime(
    world: &RuntimeWorld,
    sidecar: &RuntimeLlmSidecar,
    agent_id: &str,
    player_id: &str,
    public_key: Option<&str>,
) -> Result<(), PromptControlError> {
    ensure_agent_player_access_runtime_inner(world, sidecar, agent_id, player_id, public_key, false)
}

#[expect(
    clippy::result_large_err,
    reason = "Prompt-control protocol errors preserve the stable typed error envelope"
)]
pub(super) fn ensure_agent_player_binding_target_runtime(
    world: &RuntimeWorld,
    sidecar: &RuntimeLlmSidecar,
    agent_id: &str,
    player_id: &str,
    public_key: Option<&str>,
) -> Result<(), PromptControlError> {
    ensure_agent_player_access_runtime_inner(world, sidecar, agent_id, player_id, public_key, true)
}

#[expect(
    clippy::result_large_err,
    reason = "Prompt-control protocol errors preserve the stable typed error envelope"
)]
fn ensure_agent_player_access_runtime_inner(
    world: &RuntimeWorld,
    sidecar: &RuntimeLlmSidecar,
    agent_id: &str,
    player_id: &str,
    public_key: Option<&str>,
    allow_missing_binding: bool,
) -> Result<(), PromptControlError> {
    if !world.state().agents.contains_key(agent_id) {
        return Err(PromptControlError {
            code: "agent_not_found".to_string(),
            message: format!("agent not found: {agent_id}"),
            agent_id: Some(agent_id.to_string()),
            current_version: None,
            ..PromptControlError::default_legacy()
        });
    }
    let Some(bound_player_id) = sidecar.agent_player_bindings.get(agent_id) else {
        if allow_missing_binding {
            return Ok(());
        }
        return Err(PromptControlError {
            code: "agent_player_binding_required".to_string(),
            message: format!("agent {agent_id} has no player binding"),
            agent_id: Some(agent_id.to_string()),
            current_version: sidecar
                .prompt_profiles
                .get(agent_id)
                .map(|entry| entry.version),
            ..PromptControlError::default_legacy()
        });
    };
    if bound_player_id == player_id {
        let Some(bound_public_key) = sidecar.agent_public_key_bindings.get(agent_id) else {
            return Ok(());
        };
        let requested_public_key = normalize_optional_public_key(public_key);
        if requested_public_key.as_deref() == Some(bound_public_key.as_str()) {
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
            current_version: sidecar
                .prompt_profiles
                .get(agent_id)
                .map(|entry| entry.version),
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
        current_version: sidecar
            .prompt_profiles
            .get(agent_id)
            .map(|entry| entry.version),
        ..PromptControlError::default_legacy()
    })
}

pub(super) fn apply_prompt_patch_runtime(
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

fn sanitize_patch_string(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
}

pub(super) fn changed_prompt_fields_runtime(
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

pub(super) fn prompt_profile_digest_runtime(profile: &AgentPromptProfile) -> String {
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
pub(super) fn ensure_expected_prompt_version_runtime(
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

#[cfg(test)]
mod tests {
    use super::*;

    fn current_intent(status: &str) -> AgentIntentV2 {
        AgentIntentV2 {
            schema_version: crate::runtime::AGENT_INTENT_V2_SCHEMA_VERSION,
            agent_id: "agent-1".to_string(),
            intent_id: "intent-1".to_string(),
            kind: "agent_chat".to_string(),
            summary: "safe lifecycle summary".to_string(),
            target_id: None,
            effect_intent_id: None,
            intent_tick: None,
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            status: status.to_string(),
            source: "player".to_string(),
            logical_time: 1,
            event_seq: 1,
            updated_at: 1,
            receipt_ref: None,
            reason_code: None,
            reason_summary: None,
            replaced_by: None,
            actor_id: "player-1".to_string(),
            request_digest: "digest-1".to_string(),
        }
    }

    #[test]
    fn async_provider_reply_requires_exact_current_identity_and_live_status() {
        let accepted = current_intent("accepted");
        assert!(provider_reply_matches_current_intent(
            Some(&accepted),
            Some("intent-1"),
            Some("digest-1")
        ));

        let blocked = current_intent("blocked");
        assert!(provider_reply_matches_current_intent(
            Some(&blocked),
            Some("intent-1"),
            Some("digest-1")
        ));

        for status in [
            "rejected",
            "expired",
            "cancelled",
            "completed",
            "superseded",
        ] {
            let terminal = current_intent(status);
            assert!(!provider_reply_matches_current_intent(
                Some(&terminal),
                Some("intent-1"),
                Some("digest-1")
            ));
        }
        assert!(!provider_reply_matches_current_intent(
            Some(&accepted),
            Some("other-intent"),
            Some("digest-1")
        ));
        assert!(!provider_reply_matches_current_intent(
            Some(&accepted),
            Some("intent-1"),
            Some("other-digest")
        ));
        assert!(!provider_reply_matches_current_intent(
            None,
            Some("intent-1"),
            Some("digest-1")
        ));
    }
}
