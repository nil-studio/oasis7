use super::*;
use std::time::Duration;

/// A simple test agent that moves toward a target location.
struct PatrolAgent {
    id: String,
    target_locations: Vec<String>,
    current_target_index: usize,
    action_results: Vec<bool>,
}

impl PatrolAgent {
    fn new(id: impl Into<String>, target_locations: Vec<String>) -> Self {
        Self {
            id: id.into(),
            target_locations,
            current_target_index: 0,
            action_results: Vec::new(),
        }
    }
}

impl AgentBehavior for PatrolAgent {
    fn agent_id(&self) -> &str {
        &self.id
    }

    fn decide(&mut self, observation: &Observation) -> AgentDecision {
        if self.target_locations.is_empty() {
            return AgentDecision::Wait;
        }

        let target_id = &self.target_locations[self.current_target_index];

        let current_location = observation
            .visible_locations
            .iter()
            .find(|loc| loc.distance_cm == 0);

        if let Some(current) = current_location
            && &current.location_id == target_id
        {
            self.current_target_index =
                (self.current_target_index + 1) % self.target_locations.len();
            let next_target = &self.target_locations[self.current_target_index];

            return AgentDecision::Act(Action::MoveAgent {
                agent_id: self.id.clone(),
                to: next_target.clone(),
            });
        }

        AgentDecision::Act(Action::MoveAgent {
            agent_id: self.id.clone(),
            to: target_id.clone(),
        })
    }

    fn on_action_result(&mut self, result: &ActionResult) {
        self.action_results.push(result.success);
    }
}

/// A simple agent that always waits.
struct WaitingAgent {
    id: String,
    wait_ticks: u64,
}

struct QuoteThenServiceAgent {
    id: String,
    query_results: Vec<AgentQueryResult>,
    action_results: Vec<ActionResult>,
}

impl QuoteThenServiceAgent {
    fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            query_results: Vec::new(),
            action_results: Vec::new(),
        }
    }
}

impl AgentBehavior for QuoteThenServiceAgent {
    fn agent_id(&self) -> &str {
        &self.id
    }

    fn decide(&mut self, _observation: &Observation) -> AgentDecision {
        if self.query_results.is_empty() {
            return AgentDecision::Query(AgentQuery::EvaluateMicroDepotQuote(
                MicroDepotQuoteRequest {
                    agent_id: self.id.clone(),
                    facility_id: "depot-measured".to_string(),
                    target_id: "loc-measured".to_string(),
                    action_kind: MicroDepotActionKind::Repair,
                    base_cost_class: MicroDepotPressureClass::Medium,
                    base_risk_class: MicroDepotPressureClass::Low,
                    blocker_type: Some("supply_missing".to_string()),
                },
            ));
        }

        AgentDecision::Act(Action::ServiceMicroDepotRepair {
            agent_id: self.id.clone(),
            facility_id: "depot-measured".to_string(),
            target_id: "loc-measured".to_string(),
            base_cost_class: MicroDepotPressureClass::Medium,
            base_risk_class: MicroDepotPressureClass::Low,
            blocker_type: Some("supply_missing".to_string()),
        })
    }

    fn on_query_result(&mut self, result: &AgentQueryResult) {
        self.query_results.push(result.clone());
    }

    fn on_action_result(&mut self, result: &ActionResult) {
        self.action_results.push(result.clone());
    }
}

impl WaitingAgent {
    fn new(id: impl Into<String>, wait_ticks: u64) -> Self {
        Self {
            id: id.into(),
            wait_ticks,
        }
    }
}

impl AgentBehavior for WaitingAgent {
    fn agent_id(&self) -> &str {
        &self.id
    }

    fn decide(&mut self, _observation: &Observation) -> AgentDecision {
        if self.wait_ticks > 0 {
            AgentDecision::WaitTicks(self.wait_ticks)
        } else {
            AgentDecision::Wait
        }
    }
}

struct TraceEffectAgent {
    id: String,
    emitted: bool,
}

impl TraceEffectAgent {
    fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            emitted: false,
        }
    }

    fn durable_action(&self) -> Action {
        Action::MoveAgent {
            agent_id: self.id.clone(),
            to: "loc-2".to_string(),
        }
    }
}

impl AgentBehavior for TraceEffectAgent {
    fn agent_id(&self) -> &str {
        &self.id
    }

    fn decide(&mut self, _observation: &Observation) -> AgentDecision {
        AgentDecision::Act(self.durable_action())
    }

    fn take_decision_trace(&mut self) -> Option<AgentDecisionTrace> {
        if self.emitted {
            return None;
        }
        self.emitted = true;
        Some(AgentDecisionTrace {
            agent_id: self.id.clone(),
            time: 1,
            decision: AgentDecision::Act(self.durable_action()),
            llm_input: Some("in".to_string()),
            llm_output: Some("out".to_string()),
            llm_error: None,
            parse_error: None,
            llm_diagnostics: None,
            llm_effect_intents: vec![LlmEffectIntentTrace {
                intent_id: "llm-intent-0".to_string(),
                kind: "llm.prompt.module_call".to_string(),
                params: serde_json::json!({
                    "module": "agent.modules.list",
                    "args": {},
                }),
                cap_ref: "llm.prompt.module_access".to_string(),
                origin: "llm_agent".to_string(),
            }],
            llm_effect_receipts: vec![LlmEffectReceiptTrace {
                intent_id: "llm-intent-0".to_string(),
                status: "ok".to_string(),
                payload: serde_json::json!({
                    "ok": true,
                }),
                cost_cents: None,
            }],
            llm_step_trace: vec![],
            llm_prompt_section_trace: vec![],
            llm_chat_messages: vec![],
        })
    }
}

struct LlmLatencyTraceAgent {
    id: String,
    latency_ms: u64,
    decide_sleep_ms: u64,
}

impl LlmLatencyTraceAgent {
    fn new(id: impl Into<String>, latency_ms: u64, decide_sleep_ms: u64) -> Self {
        Self {
            id: id.into(),
            latency_ms,
            decide_sleep_ms,
        }
    }
}

impl AgentBehavior for LlmLatencyTraceAgent {
    fn agent_id(&self) -> &str {
        &self.id
    }

    fn decide(&mut self, _observation: &Observation) -> AgentDecision {
        std::thread::sleep(Duration::from_millis(self.decide_sleep_ms));
        AgentDecision::Wait
    }

    fn take_decision_trace(&mut self) -> Option<AgentDecisionTrace> {
        Some(AgentDecisionTrace {
            agent_id: self.id.clone(),
            time: 1,
            decision: AgentDecision::Wait,
            llm_input: Some("in".to_string()),
            llm_output: Some("out".to_string()),
            llm_error: None,
            parse_error: None,
            llm_diagnostics: Some(LlmDecisionDiagnostics {
                model: Some("mock-llm".to_string()),
                latency_ms: Some(self.latency_ms),
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
                retry_count: 0,
                ..LlmDecisionDiagnostics::default()
            }),
            llm_effect_intents: vec![],
            llm_effect_receipts: vec![],
            llm_step_trace: vec![],
            llm_prompt_section_trace: vec![],
            llm_chat_messages: vec![],
        })
    }
}

fn setup_kernel_with_patrol_agent(agent_id: &str) -> WorldKernel {
    let config = WorldConfig {
        visibility_range_cm: DEFAULT_VISIBILITY_RANGE_CM,
        move_cost_per_km_electricity: 0,
        ..Default::default()
    };
    let mut kernel = WorldKernel::with_config(config);

    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-2".to_string(),
        name: "outpost".to_string(),
        pos: pos(1, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: agent_id.to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.step_until_empty();

    kernel
}

fn setup_kernel_with_conflict_agents() -> WorldKernel {
    let config = WorldConfig {
        visibility_range_cm: DEFAULT_VISIBILITY_RANGE_CM,
        move_cost_per_km_electricity: 0,
        ..Default::default()
    };
    let mut kernel = WorldKernel::with_config(config);
    for (id, x) in [("loc-1", 0), ("loc-2", 1), ("loc-3", 2)] {
        kernel.submit_action(Action::RegisterLocation {
            location_id: id.to_string(),
            name: id.to_string(),
            pos: pos(x, 0),
            profile: LocationProfile::default(),
        });
    }
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-a".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-b".to_string(),
        location_id: "loc-2".to_string(),
    });
    kernel.step_until_empty();
    kernel
}

fn setup_kernel_with_wait_agent(agent_id: &str) -> WorldKernel {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: agent_id.to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.step_until_empty();
    kernel
}

#[test]
fn agent_decision_helpers() {
    let wait = AgentDecision::Wait;
    assert!(!wait.is_act());
    assert!(wait.action().is_none());

    let wait_ticks = AgentDecision::WaitTicks(5);
    assert!(!wait_ticks.is_act());
    assert!(wait_ticks.action().is_none());

    let action = Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: "loc-2".to_string(),
    };
    let act = AgentDecision::Act(action.clone());
    assert!(act.is_act());
    assert_eq!(act.action(), Some(&action));
}

#[test]
fn action_result_helpers() {
    let action = Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: "loc-2".to_string(),
    };

    let success_event = WorldEvent {
        id: 1,
        time: 1,
        kind: WorldEventKind::AgentMoved {
            agent_id: "agent-1".to_string(),
            from: "loc-1".to_string(),
            to: "loc-2".to_string(),
            distance_cm: 1000,
            electricity_cost: 1,
        },
        runtime_event: None,
    };
    let success_result = ActionResult {
        action: action.clone(),
        action_id: 1,
        success: true,
        event: success_event,
    };
    assert!(!success_result.is_rejected());
    assert!(success_result.reject_reason().is_none());

    let reject_event = WorldEvent {
        id: 2,
        time: 2,
        kind: WorldEventKind::ActionRejected {
            reason: RejectReason::AgentNotFound {
                agent_id: "agent-1".to_string(),
            },
        },
        runtime_event: None,
    };
    let reject_result = ActionResult {
        action,
        action_id: 2,
        success: false,
        event: reject_event,
    };
    assert!(reject_result.is_rejected());
    assert!(matches!(
        reject_result.reject_reason(),
        Some(RejectReason::AgentNotFound { .. })
    ));
}

#[test]
fn agent_runner_register_and_tick() {
    let mut kernel = setup_kernel_with_patrol_agent("patrol-1");
    let mut runner: AgentRunner<PatrolAgent> = AgentRunner::new();
    let patrol_agent = PatrolAgent::new("patrol-1", vec!["loc-1".to_string(), "loc-2".to_string()]);
    runner.register(patrol_agent);

    assert_eq!(runner.agent_count(), 1);
    assert_eq!(runner.agent_ids(), vec!["patrol-1".to_string()]);

    let result = runner.tick(&mut kernel);
    assert!(result.is_some());
    let result = result.unwrap();
    assert_eq!(result.agent_id, "patrol-1");
    assert!(result.has_action());
    assert!(result.is_success());

    let agent = kernel.model().agents.get("patrol-1").unwrap();
    assert_eq!(agent.location_id, "loc-2");

    let registered = runner.get("patrol-1").unwrap();
    assert_eq!(registered.action_count, 1);
    assert_eq!(registered.decision_count, 1);
    assert_eq!(registered.behavior.action_results.len(), 1);
    assert!(registered.behavior.action_results[0]);
}

#[test]
fn agent_runner_tick_decide_only_defers_world_mutation_until_notified() {
    let mut kernel = setup_kernel_with_patrol_agent("patrol-1");
    let initial_location = kernel
        .model()
        .agents
        .get("patrol-1")
        .expect("agent exists")
        .location_id
        .clone();

    let mut runner: AgentRunner<PatrolAgent> = AgentRunner::new();
    runner.register(PatrolAgent::new(
        "patrol-1",
        vec!["loc-1".to_string(), "loc-2".to_string()],
    ));

    let tick = runner.tick_decide_only(&mut kernel).expect("tick result");
    assert!(matches!(tick.decision, AgentDecision::Act(_)));
    assert!(tick.action_result.is_none());

    let location_after_decide = kernel
        .model()
        .agents
        .get("patrol-1")
        .expect("agent exists")
        .location_id
        .clone();
    assert_eq!(location_after_decide, initial_location);
    assert!(
        runner
            .get("patrol-1")
            .expect("registered")
            .behavior
            .action_results
            .is_empty()
    );

    let action = tick.decision.action().expect("decision action").clone();
    let action_id = kernel.submit_action_from_agent("patrol-1", action.clone());
    let event = kernel.step().expect("event");
    let success = !matches!(event.kind, WorldEventKind::ActionRejected { .. });
    let action_result = ActionResult {
        action,
        action_id,
        success,
        event,
    };
    assert!(runner.notify_action_result("patrol-1", &action_result));
    assert_eq!(
        runner
            .get("patrol-1")
            .expect("registered")
            .behavior
            .action_results,
        vec![true]
    );
}

#[test]
fn agent_runner_query_returns_preview_without_durable_action_side_effects_then_confirms_service() {
    let mut kernel =
        super::micro_depot_measured_supply::installed_micro_depot_with_measured_supply(5, 4, 2);
    let mut control =
        super::micro_depot_measured_supply::installed_micro_depot_with_measured_supply(5, 4, 2);
    kernel
        .observe("agent-measured")
        .expect("warm quote-agent observation");
    control
        .observe("agent-measured")
        .expect("warm control observation");
    let model_before_query = kernel.model().clone();
    let time_before_query = kernel.time();
    let journal_before_query = kernel.journal_snapshot();

    let mut runner = AgentRunner::with_rate_limit(RateLimitPolicy::new(1, 10));
    runner.register(QuoteThenServiceAgent::new("agent-measured"));

    let quote_tick = runner.tick(&mut kernel).expect("quote decision result");
    assert!(matches!(quote_tick.decision, AgentDecision::Query(_)));
    assert!(quote_tick.action_result.is_none());
    assert!(quote_tick.query_result.is_some());
    assert_eq!(kernel.pending_actions(), 0);
    assert_eq!(kernel.time(), time_before_query);
    assert_eq!(kernel.journal_snapshot(), journal_before_query);
    assert_eq!(kernel.model(), &model_before_query);
    assert_eq!(
        runner
            .get("agent-measured")
            .expect("registered")
            .action_count,
        0,
        "query must not count against the durable action quota"
    );
    assert!(
        !runner.is_rate_limited("agent-measured", kernel.time()),
        "query must not consume the durable action rate limit"
    );
    let quoted = runner.get("agent-measured").expect("registered");
    assert_eq!(quoted.behavior.query_results.len(), 1);
    assert!(quoted.behavior.action_results.is_empty());
    assert!(
        serde_json::to_string(&quoted.behavior.query_results[0])
            .expect("serialize structured quote result")
            .contains("resource_debits"),
        "the next decision context must receive the structured quote preview"
    );

    let control_action_id = control.submit_action_from_agent(
        "agent-measured",
        Action::ServiceMicroDepotRepair {
            agent_id: "agent-measured".to_string(),
            facility_id: "depot-measured".to_string(),
            target_id: "loc-measured".to_string(),
            base_cost_class: MicroDepotPressureClass::Medium,
            base_risk_class: MicroDepotPressureClass::Low,
            blocker_type: Some("supply_missing".to_string()),
        },
    );
    let control_event = control.step().expect("control service");

    let service_tick = runner.tick(&mut kernel).expect("service decision result");
    let service_result = service_tick.action_result.expect("service action result");
    assert_eq!(service_result.action_id, control_action_id);
    assert_eq!(service_result.event, control_event);
    assert!(service_result.success);
    let behavior = &runner.get("agent-measured").expect("registered").behavior;
    assert_eq!(behavior.query_results.len(), 1);
    assert_eq!(behavior.action_results.len(), 1);
}

#[test]
fn agent_runner_decide_only_query_is_read_only_and_not_an_action_result() {
    let mut kernel =
        super::micro_depot_measured_supply::installed_micro_depot_with_measured_supply(5, 4, 2);
    kernel
        .observe("agent-measured")
        .expect("warm decide-only quote-agent observation");
    let model_before_query = kernel.model().clone();
    let time_before_query = kernel.time();
    let journal_before_query = kernel.journal_snapshot();
    let mut runner: AgentRunner<QuoteThenServiceAgent> = AgentRunner::new();
    runner.register(QuoteThenServiceAgent::new("agent-measured"));

    let tick = runner
        .tick_decide_only(&mut kernel)
        .expect("query decision");
    assert!(matches!(tick.decision, AgentDecision::Query(_)));
    assert!(tick.action_result.is_none());
    assert!(tick.query_result.is_some());
    assert_eq!(kernel.pending_actions(), 0);
    assert_eq!(kernel.time(), time_before_query);
    assert_eq!(kernel.journal_snapshot(), journal_before_query);
    assert_eq!(kernel.model(), &model_before_query);
    let registered = runner.get("agent-measured").expect("registered");
    assert_eq!(registered.action_count, 0);
    assert_eq!(registered.behavior.query_results.len(), 1);
    assert!(registered.behavior.action_results.is_empty());
}

#[test]
fn agent_runner_batch_query_uses_precommit_snapshot_and_excludes_query_from_durable_intents() {
    let mut kernel =
        super::micro_depot_measured_supply::installed_micro_depot_with_measured_supply(5, 4, 2);
    let time_before_batch = kernel.time();
    let journal_before_batch = kernel.journal_snapshot();
    let mut runner: AgentRunner<QuoteThenServiceAgent> = AgentRunner::new();
    runner.register(QuoteThenServiceAgent::new("agent-measured"));

    let results = runner.tick_collect_intents_and_commit(&mut kernel, 2);
    assert_eq!(results.len(), 2);
    assert!(matches!(results[0].decision, AgentDecision::Query(_)));
    assert!(results[0].action_result.is_none());
    assert!(results[0].query_result.is_some());
    assert!(matches!(results[1].decision, AgentDecision::Act(_)));
    assert!(results[1].action_result.is_some());
    let report = kernel
        .last_intent_batch_report()
        .expect("one durable service intent committed");
    assert_eq!(
        report.intent_count, 1,
        "query must not become a durable intent"
    );
    assert_eq!(kernel.time(), time_before_batch.saturating_add(1));
    assert_eq!(
        kernel.journal_snapshot().events.len(),
        journal_before_batch.events.len() + 1,
        "only the confirmed service may append history"
    );
    let behavior = &runner.get("agent-measured").expect("registered").behavior;
    assert_eq!(behavior.query_results.len(), 1);
    let query_json = serde_json::to_string(&behavior.query_results[0])
        .expect("serialize pre-commit quote result");
    assert!(query_json.contains("available_units_by_kind"));
    assert!(
        query_json.contains("5"),
        "query must observe stock before service debit"
    );
    assert_eq!(behavior.action_results.len(), 1);
}

#[test]
fn agent_runner_batch_intents_conflict_is_deterministic_and_explainable() {
    let mut kernel = setup_kernel_with_conflict_agents();
    let mut runner: AgentRunner<PatrolAgent> = AgentRunner::new();
    runner.register(PatrolAgent::new("agent-a", vec!["loc-3".to_string()]));
    runner.register(PatrolAgent::new("agent-b", vec!["loc-3".to_string()]));

    let results = runner.tick_collect_intents_and_commit(&mut kernel, 2);
    assert_eq!(results.len(), 2);

    let report = kernel
        .last_intent_batch_report()
        .expect("intent batch report exists");
    assert_eq!(report.intent_count, 2);
    assert_eq!(report.conflicts.len(), 1);
    assert_eq!(report.conflicts[0].conflict_key, "move_to:loc-3");
    assert_eq!(
        report.conflicts[0].winner_action_id,
        report.accepted_action_ids[0]
    );
    assert_eq!(
        report.conflicts[0].loser_action_ids,
        report.rejected_action_ids
    );

    let success_agents: Vec<_> = results
        .iter()
        .filter(|result| result.is_success())
        .map(|result| result.agent_id.clone())
        .collect();
    let failed_agents: Vec<_> = results
        .iter()
        .filter(|result| {
            result
                .action_result
                .as_ref()
                .is_some_and(|r| r.is_rejected())
        })
        .map(|result| result.agent_id.clone())
        .collect();
    assert_eq!(success_agents, vec!["agent-a".to_string()]);
    assert_eq!(failed_agents, vec!["agent-b".to_string()]);

    let rejected = results
        .iter()
        .find(|result| result.agent_id == "agent-b")
        .and_then(|result| result.action_result.as_ref())
        .expect("agent-b has rejected action");
    let reject_reason = rejected.reject_reason().expect("reject reason");
    match reject_reason {
        RejectReason::RuleDenied { notes } => {
            assert!(
                notes
                    .iter()
                    .any(|note| note.contains("intent conflict on key=move_to:loc-3")),
                "missing conflict reason notes: {notes:?}"
            );
        }
        other => panic!("unexpected reject reason: {other:?}"),
    }

    // Replay with same intents should produce the same winner.
    let mut kernel_replay = setup_kernel_with_conflict_agents();
    let mut runner_replay: AgentRunner<PatrolAgent> = AgentRunner::new();
    runner_replay.register(PatrolAgent::new("agent-a", vec!["loc-3".to_string()]));
    runner_replay.register(PatrolAgent::new("agent-b", vec!["loc-3".to_string()]));
    let replay_results = runner_replay.tick_collect_intents_and_commit(&mut kernel_replay, 2);
    let replay_success_agents: Vec<_> = replay_results
        .iter()
        .filter(|result| result.is_success())
        .map(|result| result.agent_id.clone())
        .collect();
    assert_eq!(replay_success_agents, vec!["agent-a".to_string()]);
}

#[test]
fn observation_intel_ttl_uses_cached_view_until_expired() {
    let mut kernel = setup_kernel_with_conflict_agents();
    kernel.set_intel_ttl_ticks(2);

    let initial = kernel.observe("agent-a").expect("initial observation");
    let initial_b_location = initial
        .visible_agents
        .iter()
        .find(|agent| agent.agent_id == "agent-b")
        .map(|agent| agent.location_id.clone())
        .expect("agent-b visible");
    assert_eq!(initial_b_location, "loc-2");

    kernel.submit_action(Action::MoveAgent {
        agent_id: "agent-b".to_string(),
        to: "loc-3".to_string(),
    });
    kernel.step().expect("move agent-b");

    let cached = kernel.observe("agent-a").expect("cached observation");
    let cached_b_location = cached
        .visible_agents
        .iter()
        .find(|agent| agent.agent_id == "agent-b")
        .map(|agent| agent.location_id.clone())
        .expect("agent-b visible");
    assert_eq!(cached_b_location, "loc-2");

    kernel.submit_action(Action::MoveAgent {
        agent_id: "agent-a".to_string(),
        to: "loc-1".to_string(),
    });
    kernel.step().expect("advance tick");

    let refreshed = kernel.observe("agent-a").expect("refreshed observation");
    let refreshed_b_location = refreshed
        .visible_agents
        .iter()
        .find(|agent| agent.agent_id == "agent-b")
        .map(|agent| agent.location_id.clone())
        .expect("agent-b visible");
    assert_eq!(refreshed_b_location, "loc-3");
}

#[test]
fn agent_runner_round_robin() {
    let config = WorldConfig {
        visibility_range_cm: DEFAULT_VISIBILITY_RANGE_CM,
        move_cost_per_km_electricity: 0,
        ..Default::default()
    };
    let mut kernel = WorldKernel::with_config(config);
    for idx in 0..3 {
        kernel.submit_action(Action::RegisterLocation {
            location_id: format!("loc-{idx}"),
            name: format!("loc-{idx}"),
            pos: pos(idx as i64, 0),
            profile: LocationProfile::default(),
        });
        kernel.submit_action(Action::RegisterAgent {
            agent_id: format!("agent-{idx}"),
            location_id: format!("loc-{idx}"),
        });
    }
    kernel.step_until_empty();

    let mut runner: AgentRunner<PatrolAgent> = AgentRunner::new();
    for idx in 0..3 {
        let agent = PatrolAgent::new(format!("agent-{idx}"), vec![format!("loc-{idx}")]);
        runner.register(agent);
    }

    let mut seen = Vec::new();
    for _ in 0..3 {
        let tick = runner.tick(&mut kernel).unwrap();
        seen.push(tick.agent_id);
    }

    assert_eq!(seen, vec!["agent-0", "agent-1", "agent-2"]);
}

#[test]
fn agent_runner_round_robin_skips_not_ready_and_wraps() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    for idx in 0..3 {
        kernel.submit_action(Action::RegisterAgent {
            agent_id: format!("agent-{idx}"),
            location_id: "loc-1".to_string(),
        });
    }
    kernel.step_until_empty();

    let mut runner: AgentRunner<WaitingAgent> = AgentRunner::new();
    for idx in 0..3 {
        runner.register(WaitingAgent::new(format!("agent-{idx}"), 0));
    }
    runner.get_mut("agent-1").unwrap().wait_until = Some(kernel.time().saturating_add(10));

    let mut seen = Vec::new();
    for _ in 0..3 {
        let tick = runner.tick_decide_only(&mut kernel).unwrap();
        seen.push(tick.agent_id);
    }

    assert_eq!(seen, vec!["agent-0", "agent-2", "agent-0"]);
}

#[test]
fn agent_runner_wait_ticks_sets_wait_until() {
    let mut kernel = setup_kernel_with_wait_agent("agent-1");
    let mut runner: AgentRunner<WaitingAgent> = AgentRunner::new();
    runner.register(WaitingAgent::new("agent-1", 2));

    let now = kernel.time();
    let tick = runner.tick(&mut kernel).unwrap();
    assert!(matches!(tick.decision, AgentDecision::WaitTicks(2)));
    assert!(tick.action_result.is_none());

    let registered = runner.get("agent-1").unwrap();
    assert_eq!(registered.wait_until, Some(now.saturating_add(2)));
}

#[test]
fn agent_runner_run_multiple_ticks() {
    let mut kernel = setup_kernel_with_patrol_agent("patrol-1");
    let mut runner: AgentRunner<PatrolAgent> = AgentRunner::new();
    let patrol_agent = PatrolAgent::new("patrol-1", vec!["loc-1".to_string(), "loc-2".to_string()]);
    runner.register(patrol_agent);

    for _ in 0..3 {
        runner.tick(&mut kernel).unwrap();
    }

    let agent = kernel.model().agents.get("patrol-1").unwrap();
    assert!(agent.location_id == "loc-1" || agent.location_id == "loc-2");
}

#[test]
fn agent_runner_unregister() {
    let mut runner: AgentRunner<WaitingAgent> = AgentRunner::new();
    runner.register(WaitingAgent::new("agent-1", 0));
    assert_eq!(runner.agent_count(), 1);

    runner.unregister("agent-1");
    assert_eq!(runner.agent_count(), 0);
}

#[test]
fn registered_agent_is_ready() {
    let mut runner: AgentRunner<WaitingAgent> = AgentRunner::new();
    runner.register(WaitingAgent::new("agent-1", 0));
    let registered = runner.get("agent-1").unwrap();
    assert!(registered.is_ready(1));
}

#[test]
fn agent_quota_max_actions() {
    let mut kernel = setup_kernel_with_patrol_agent("patrol-1");
    let mut runner: AgentRunner<PatrolAgent> = AgentRunner::new();
    runner.set_default_quota(Some(AgentQuota::max_actions(1)));
    runner.register(PatrolAgent::new(
        "patrol-1",
        vec!["loc-1".to_string(), "loc-2".to_string()],
    ));

    let tick1 = runner.tick(&mut kernel).unwrap();
    assert!(tick1.has_action());
    assert!(runner.tick(&mut kernel).is_none());
}

#[test]
fn agent_quota_max_decisions() {
    let quota = AgentQuota::max_decisions(1);
    let mut runner: AgentRunner<WaitingAgent> = AgentRunner::new();
    runner.set_default_quota(Some(quota));
    runner.register(WaitingAgent::new("agent-1", 0));
    let mut kernel = setup_kernel_with_wait_agent("agent-1");

    let tick1 = runner.tick(&mut kernel).unwrap();
    assert!(matches!(tick1.decision, AgentDecision::Wait));
    assert!(runner.tick(&mut kernel).is_none());
}

#[test]
fn agent_quota_both_limits() {
    let mut kernel = setup_kernel_with_patrol_agent("patrol-1");
    let mut runner: AgentRunner<PatrolAgent> = AgentRunner::new();
    runner.set_default_quota(Some(AgentQuota::new(Some(1), Some(1))));
    runner.register(PatrolAgent::new(
        "patrol-1",
        vec!["loc-1".to_string(), "loc-2".to_string()],
    ));

    let tick1 = runner.tick(&mut kernel).unwrap();
    assert!(tick1.has_action());
    assert!(runner.tick(&mut kernel).is_none());
}

#[test]
fn rate_limit_policy_actions_per_tick() {
    let policy = RateLimitPolicy::new(1, 2);
    let mut runner: AgentRunner<PatrolAgent> = AgentRunner::new();
    runner.set_rate_limit(Some(policy));
    runner.register(PatrolAgent::new(
        "patrol-1",
        vec!["loc-1".to_string(), "loc-2".to_string()],
    ));
    let mut kernel = setup_kernel_with_patrol_agent("patrol-1");

    let tick1 = runner.tick(&mut kernel).unwrap();
    assert!(tick1.has_action());
    assert!(runner.is_rate_limited("patrol-1", kernel.time()));

    runner.reset_rate_limit("patrol-1");
    assert!(!runner.is_rate_limited("patrol-1", kernel.time()));
}

#[test]
fn rate_limit_state_basic() {
    let policy = RateLimitPolicy::new(1, 2);
    let mut state = RateLimitState::default();

    assert!(!state.is_limited(0, &policy));
    state.record_action(0, &policy);
    assert!(state.is_limited(0, &policy));
    assert!(state.is_limited(1, &policy));
    assert!(!state.is_limited(2, &policy));
}

#[test]
fn rate_limit_state_reset() {
    let policy = RateLimitPolicy::new(1, 2);
    let mut state = RateLimitState::default();
    state.record_action(0, &policy);
    assert!(state.is_limited(1, &policy));

    state.reset();
    assert!(!state.is_limited(1, &policy));
}

#[test]
fn agent_runner_per_agent_quota() {
    let mut kernel = setup_kernel_with_wait_agent("agent-1");
    let mut runner: AgentRunner<WaitingAgent> = AgentRunner::new();
    runner.register_with_quota(
        WaitingAgent::new("agent-1", 0),
        AgentQuota::max_decisions(1),
    );

    let tick1 = runner.tick(&mut kernel).unwrap();
    assert!(matches!(tick1.decision, AgentDecision::Wait));
    assert!(runner.tick(&mut kernel).is_none());
}

#[test]
fn runner_metrics_basic() {
    let mut kernel = setup_kernel_with_patrol_agent("patrol-1");
    let mut runner: AgentRunner<PatrolAgent> = AgentRunner::new();
    runner.register(PatrolAgent::new(
        "patrol-1",
        vec!["loc-1".to_string(), "loc-2".to_string()],
    ));

    runner.tick(&mut kernel);
    runner.tick(&mut kernel);

    let metrics = runner.metrics();
    assert_eq!(metrics.total_ticks, 2);
    assert_eq!(metrics.total_agents, 1);
    assert_eq!(metrics.total_actions, 2);
    assert_eq!(metrics.total_decisions, 2);
}

#[test]
fn runner_metrics_with_quota() {
    let mut kernel = setup_kernel_with_patrol_agent("patrol-1");
    let mut runner: AgentRunner<PatrolAgent> = AgentRunner::new();
    runner.set_default_quota(Some(AgentQuota::max_actions(1)));
    runner.register(PatrolAgent::new(
        "patrol-1",
        vec!["loc-1".to_string(), "loc-2".to_string()],
    ));

    runner.tick(&mut kernel);
    runner.tick(&mut kernel);

    let metrics = runner.metrics();
    assert_eq!(metrics.total_ticks, 2);
    assert_eq!(metrics.total_actions, 1);
    assert_eq!(metrics.agents_quota_exhausted, 1);
}

#[test]
fn runner_agent_stats() {
    let mut kernel = setup_kernel_with_wait_agent("agent-1");
    let mut runner: AgentRunner<WaitingAgent> = AgentRunner::new();
    runner.register(WaitingAgent::new("agent-1", 0));

    runner.tick(&mut kernel);

    let stats = runner.agent_stats();
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].agent_id, "agent-1");
    assert_eq!(stats[0].action_count, 0);
    assert_eq!(stats[0].decision_count, 1);
    assert!(!stats[0].is_quota_exhausted);
    assert_eq!(stats[0].wait_until, None);
}

#[test]
fn runner_log_entry_serialization() {
    let entry = RunnerLogEntry {
        tick: 1,
        time: 1,
        kind: RunnerLogKind::ActionExecuted {
            agent_id: "agent-1".to_string(),
            action: Action::MoveAgent {
                agent_id: "agent-1".to_string(),
                to: "loc-1".to_string(),
            },
            success: true,
        },
    };
    let serialized = serde_json::to_string(&entry).unwrap();
    assert!(serialized.contains("agent-1"));
}

#[test]
fn runner_metrics_default() {
    let metrics = RunnerMetrics::default();
    assert_eq!(metrics.total_ticks, 0);
    assert_eq!(metrics.total_agents, 0);
    assert_eq!(metrics.total_actions, 0);
    assert_eq!(metrics.total_decisions, 0);
    assert_eq!(metrics.runtime_perf.health, RuntimePerfHealth::Unknown);
}

#[test]
fn runner_runtime_perf_snapshot_tracks_tick_and_decision_samples() {
    let mut kernel = setup_kernel_with_wait_agent("agent-1");
    let mut runner: AgentRunner<LlmLatencyTraceAgent> = AgentRunner::new();
    runner.register(LlmLatencyTraceAgent::new("agent-1", 2, 6));

    let _ = runner.tick(&mut kernel).expect("tick result");
    let perf = runner.runtime_perf_snapshot();
    assert!(perf.tick.samples_total >= 1);
    assert!(perf.decision.samples_total >= 1);
    assert_ne!(perf.health, RuntimePerfHealth::Unknown);
}

#[test]
fn runner_external_action_execution_duration_updates_perf_snapshot() {
    let mut runner: AgentRunner<WaitingAgent> = AgentRunner::new();
    runner.record_external_action_execution_duration(Duration::from_micros(25_000));

    let perf = runner.runtime_perf_snapshot();
    assert_eq!(perf.action_execution.samples_total, 1);
    assert!(perf.action_execution.p95_ms >= 25.0);
    assert_eq!(perf.bottleneck, RuntimePerfBottleneck::ActionExecution);
}

#[test]
fn runner_runtime_perf_separates_llm_api_from_local_execution() {
    let mut kernel = setup_kernel_with_wait_agent("agent-1");
    let mut runner: AgentRunner<LlmLatencyTraceAgent> = AgentRunner::new();
    runner.register(LlmLatencyTraceAgent::new("agent-1", 200, 220));

    let _ = runner.tick_decide_only(&mut kernel).expect("tick result");
    let perf = runner.runtime_perf_snapshot();
    assert_eq!(perf.llm_api.samples_total, 1);
    assert!(perf.llm_api.p95_ms >= 200.0);
    assert!(perf.decision.samples_total >= 1);
    assert!(perf.tick.samples_total >= 1);
    assert!(perf.decision.p95_ms < perf.llm_api.p95_ms);
    assert!(perf.tick.p95_ms < perf.llm_api.p95_ms);
}

#[test]
fn runner_persists_llm_effect_trace_to_kernel_journal() {
    let mut kernel = setup_kernel_with_patrol_agent("agent-1");
    let mut runner: AgentRunner<TraceEffectAgent> = AgentRunner::new();
    runner.register(TraceEffectAgent::new("agent-1"));

    let _ = runner.tick(&mut kernel).expect("tick result");

    assert_llm_effect_trace_journaled(&kernel, "agent-1");
}

#[test]
fn runner_decide_only_persists_llm_effect_trace_to_kernel_journal() {
    let mut kernel = setup_kernel_with_patrol_agent("agent-1");
    let mut runner: AgentRunner<TraceEffectAgent> = AgentRunner::new();
    runner.register(TraceEffectAgent::new("agent-1"));

    let _ = runner.tick_decide_only(&mut kernel).expect("tick result");

    assert_llm_effect_trace_journaled(&kernel, "agent-1");
}

#[test]
fn runner_batch_collect_persists_llm_effect_trace_to_kernel_journal() {
    let mut kernel = setup_kernel_with_patrol_agent("agent-1");
    let mut runner: AgentRunner<TraceEffectAgent> = AgentRunner::new();
    runner.register(TraceEffectAgent::new("agent-1"));

    let results = runner.tick_collect_intents_and_commit(&mut kernel, 1);
    assert_eq!(results.len(), 1);

    assert_llm_effect_trace_journaled(&kernel, "agent-1");
}

fn assert_llm_effect_trace_journaled(kernel: &WorldKernel, expected_agent_id: &str) {
    let mut has_intent = false;
    let mut has_receipt = false;
    for event in kernel.journal() {
        match &event.kind {
            WorldEventKind::LlmEffectQueued { agent_id, intent } => {
                has_intent = true;
                assert_eq!(agent_id, expected_agent_id);
                assert_eq!(intent.intent_id, "llm-intent-0");
            }
            WorldEventKind::LlmReceiptAppended { agent_id, receipt } => {
                has_receipt = true;
                assert_eq!(agent_id, expected_agent_id);
                assert_eq!(receipt.intent_id, "llm-intent-0");
            }
            _ => {}
        }
    }

    assert!(has_intent);
    assert!(has_receipt);
}
