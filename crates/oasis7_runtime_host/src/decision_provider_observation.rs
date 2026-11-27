use super::{
    ActionCatalogEntry, Observation, ProviderExecutionMode, ProviderInteractionTarget,
    ProviderMissionContext, ProviderNavigationNode, ProviderNearbyEntity, ProviderObservation,
    ProviderRecentEvent, ProviderSelfState,
};

pub(super) fn provider_observation_from_runtime_observation(
    mode: ProviderExecutionMode,
    observation: &Observation,
    memory_summary: Option<&str>,
    recent_event_summary: &[String],
    action_catalog: &[ActionCatalogEntry],
) -> ProviderObservation {
    provider_observation_from_runtime_observation_with_goal(
        mode,
        observation,
        memory_summary,
        recent_event_summary,
        action_catalog,
        None,
    )
}

pub(super) fn provider_observation_from_runtime_observation_with_goal(
    mode: ProviderExecutionMode,
    observation: &Observation,
    _memory_summary: Option<&str>,
    recent_event_summary: &[String],
    action_catalog: &[ActionCatalogEntry],
    goal_summary: Option<&str>,
) -> ProviderObservation {
    let mut sorted_visible_locations = observation.visible_locations.clone();
    sorted_visible_locations.sort_by(|left, right| {
        left.distance_cm
            .cmp(&right.distance_cm)
            .then_with(|| left.location_id.cmp(&right.location_id))
    });
    let mut sorted_visible_agents = observation.visible_agents.clone();
    sorted_visible_agents.sort_by(|left, right| {
        left.distance_cm
            .cmp(&right.distance_cm)
            .then_with(|| left.agent_id.cmp(&right.agent_id))
    });
    let current_location_ref = current_location_ref(observation)
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

    let mut nearby_entities = sorted_visible_locations
        .iter()
        .enumerate()
        .map(|(index, location)| {
            let relation = if location.distance_cm == 0 {
                "current_location"
            } else {
                "reachable_location"
            };
            let relative_hint = match mode {
                ProviderExecutionMode::PlayerParity => {
                    if location.distance_cm == 0 {
                        "current visible location".to_string()
                    } else if index == 1 {
                        "nearest visible reachable location".to_string()
                    } else {
                        "visible reachable location".to_string()
                    }
                }
                ProviderExecutionMode::HeadlessAgent => {
                    format!(
                        "reachable location distance_cm={}",
                        location.distance_cm.max(0)
                    )
                }
            };
            ProviderNearbyEntity {
                entity_ref: location.location_id.clone(),
                kind: "location".to_string(),
                relation: relation.to_string(),
                relative_hint,
                interaction_hint: if location.distance_cm > 0 && move_available {
                    Some("move_agent".to_string())
                } else {
                    None
                },
            }
        })
        .collect::<Vec<_>>();
    nearby_entities.extend(
        sorted_visible_agents
            .iter()
            .map(|agent| ProviderNearbyEntity {
                entity_ref: agent.agent_id.clone(),
                kind: "agent".to_string(),
                relation: "nearby_agent".to_string(),
                relative_hint: match mode {
                    ProviderExecutionMode::PlayerParity => "nearby visible agent".to_string(),
                    ProviderExecutionMode::HeadlessAgent => {
                        format!("nearby agent distance_cm={}", agent.distance_cm.max(0))
                    }
                },
                interaction_hint: if speak_available {
                    Some("speak_to_nearby".to_string())
                } else if inspect_available {
                    Some("inspect_target".to_string())
                } else {
                    None
                },
            }),
    );

    let recent_events = recent_event_summary
        .iter()
        .rev()
        .enumerate()
        .map(|(index, summary)| ProviderRecentEvent {
            event_ref: format!("recent_event_{index}"),
            kind: "event_summary".to_string(),
            summary: summary.clone(),
            age_ticks: index as u64,
        })
        .collect::<Vec<_>>();

    let local_navigation_graph = if matches!(mode, ProviderExecutionMode::HeadlessAgent) {
        sorted_visible_locations
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

    let interaction_targets =
        if matches!(mode, ProviderExecutionMode::HeadlessAgent) {
            let mut targets = Vec::new();
            if move_available {
                targets.extend(
                    sorted_visible_locations
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
                targets.extend(sorted_visible_agents.iter().map(|agent| {
                    ProviderInteractionTarget {
                        target_ref: agent.agent_id.clone(),
                        target_kind: "agent".to_string(),
                        interaction_hint: "inspect_target".to_string(),
                    }
                }));
            }
            targets
        } else {
            Vec::new()
        };

    ProviderObservation {
        self_state: ProviderSelfState {
            location_ref: current_location_ref.clone(),
            pose_hint: match mode {
                ProviderExecutionMode::PlayerParity => {
                    format!("player_visible_pose@{current_location_ref}")
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
            goal_summary: goal_summary
                .map(str::to_string)
                .unwrap_or_else(|| match mode {
                    ProviderExecutionMode::PlayerParity => {
                        "preserve player-visible forward progress".to_string()
                    }
                    ProviderExecutionMode::HeadlessAgent => {
                        "preserve deterministic local progress with structured hints".to_string()
                    }
                }),
            blocked_reason: None,
        },
        nearby_entities,
        recent_events,
        local_navigation_graph,
        hazard_summary: Vec::new(),
        interaction_targets,
    }
}

fn current_location_ref(observation: &Observation) -> Option<String> {
    observation
        .visible_locations
        .iter()
        .find(|location| location.distance_cm == 0)
        .or_else(|| {
            observation
                .visible_locations
                .iter()
                .min_by_key(|location| location.distance_cm)
        })
        .map(|location| location.location_id.clone())
}
