use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::Path;
use std::process;
use std::time::Instant;

use oasis7::simulator::{
    ActionCatalogEntry, ActionResult, AgentBehavior, AgentDecision, AgentDecisionTrace,
    AgentRunner, LlmAgentBehavior, OpenAiChatCompletionClient, ProviderBackedAgentBehavior,
    ProviderExecutionMode, ProviderLoopbackAdapter, RuntimePerfSnapshot, WorldEvent,
    initialize_kernel,
};
use oasis7_simulator::{
    Action as SimulatorAction, RejectReason, WorldConfig, WorldInitConfig, WorldScenario,
};
use serde::{Deserialize, Serialize};

#[path = "llm_io.rs"]
mod llm_io;
#[path = "runtime_bridge.rs"]
mod runtime_bridge;

use llm_io::print_llm_io_trace;
#[cfg(test)]
use llm_io::truncate_for_llm_io_log;
use runtime_bridge::{
    RuntimeGameplayBridge, RuntimeGameplayPreset, RuntimeGameplayPresetHandles,
    advance_kernel_time_with_noop_move, execute_action_in_kernel, execute_system_action_in_kernel,
    is_bridgeable_action,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PromptSwitchSpec {
    tick: u64,
    #[serde(default, alias = "system_prompt")]
    llm_system_prompt: Option<String>,
    #[serde(default, alias = "short_term_goal")]
    llm_short_term_goal: Option<String>,
    #[serde(default, alias = "long_term_goal")]
    llm_long_term_goal: Option<String>,
}

impl PromptSwitchSpec {
    fn has_override(&self) -> bool {
        self.llm_system_prompt.is_some()
            || self.llm_short_term_goal.is_some()
            || self.llm_long_term_goal.is_some()
    }
}

#[derive(Debug, Clone, PartialEq)]
struct CliOptions {
    scenario: WorldScenario,
    ticks: u64,
    coverage_bootstrap_profile: CoverageBootstrapProfile,
    runtime_gameplay_bridge: bool,
    runtime_gameplay_preset: RuntimeGameplayPreset,
    load_state_dir: Option<String>,
    save_state_dir: Option<String>,
    report_json: Option<String>,
    print_llm_io: bool,
    llm_io_max_chars: Option<usize>,
    llm_system_prompt: Option<String>,
    llm_short_term_goal: Option<String>,
    llm_long_term_goal: Option<String>,
    prompt_switch_tick: Option<u64>,
    switch_llm_system_prompt: Option<String>,
    switch_llm_short_term_goal: Option<String>,
    switch_llm_long_term_goal: Option<String>,
    prompt_switches_json: Option<String>,
    prompt_switches: Vec<PromptSwitchSpec>,
    decision_source: DecisionSource,
    agent_provider_url: Option<String>,
    agent_provider_auth_token: Option<String>,
    agent_provider_connect_timeout_ms: u64,
    agent_provider_decision_timeout_ms: u64,
    agent_provider_profile: String,
}

impl Default for CliOptions {
    fn default() -> Self {
        Self {
            scenario: WorldScenario::LlmBootstrap,
            ticks: 20,
            coverage_bootstrap_profile: CoverageBootstrapProfile::None,
            runtime_gameplay_bridge: true,
            runtime_gameplay_preset: RuntimeGameplayPreset::None,
            load_state_dir: None,
            save_state_dir: None,
            report_json: None,
            print_llm_io: false,
            llm_io_max_chars: None,
            llm_system_prompt: None,
            llm_short_term_goal: None,
            llm_long_term_goal: None,
            prompt_switch_tick: None,
            switch_llm_system_prompt: None,
            switch_llm_short_term_goal: None,
            switch_llm_long_term_goal: None,
            prompt_switches_json: None,
            prompt_switches: Vec::new(),
            decision_source: DecisionSource::LlmResponses,
            agent_provider_url: None,
            agent_provider_auth_token: None,
            agent_provider_connect_timeout_ms: 60_000,
            agent_provider_decision_timeout_ms: 60_000,
            agent_provider_profile: "oasis7_p0_low_freq_npc".to_string(),
        }
    }
}

impl CliOptions {
    fn has_initial_prompt_override(&self) -> bool {
        self.llm_system_prompt.is_some()
            || self.llm_short_term_goal.is_some()
            || self.llm_long_term_goal.is_some()
    }

    fn has_switch_prompt_override(&self) -> bool {
        self.switch_llm_system_prompt.is_some()
            || self.switch_llm_short_term_goal.is_some()
            || self.switch_llm_long_term_goal.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CoverageBootstrapProfile {
    None,
    Industrial,
    Gameplay,
    Hybrid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecisionSource {
    LlmResponses,
    ProviderLoopbackHttp,
}

impl DecisionSource {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "" | "llm" | "llm_responses" | "responses" | "builtin" => Some(Self::LlmResponses),
            "provider_loopback_http" | "provider_local_bridge" | "provider_backed" => {
                Some(Self::ProviderLoopbackHttp)
            }
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::LlmResponses => "llm_responses",
            Self::ProviderLoopbackHttp => "provider_loopback_http",
        }
    }
}

#[expect(
    clippy::large_enum_variant,
    reason = "Stable runtime enum preserves established variant payload ownership and wire behavior."
)]
enum DemoAgentBehavior {
    Llm(LlmAgentBehavior<OpenAiChatCompletionClient>),
    ProviderBacked {
        behavior: ProviderBackedAgentBehavior<ProviderLoopbackAdapter>,
        prompt_context: ProviderPromptContext,
    },
}

#[derive(Debug, Clone, Default)]
struct ProviderPromptContext {
    system_prompt: Option<String>,
    short_term_goal: Option<String>,
    long_term_goal: Option<String>,
}

impl ProviderPromptContext {
    fn from_options(options: &CliOptions) -> Self {
        Self {
            system_prompt: options.llm_system_prompt.clone(),
            short_term_goal: options.llm_short_term_goal.clone(),
            long_term_goal: options.llm_long_term_goal.clone(),
        }
    }

    fn apply_switch(&mut self, switch: &PromptSwitchSpec) {
        if switch.llm_system_prompt.is_some() {
            self.system_prompt = switch.llm_system_prompt.clone();
        }
        if switch.llm_short_term_goal.is_some() {
            self.short_term_goal = switch.llm_short_term_goal.clone();
        }
        if switch.llm_long_term_goal.is_some() {
            self.long_term_goal = switch.llm_long_term_goal.clone();
        }
    }

    fn memory_summary(&self) -> Option<String> {
        provider_memory_summary_from_parts(
            self.system_prompt.as_deref(),
            self.short_term_goal.as_deref(),
            self.long_term_goal.as_deref(),
        )
    }
}

impl DemoAgentBehavior {
    fn apply_prompt_switch(&mut self, switch: &PromptSwitchSpec) {
        match self {
            Self::Llm(behavior) => {
                let current = behavior.prompt_overrides();
                behavior.apply_prompt_overrides(
                    switch.llm_system_prompt.clone().or(current.system_prompt),
                    switch
                        .llm_short_term_goal
                        .clone()
                        .or(current.short_term_goal),
                    switch.llm_long_term_goal.clone().or(current.long_term_goal),
                );
            }
            Self::ProviderBacked {
                behavior,
                prompt_context,
            } => {
                prompt_context.apply_switch(switch);
                behavior.replace_memory_summary(prompt_context.memory_summary());
            }
        }
    }
}

impl AgentBehavior for DemoAgentBehavior {
    fn agent_id(&self) -> &str {
        match self {
            Self::Llm(behavior) => behavior.agent_id(),
            Self::ProviderBacked { behavior, .. } => behavior.agent_id(),
        }
    }

    fn decide(&mut self, observation: &oasis7_simulator::Observation) -> AgentDecision {
        match self {
            Self::Llm(behavior) => behavior.decide(observation),
            Self::ProviderBacked { behavior, .. } => behavior.decide(observation),
        }
    }

    fn on_action_result(&mut self, result: &ActionResult) {
        match self {
            Self::Llm(behavior) => behavior.on_action_result(result),
            Self::ProviderBacked { behavior, .. } => behavior.on_action_result(result),
        }
    }

    fn on_event(&mut self, event: &WorldEvent) {
        match self {
            Self::Llm(behavior) => behavior.on_event(event),
            Self::ProviderBacked { behavior, .. } => behavior.on_event(event),
        }
    }

    fn take_decision_trace(&mut self) -> Option<AgentDecisionTrace> {
        match self {
            Self::Llm(behavior) => behavior.take_decision_trace(),
            Self::ProviderBacked { behavior, .. } => behavior.take_decision_trace(),
        }
    }
}

fn provider_phase1_action_catalog() -> Vec<ActionCatalogEntry> {
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

fn provider_memory_summary_from_parts(
    system_prompt: Option<&str>,
    short_term_goal: Option<&str>,
    long_term_goal: Option<&str>,
) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(goal) = short_term_goal {
        parts.push(format!("short_term_goal={goal}"));
    }
    if let Some(goal) = long_term_goal {
        parts.push(format!("long_term_goal={goal}"));
    }
    if let Some(system) = system_prompt {
        parts.push(format!("system_hint={system}"));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

impl CoverageBootstrapProfile {
    fn parse(raw: &str) -> Option<Self> {
        let normalized = raw.trim().to_ascii_lowercase().replace('-', "_");
        match normalized.as_str() {
            "" | "none" | "off" => Some(Self::None),
            "industrial" => Some(Self::Industrial),
            "gameplay" => Some(Self::Gameplay),
            "hybrid" => Some(Self::Hybrid),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Industrial => "industrial",
            Self::Gameplay => "gameplay",
            Self::Hybrid => "hybrid",
        }
    }

    fn requires_runtime_bridge(&self) -> bool {
        matches!(self, Self::Gameplay | Self::Hybrid)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
struct DecisionCounts {
    wait: u64,
    wait_ticks: u64,
    act: u64,
    query: u64,
    module_command: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
struct TraceCounts {
    traces: u64,
    llm_skipped_ticks: u64,
    llm_skipped_tick_ratio_ppm: u64,
    llm_errors: u64,
    parse_errors: u64,
    repair_rounds_total: u64,
    repair_rounds_max: u32,
    llm_input_chars_total: u64,
    llm_input_chars_avg: u64,
    llm_input_chars_max: usize,
    step_entries: u64,
    prompt_section_entries: u64,
    prompt_section_clipped: u64,
    step_type_counts: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct DemoRunReport {
    scenario: String,
    ticks_requested: u64,
    active_ticks: u64,
    runtime_bridge_actions: u64,
    runtime_bridge_action_success: u64,
    runtime_bridge_action_failure: u64,
    total_actions: u64,
    total_decisions: u64,
    action_success: u64,
    action_failure: u64,
    action_reject_reason_counts: BTreeMap<String, u64>,
    action_kind_counts: BTreeMap<String, u64>,
    action_kind_success_counts: BTreeMap<String, u64>,
    action_kind_failure_counts: BTreeMap<String, u64>,
    first_action_tick: BTreeMap<String, u64>,
    decision_counts: DecisionCounts,
    trace_counts: TraceCounts,
    runtime_perf: RuntimePerfSnapshot,
    world_time: u64,
    journal_events: usize,
}

impl DemoRunReport {
    fn new(scenario: String, ticks_requested: u64) -> Self {
        Self {
            scenario,
            ticks_requested,
            active_ticks: 0,
            runtime_bridge_actions: 0,
            runtime_bridge_action_success: 0,
            runtime_bridge_action_failure: 0,
            total_actions: 0,
            total_decisions: 0,
            action_success: 0,
            action_failure: 0,
            action_reject_reason_counts: BTreeMap::new(),
            action_kind_counts: BTreeMap::new(),
            action_kind_success_counts: BTreeMap::new(),
            action_kind_failure_counts: BTreeMap::new(),
            first_action_tick: BTreeMap::new(),
            decision_counts: DecisionCounts::default(),
            trace_counts: TraceCounts::default(),
            runtime_perf: RuntimePerfSnapshot::default(),
            world_time: 0,
            journal_events: 0,
        }
    }

    fn observe_decision(&mut self, decision: &AgentDecision) {
        match decision {
            AgentDecision::Wait => {
                self.decision_counts.wait += 1;
            }
            AgentDecision::WaitTicks(_) => {
                self.decision_counts.wait_ticks += 1;
            }
            AgentDecision::Act(_) => {
                self.decision_counts.act += 1;
            }
            AgentDecision::Query(_) => {
                self.decision_counts.query += 1;
            }
            AgentDecision::ModuleCommand { .. } => {
                self.decision_counts.module_command += 1;
            }
        }
    }

    fn observe_trace(&mut self, trace: &AgentDecisionTrace) {
        self.trace_counts.traces += 1;
        if trace_skipped_llm_tick(trace) {
            self.trace_counts.llm_skipped_ticks += 1;
        }

        if trace.llm_error.is_some() {
            self.trace_counts.llm_errors += 1;
        }
        if trace.parse_error.is_some() {
            self.trace_counts.parse_errors += 1;
        }

        let retry_count = trace
            .llm_diagnostics
            .as_ref()
            .map(|diagnostics| diagnostics.retry_count)
            .unwrap_or(0);
        self.trace_counts.repair_rounds_total += retry_count as u64;
        self.trace_counts.repair_rounds_max = self.trace_counts.repair_rounds_max.max(retry_count);

        if let Some(input) = trace.llm_input.as_ref() {
            let chars = input.chars().count();
            self.trace_counts.llm_input_chars_total += chars as u64;
            self.trace_counts.llm_input_chars_max =
                self.trace_counts.llm_input_chars_max.max(chars);
        }

        self.trace_counts.step_entries += trace.llm_step_trace.len() as u64;
        self.trace_counts.prompt_section_entries += trace.llm_prompt_section_trace.len() as u64;

        for step in &trace.llm_step_trace {
            *self
                .trace_counts
                .step_type_counts
                .entry(step.step_type.clone())
                .or_insert(0) += 1;
        }

        for section in &trace.llm_prompt_section_trace {
            if !section.included || section.emitted_tokens < section.estimated_tokens {
                self.trace_counts.prompt_section_clipped += 1;
            }
        }
    }

    fn observe_action_result(&mut self, tick: u64, action_result: &ActionResult) {
        let action_kind = action_metric_key(&action_result.action);
        *self
            .action_kind_counts
            .entry(action_kind.clone())
            .or_insert(0) += 1;
        self.first_action_tick
            .entry(action_kind.clone())
            .or_insert(tick);

        if action_result.success {
            self.action_success += 1;
            *self
                .action_kind_success_counts
                .entry(action_kind)
                .or_insert(0) += 1;
            return;
        }
        self.action_failure += 1;
        *self
            .action_kind_failure_counts
            .entry(action_kind)
            .or_insert(0) += 1;
        if let Some(reason) = action_result.reject_reason() {
            let key = reject_reason_metric_key(reason);
            *self.action_reject_reason_counts.entry(key).or_insert(0) += 1;
        }
    }

    fn observe_runtime_bridge_result(&mut self, action_result: &ActionResult) {
        self.runtime_bridge_actions += 1;
        if action_result.success {
            self.runtime_bridge_action_success += 1;
        } else {
            self.runtime_bridge_action_failure += 1;
        }
    }

    fn finalize(&mut self) {
        self.trace_counts.llm_input_chars_avg = self
            .trace_counts
            .llm_input_chars_total
            .checked_div(self.trace_counts.traces)
            .unwrap_or(0);
        self.trace_counts.llm_skipped_tick_ratio_ppm = self
            .trace_counts
            .llm_skipped_ticks
            .saturating_mul(1_000_000)
            .checked_div(self.active_ticks)
            .unwrap_or(0);
    }
}

fn trace_skipped_llm_tick(trace: &AgentDecisionTrace) -> bool {
    trace.llm_input.is_none()
        || trace.llm_step_trace.iter().any(|step| {
            step.input_summary == "skip_llm_with_active_execute_until"
                || step.step_type == "execute_until_continue"
        })
}

fn reject_reason_metric_key(reason: &RejectReason) -> String {
    serde_json::to_value(reason)
        .ok()
        .and_then(|value| {
            value
                .get("type")
                .and_then(|inner| inner.as_str())
                .map(normalize_reason_metric_key)
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn action_metric_key(action: &SimulatorAction) -> String {
    serde_json::to_value(action)
        .ok()
        .and_then(|value| {
            value
                .get("type")
                .and_then(|inner| inner.as_str())
                .map(normalize_reason_metric_key)
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn normalize_reason_metric_key(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "unknown".to_string();
    }

    if trimmed
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
    {
        return trimmed.to_string();
    }

    let mut normalized = String::with_capacity(trimmed.len() + 8);
    for (index, ch) in trimmed.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                normalized.push('_');
            }
            normalized.push(ch.to_ascii_lowercase());
        } else if ch == '-' || ch == ' ' {
            normalized.push('_');
        } else {
            normalized.push(ch.to_ascii_lowercase());
        }
    }
    normalized
}

fn merge_runtime_gameplay_preset_handles(
    target: &mut RuntimeGameplayPresetHandles,
    source: RuntimeGameplayPresetHandles,
) {
    if target.governance_proposal_key.is_none() {
        target.governance_proposal_key = source.governance_proposal_key;
    }
    if target.governance_vote_option.is_none() {
        target.governance_vote_option = source.governance_vote_option;
    }
    if target.crisis_id.is_none() {
        target.crisis_id = source.crisis_id;
    }
    if target.economic_contract_id.is_none() {
        target.economic_contract_id = source.economic_contract_id;
    }
    if target.economic_contract_counterparty.is_none() {
        target.economic_contract_counterparty = source.economic_contract_counterparty;
    }
}

fn bootstrap_error_from_action_result(label: &str, result: &ActionResult) -> String {
    if let Some(reason) = result.reject_reason() {
        return format!("{label} rejected: {reason:?}");
    }
    format!("{label} failed without reject reason")
}

fn execute_required_kernel_bootstrap_action(
    kernel: &mut oasis7::simulator::WorldKernel,
    actor_agent_id: &str,
    action: SimulatorAction,
    run_report: &mut DemoRunReport,
    label: &str,
) -> Result<u64, String> {
    let result = execute_action_in_kernel(kernel, actor_agent_id, action);
    run_report.observe_action_result(result.event.time, &result);
    if result.success {
        Ok(1)
    } else {
        Err(bootstrap_error_from_action_result(label, &result))
    }
}

fn grant_agent_resource_for_bootstrap(
    kernel: &mut oasis7::simulator::WorldKernel,
    owner: &oasis7_simulator::ResourceOwner,
    kind: oasis7_agent_api::world_action::ResourceKind,
    amount: i64,
    label: &str,
) -> Result<(), String> {
    let result = execute_system_action_in_kernel(
        kernel,
        SimulatorAction::DebugGrantResource {
            owner: owner.clone(),
            kind,
            amount,
        },
    );
    if result.success {
        Ok(())
    } else {
        Err(bootstrap_error_from_action_result(label, &result))
    }
}

fn run_industrial_coverage_bootstrap(
    kernel: &mut oasis7::simulator::WorldKernel,
    run_report: &mut DemoRunReport,
) -> Result<u64, String> {
    let mut agent_ids: Vec<String> = kernel.model().agents.keys().cloned().collect();
    agent_ids.sort();
    let actor_agent_id = agent_ids
        .first()
        .cloned()
        .ok_or_else(|| "industrial coverage bootstrap requires at least 1 agent".to_string())?;
    let actor_location_id = kernel
        .model()
        .agents
        .get(actor_agent_id.as_str())
        .map(|agent| agent.location_id.clone())
        .ok_or_else(|| {
            format!(
                "industrial coverage bootstrap missing agent state for {}",
                actor_agent_id
            )
        })?;
    let owner = oasis7_simulator::ResourceOwner::Agent {
        agent_id: actor_agent_id.clone(),
    };

    grant_agent_resource_for_bootstrap(
        kernel,
        &owner,
        oasis7_agent_api::world_action::ResourceKind::Electricity,
        200_000,
        "industrial coverage bootstrap grant electricity pre-harvest",
    )?;
    grant_agent_resource_for_bootstrap(
        kernel,
        &owner,
        oasis7_agent_api::world_action::ResourceKind::Data,
        200_000,
        "industrial coverage bootstrap grant data pre-harvest",
    )?;

    let mut action_count = 0_u64;
    action_count += execute_required_kernel_bootstrap_action(
        kernel,
        actor_agent_id.as_str(),
        SimulatorAction::HarvestRadiation {
            agent_id: actor_agent_id.clone(),
            max_amount: 100,
        },
        run_report,
        "industrial coverage bootstrap harvest_radiation",
    )?;
    action_count += execute_required_kernel_bootstrap_action(
        kernel,
        actor_agent_id.as_str(),
        SimulatorAction::MineCompound {
            owner: owner.clone(),
            location_id: actor_location_id.clone(),
            compound_mass_g: 2_000,
        },
        run_report,
        "industrial coverage bootstrap mine_compound",
    )?;
    action_count += execute_required_kernel_bootstrap_action(
        kernel,
        actor_agent_id.as_str(),
        SimulatorAction::RefineCompound {
            owner: owner.clone(),
            compound_mass_g: 1_000,
        },
        run_report,
        "industrial coverage bootstrap refine_compound",
    )?;

    grant_agent_resource_for_bootstrap(
        kernel,
        &owner,
        oasis7_agent_api::world_action::ResourceKind::Electricity,
        200_000,
        "industrial coverage bootstrap grant electricity pre-factory",
    )?;
    grant_agent_resource_for_bootstrap(
        kernel,
        &owner,
        oasis7_agent_api::world_action::ResourceKind::Data,
        200_000,
        "industrial coverage bootstrap grant data pre-factory",
    )?;

    let factory_id = format!(
        "coverage.factory.assembler.{}.{}",
        actor_agent_id.replace('-', "_"),
        kernel.time().saturating_add(1)
    );
    action_count += execute_required_kernel_bootstrap_action(
        kernel,
        actor_agent_id.as_str(),
        SimulatorAction::BuildFactory {
            owner: owner.clone(),
            location_id: actor_location_id,
            factory_id: factory_id.clone(),
            factory_kind: "factory.assembler.mk1".to_string(),
        },
        run_report,
        "industrial coverage bootstrap build_factory",
    )?;

    grant_agent_resource_for_bootstrap(
        kernel,
        &owner,
        oasis7_agent_api::world_action::ResourceKind::Electricity,
        200_000,
        "industrial coverage bootstrap grant electricity pre-recipe",
    )?;
    grant_agent_resource_for_bootstrap(
        kernel,
        &owner,
        oasis7_agent_api::world_action::ResourceKind::Data,
        200_000,
        "industrial coverage bootstrap grant data pre-recipe",
    )?;

    action_count += execute_required_kernel_bootstrap_action(
        kernel,
        actor_agent_id.as_str(),
        SimulatorAction::ScheduleRecipe {
            owner,
            factory_id,
            recipe_id: "recipe.control_chip".to_string(),
            batches: 1,
        },
        run_report,
        "industrial coverage bootstrap schedule_recipe",
    )?;
    Ok(action_count)
}

fn execute_required_runtime_bridge_bootstrap_action(
    kernel: &mut oasis7::simulator::WorldKernel,
    runtime_bridge: &mut RuntimeGameplayBridge,
    actor_agent_id: &str,
    tick: u64,
    action: SimulatorAction,
    run_report: &mut DemoRunReport,
    label: &str,
) -> Result<u64, String> {
    let result = runtime_bridge
        .execute(tick, actor_agent_id, action)
        .map_err(|err| format!("{label} bridge execution failed: {err}"))?;
    run_report.observe_runtime_bridge_result(&result);
    run_report.observe_action_result(tick, &result);
    if !result.success {
        return Err(bootstrap_error_from_action_result(label, &result));
    }
    advance_kernel_time_with_noop_move(kernel, actor_agent_id);
    Ok(1)
}

fn run_gameplay_coverage_bootstrap(
    kernel: &mut oasis7::simulator::WorldKernel,
    runtime_bridge: &mut RuntimeGameplayBridge,
    runtime_gameplay_preset_handles: &mut RuntimeGameplayPresetHandles,
    run_report: &mut DemoRunReport,
) -> Result<u64, String> {
    if runtime_gameplay_preset_handles.crisis_id.is_none() {
        let seeded_handles = runtime_bridge.apply_preset(RuntimeGameplayPreset::CivicHotspotV1)?;
        merge_runtime_gameplay_preset_handles(runtime_gameplay_preset_handles, seeded_handles);
    }

    let mut agent_ids: Vec<String> = kernel.model().agents.keys().cloned().collect();
    agent_ids.sort();
    if agent_ids.len() < 2 {
        return Err("gameplay coverage bootstrap requires at least 2 agents".to_string());
    }

    let proposer = agent_ids[0].clone();
    let voter = agent_ids[1].clone();
    let progress_target = agent_ids.get(2).cloned().unwrap_or_else(|| voter.clone());
    let crisis_id = runtime_gameplay_preset_handles
        .crisis_id
        .clone()
        .ok_or_else(|| "gameplay coverage bootstrap missing active crisis handle".to_string())?;
    let vote_option = runtime_gameplay_preset_handles
        .governance_vote_option
        .clone()
        .unwrap_or_else(|| "approve".to_string());
    let proposal_key = format!("coverage.governance.{}", kernel.time().saturating_add(1));

    let mut tick = kernel.time().saturating_add(1);
    let mut action_count = 0_u64;
    action_count += execute_required_runtime_bridge_bootstrap_action(
        kernel,
        runtime_bridge,
        proposer.as_str(),
        tick,
        SimulatorAction::OpenGovernanceProposal {
            proposer_agent_id: proposer.clone(),
            proposal_key: proposal_key.clone(),
            title: "coverage gameplay proposal".to_string(),
            description: "coverage bootstrap proposal".to_string(),
            options: vec![vote_option.clone(), "reject".to_string()],
            voting_window_ticks: 24,
            quorum_weight: 1,
            pass_threshold_bps: 5_000,
        },
        run_report,
        "gameplay coverage bootstrap open_governance_proposal",
    )?;
    tick = kernel.time().saturating_add(1);
    action_count += execute_required_runtime_bridge_bootstrap_action(
        kernel,
        runtime_bridge,
        voter.as_str(),
        tick,
        SimulatorAction::CastGovernanceVote {
            voter_agent_id: voter.clone(),
            proposal_key,
            option: vote_option,
            weight: 1,
        },
        run_report,
        "gameplay coverage bootstrap cast_governance_vote",
    )?;
    tick = kernel.time().saturating_add(1);
    action_count += execute_required_runtime_bridge_bootstrap_action(
        kernel,
        runtime_bridge,
        proposer.as_str(),
        tick,
        SimulatorAction::ResolveCrisis {
            resolver_agent_id: proposer.clone(),
            crisis_id,
            strategy: "coverage_bootstrap_strategy".to_string(),
            success: true,
        },
        run_report,
        "gameplay coverage bootstrap resolve_crisis",
    )?;
    tick = kernel.time().saturating_add(1);
    action_count += execute_required_runtime_bridge_bootstrap_action(
        kernel,
        runtime_bridge,
        proposer.as_str(),
        tick,
        SimulatorAction::GrantMetaProgress {
            operator_agent_id: proposer.clone(),
            target_agent_id: progress_target,
            track: "civic".to_string(),
            points: 5,
            achievement_id: Some("coverage_bootstrap_achievement".to_string()),
        },
        run_report,
        "gameplay coverage bootstrap grant_meta_progress",
    )?;
    Ok(action_count)
}

fn run_coverage_bootstrap(
    profile: CoverageBootstrapProfile,
    kernel: &mut oasis7::simulator::WorldKernel,
    runtime_gameplay_bridge: &mut Option<RuntimeGameplayBridge>,
    runtime_gameplay_preset_handles: &mut RuntimeGameplayPresetHandles,
    run_report: &mut DemoRunReport,
) -> Result<u64, String> {
    let mut action_count = 0_u64;
    if matches!(
        profile,
        CoverageBootstrapProfile::Industrial | CoverageBootstrapProfile::Hybrid
    ) {
        action_count += run_industrial_coverage_bootstrap(kernel, run_report)?;
    }
    if matches!(
        profile,
        CoverageBootstrapProfile::Gameplay | CoverageBootstrapProfile::Hybrid
    ) {
        let runtime_bridge = runtime_gameplay_bridge.as_mut().ok_or_else(|| {
            format!(
                "coverage bootstrap profile {} requires runtime gameplay bridge",
                profile.as_str()
            )
        })?;
        action_count += run_gameplay_coverage_bootstrap(
            kernel,
            runtime_bridge,
            runtime_gameplay_preset_handles,
            run_report,
        )?;
    }
    Ok(action_count)
}
