use super::*;

impl<C: LlmCompletionClient> LlmAgentBehavior<C> {
    pub(super) fn effective_system_prompt(&self) -> &str {
        self.prompt_overrides
            .system_prompt
            .as_deref()
            .unwrap_or(self.config.system_prompt.as_str())
    }

    pub(super) fn effective_short_term_goal(&self) -> &str {
        self.prompt_overrides
            .short_term_goal
            .as_deref()
            .unwrap_or(self.config.short_term_goal.as_str())
    }

    pub(super) fn effective_long_term_goal(&self) -> &str {
        self.prompt_overrides
            .long_term_goal
            .as_deref()
            .unwrap_or(self.config.long_term_goal.as_str())
    }

    #[cfg(test)]
    pub(super) fn system_prompt(&self) -> String {
        let prompt_budget = self.config.prompt_budget();
        let prompt: PromptAssemblyOutput = PromptAssembler::assemble(PromptAssemblyInput {
            agent_id: self.agent_id.as_str(),
            base_system_prompt: self.effective_system_prompt(),
            short_term_goal: self.effective_short_term_goal(),
            long_term_goal: self.effective_long_term_goal(),
            observation_json: "{}",
            module_history_json: "[]",
            conversation_history_json: "[]",
            memory_digest: None,
            step_context: PromptStepContext::default(),
            harvest_max_amount_cap: self.config.harvest_max_amount_cap,
            prompt_budget,
        });
        prompt.system_prompt
    }

    #[cfg(test)]
    pub(super) fn user_prompt(
        &self,
        observation: &Observation,
        module_history: &[ModuleCallExchange],
        step_index: usize,
        max_steps: usize,
    ) -> String {
        self.assemble_prompt_output(observation, module_history, step_index, max_steps)
            .user_prompt
    }

    pub(super) fn assemble_prompt_output(
        &self,
        observation: &Observation,
        module_history: &[ModuleCallExchange],
        step_index: usize,
        max_steps: usize,
    ) -> PromptAssemblyOutput {
        let observation_json = self.observation_json_for_prompt(observation);
        let history_start = module_history
            .len()
            .saturating_sub(self.config.prompt_max_history_items);
        let history_slice = &module_history[history_start..];
        let history_json = Self::module_history_json_for_prompt(history_slice);
        let memory_selector_config = self.config.memory_selector_config();
        let memory_selection =
            MemorySelector::select(&self.memory, observation.time, &memory_selector_config);
        let memory_digest = Self::memory_digest_for_prompt(memory_selection.digest.as_str());
        let conversation_json = self.conversation_history_json_for_prompt();
        let prompt_budget = self.config.prompt_budget();
        PromptAssembler::assemble(PromptAssemblyInput {
            agent_id: self.agent_id.as_str(),
            base_system_prompt: self.effective_system_prompt(),
            short_term_goal: self.effective_short_term_goal(),
            long_term_goal: self.effective_long_term_goal(),
            observation_json: observation_json.as_str(),
            module_history_json: history_json.as_str(),
            conversation_history_json: conversation_json.as_str(),
            memory_digest: Some(memory_digest.as_str()),
            step_context: PromptStepContext {
                step_index,
                max_steps,
                module_calls_used: module_history.len(),
                module_calls_max: self.config.max_module_calls,
            },
            harvest_max_amount_cap: self.config.harvest_max_amount_cap,
            prompt_budget,
        })
    }

    pub(super) fn observation_json_for_prompt(&self, observation: &Observation) -> String {
        let mut visible_agents = observation.visible_agents.iter().collect::<Vec<_>>();
        visible_agents.sort_by_key(|agent| agent.distance_cm);
        let visible_agents = visible_agents
            .into_iter()
            .take(PROMPT_OBSERVATION_VISIBLE_AGENTS_MAX)
            .map(|agent| {
                serde_json::json!({
                    "agent_id": agent.agent_id,
                    "distance_cm": agent.distance_cm,
                })
            })
            .collect::<Vec<_>>();

        let mut visible_locations = observation.visible_locations.iter().collect::<Vec<_>>();
        visible_locations.sort_by_key(|location| location.distance_cm);
        let visible_locations = visible_locations
            .into_iter()
            .take(PROMPT_OBSERVATION_VISIBLE_LOCATIONS_MAX)
            .map(|location| {
                serde_json::json!({
                    "location_id": location.location_id,
                    "distance_cm": location.distance_cm,
                })
            })
            .collect::<Vec<_>>();

        let last_action = self
            .last_action_summary
            .as_ref()
            .map(|summary| {
                serde_json::json!({
                    "kind": summary.kind,
                    "success": summary.success,
                    "reject_reason": summary.reject_reason,
                    "decision_rewrite": summary.decision_rewrite,
                })
            })
            .unwrap_or(serde_json::Value::Null);
        let recipe_coverage = self.recipe_coverage.summary_json();
        let mut prompt_observation = serde_json::json!({
            "time": observation.time,
            "agent_id": observation.agent_id,
            "pos": observation.pos,
            "self_resources": observation.self_resources,
            "last_action": last_action,
            "recipe_coverage": recipe_coverage,
            "visibility_range_cm": observation.visibility_range_cm,
            "visible_agents_total": observation.visible_agents.len(),
            "visible_agents_omitted": observation
                .visible_agents
                .len()
                .saturating_sub(visible_agents.len()),
            "visible_agents": visible_agents,
            "visible_locations_total": observation.visible_locations.len(),
            "visible_locations_omitted": observation
                .visible_locations
                .len()
                .saturating_sub(visible_locations.len()),
            "visible_locations": visible_locations,
        });
        if let Some(last_query) = self.last_query_result.as_ref() {
            prompt_observation["last_query"] = serde_json::json!(last_query);
        }

        serde_json::to_string(&prompt_observation)
            .unwrap_or_else(|_| "{\"error\":\"observation serialize failed\"}".to_string())
    }

    pub(super) fn action_kind_name_for_prompt(action: &Action) -> &'static str {
        match action {
            Action::RegisterLocation { .. } => "register_location",
            Action::RegisterAgent { .. } => "register_agent",
            Action::RegisterPowerPlant { .. } => "register_power_plant",
            Action::UpsertModuleVisualEntity { .. } => "upsert_module_visual_entity",
            Action::RemoveModuleVisualEntity { .. } => "remove_module_visual_entity",
            Action::MoveAgent { .. } => "move_agent",
            Action::SpeakToNearby { .. } => "speak_to_nearby",
            Action::InspectTarget { .. } => "inspect_target",
            Action::SimpleInteract { .. } => "simple_interact",
            Action::HarvestRadiation { .. } => "harvest_radiation",
            Action::BuyPower { .. } => "buy_power",
            Action::SellPower { .. } => "sell_power",
            Action::PlacePowerOrder { .. } => "place_power_order",
            Action::CancelPowerOrder { .. } => "cancel_power_order",
            Action::TransferResource { .. } => "transfer_resource",
            Action::DebugGrantResource { .. } => "debug_grant_resource",
            Action::MineCompound { .. } => "mine_compound",
            Action::RefineCompound { .. } => "refine_compound",
            Action::BuildFactory { .. } => "build_factory",
            Action::ScheduleRecipe { .. } => "schedule_recipe",
            Action::CompileModuleArtifactFromSource { .. } => "compile_module_artifact_from_source",
            Action::DeployModuleArtifact { .. } => "deploy_module_artifact",
            Action::InstallModuleFromArtifact { .. } => "install_module_from_artifact",
            Action::InstallModuleToTargetFromArtifact { .. } => {
                "install_module_to_target_from_artifact"
            }
            Action::InstallMicroDepot { .. } => "install_micro_depot",
            Action::ServiceMicroDepotRepair { .. } => "service_micro_depot_repair",
            Action::ServiceMicroDepotLogistics { .. } => "service_micro_depot_logistics",
            Action::PayMicroDepotUpkeep { .. } => "pay_micro_depot_upkeep",
            Action::SuspendMicroDepot { .. } => "suspend_micro_depot",
            Action::ReclaimMicroDepot { .. } => "reclaim_micro_depot",
            Action::ListModuleArtifactForSale { .. } => "list_module_artifact_for_sale",
            Action::BuyModuleArtifact { .. } => "buy_module_artifact",
            Action::DelistModuleArtifact { .. } => "delist_module_artifact",
            Action::DestroyModuleArtifact { .. } => "destroy_module_artifact",
            Action::PlaceModuleArtifactBid { .. } => "place_module_artifact_bid",
            Action::CancelModuleArtifactBid { .. } => "cancel_module_artifact_bid",
            Action::PublishSocialFact { .. } => "publish_social_fact",
            Action::ChallengeSocialFact { .. } => "challenge_social_fact",
            Action::AdjudicateSocialFact { .. } => "adjudicate_social_fact",
            Action::RevokeSocialFact { .. } => "revoke_social_fact",
            Action::DeclareSocialEdge { .. } => "declare_social_edge",
            Action::FormAlliance { .. } => "form_alliance",
            Action::JoinAlliance { .. } => "join_alliance",
            Action::LeaveAlliance { .. } => "leave_alliance",
            Action::DissolveAlliance { .. } => "dissolve_alliance",
            Action::DeclareWar { .. } => "declare_war",
            Action::OpenGovernanceProposal { .. } => "open_governance_proposal",
            Action::CastGovernanceVote { .. } => "cast_governance_vote",
            Action::ResolveCrisis { .. } => "resolve_crisis",
            Action::GrantMetaProgress { .. } => "grant_meta_progress",
            Action::OpenEconomicContract { .. } => "open_economic_contract",
            Action::AcceptEconomicContract { .. } => "accept_economic_contract",
            Action::SettleEconomicContract { .. } => "settle_economic_contract",
        }
    }

    pub(super) fn reject_reason_code_for_prompt(reason: &RejectReason) -> String {
        match reason {
            RejectReason::InsufficientResource { kind, .. } => {
                let kind = match kind {
                    ResourceKind::Electricity => "electricity",
                    ResourceKind::Data => "data",
                };
                format!("insufficient_resource.{kind}")
            }
            RejectReason::FacilityNotFound { .. } => "factory_not_found".to_string(),
            RejectReason::FacilityAlreadyExists { .. } => "facility_already_exists".to_string(),
            RejectReason::LocationNotFound { .. } => "location_not_found".to_string(),
            RejectReason::AgentAlreadyAtLocation { .. } => "agent_already_at_location".to_string(),
            RejectReason::AgentNotAtLocation { .. } => "agent_not_at_location".to_string(),
            RejectReason::RuleDenied { .. } => "rule_denied".to_string(),
            RejectReason::ThermalOverload { .. } => "thermal_overload".to_string(),
            RejectReason::RadiationUnavailable { .. } => "radiation_unavailable".to_string(),
            _ => "other".to_string(),
        }
    }

    pub(super) fn summarize_action_result_for_prompt(
        result: &ActionResult,
        decision_rewrite: Option<DecisionRewriteReceipt>,
    ) -> PromptLastActionSummary {
        PromptLastActionSummary {
            kind: Self::action_kind_name_for_prompt(&result.action).to_string(),
            success: result.success,
            reject_reason: result
                .reject_reason()
                .map(Self::reject_reason_code_for_prompt),
            decision_rewrite,
        }
    }

    pub(super) fn decision_label_for_rewrite(decision: &AgentDecision) -> String {
        match decision {
            AgentDecision::Act(action) => Self::action_label_for_rewrite(action),
            AgentDecision::Query(AgentQuery::EvaluateMicroDepotQuote(_)) => {
                "evaluate_micro_depot_quote".to_string()
            }
            AgentDecision::Query(AgentQuery::QuoteMicroDepotInstall(_)) => {
                "quote_micro_depot_install".to_string()
            }
            AgentDecision::Wait => "wait".to_string(),
            AgentDecision::WaitTicks(_) => "wait_ticks".to_string(),
            AgentDecision::ModuleCommand { .. } => "module_command".to_string(),
        }
    }

    pub(super) fn action_label_for_rewrite(action: &Action) -> String {
        Self::action_kind_name_for_prompt(action).to_string()
    }

    pub(super) fn decision_rewrite_receipt(
        from: &AgentDecision,
        to: &AgentDecision,
        reason: Option<&str>,
    ) -> Option<DecisionRewriteReceipt> {
        let from_label = Self::decision_label_for_rewrite(from);
        let to_label = Self::decision_label_for_rewrite(to);
        if from_label == to_label {
            return None;
        }
        Some(DecisionRewriteReceipt {
            from: from_label,
            to: to_label,
            reason: reason
                .unwrap_or("decision rewritten by guardrail")
                .trim()
                .to_string(),
        })
    }

    pub(super) fn action_rewrite_receipt(
        from: &Action,
        to: &Action,
        reason: Option<&str>,
    ) -> Option<DecisionRewriteReceipt> {
        let from_label = Self::action_label_for_rewrite(from);
        let to_label = Self::action_label_for_rewrite(to);
        if from_label == to_label {
            return None;
        }
        Some(DecisionRewriteReceipt {
            from: from_label,
            to: to_label,
            reason: reason
                .unwrap_or("action rewritten by guardrail")
                .trim()
                .to_string(),
        })
    }

    pub(super) fn decision_rewrite_receipt_json(receipt: &DecisionRewriteReceipt) -> String {
        serde_json::to_string(receipt).unwrap_or_else(|_| {
            format!(
                r#"{{"from":"{}","to":"{}","reason":"{}"}}"#,
                receipt.from, receipt.to, receipt.reason
            )
        })
    }

    pub(super) fn record_decision_rewrite_receipt(
        &mut self,
        time: u64,
        receipt: &DecisionRewriteReceipt,
        turn_output_summary: &mut String,
    ) {
        let receipt_json = Self::decision_rewrite_receipt_json(receipt);
        let note = format!("decision_rewrite: {receipt_json}");
        self.memory.record_note(time, note.clone());
        let _ = self.append_conversation_message(time, LlmChatRole::System, note.as_str());
        *turn_output_summary = format!(
            "{}; decision_rewrite={}",
            turn_output_summary,
            summarize_trace_text(receipt_json.as_str(), 200)
        );
    }

    pub(super) fn memory_digest_for_prompt(digest: &str) -> String {
        summarize_trace_text(digest, PROMPT_MEMORY_DIGEST_MAX_CHARS)
    }

    pub(super) fn conversation_history_json_for_prompt(&self) -> String {
        let start = self
            .conversation_history
            .len()
            .saturating_sub(PROMPT_CONVERSATION_MAX_ITEMS);
        let compact = self.conversation_history[start..]
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "time": entry.time,
                    "role": match entry.role {
                        LlmChatRole::Player => "player",
                        LlmChatRole::Agent => "agent",
                        LlmChatRole::Tool => "tool",
                        LlmChatRole::System => "system",
                    },
                    "content": summarize_trace_text(
                        entry.content.as_str(),
                        PROMPT_CONVERSATION_ITEM_MAX_CHARS,
                    ),
                })
            })
            .collect::<Vec<_>>();
        serde_json::to_string(&compact).unwrap_or_else(|_| "[]".to_string())
    }

    pub(super) fn module_history_json_for_prompt(module_history: &[ModuleCallExchange]) -> String {
        let compact_history = module_history
            .iter()
            .map(|exchange| {
                serde_json::json!({
                    "module": exchange.module,
                    "args": Self::compact_json_value_for_prompt(
                        &exchange.args,
                        PROMPT_MODULE_ARGS_MAX_CHARS,
                    ),
                    "result": Self::module_result_for_prompt(&exchange.result),
                })
            })
            .collect::<Vec<_>>();

        serde_json::to_string(&compact_history).unwrap_or_else(|_| "[]".to_string())
    }

    pub(super) fn compact_json_value_for_prompt(
        value: &serde_json::Value,
        max_chars: usize,
    ) -> serde_json::Value {
        let serialized = serde_json::to_string(value).unwrap_or_else(|_| "null".to_string());
        let total_chars = serialized.chars().count();
        if total_chars <= max_chars {
            return value.clone();
        }

        serde_json::json!({
            "truncated": true,
            "original_chars": total_chars,
            "preview": summarize_trace_text(serialized.as_str(), max_chars),
        })
    }

    pub(super) fn module_result_for_prompt(result: &serde_json::Value) -> serde_json::Value {
        Self::compact_json_value_for_prompt(result, PROMPT_MODULE_RESULT_MAX_CHARS)
    }

    pub(super) fn trace_input(system_prompt: &str, user_prompt: &str) -> String {
        format!("[system]\n{}\n\n[user]\n{}", system_prompt, user_prompt)
    }
}
