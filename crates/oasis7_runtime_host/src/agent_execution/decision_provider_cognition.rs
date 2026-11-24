use super::super::continuous_agent_harness::ContinuousAgentTurnContextV1;
use super::{
    DecisionRequest, Observation, ObservationEnvelope, ProviderBackedAgentBehavior,
    provider_observation_from_runtime_observation_with_goal,
};

use super::DecisionProvider;

impl<P: DecisionProvider> ProviderBackedAgentBehavior<P> {
    pub(super) fn build_request(&self, observation: &Observation) -> DecisionRequest {
        let memory_summary = self.composed_memory_summary();
        let goal_summary =
            self.continuous_turn_context
                .as_ref()
                .map(|context: &ContinuousAgentTurnContextV1| {
                    context.goal_snapshot.short_term_summary.as_str()
                });
        DecisionRequest {
            observation: ObservationEnvelope {
                agent_id: self.agent_id.clone(),
                world_time: observation.time,
                mode: self.execution_mode,
                observation_schema_version: self.observation_schema_version.clone(),
                action_schema_version: self.action_schema_version.clone(),
                environment_class: self.environment_class.clone(),
                fallback_reason: self.fallback_reason.clone(),
                observation: provider_observation_from_runtime_observation_with_goal(
                    self.execution_mode,
                    observation,
                    memory_summary.as_deref(),
                    &self
                        .recent_event_summary
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>(),
                    &self.action_catalog,
                    goal_summary,
                ),
                recent_event_summary: self.recent_event_summary.iter().cloned().collect(),
                memory_summary: memory_summary.clone(),
                action_catalog: self.action_catalog.clone(),
                module_command_catalog: self.module_command_catalog.clone(),
                timeout_budget_ms: self.timeout_budget_ms,
            },
            provider_config_ref: self.provider_config_ref.clone(),
            agent_profile: self.agent_profile.clone(),
            fixture_id: self.fixture_id.clone(),
            replay_id: self.replay_id.clone(),
            capability_catalog: self
                .capability_context
                .as_ref()
                .map(|context| context.catalog.clone()),
            capability_invocation_context: self
                .capability_context
                .as_ref()
                .map(|context| context.invocation.clone()),
            timeout_budget_ms: self.timeout_budget_ms,
        }
    }
}
