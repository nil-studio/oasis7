use std::collections::BTreeMap;

use oasis7::simulator::{
    ActionCatalogEntry, ActionResult, AgentDecision, AgentDecisionTrace, ProviderExecutionMode,
};
use oasis7_simulator::{Action, Observation};

use super::{DEFAULT_MAX_MOVE_DISTANCE_CM_PER_TICK, RecoveryLineage};

pub(super) fn apply_builtin_parity_guardrail(
    scenario_id: &str,
    agent_id: &str,
    observation: &Observation,
    decision: AgentDecision,
) -> (AgentDecision, Option<String>) {
    if scenario_id != "P0-001" {
        return (decision, None);
    }
    let Some(preferred_location) = preferred_patrol_move_target(observation) else {
        return (decision, None);
    };
    if decision_is_valid_patrol_move(&decision, observation) {
        return (decision, None);
    }
    (
        AgentDecision::Act(Action::MoveAgent {
            agent_id: agent_id.to_string(),
            to: preferred_location.clone(),
        }),
        Some(format!(
            "builtin_parity_guardrail: reroute {} -> move_agent({})",
            decision_label(&decision),
            preferred_location,
        )),
    )
}

fn decision_is_valid_patrol_move(decision: &AgentDecision, observation: &Observation) -> bool {
    let current_location_id = estimated_current_location_id(observation);
    matches!(
        decision,
        AgentDecision::Act(Action::MoveAgent { to, .. })
            if observation.visible_locations.iter().any(|location| {
                location.location_id == *to
                    && location.distance_cm > 0
                    && location.distance_cm <= DEFAULT_MAX_MOVE_DISTANCE_CM_PER_TICK
                    && Some(location.location_id.as_str()) != current_location_id
            })
    )
}

fn preferred_patrol_move_target(observation: &Observation) -> Option<String> {
    let current_location_id = estimated_current_location_id(observation);
    observation
        .visible_locations
        .iter()
        .filter(|location| {
            location.distance_cm > 0
                && location.distance_cm <= DEFAULT_MAX_MOVE_DISTANCE_CM_PER_TICK
                && Some(location.location_id.as_str()) != current_location_id
        })
        .min_by_key(|location| location.distance_cm)
        .map(|location| location.location_id.clone())
}

fn estimated_current_location_id(observation: &Observation) -> Option<&str> {
    observation
        .visible_locations
        .iter()
        .min_by_key(|location| location.distance_cm)
        .map(|location| location.location_id.as_str())
}

pub(super) fn execution_environment_class(mode: ProviderExecutionMode) -> &'static str {
    match mode {
        ProviderExecutionMode::PlayerParity => "player_parity_linux",
        ProviderExecutionMode::HeadlessAgent => "headless_linux",
    }
}

pub(super) fn builtin_parity_short_term_goal(scenario_id: &str) -> Option<String> {
    parity_memory_summary(scenario_id).map(str::to_string)
}

pub(super) fn parity_memory_summary(scenario_id: &str) -> Option<&'static str> {
    match scenario_id {
        "P0-001" => Some(
            "goal=巡游移动; prefer move_agent to the nearest visible non-current location; do not idle when a legal move is available",
        ),
        "P0-002" => Some(
            "goal=近邻观察; prefer inspect_target on a visible agent or location before waiting",
        ),
        "P0-003" => Some(
            "goal=简单对话; prefer speak_to_nearby with a short nearby message instead of idle waiting",
        ),
        "P0-004" => Some(
            "goal=简单交互; prefer one legal simple_interact on a visible target before waiting",
        ),
        "P0-005" => Some(
            "goal=拒绝路径恢复; after one recoverable failure, prefer a legal recovery action or short wait_ticks with an explicit recovery attempt",
        ),
        _ => None,
    }
}

pub(super) fn phase1_action_catalog() -> Vec<ActionCatalogEntry> {
    vec![
        ActionCatalogEntry::new("wait", "yield current turn without acting"),
        ActionCatalogEntry::new("wait_ticks", "sleep for a bounded number of ticks"),
        ActionCatalogEntry::new("move_agent", "move to a neighboring location"),
        ActionCatalogEntry::new("speak_to_nearby", "emit a lightweight nearby speech event"),
        ActionCatalogEntry::new(
            "inspect_target",
            "emit a lightweight target inspection event",
        ),
        ActionCatalogEntry::new(
            "simple_interact",
            "emit a lightweight single-step interaction event",
        ),
    ]
}

pub(super) fn action_ref_from_decision(decision: &AgentDecision) -> Option<String> {
    match decision {
        AgentDecision::Wait => Some("wait".to_string()),
        AgentDecision::WaitTicks(_) => Some("wait_ticks".to_string()),
        AgentDecision::Act(action) => Some(action_ref_from_action(action).to_string()),
        AgentDecision::Query(_) => Some("evaluate_micro_depot_quote".to_string()),
        AgentDecision::ModuleCommand { .. } => Some("module_command".to_string()),
    }
}

pub(super) fn action_ref_from_action(action: &Action) -> &'static str {
    match action {
        Action::MoveAgent { .. } => "move_agent",
        Action::SpeakToNearby { .. } => "speak_to_nearby",
        Action::InspectTarget { .. } => "inspect_target",
        Action::SimpleInteract { .. } => "simple_interact",
        _ => "other",
    }
}

pub(super) fn decision_label(decision: &AgentDecision) -> String {
    match decision {
        AgentDecision::Wait => "wait".to_string(),
        AgentDecision::WaitTicks(ticks) => format!("wait_ticks:{ticks}"),
        AgentDecision::Act(action) => format!("act:{}", action_ref_from_action(action)),
        AgentDecision::Query(_) => "query:evaluate_micro_depot_quote".to_string(),
        AgentDecision::ModuleCommand { .. } => "module_command".to_string(),
    }
}

pub(super) fn unavailable_recovery_lineage(agent_id: &str) -> RecoveryLineage {
    RecoveryLineage {
        agent_id: agent_id.to_string(),
        agent_session_id: String::new(),
        recovery_chain_id: String::new(),
        agent_turn_id: String::new(),
        decision_request_id: String::new(),
        request_digest: String::new(),
    }
}

pub(super) fn classify_trace_error(
    trace: Option<&AgentDecisionTrace>,
    action_result: Option<&ActionResult>,
) -> Option<String> {
    if let Some(result) = action_result
        && !result.success
    {
        return Some("action_rejected".to_string());
    }
    let err =
        trace.and_then(|value| value.llm_error.as_deref().or(value.parse_error.as_deref()))?;
    let lowered = err.to_ascii_lowercase();
    if lowered.contains("timeout") {
        Some("timeout".to_string())
    } else if lowered.contains("provider_unreachable") || lowered.contains("unreachable") {
        Some("provider_unreachable".to_string())
    } else if lowered.contains("invalid_action_schema") || lowered.contains("schema") {
        Some("invalid_action_schema".to_string())
    } else if lowered.contains("session_cross_talk") || lowered.contains("cross talk") {
        Some("session_cross_talk".to_string())
    } else if lowered.contains("context_drift") || lowered.contains("drift") {
        Some("context_drift".to_string())
    } else {
        None
    }
}

pub(super) fn derive_status(
    goal_completed: bool,
    error_counts: &BTreeMap<String, u64>,
    notes: &[String],
) -> String {
    if error_counts.contains_key("session_cross_talk") {
        return "blocked".to_string();
    }
    if notes.iter().any(|note| note.contains("invalid_fixture")) {
        return "invalid_fixture".to_string();
    }
    if goal_completed {
        "passed".to_string()
    } else {
        "failed".to_string()
    }
}

pub(super) fn ratio_ppm(numerator: u64, denominator: u64) -> u64 {
    if denominator == 0 {
        0
    } else {
        numerator
            .saturating_mul(1_000_000)
            .saturating_div(denominator)
    }
}

pub(super) fn percentile_u64(values: &[u64], percentile: f64) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let rank = (((sorted.len() - 1) as f64) * percentile / 100.0).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}
