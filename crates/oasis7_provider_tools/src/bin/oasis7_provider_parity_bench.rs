use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use oasis7::simulator::{
    ActionResult, AgentBehavior, AgentDecision, AgentDecisionTrace, AgentRunner,
    ContinuousAgentRequestContextV1, ContinuousAgentTurnContextV1,
    DEFAULT_PROVIDER_ACTION_SCHEMA_VERSION, DEFAULT_PROVIDER_OBSERVATION_SCHEMA_VERSION,
    LlmAgentBehavior, MockDecisionProvider, OpenAiChatCompletionClient,
    ProviderCompatibilityStatus, ProviderExecutionMode, ProviderLoopbackAdapter,
    ProviderLoopbackHttpClient, RuntimePerfSnapshot, WorldEvent, evaluate_provider_compatibility,
    initialize_kernel, provider_phase1_required_actions, provider_phase1_required_capabilities,
};
use oasis7_simulator::{Observation, WorldConfig, WorldInitConfig, WorldScenario};
use serde::{Deserialize, Serialize};

#[path = "oasis7_provider_parity_bench/behavior_support.rs"]
mod behavior_support;
#[path = "oasis7_provider_parity_bench/io_support.rs"]
mod io_support;
#[path = "oasis7_provider_parity_bench/recovery_ledger.rs"]
mod recovery_ledger;
#[path = "oasis7_provider_parity_bench/target_context.rs"]
mod target_context;

use self::behavior_support::{
    action_ref_from_decision, apply_builtin_parity_guardrail, builtin_parity_short_term_goal,
    classify_trace_error, decision_label, derive_status, execution_environment_class,
    parity_memory_summary, percentile_u64, phase1_action_catalog, ratio_ppm,
    unavailable_recovery_lineage,
};
use self::io_support::{parse_options, print_help, sanitize_filename, write_json, write_jsonl};
use self::recovery_ledger::{
    RECOVERY_METRIC_SCHEMA_VERSION, RecoveryErrorEvidence, RecoveryLedger, RecoveryLineage,
    RecoveryMetricSummary, metric_summary, scenario_goal_completed,
};
use self::target_context::build_target_context;

const DEFAULT_PROTOCOL_VERSION: &str = "2026-03-12";
const DEFAULT_ADAPTER_VERSION: &str = "provider_phase1_adapter_v1";
const DEFAULT_TIMEOUT_MS: u64 = 15_000;
const DEFAULT_TICKS: u64 = 20;
const DEFAULT_PROVIDER_CONNECT_TIMEOUT_MS: u64 = 15_000;
const DEFAULT_PROVIDER_AGENT_PROFILE: &str = "oasis7_p0_low_freq_npc";
const DEFAULT_MAX_MOVE_DISTANCE_CM_PER_TICK: i64 = 1_000_000;
const LOCAL_EXECUTION_AUTHORITY: &str = "simulator_world_kernel";
const RUNTIME_CERTIFICATION_STATUS: &str = "not_certified";
const RUNTIME_CERTIFICATION_REASON: &str = "unified Runtime execution and receipt authority is not wired; this is local simulator smoke only";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BenchProviderKind {
    Builtin,
    ProviderLoopbackHttp,
}

impl BenchProviderKind {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "builtin" => Some(Self::Builtin),
            "provider_loopback_http" | "provider_local_bridge" => Some(Self::ProviderLoopbackHttp),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::ProviderLoopbackHttp => "provider_loopback_http",
        }
    }

    fn summary_suffix(self) -> &'static str {
        self.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliOptions {
    provider: BenchProviderKind,
    scenario: WorldScenario,
    scenario_id: String,
    parity_tier: String,
    benchmark_run_id: String,
    fixture_id: Option<String>,
    protocol_version: String,
    adapter_version: String,
    ticks: u64,
    timeout_ms: u64,
    out_dir: PathBuf,
    provider_base_url: Option<String>,
    provider_auth_token: Option<String>,
    agent_provider_connect_timeout_ms: u64,
    agent_provider_profile: String,
    execution_mode: ProviderExecutionMode,
}

impl Default for CliOptions {
    fn default() -> Self {
        Self {
            provider: BenchProviderKind::Builtin,
            scenario: WorldScenario::LlmBootstrap,
            scenario_id: "P0-001".to_string(),
            parity_tier: "P0".to_string(),
            benchmark_run_id: "manual".to_string(),
            fixture_id: None,
            protocol_version: DEFAULT_PROTOCOL_VERSION.to_string(),
            adapter_version: DEFAULT_ADAPTER_VERSION.to_string(),
            ticks: DEFAULT_TICKS,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            out_dir: PathBuf::from("output/provider_parity/manual"),
            provider_base_url: None,
            provider_auth_token: None,
            agent_provider_connect_timeout_ms: DEFAULT_PROVIDER_CONNECT_TIMEOUT_MS,
            agent_provider_profile: DEFAULT_PROVIDER_AGENT_PROFILE.to_string(),
            execution_mode: ProviderExecutionMode::HeadlessAgent,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ProviderRunInfo {
    provider_kind: String,
    provider_version: String,
    adapter_version: String,
    protocol_version: String,
    compatibility_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    fallback_reason: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    capabilities: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    supported_action_sets: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_last_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_queue_depth: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_profile: Option<String>,
    /// Route evidence is persisted with every provider run so T4/T5 artifacts
    /// cannot be mistaken for legacy cognition traffic.
    cognition_lane: String,
    decision_route: String,
    feedback_route: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct FixtureRefs {
    initial_world_snapshot_ref: String,
    observation_sequence_ref: String,
    goal_definition: String,
    action_catalog_ref: String,
    player_context_ref: String,
    memory_fixture_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct StepTraceRecord {
    benchmark_run_id: String,
    mode: String,
    observation_schema_version: String,
    action_schema_version: String,
    environment_class: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    fallback_reason: Option<String>,
    parity_tier: String,
    scenario_id: String,
    fixture_id: String,
    provider_kind: String,
    provider_version: String,
    adapter_version: String,
    protocol_version: String,
    step_index: u64,
    agent_id: String,
    decision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    action_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_code: Option<String>,
    retry_count: u32,
    trace_present: bool,
    trace_message_count: usize,
    trace_tool_call_count: usize,
    context_drift_flag: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    action_success: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct SampleSummary {
    benchmark_run_id: String,
    mode: String,
    observation_schema_version: String,
    action_schema_version: String,
    environment_class: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    fallback_reason: Option<String>,
    parity_tier: String,
    scenario_id: String,
    fixture_id: String,
    provider_kind: String,
    provider_version: String,
    adapter_version: String,
    protocol_version: String,
    scenario: String,
    seed: String,
    status: String,
    execution_authority: String,
    runtime_certification_status: String,
    runtime_certification_reason: String,
    goal_completed: bool,
    completion_time_ms: u64,
    decision_steps: u64,
    invalid_action_count: u64,
    illegal_schema_count: u64,
    illegal_schema_rate: RecoveryMetricSummary,
    timeout_count: u64,
    recoverable_error_count: u64,
    fatal_error_count: u64,
    metric_schema_version: String,
    sample_id: String,
    trace_validity: String,
    recovery_events: Vec<recovery_ledger::RecoveryEvent>,
    recoverable_error_resolution_rate: RecoveryMetricSummary,
    trace_completeness_ratio_ppm: u64,
    median_latency_ms: u64,
    p95_latency_ms: u64,
    context_drift_count: u64,
    action_kind_counts: BTreeMap<String, u64>,
    error_counts: BTreeMap<String, u64>,
    fixture_refs: FixtureRefs,
    provider: ProviderRunInfo,
    notes: Vec<String>,
    runtime_perf: RuntimePerfSnapshot,
}

#[expect(
    clippy::large_enum_variant,
    reason = "Benchmark behavior variants preserve direct ownership to keep provider parity setup explicit."
)]
enum BenchBehavior {
    Builtin(BuiltinParityBehavior),
    ProviderBacked(ProviderBackedLoopbackBehavior),
}

struct BuiltinParityBehavior {
    inner: LlmAgentBehavior<OpenAiChatCompletionClient>,
    scenario_id: String,
    pending_trace: Option<AgentDecisionTrace>,
}

struct ProviderBackedLoopbackBehavior {
    inner: oasis7::simulator::ProviderBackedAgentBehavior<ProviderLoopbackAdapter>,
    request_builder: oasis7::simulator::ProviderBackedAgentBehavior<MockDecisionProvider>,
    request_builder_state: Arc<Mutex<oasis7::simulator::MockDecisionProviderState>>,
    fixture_id: String,
    session_id: String,
    next_turn: u64,
    retry_seq: u64,
    current_recovery_lineage: Option<RecoveryLineage>,
    recovery_chain_id: String,
    recovery_origin: Option<RecoveryLineage>,
}

impl AgentBehavior for BenchBehavior {
    fn agent_id(&self) -> &str {
        match self {
            Self::Builtin(inner) => inner.agent_id(),
            Self::ProviderBacked(inner) => inner.agent_id(),
        }
    }

    fn decide(&mut self, observation: &Observation) -> AgentDecision {
        match self {
            Self::Builtin(inner) => inner.decide(observation),
            Self::ProviderBacked(inner) => inner.decide(observation),
        }
    }

    fn on_action_result(&mut self, result: &ActionResult) {
        match self {
            Self::Builtin(inner) => inner.on_action_result(result),
            Self::ProviderBacked(inner) => inner.on_action_result(result),
        }
    }

    fn on_event(&mut self, event: &WorldEvent) {
        match self {
            Self::Builtin(inner) => inner.on_event(event),
            Self::ProviderBacked(inner) => inner.on_event(event),
        }
    }

    fn take_decision_trace(&mut self) -> Option<AgentDecisionTrace> {
        match self {
            Self::Builtin(inner) => inner.take_decision_trace(),
            Self::ProviderBacked(inner) => inner.take_decision_trace(),
        }
    }
}

impl BenchBehavior {
    fn current_recovery_lineage(&self) -> Option<RecoveryLineage> {
        match self {
            Self::Builtin(_) => None,
            Self::ProviderBacked(inner) => inner.current_recovery_lineage(),
        }
    }

    fn note_recoverable_error(&mut self, lineage: RecoveryLineage) {
        if let Self::ProviderBacked(inner) = self {
            inner.note_recoverable_error(lineage);
        }
    }
}

impl AgentBehavior for BuiltinParityBehavior {
    fn agent_id(&self) -> &str {
        self.inner.agent_id()
    }

    fn decide(&mut self, observation: &Observation) -> AgentDecision {
        let original_decision = self.inner.decide(observation);
        let mut trace = self.inner.take_decision_trace();
        let (decision, guardrail_note) = apply_builtin_parity_guardrail(
            self.scenario_id.as_str(),
            self.inner.agent_id(),
            observation,
            original_decision.clone(),
        );
        if let Some(note) = guardrail_note
            && let Some(trace) = trace.as_mut()
        {
            trace.decision = decision.clone();
            trace.llm_step_trace.push(oasis7::simulator::LlmStepTrace {
                step_index: trace.llm_step_trace.len(),
                step_type: "builtin_parity_guardrail".to_string(),
                input_summary: decision_label(&original_decision),
                output_summary: decision_label(&decision),
                status: note,
            });
        }
        self.pending_trace = trace;
        decision
    }

    fn on_action_result(&mut self, result: &ActionResult) {
        self.inner.on_action_result(result);
    }

    fn on_event(&mut self, event: &WorldEvent) {
        self.inner.on_event(event);
    }

    fn take_decision_trace(&mut self) -> Option<AgentDecisionTrace> {
        self.pending_trace.take()
    }
}

impl AgentBehavior for ProviderBackedLoopbackBehavior {
    fn agent_id(&self) -> &str {
        self.inner.agent_id()
    }

    fn decide(&mut self, observation: &Observation) -> AgentDecision {
        let (turn_context, request_context) = self.target_context(observation);
        self.current_recovery_lineage = Some(target_context::recovery_lineage(
            &request_context,
            &self.recovery_chain_id,
        ));
        self.inner.set_continuous_turn_context(Some(&turn_context));
        self.inner
            .set_continuous_request_context(Some(&request_context));
        let decision = self.inner.decide(observation);
        let _ = self.inner.take_continuous_response_context();
        decision
    }

    fn on_action_result(&mut self, result: &ActionResult) {
        self.request_builder.on_action_result(result);
        if result.success {
            // This executable owns only the simulator smoke loop. A successful
            // simulator action cannot manufacture Runtime receipt evidence;
            // the pending recovery remains unresolved until a unified Runtime
            // executor is wired into the runner.
            self.recovery_origin = None;
            self.retry_seq = 1;
        }
    }

    fn on_event(&mut self, event: &WorldEvent) {
        self.request_builder.on_event(event);
        self.inner.on_event(event);
    }

    fn take_decision_trace(&mut self) -> Option<AgentDecisionTrace> {
        self.inner.take_decision_trace()
    }
}

impl ProviderBackedLoopbackBehavior {
    fn current_recovery_lineage(&self) -> Option<RecoveryLineage> {
        self.current_recovery_lineage.clone()
    }

    fn note_recoverable_error(&mut self, lineage: RecoveryLineage) {
        // Multiple unresolved errors cannot be safely assigned to the next
        // action without an authority-backed receipt. Clear the candidate
        // rather than falling back to FIFO or another inferred association.
        match self.recovery_origin.as_ref() {
            None => self.recovery_origin = Some(lineage),
            Some(existing) if existing == &lineage => {}
            Some(_) => self.recovery_origin = None,
        }
    }
}

impl ProviderBackedLoopbackBehavior {
    fn target_context(
        &mut self,
        observation: &Observation,
    ) -> (
        ContinuousAgentTurnContextV1,
        ContinuousAgentRequestContextV1,
    ) {
        // The compatibility-only helper is used solely to reuse the canonical
        // provider request projection. The request is immediately wrapped in
        // the target outer V1 context before it reaches the real adapter.
        let _ = self.request_builder.decide(observation);
        let _ = self.request_builder.take_decision_trace();
        let request = self
            .request_builder_state
            .lock()
            .expect("parity request builder state lock")
            .recorded_requests
            .last()
            .cloned()
            .expect("request builder must record a provider request");
        let turn = self.next_turn;
        self.next_turn = self.next_turn.saturating_add(1);

        if let Some(origin) = self.recovery_origin.clone() {
            self.retry_seq = self.retry_seq.saturating_add(1).max(2);
            target_context::build_target_context_for_retry(
                request,
                observation,
                &self.fixture_id,
                &self.session_id,
                turn,
                self.retry_seq,
                &origin,
            )
        } else {
            self.retry_seq = 1;
            build_target_context(
                request,
                observation,
                &self.fixture_id,
                &self.session_id,
                turn,
            )
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let options = match parse_options(args.iter().skip(1).map(String::as_str)) {
        Ok(options) => options,
        Err(err) => {
            eprintln!("{err}");
            print_help();
            process::exit(1);
        }
    };

    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(options.scenario, &config);
    let (mut kernel, init_report) = match initialize_kernel(config, init) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("failed to initialize world: {err:?}");
            process::exit(1);
        }
    };
    let seed = init_report.seed.to_string();
    let fixture_id = options
        .fixture_id
        .clone()
        .unwrap_or_else(|| format!("{}-{}", options.scenario.as_str(), seed));

    let provider = match prepare_provider_info(&options) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("failed to prepare provider: {err}");
            process::exit(1);
        }
    };

    let raw_dir = options.out_dir.join("raw");
    let summary_dir = options.out_dir.join("summary");
    if let Err(err) = fs::create_dir_all(&raw_dir) {
        eprintln!("failed to create raw dir {}: {err}", raw_dir.display());
        process::exit(1);
    }
    if let Err(err) = fs::create_dir_all(&summary_dir) {
        eprintln!(
            "failed to create summary dir {}: {err}",
            summary_dir.display()
        );
        process::exit(1);
    }

    let raw_path = raw_dir.join(format!(
        "{}.{}.jsonl",
        sanitize_filename(fixture_id.as_str()),
        options.provider.summary_suffix()
    ));
    let summary_path = summary_dir.join(format!(
        "{}.{}.json",
        sanitize_filename(options.scenario_id.as_str()),
        options.provider.summary_suffix()
    ));

    let mut runner: AgentRunner<BenchBehavior> = AgentRunner::new();
    let mut agent_ids: Vec<String> = kernel.model().agents.keys().cloned().collect();
    agent_ids.sort();
    if agent_ids.is_empty() {
        eprintln!("no agents in scenario {}", options.scenario.as_str());
        process::exit(1);
    }

    for agent_id in &agent_ids {
        let behavior =
            match build_behavior(agent_id.as_str(), &options, fixture_id.as_str(), &provider) {
                Ok(value) => value,
                Err(err) => {
                    eprintln!("failed to build behavior for {agent_id}: {err}");
                    process::exit(1);
                }
            };
        runner.register(behavior);
    }

    let run_started_at = Instant::now();
    let mut step_records = Vec::new();
    let mut notes = Vec::new();
    let mut action_kind_counts = BTreeMap::new();
    let mut error_counts = BTreeMap::new();
    let mut invalid_action_count = 0_u64;
    let mut illegal_schema_count = 0_u64;
    let mut timeout_count = 0_u64;
    let mut recoverable_error_count = 0_u64;
    let mut fatal_error_count = 0_u64;
    let mut context_drift_count = 0_u64;
    let mut trace_present_count = 0_u64;
    let mut decision_steps = 0_u64;
    let mut latencies = Vec::new();
    let sample_id = fixture_id.clone();
    let mut recovery_ledger = RecoveryLedger::new(sample_id.clone());
    notes.push(format!(
        "runtime certification unavailable: {RUNTIME_CERTIFICATION_REASON}"
    ));

    for step_index in 1..=options.ticks {
        let Some(result) = runner.tick(&mut kernel) else {
            notes.push(format!("step {step_index}: runner returned no result"));
            continue;
        };
        decision_steps += 1;
        let recovery_lineage = runner
            .get_mut(result.agent_id.as_str())
            .and_then(|agent| agent.behavior.current_recovery_lineage());
        let action_ref = action_ref_from_decision(&result.decision);
        if let Some(action_ref) = action_ref.as_ref() {
            let entry = action_kind_counts.entry(action_ref.clone()).or_insert(0);
            *entry += 1;
        }

        let trace_present = result.decision_trace.is_some();
        if trace_present {
            trace_present_count += 1;
        }
        let latency_ms = result
            .decision_trace
            .as_ref()
            .and_then(|trace| trace.llm_diagnostics.as_ref())
            .and_then(|diagnostics| diagnostics.latency_ms);
        if let Some(latency_ms) = latency_ms {
            latencies.push(latency_ms);
        }

        let mut error_code = classify_trace_error(
            result.decision_trace.as_ref(),
            result.action_result.as_ref(),
        );
        if let Some(code) = error_code.as_ref() {
            let entry = error_counts.entry(code.clone()).or_insert(0);
            *entry += 1;
            match code.as_str() {
                "timeout" => {
                    timeout_count += 1;
                    recoverable_error_count += 1;
                }
                "provider_unreachable" | "invalid_action_schema" | "action_rejected" => {
                    recoverable_error_count += 1;
                    if code == "invalid_action_schema" {
                        illegal_schema_count += 1;
                    }
                }
                "context_drift" => {
                    context_drift_count += 1;
                }
                "session_cross_talk" => {
                    fatal_error_count += 1;
                }
                "trace_missing" => {}
                _ => {
                    fatal_error_count += 1;
                }
            }
            if matches!(
                code.as_str(),
                "timeout" | "provider_unreachable" | "invalid_action_schema" | "action_rejected"
            ) {
                let lineage = recovery_lineage
                    .clone()
                    .unwrap_or_else(|| unavailable_recovery_lineage(result.agent_id.as_str()));
                recovery_ledger.record_recoverable_error(RecoveryErrorEvidence {
                    error_code: code.clone(),
                    lineage: lineage.clone(),
                });
                if let Some(agent) = runner.get_mut(result.agent_id.as_str()) {
                    agent.behavior.note_recoverable_error(lineage);
                }
            }
        }

        let action_success = result.action_result.as_ref().map(|value| value.success);
        if matches!(action_success, Some(false)) {
            invalid_action_count += 1;
            if error_code.is_none() {
                let entry = error_counts
                    .entry("action_rejected".to_string())
                    .or_insert(0);
                *entry += 1;
                error_code = Some("action_rejected".to_string());
                recoverable_error_count += 1;
                let lineage = recovery_lineage
                    .clone()
                    .unwrap_or_else(|| unavailable_recovery_lineage(result.agent_id.as_str()));
                recovery_ledger.record_recoverable_error(RecoveryErrorEvidence {
                    error_code: "action_rejected".to_string(),
                    lineage: lineage.clone(),
                });
                if let Some(agent) = runner.get_mut(result.agent_id.as_str()) {
                    agent.behavior.note_recoverable_error(lineage);
                }
            }
        }

        if let Some(result_action) = result.action_result.as_ref()
            && let Some(reject_reason) = result_action.reject_reason()
        {
            notes.push(format!(
                "step {step_index}: action rejected for agent {} with {:?}",
                result.agent_id, reject_reason
            ));
        }

        step_records.push(StepTraceRecord {
            benchmark_run_id: options.benchmark_run_id.clone(),
            mode: options.execution_mode.as_str().to_string(),
            observation_schema_version: DEFAULT_PROVIDER_OBSERVATION_SCHEMA_VERSION.to_string(),
            action_schema_version: DEFAULT_PROVIDER_ACTION_SCHEMA_VERSION.to_string(),
            environment_class: execution_environment_class(options.execution_mode).to_string(),
            fallback_reason: provider.fallback_reason.clone(),
            parity_tier: options.parity_tier.clone(),
            scenario_id: options.scenario_id.clone(),
            fixture_id: fixture_id.clone(),
            provider_kind: options.provider.as_str().to_string(),
            provider_version: provider.provider_version.clone(),
            adapter_version: options.adapter_version.clone(),
            protocol_version: options.protocol_version.clone(),
            step_index,
            agent_id: result.agent_id.clone(),
            decision: decision_label(&result.decision),
            action_ref,
            latency_ms,
            error_code,
            retry_count: result
                .decision_trace
                .as_ref()
                .and_then(|trace| trace.llm_diagnostics.as_ref())
                .map(|diagnostics| diagnostics.retry_count)
                .unwrap_or(0),
            trace_present,
            trace_message_count: result
                .decision_trace
                .as_ref()
                .map(|trace| trace.llm_chat_messages.len())
                .unwrap_or(0),
            trace_tool_call_count: result
                .decision_trace
                .as_ref()
                .map(|trace| trace.llm_step_trace.len())
                .unwrap_or(0),
            context_drift_flag: false,
            action_success,
        });
    }

    let recovery_assessment = recovery_ledger.assess();
    for error in &recovery_assessment.errors {
        notes.push(format!("recovery ledger blocked: {error}"));
    }
    let goal_completed = scenario_goal_completed(
        options.scenario_id.as_str(),
        &action_kind_counts,
        &error_counts,
        invalid_action_count,
        &recovery_assessment.metric,
    );
    let mut status = derive_status(goal_completed, &error_counts, &notes);
    if recovery_assessment.trace_validity == recovery_ledger::TraceValidity::Blocked {
        status = "blocked".to_string();
    }
    let trace_completeness_ratio_ppm = ratio_ppm(trace_present_count, decision_steps);
    let illegal_schema_rate = metric_summary(illegal_schema_count, decision_steps);
    let summary = SampleSummary {
        benchmark_run_id: options.benchmark_run_id.clone(),
        mode: options.execution_mode.as_str().to_string(),
        observation_schema_version: DEFAULT_PROVIDER_OBSERVATION_SCHEMA_VERSION.to_string(),
        action_schema_version: DEFAULT_PROVIDER_ACTION_SCHEMA_VERSION.to_string(),
        environment_class: execution_environment_class(options.execution_mode).to_string(),
        fallback_reason: provider.fallback_reason.clone(),
        parity_tier: options.parity_tier.clone(),
        scenario_id: options.scenario_id.clone(),
        fixture_id,
        provider_kind: options.provider.as_str().to_string(),
        provider_version: provider.provider_version.clone(),
        adapter_version: options.adapter_version.clone(),
        protocol_version: options.protocol_version.clone(),
        scenario: options.scenario.as_str().to_string(),
        seed,
        status,
        execution_authority: LOCAL_EXECUTION_AUTHORITY.to_string(),
        runtime_certification_status: RUNTIME_CERTIFICATION_STATUS.to_string(),
        runtime_certification_reason: RUNTIME_CERTIFICATION_REASON.to_string(),
        goal_completed,
        completion_time_ms: run_started_at.elapsed().as_millis().min(u64::MAX as u128) as u64,
        decision_steps,
        invalid_action_count,
        illegal_schema_count,
        illegal_schema_rate,
        timeout_count,
        recoverable_error_count,
        fatal_error_count,
        metric_schema_version: RECOVERY_METRIC_SCHEMA_VERSION.to_string(),
        sample_id,
        trace_validity: recovery_assessment.trace_validity.as_str().to_string(),
        recovery_events: recovery_assessment.recovery_events,
        recoverable_error_resolution_rate: recovery_assessment.metric,
        trace_completeness_ratio_ppm,
        median_latency_ms: percentile_u64(&latencies, 50.0),
        p95_latency_ms: percentile_u64(&latencies, 95.0),
        context_drift_count,
        action_kind_counts,
        error_counts,
        fixture_refs: FixtureRefs {
            initial_world_snapshot_ref: format!(
                "scenario://{}/snapshot",
                options.scenario.as_str()
            ),
            observation_sequence_ref: format!(
                "scenario://{}/observations",
                options.scenario.as_str()
            ),
            goal_definition: format!("parity://{}/{}", options.parity_tier, options.scenario_id),
            action_catalog_ref: "catalog://provider/phase1".to_string(),
            player_context_ref: "player://default".to_string(),
            memory_fixture_ref: "memory://default".to_string(),
        },
        provider,
        notes,
        runtime_perf: runner.runtime_perf_snapshot(),
    };

    if let Err(err) = write_jsonl(raw_path.as_path(), &step_records) {
        eprintln!("failed to write raw trace jsonl: {err}");
        process::exit(1);
    }
    if let Err(err) = write_json(summary_path.as_path(), &summary) {
        eprintln!("failed to write summary json: {err}");
        process::exit(1);
    }

    println!("provider: {}", options.provider.as_str());
    println!("scenario: {}", options.scenario.as_str());
    println!("scenario_id: {}", options.scenario_id);
    println!("benchmark_run_id: {}", options.benchmark_run_id);
    println!("summary_json: {}", summary_path.display());
    println!("raw_jsonl: {}", raw_path.display());
    println!("status: {}", summary.status);
    println!(
        "goal_completed: {}",
        if summary.goal_completed { 1 } else { 0 }
    );
    println!("decision_steps: {}", summary.decision_steps);
    println!("invalid_action_count: {}", summary.invalid_action_count);
    println!(
        "illegal_schema_rate: {}/{} ({})",
        summary.illegal_schema_rate.numerator,
        summary.illegal_schema_rate.denominator,
        summary
            .illegal_schema_rate
            .value
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".to_string())
    );
    println!("timeout_count: {}", summary.timeout_count);
    println!(
        "trace_completeness_ratio_ppm: {}",
        summary.trace_completeness_ratio_ppm
    );
    println!("median_latency_ms: {}", summary.median_latency_ms);
    println!("p95_latency_ms: {}", summary.p95_latency_ms);
    println!(
        "recoverable_error_resolution_rate: {}/{} ({})",
        summary.recoverable_error_resolution_rate.numerator,
        summary.recoverable_error_resolution_rate.denominator,
        summary
            .recoverable_error_resolution_rate
            .value
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".to_string())
    );
    let exit_code = exit_code_for_status(summary.status.as_str());
    if exit_code != 0 {
        process::exit(exit_code);
    }
}

fn exit_code_for_status(status: &str) -> i32 {
    if status == "passed" { 0 } else { 1 }
}

fn prepare_provider_info(options: &CliOptions) -> Result<ProviderRunInfo, String> {
    match options.provider {
        BenchProviderKind::Builtin => Ok(ProviderRunInfo {
            provider_kind: options.provider.as_str().to_string(),
            provider_version: "builtin_llm_env".to_string(),
            adapter_version: options.adapter_version.clone(),
            protocol_version: options.protocol_version.clone(),
            compatibility_status: ProviderCompatibilityStatus::Ready.as_str().to_string(),
            fallback_reason: None,
            capabilities: provider_phase1_required_capabilities()
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            supported_action_sets: provider_phase1_required_actions()
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            provider_status: None,
            provider_last_error: None,
            provider_queue_depth: None,
            agent_profile: None,
            cognition_lane: "builtin_host_runner".to_string(),
            decision_route: "builtin".to_string(),
            feedback_route: "builtin".to_string(),
        }),
        BenchProviderKind::ProviderLoopbackHttp => {
            let base_url = options.provider_base_url.as_deref().ok_or_else(|| {
                "--agent-provider-url is required for provider_loopback_http".to_string()
            })?;
            let client = ProviderLoopbackHttpClient::new(
                base_url,
                options.provider_auth_token.as_deref(),
                options.agent_provider_connect_timeout_ms,
            )
            .map_err(|err| err.to_string())?;
            let info = client.provider_info().map_err(|err| err.to_string())?;
            let health = client.provider_health().map_err(|err| err.to_string())?;
            let compatibility = evaluate_provider_compatibility(&info, Some(&health));
            Ok(ProviderRunInfo {
                provider_kind: options.provider.as_str().to_string(),
                provider_version: info.version.unwrap_or_else(|| "unknown".to_string()),
                adapter_version: options.adapter_version.clone(),
                protocol_version: info
                    .protocol_version
                    .unwrap_or_else(|| options.protocol_version.clone()),
                compatibility_status: compatibility.status.as_str().to_string(),
                fallback_reason: compatibility.fallback_reason,
                capabilities: info.capabilities,
                supported_action_sets: info.supported_action_sets,
                provider_status: health.status,
                provider_last_error: health.last_error,
                provider_queue_depth: health.queue_depth,
                agent_profile: Some(options.agent_provider_profile.clone()),
                cognition_lane: "target_outer_context_v1".to_string(),
                decision_route: "/v1/world-simulator/decision-context".to_string(),
                feedback_route: "/v1/world-simulator/feedback-context".to_string(),
            })
        }
    }
}

fn build_behavior(
    agent_id: &str,
    options: &CliOptions,
    fixture_id: &str,
    provider: &ProviderRunInfo,
) -> Result<BenchBehavior, String> {
    match options.provider {
        BenchProviderKind::Builtin => {
            if options.execution_mode != ProviderExecutionMode::HeadlessAgent {
                return Err(
                    "--execution-mode=player_parity is only supported with --provider provider_loopback_http"
                        .to_string(),
                );
            }
            let mut behavior =
                LlmAgentBehavior::from_env(agent_id.to_string()).map_err(|err| err.to_string())?;
            if let Some(goal) = builtin_parity_short_term_goal(options.scenario_id.as_str()) {
                behavior.apply_prompt_overrides(None, Some(goal), None);
            }
            Ok(BenchBehavior::Builtin(BuiltinParityBehavior {
                inner: behavior,
                scenario_id: options.scenario_id.clone(),
                pending_trace: None,
            }))
        }
        BenchProviderKind::ProviderLoopbackHttp => {
            let base_url = options.provider_base_url.as_deref().ok_or_else(|| {
                "--agent-provider-url is required for provider_loopback_http".to_string()
            })?;
            let adapter = ProviderLoopbackAdapter::new(
                base_url,
                options.provider_auth_token.as_deref(),
                options.agent_provider_connect_timeout_ms,
            )
            .map_err(|err| err.to_string())?;
            let mut behavior = oasis7::simulator::ProviderBackedAgentBehavior::new(
                agent_id.to_string(),
                adapter,
                phase1_action_catalog(),
            )
            .require_continuous_request_context()
            .with_provider_config_ref(format!(
                "provider://loopback-http/parity/{}/{}",
                options.benchmark_run_id, agent_id
            ))
            .with_agent_profile(options.agent_provider_profile.clone())
            .with_execution_mode(options.execution_mode)
            .with_environment_class(execution_environment_class(options.execution_mode))
            .with_fixture_id(fixture_id)
            .with_replay_id(format!("{}:{}", options.benchmark_run_id, fixture_id));
            if let Some(fallback_reason) = provider.fallback_reason.as_deref() {
                behavior = behavior.with_fallback_reason(fallback_reason);
            }
            if let Some(memory_summary) = parity_memory_summary(options.scenario_id.as_str()) {
                behavior = behavior.with_memory_summary(memory_summary);
            }
            let request_builder_provider = MockDecisionProvider::new("parity-request-builder");
            let request_builder_state = request_builder_provider.shared_state();
            let mut request_builder =
                oasis7::simulator::ProviderBackedAgentBehavior::new_legacy_compatibility(
                    agent_id.to_string(),
                    request_builder_provider,
                    phase1_action_catalog(),
                )
                .with_provider_config_ref(format!(
                    "provider://loopback-http/parity/{}/{}",
                    options.benchmark_run_id, agent_id
                ))
                .with_agent_profile(options.agent_provider_profile.clone())
                .with_execution_mode(options.execution_mode)
                .with_environment_class(execution_environment_class(options.execution_mode))
                .with_fixture_id(fixture_id)
                .with_replay_id(format!("{}:{}", options.benchmark_run_id, fixture_id));
            if let Some(fallback_reason) = provider.fallback_reason.as_deref() {
                request_builder = request_builder.with_fallback_reason(fallback_reason);
            }
            if let Some(memory_summary) = parity_memory_summary(options.scenario_id.as_str()) {
                request_builder = request_builder.with_memory_summary(memory_summary);
            }
            let session_id = format!("parity-session-{}-{}", options.benchmark_run_id, fixture_id);
            Ok(BenchBehavior::ProviderBacked(
                ProviderBackedLoopbackBehavior {
                    inner: behavior,
                    request_builder,
                    request_builder_state,
                    fixture_id: fixture_id.to_string(),
                    session_id: session_id.clone(),
                    next_turn: 0,
                    retry_seq: 1,
                    current_recovery_lineage: None,
                    recovery_chain_id: target_context::recovery_chain_id(
                        fixture_id,
                        &session_id,
                        agent_id,
                    ),
                    recovery_origin: None,
                },
            ))
        }
    }
}

#[cfg(test)]
#[path = "oasis7_provider_parity_bench/tests.rs"]
mod tests;
