use super::agent_chat_intent::apply_short_term_goal_primary_intent;
use super::*;

impl ViewerRuntimeLiveServer {
    pub(super) fn current_prompt_version(&self, agent_id: &str) -> Option<u64> {
        self.llm_sidecar
            .prompt_profiles
            .get(agent_id)
            .map(|profile| profile.version)
    }

    pub(super) fn record_primary_intent_from_short_term_goal(
        &mut self,
        agent_id: &str,
        short_term_goal: Option<&str>,
    ) {
        let primary_intent = apply_short_term_goal_primary_intent(
            self.llm_sidecar.primary_intents.get(agent_id),
            short_term_goal,
        );
        self.llm_sidecar
            .primary_intents
            .insert(agent_id.to_string(), primary_intent);
    }

    #[expect(
        clippy::result_large_err,
        reason = "Prompt-control protocol errors preserve the stable typed error envelope"
    )]
    pub(super) fn current_prompt_profile(
        &self,
        agent_id: &str,
    ) -> Result<AgentPromptProfile, PromptControlError> {
        if !self.world.state().agents.contains_key(agent_id) {
            return Err(PromptControlError {
                code: "agent_not_found".to_string(),
                message: format!("agent not found: {agent_id}"),
                agent_id: Some(agent_id.to_string()),
                current_version: None,
                ..PromptControlError::default_legacy()
            });
        }
        Ok(self
            .llm_sidecar
            .prompt_profiles
            .get(agent_id)
            .cloned()
            .unwrap_or_else(|| AgentPromptProfile::for_agent(agent_id.to_string())))
    }

    pub(super) fn lookup_prompt_profile_version(
        &self,
        agent_id: &str,
        version: u64,
    ) -> Option<AgentPromptProfile> {
        self.llm_sidecar
            .prompt_profile_history
            .get(agent_id)
            .and_then(|versions| versions.get(&version).cloned())
            .or_else(|| {
                let profile = self.llm_sidecar.prompt_profiles.get(agent_id)?;
                (profile.version == version).then(|| profile.clone())
            })
    }
}
