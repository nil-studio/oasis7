use super::*;

pub(super) fn sample_request() -> DecisionRequest {
    DecisionRequest {
        observation: ObservationEnvelope {
            agent_id: "agent-1".to_string(),
            world_time: 7,
            mode: ProviderExecutionMode::HeadlessAgent,
            observation_schema_version: DEFAULT_PROVIDER_OBSERVATION_SCHEMA_VERSION.to_string(),
            action_schema_version: DEFAULT_PROVIDER_ACTION_SCHEMA_VERSION.to_string(),
            environment_class: Some("provider_local_bridge".to_string()),
            fallback_reason: None,
            observation: ProviderObservation {
                self_state: ProviderSelfState {
                    location_ref: "loc-1".to_string(),
                    pose_hint: "grid_pose=(0, 0, 0) visibility_range_cm=1000".to_string(),
                    status_flags: Vec::new(),
                    resource_summary: BTreeMap::from([(String::from("Electricity"), 24)]),
                },
                mission_context: ProviderMissionContext {
                    goal_summary: "prefer safe low-frequency actions".to_string(),
                    blocked_reason: None,
                },
                nearby_entities: vec![
                    ProviderNearbyEntity {
                        entity_ref: "loc-1".to_string(),
                        kind: "location".to_string(),
                        relation: "current_location".to_string(),
                        relative_hint: "current visible location".to_string(),
                        interaction_hint: None,
                    },
                    ProviderNearbyEntity {
                        entity_ref: "loc-2".to_string(),
                        kind: "location".to_string(),
                        relation: "reachable_location".to_string(),
                        relative_hint: "reachable location distance_cm=100".to_string(),
                        interaction_hint: Some("move_agent".to_string()),
                    },
                    ProviderNearbyEntity {
                        entity_ref: "agent-2".to_string(),
                        kind: "agent".to_string(),
                        relation: "nearby_agent".to_string(),
                        relative_hint: "nearby agent distance_cm=100".to_string(),
                        interaction_hint: Some("inspect_target".to_string()),
                    },
                ],
                recent_events: vec![ProviderRecentEvent {
                    event_ref: "recent_event_0".to_string(),
                    kind: "event_summary".to_string(),
                    summary: "event: AgentRegistered".to_string(),
                    age_ticks: 0,
                }],
                local_navigation_graph: vec![
                    ProviderNavigationNode {
                        node_ref: "loc-1".to_string(),
                        relation: "current_location".to_string(),
                        relative_hint: "distance_cm=0 visible_name=current".to_string(),
                        traversable: true,
                    },
                    ProviderNavigationNode {
                        node_ref: "loc-2".to_string(),
                        relation: "reachable_location".to_string(),
                        relative_hint: "distance_cm=100 visible_name=neighbor".to_string(),
                        traversable: true,
                    },
                ],
                hazard_summary: Vec::new(),
                interaction_targets: vec![ProviderInteractionTarget {
                    target_ref: "loc-2".to_string(),
                    target_kind: "location".to_string(),
                    interaction_hint: "move_agent".to_string(),
                }],
            },
            recent_event_summary: vec!["event: AgentRegistered".to_string()],
            memory_summary: Some("prefer safe low-frequency actions".to_string()),
            action_catalog: vec![
                ActionCatalogEntry::new("move_agent", "move to a visible location"),
                ActionCatalogEntry::new("wait", "do nothing this tick"),
            ],
            module_command_catalog: Vec::new(),
            timeout_budget_ms: 7000,
        },
        provider_config_ref: Some("provider://local-bridge".to_string()),
        agent_profile: Some(DEFAULT_PROVIDER_AGENT_PROFILE.to_string()),
        fixture_id: None,
        replay_id: None,
        capability_catalog: None,
        capability_invocation_context: None,
        timeout_budget_ms: 7000,
    }
}
