use crate::runtime::{FactoryProductionStatus, FactoryState, WorldState};
use crate::runtime_host::archive::{
    PlayerGameplayAction, PlayerGameplayCausalityKind, PlayerGameplayExecutionState,
    PlayerGameplaySnapshot, PlayerGameplayStageStatus,
};
use crate::viewer::{ACTION_CLAIM_FIRST_AGENT, ACTION_CLAIM_STARTER_OC, FACTORY_SMELTER_MK1};

pub(super) fn apply_runtime_snapshot_empty_entities_blocker(
    gameplay: &mut PlayerGameplaySnapshot,
    missing_agents: bool,
    missing_locations: bool,
) {
    if !missing_agents && !missing_locations {
        return;
    }
    let mut missing_parts = Vec::new();
    if missing_agents {
        missing_parts.push("agents");
    }
    if missing_locations {
        missing_parts.push("locations");
    }
    let missing_summary = missing_parts.join("/");
    let disabled_reason = format!(
        "runtime snapshot is missing {missing_summary}; refresh snapshot or repair runtime bootstrap first"
    );
    gameplay.stage_status = PlayerGameplayStageStatus::Blocked;
    gameplay.execution_state = PlayerGameplayExecutionState::Blocked;
    gameplay.blocker_kind = Some("runtime_snapshot_empty_entities".to_string());
    gameplay.blocker_detail = Some(format!(
        "runtime exposed an empty new-user world with no {missing_summary}; claim the first Agent to start the onboarding flow"
    ));
    gameplay.next_step_hint =
        "Use claim_first_agent if it is available; otherwise request a fresh snapshot and repair runtime bootstrap only if the claim action is missing."
            .to_string();
    gameplay.causality_kind = Some(PlayerGameplayCausalityKind::WorldConstraint);
    gameplay.causality_detail = gameplay.blocker_detail.clone();
    gameplay.status_reason = gameplay.blocker_detail.clone();
    gameplay.primary_blocker = gameplay.blocker_detail.clone();
    gameplay.resume_next_step = Some(gameplay.next_step_hint.clone());
    for action in &mut gameplay.available_actions {
        if action.protocol_action == "request_snapshot"
            || action.action_id == ACTION_CLAIM_FIRST_AGENT
            || action.action_id == ACTION_CLAIM_STARTER_OC
        {
            continue;
        }
        action.disabled_reason = Some(disabled_reason.clone());
    }
}

pub(super) fn base_available_actions(
    first_agent_id: Option<&str>,
    gameplay_enabled: bool,
    gameplay_disabled_reason: Option<&str>,
    supports_agent_chat: bool,
) -> Vec<PlayerGameplayAction> {
    let disabled_reason = (!gameplay_enabled).then(|| {
        gameplay_disabled_reason
            .unwrap_or("gameplay requires runtime live server running with --llm")
            .to_string()
    });
    let mut actions = vec![
        PlayerGameplayAction {
            action_id: "request_snapshot".to_string(),
            label: "Refresh gameplay snapshot".to_string(),
            protocol_action: "request_snapshot".to_string(),
            target_agent_id: None,
            disabled_reason: None,
        },
        PlayerGameplayAction {
            action_id: "advance_step".to_string(),
            label: "Advance 1 step".to_string(),
            protocol_action: "live_control.step".to_string(),
            target_agent_id: None,
            disabled_reason: disabled_reason.clone(),
        },
        PlayerGameplayAction {
            action_id: "resume_play".to_string(),
            label: "Resume live play".to_string(),
            protocol_action: "live_control.play".to_string(),
            target_agent_id: None,
            disabled_reason,
        },
    ];
    if supports_agent_chat {
        if let Some(agent_id) = first_agent_id {
            actions.push(PlayerGameplayAction {
                action_id: "chat_first_agent".to_string(),
                label: "Send one chat/command to the first available agent".to_string(),
                protocol_action: "agent_chat".to_string(),
                target_agent_id: Some(agent_id.to_string()),
                disabled_reason: None,
            });
            actions.push(PlayerGameplayAction {
                action_id: "reprioritize".to_string(),
                label: "Replace this Agent's short-term goal".to_string(),
                protocol_action: "prompt_control.apply".to_string(),
                target_agent_id: Some(agent_id.to_string()),
                disabled_reason: None,
            });
        }
    } else if let Some(agent_id) = first_agent_id {
        actions.push(PlayerGameplayAction {
            action_id: "reprioritize".to_string(),
            label: "Replace this Agent's short-term goal".to_string(),
            protocol_action: "prompt_control.apply".to_string(),
            target_agent_id: Some(agent_id.to_string()),
            disabled_reason: Some(
                "reprioritize requires an LLM-capable Agent with prompt-control support"
                    .to_string(),
            ),
        });
    }
    actions
}

pub(super) fn blocker_next_step(kind: &str, detail: &str) -> String {
    let haystack = format!("{kind} {detail}");
    if haystack.contains("power") || haystack.contains("energy") {
        return "Restore energy first, then advance again to verify recovery.".to_string();
    }
    if haystack.contains("material") || haystack.contains("iron") || haystack.contains("input") {
        return "Replenish upstream materials, then advance again to confirm the line resumes."
            .to_string();
    }
    if haystack.contains("logistics") || haystack.contains("transit") {
        return "Repair the transport path or re-route the ledger flow before stepping again."
            .to_string();
    }
    "Inspect the blocker details, recover the line, then advance again to confirm progress."
        .to_string()
}

pub(super) fn primary_factory_for_player_gameplay(state: &WorldState) -> Option<&FactoryState> {
    state
        .factories
        .values()
        .max_by_key(|factory| primary_factory_priority(factory))
}

fn primary_factory_priority(factory: &FactoryState) -> (bool, bool, bool, bool, u64, u64) {
    let production = &factory.production;
    (
        production.completed_jobs > 0 || production.last_completed_at.is_some(),
        production.status != FactoryProductionStatus::Blocked,
        production.status == FactoryProductionStatus::Running,
        factory.factory_id == FACTORY_SMELTER_MK1,
        production.completed_jobs,
        production.last_completed_at.unwrap_or(0),
    )
}

/// A player loop advances from owner-bound Runtime evidence, never a shared
/// clock/event counter. These facts are reconstructed from the current world;
/// no Viewer-local completion bit survives or substitutes for Runtime state.
pub(super) fn has_player_world_effect(
    state: &WorldState,
    controlled_agent_id: Option<&str>,
    primary_intent: Option<&crate::runtime_host::archive::PlayerGameplayPrimaryIntent>,
) -> bool {
    let Some(agent_id) = controlled_agent_id else {
        return false;
    };
    super::starter_industrial_outcome::settled_outcome(state, Some(agent_id)).is_some()
        || primary_intent.is_some_and(|intent| {
            intent.agent_id.as_deref() == Some(agent_id)
                && intent.status == "completed"
                && super::player_agency_projection::canonical_accepted_runtime_intent_id(
                    &intent.status,
                    intent.source_class.as_deref(),
                    intent.freshness.as_deref(),
                    intent.intent_id.as_deref(),
                )
                .is_some()
                && intent.receipt_ref.as_ref().is_some_and(|receipt| {
                    receipt.is_object()
                        && receipt.get("intent_id").and_then(serde_json::Value::as_str)
                            == intent.intent_id.as_deref()
                        && receipt
                            .get("receipt_id")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|id| !id.trim().is_empty())
                })
        })
}
