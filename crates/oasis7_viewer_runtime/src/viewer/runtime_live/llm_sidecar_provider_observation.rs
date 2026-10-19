//! Stable provider-facing observation normalization and bounded Runtime event summaries.
use super::*;

pub(super) fn provider_observation_from_runtime_observation(
    observation: &Observation,
    mode: ProviderExecutionMode,
    action_catalog: &[ActionCatalogEntry],
    recent_event_summary: &[String],
) -> ProviderObservation {
    let mut locations = observation.visible_locations.clone();
    locations.sort_by(|left, right| {
        left.distance_cm
            .cmp(&right.distance_cm)
            .then_with(|| left.location_id.cmp(&right.location_id))
    });
    let mut agents = observation.visible_agents.clone();
    agents.sort_by(|left, right| {
        left.distance_cm
            .cmp(&right.distance_cm)
            .then_with(|| left.agent_id.cmp(&right.agent_id))
    });
    let current_location = locations
        .iter()
        .find(|location| location.distance_cm == 0)
        .or_else(|| locations.first())
        .map(|location| location.location_id.clone())
        .unwrap_or_else(|| format!("agent:{}:position", observation.agent_id));
    let move_available = action_catalog
        .iter()
        .any(|entry| entry.action_ref == "move_agent");
    let inspect_available = action_catalog
        .iter()
        .any(|entry| entry.action_ref == "inspect_target");
    let speak_available = action_catalog
        .iter()
        .any(|entry| entry.action_ref == "speak_to_nearby");
    let mut nearby_entities = locations
        .iter()
        .map(|location| ProviderNearbyEntity {
            entity_ref: location.location_id.clone(),
            kind: "location".to_string(),
            relation: if location.distance_cm == 0 {
                "current_location".to_string()
            } else {
                "reachable_location".to_string()
            },
            relative_hint: format!(
                "distance_cm={} visible_name={}",
                location.distance_cm.max(0),
                location.name
            ),
            interaction_hint: (location.distance_cm > 0 && move_available)
                .then(|| "move_agent".to_string()),
        })
        .collect::<Vec<_>>();
    nearby_entities.extend(agents.iter().map(|agent| ProviderNearbyEntity {
        entity_ref: agent.agent_id.clone(),
        kind: "agent".to_string(),
        relation: "nearby_agent".to_string(),
        relative_hint: format!("distance_cm={}", agent.distance_cm.max(0)),
        interaction_hint: if speak_available {
            Some("speak_to_nearby".to_string())
        } else if inspect_available {
            Some("inspect_target".to_string())
        } else {
            None
        },
    }));
    let local_navigation_graph = if matches!(mode, ProviderExecutionMode::HeadlessAgent) {
        locations
            .iter()
            .map(|location| ProviderNavigationNode {
                node_ref: location.location_id.clone(),
                relation: if location.distance_cm == 0 {
                    "current_location".to_string()
                } else {
                    "reachable_location".to_string()
                },
                relative_hint: format!(
                    "distance_cm={} visible_name={}",
                    location.distance_cm.max(0),
                    location.name
                ),
                traversable: location.distance_cm >= 0,
            })
            .collect()
    } else {
        Vec::new()
    };
    let interaction_targets = if matches!(mode, ProviderExecutionMode::HeadlessAgent) {
        let mut targets = Vec::new();
        if move_available {
            targets.extend(
                locations
                    .iter()
                    .filter(|location| location.distance_cm > 0)
                    .map(|location| ProviderInteractionTarget {
                        target_ref: location.location_id.clone(),
                        target_kind: "location".to_string(),
                        interaction_hint: "move_agent".to_string(),
                    }),
            );
        }
        if inspect_available {
            targets.extend(agents.iter().map(|agent| ProviderInteractionTarget {
                target_ref: agent.agent_id.clone(),
                target_kind: "agent".to_string(),
                interaction_hint: "inspect_target".to_string(),
            }));
        }
        targets
    } else {
        Vec::new()
    };
    ProviderObservation {
        self_state: ProviderSelfState {
            location_ref: current_location.clone(),
            pose_hint: match mode {
                ProviderExecutionMode::PlayerParity => {
                    format!("player_visible_pose@{current_location}")
                }
                ProviderExecutionMode::HeadlessAgent => format!(
                    "grid_pose=({}, {}, {}) visibility_range_cm={}",
                    observation.pos.x_cm,
                    observation.pos.y_cm,
                    observation.pos.z_cm,
                    observation.visibility_range_cm
                ),
            },
            status_flags: Vec::new(),
            resource_summary: observation
                .self_resources
                .amounts
                .iter()
                .map(|(kind, amount)| (format!("{kind:?}"), *amount))
                .collect(),
        },
        mission_context: ProviderMissionContext {
            goal_summary: match mode {
                ProviderExecutionMode::PlayerParity => {
                    "preserve player-visible forward progress".to_string()
                }
                ProviderExecutionMode::HeadlessAgent => {
                    "preserve deterministic local progress with structured hints".to_string()
                }
            },
            blocked_reason: None,
        },
        nearby_entities,
        recent_events: recent_event_summary
            .iter()
            .rev()
            .enumerate()
            .map(|(index, summary)| ProviderRecentEvent {
                event_ref: format!("recent_event_{index}"),
                kind: "runtime_event_summary".to_string(),
                summary: summary.clone(),
                age_ticks: index as u64,
            })
            .collect(),
        local_navigation_graph,
        hazard_summary: Vec::new(),
        interaction_targets,
    }
}

pub(super) fn recent_runtime_event_summaries(world: &RuntimeWorld) -> Vec<String> {
    let mut recent = world
        .journal()
        .events
        .iter()
        .rev()
        .take(8)
        .collect::<Vec<_>>();
    recent.reverse();
    recent
        .into_iter()
        .map(|event| {
            let body = serde_json::to_string(&event.body)
                .unwrap_or_else(|_| "<runtime event body unavailable>".to_string());
            format!(
                "runtime_event_id={} time={} body={body}",
                event.id, event.time
            )
        })
        .collect()
}
