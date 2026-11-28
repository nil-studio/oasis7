use super::kernel_wasm_sandbox_bridge::DynamicMicroDepotSandbox;
use super::*;
use oasis7_wasm_abi::ModuleLimits;
use std::sync::{Arc, Mutex};

struct TracedQuoteAgent {
    id: String,
    trace_emitted: bool,
    query_results: Vec<AgentQueryResult>,
}

/// A test-only agent that requests a prospective install quote. The query must
/// remain distinct from an accepted `InstallMicroDepot` intent.
struct TracedInstallQuoteAgent {
    id: String,
    action: Action,
    query_results: Vec<AgentQueryResult>,
}

impl TracedInstallQuoteAgent {
    fn new(id: impl Into<String>, action: Action) -> Self {
        Self {
            id: id.into(),
            action,
            query_results: Vec::new(),
        }
    }
}

impl AgentBehavior for TracedInstallQuoteAgent {
    fn agent_id(&self) -> &str {
        &self.id
    }

    fn decide(&mut self, _observation: &Observation) -> AgentDecision {
        AgentDecision::Query(AgentQuery::QuoteMicroDepotInstall(self.action.clone()))
    }

    fn on_query_result(&mut self, result: &AgentQueryResult) {
        self.query_results.push(result.clone());
    }
}

impl TracedQuoteAgent {
    fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            trace_emitted: false,
            query_results: Vec::new(),
        }
    }

    fn query(&self) -> AgentQuery {
        AgentQuery::EvaluateMicroDepotQuote(MicroDepotQuoteRequest {
            agent_id: self.id.clone(),
            facility_id: "depot-measured".to_string(),
            target_id: "loc-measured".to_string(),
            action_kind: MicroDepotActionKind::Repair,
            base_cost_class: MicroDepotPressureClass::Medium,
            base_risk_class: MicroDepotPressureClass::Low,
            blocker_type: Some("supply_missing".to_string()),
        })
    }
}

impl AgentBehavior for TracedQuoteAgent {
    fn agent_id(&self) -> &str {
        &self.id
    }

    fn decide(&mut self, _observation: &Observation) -> AgentDecision {
        AgentDecision::Query(self.query())
    }

    fn on_query_result(&mut self, result: &AgentQueryResult) {
        self.query_results.push(result.clone());
    }

    fn take_decision_trace(&mut self) -> Option<AgentDecisionTrace> {
        if self.trace_emitted {
            return None;
        }
        self.trace_emitted = true;
        Some(AgentDecisionTrace {
            agent_id: self.id.clone(),
            time: 0,
            decision: AgentDecision::Query(self.query()),
            llm_input: Some("evaluate micro-depot quote".to_string()),
            llm_output: Some("query preview".to_string()),
            llm_error: None,
            parse_error: None,
            llm_diagnostics: None,
            llm_effect_intents: vec![LlmEffectIntentTrace {
                intent_id: "quote-module-call".to_string(),
                kind: "llm.prompt.module_call".to_string(),
                params: serde_json::json!({ "module": "regional.micro_depot" }),
                cap_ref: "llm.prompt.module_access".to_string(),
                origin: "llm_agent".to_string(),
            }],
            llm_effect_receipts: vec![LlmEffectReceiptTrace {
                intent_id: "quote-module-call".to_string(),
                status: "ok".to_string(),
                payload: serde_json::json!({ "preview": "available" }),
                cost_cents: None,
            }],
            llm_step_trace: vec![],
            llm_prompt_section_trace: vec![],
            llm_chat_messages: vec![],
        })
    }
}

fn traced_query_kernel() -> WorldKernel {
    let mut kernel =
        super::micro_depot_measured_supply::installed_micro_depot_with_measured_supply(5, 4, 2);
    kernel
        .observe("agent-measured")
        .expect("warm quote-agent observation");
    kernel
}

fn measured_service_action() -> Action {
    Action::ServiceMicroDepotRepair {
        agent_id: "agent-measured".to_string(),
        facility_id: "depot-measured".to_string(),
        target_id: "loc-measured".to_string(),
        base_cost_class: MicroDepotPressureClass::Medium,
        base_risk_class: MicroDepotPressureClass::Low,
        blocker_type: Some("supply_missing".to_string()),
    }
}

fn prospective_install_action(facility_id: &str) -> Action {
    Action::InstallMicroDepot {
        installer_agent_id: "agent-install-quote".to_string(),
        facility_id: facility_id.to_string(),
        location_id: "loc-install-quote".to_string(),
        owner_claim_id: "claim-install-quote".to_string(),
        regional_blocker_receipt_id: "repair-logistics:loc-install-quote:1".to_string(),
        module_id: "regional.micro_depot".to_string(),
        module_version: "0.2.0".to_string(),
        wasm_hash: "hash-install-quote".to_string(),
        entrypoint: "evaluate_quote".to_string(),
        service_radius_cm: 250_000,
        supported_resource_kinds: vec!["data".to_string()],
    }
}

fn install_quote_kernel(data_units: i64) -> WorldKernel {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-install-quote".to_string(),
        name: "Install Quote Hub".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.step().expect("register install quote location");
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-install-quote".to_string(),
        location_id: "loc-install-quote".to_string(),
    });
    kernel.step().expect("register install quote agent");
    super::seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "agent-install-quote".to_string(),
        },
        ResourceKind::Data,
        data_units,
    );
    let sandbox = Arc::new(Mutex::new(DynamicMicroDepotSandbox {
        resource_debits: Some(vec![MicroDepotProposalResourceDebit {
            resource_kind: ResourceKind::Data,
            units: 2,
        }]),
        ..DynamicMicroDepotSandbox::default()
    }));
    kernel.set_micro_depot_wasm_module_evaluator(
        "regional.micro_depot",
        "hash-install-quote",
        "evaluate_quote",
        vec![0x00, 0x61, 0x73, 0x6d],
        ModuleLimits::unbounded(),
        sandbox,
    );
    kernel
}

fn materialize_install_quote_observation(kernel: &mut WorldKernel) {
    // Observation owns lazy chunk materialization. Quote purity starts after
    // that established runtime observation boundary.
    kernel
        .observe("agent-install-quote")
        .expect("materialize install-quote observation");
}

fn assert_install_quote_query_is_pure(
    kernel: &WorldKernel,
    model_before: &WorldModel,
    snapshot_before: &WorldSnapshot,
    journal_before: &WorldJournal,
    data_before: i64,
) {
    assert_eq!(kernel.model(), model_before, "quote must not mutate model");
    assert_eq!(
        kernel.snapshot(),
        *snapshot_before,
        "quote must not mutate snapshot"
    );
    assert_eq!(
        kernel.journal_snapshot(),
        *journal_before,
        "quote must not append history"
    );
    assert_eq!(
        kernel
            .model()
            .agents
            .get("agent-install-quote")
            .expect("install quote agent")
            .resources
            .get(ResourceKind::Data),
        data_before,
        "quote must not reserve or debit install resources"
    );
}

fn assert_query_is_checkpoint_pure(
    kernel: &WorldKernel,
    snapshot_before: &WorldSnapshot,
    journal_before: &WorldJournal,
) {
    assert_eq!(kernel.pending_actions(), 0);
    assert_eq!(kernel.time(), snapshot_before.time);
    assert_eq!(kernel.journal_snapshot(), *journal_before);
    assert_eq!(
        kernel.snapshot(),
        *snapshot_before,
        "query trace must not perturb canonical or replay checkpoint state"
    );
}

#[test]
fn traced_query_tick_preserves_canonical_history_and_next_service_identity() {
    let mut kernel = traced_query_kernel();
    let mut control = traced_query_kernel();
    let snapshot_before = kernel.snapshot();
    let journal_before = kernel.journal_snapshot();
    let mut runner: AgentRunner<TracedQuoteAgent> = AgentRunner::new();
    runner.register(TracedQuoteAgent::new("agent-measured"));

    let query_tick = runner.tick(&mut kernel).expect("query tick");

    assert!(matches!(query_tick.decision, AgentDecision::Query(_)));
    assert!(query_tick.action_result.is_none());
    assert!(query_tick.query_result.is_some());
    assert_eq!(
        runner
            .get("agent-measured")
            .expect("registered")
            .behavior
            .query_results
            .len(),
        1
    );
    assert_query_is_checkpoint_pure(&kernel, &snapshot_before, &journal_before);

    let service_id_after_query =
        kernel.submit_action_from_agent("agent-measured", measured_service_action());
    let control_service_id =
        control.submit_action_from_agent("agent-measured", measured_service_action());
    assert_eq!(service_id_after_query, control_service_id);
    assert_eq!(kernel.step(), control.step());
}

#[test]
fn traced_query_decide_only_preserves_canonical_history_and_feedback() {
    let mut kernel = traced_query_kernel();
    let snapshot_before = kernel.snapshot();
    let journal_before = kernel.journal_snapshot();
    let mut runner: AgentRunner<TracedQuoteAgent> = AgentRunner::new();
    runner.register(TracedQuoteAgent::new("agent-measured"));

    let query_tick = runner
        .tick_decide_only(&mut kernel)
        .expect("query decision");

    assert!(matches!(query_tick.decision, AgentDecision::Query(_)));
    assert!(query_tick.action_result.is_none());
    assert!(query_tick.query_result.is_some());
    assert_eq!(
        runner
            .get("agent-measured")
            .expect("registered")
            .behavior
            .query_results
            .len(),
        1
    );
    assert_query_is_checkpoint_pure(&kernel, &snapshot_before, &journal_before);
}

#[test]
fn traced_query_batch_excludes_trace_from_durable_intents_and_history() {
    let mut kernel = traced_query_kernel();
    let mut control = traced_query_kernel();
    let snapshot_before = kernel.snapshot();
    let journal_before = kernel.journal_snapshot();
    let mut runner: AgentRunner<TracedQuoteAgent> = AgentRunner::new();
    runner.register(TracedQuoteAgent::new("agent-measured"));

    let results = runner.tick_collect_intents_and_commit(&mut kernel, 1);

    assert_eq!(results.len(), 1);
    assert!(matches!(results[0].decision, AgentDecision::Query(_)));
    assert!(results[0].action_result.is_none());
    assert!(results[0].query_result.is_some());
    assert_eq!(
        runner
            .get("agent-measured")
            .expect("registered")
            .behavior
            .query_results
            .len(),
        1
    );
    assert_query_is_checkpoint_pure(&kernel, &snapshot_before, &journal_before);

    let service_id_after_batch =
        kernel.submit_action_from_agent("agent-measured", measured_service_action());
    let control_service_id =
        control.submit_action_from_agent("agent-measured", measured_service_action());
    assert_eq!(service_id_after_batch, control_service_id);
    assert_eq!(kernel.step(), control.step());
}

#[test]
fn install_quote_query_returns_the_canonical_deployable_dto_without_mutation() {
    let mut kernel = install_quote_kernel(10);
    materialize_install_quote_observation(&mut kernel);
    let action = prospective_install_action("depot-query-deployable");
    let expected = kernel
        .micro_depot_install_quote(&action)
        .expect("canonical deployable install quote");
    assert!(
        expected.deployable,
        "fixture must exercise deployable quote"
    );
    let model_before = kernel.model().clone();
    let snapshot_before = kernel.snapshot();
    let journal_before = kernel.journal_snapshot();
    let data_before = kernel
        .model()
        .agents
        .get("agent-install-quote")
        .expect("install quote agent")
        .resources
        .get(ResourceKind::Data);
    let mut runner: AgentRunner<TracedInstallQuoteAgent> = AgentRunner::new();
    runner.register(TracedInstallQuoteAgent::new(
        "agent-install-quote",
        action.clone(),
    ));

    let tick = runner.tick(&mut kernel).expect("install quote query tick");

    assert!(matches!(tick.decision, AgentDecision::Query(_)));
    assert!(tick.action_result.is_none());
    let query_results = &runner
        .get("agent-install-quote")
        .expect("registered install quote agent")
        .behavior
        .query_results;
    assert_eq!(
        query_results.len(),
        1,
        "runner must return exactly one quote"
    );
    let result = &query_results[0];
    match result {
        AgentQueryResult::QuoteMicroDepotInstall {
            action: queried_action,
            result,
        } => {
            assert_eq!(queried_action, &action);
            assert_eq!(result.as_ref().expect("deployable quote result"), &expected);
        }
        other => panic!("unexpected install quote query result: {other:?}"),
    }
    assert_install_quote_query_is_pure(
        &kernel,
        &model_before,
        &snapshot_before,
        &journal_before,
        data_before,
    );
}

#[test]
fn install_quote_query_decide_only_returns_canonical_dto_without_mutation() {
    let mut kernel = install_quote_kernel(10);
    materialize_install_quote_observation(&mut kernel);
    let action = prospective_install_action("depot-query-decide-only");
    let expected = kernel
        .micro_depot_install_quote(&action)
        .expect("canonical deployable install quote");
    let model_before = kernel.model().clone();
    let snapshot_before = kernel.snapshot();
    let journal_before = kernel.journal_snapshot();
    let data_before = kernel
        .model()
        .agents
        .get("agent-install-quote")
        .expect("install quote agent")
        .resources
        .get(ResourceKind::Data);
    let mut runner: AgentRunner<TracedInstallQuoteAgent> = AgentRunner::new();
    runner.register(TracedInstallQuoteAgent::new(
        "agent-install-quote",
        action.clone(),
    ));

    let tick = runner
        .tick_decide_only(&mut kernel)
        .expect("install quote decide-only tick");

    assert!(matches!(tick.decision, AgentDecision::Query(_)));
    assert!(tick.action_result.is_none());
    match tick.query_result.as_ref().expect("query result") {
        AgentQueryResult::QuoteMicroDepotInstall {
            action: queried_action,
            result,
        } => {
            assert_eq!(queried_action, &action);
            assert_eq!(result.as_ref().expect("deployable quote result"), &expected);
        }
        other => panic!("unexpected decide-only install quote result: {other:?}"),
    }
    assert_install_quote_query_is_pure(
        &kernel,
        &model_before,
        &snapshot_before,
        &journal_before,
        data_before,
    );
}

#[test]
fn install_quote_query_returns_structured_blocker_without_mutation() {
    let mut kernel = install_quote_kernel(9);
    materialize_install_quote_observation(&mut kernel);
    let action = prospective_install_action("depot-query-blocked");
    let expected = kernel
        .micro_depot_install_quote(&action)
        .expect("canonical structured blocked install quote");
    assert!(
        !expected.deployable,
        "fixture must exercise a blocked quote"
    );
    assert!(
        !expected.reasons.is_empty(),
        "blocked quote must preserve player-readable structured reasons"
    );
    let model_before = kernel.model().clone();
    let snapshot_before = kernel.snapshot();
    let journal_before = kernel.journal_snapshot();
    let data_before = kernel
        .model()
        .agents
        .get("agent-install-quote")
        .expect("install quote agent")
        .resources
        .get(ResourceKind::Data);
    let mut runner: AgentRunner<TracedInstallQuoteAgent> = AgentRunner::new();
    runner.register(TracedInstallQuoteAgent::new(
        "agent-install-quote",
        action.clone(),
    ));

    runner
        .tick(&mut kernel)
        .expect("blocked install quote query tick");

    let query_results = &runner
        .get("agent-install-quote")
        .expect("registered install quote agent")
        .behavior
        .query_results;
    assert_eq!(
        query_results.len(),
        1,
        "runner must return exactly one quote"
    );
    let result = &query_results[0];
    match result {
        AgentQueryResult::QuoteMicroDepotInstall {
            action: queried_action,
            result,
        } => {
            assert_eq!(queried_action, &action);
            let quote = result.as_ref().expect("structured blocked quote result");
            assert_eq!(quote, &expected);
            assert!(!quote.deployable);
            assert!(!quote.reasons.is_empty());
        }
        other => panic!("unexpected blocked install quote result: {other:?}"),
    }
    assert_install_quote_query_is_pure(
        &kernel,
        &model_before,
        &snapshot_before,
        &journal_before,
        data_before,
    );
}
