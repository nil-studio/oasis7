use super::*;
#[cfg(feature = "test_tier_full")]
use oasis7::runtime::{EconomicContractStatus, GovernanceProposalStatus};
use oasis7::simulator::{AgentDecision, AgentDecisionTrace, LlmStepTrace, WorldEvent};
#[cfg(feature = "test_tier_full")]
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_simulator::{Action, WorldEventKind};

#[cfg(feature = "test_tier_full")]
fn tracked_baseline_fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("fixtures/llm_baseline/state_01")
}

fn make_trace(llm_input: Option<&str>, step_type: &str, input_summary: &str) -> AgentDecisionTrace {
    AgentDecisionTrace {
        agent_id: "agent-0".to_string(),
        time: 1,
        decision: AgentDecision::Wait,
        llm_input: llm_input.map(ToString::to_string),
        llm_output: Some("out".to_string()),
        llm_error: None,
        parse_error: None,
        llm_diagnostics: None,
        llm_effect_intents: vec![],
        llm_effect_receipts: vec![],
        llm_step_trace: vec![LlmStepTrace {
            step_index: 0,
            step_type: step_type.to_string(),
            input_summary: input_summary.to_string(),
            output_summary: "ok".to_string(),
            status: "ok".to_string(),
        }],
        llm_prompt_section_trace: vec![],
        llm_chat_messages: vec![],
    }
}

#[test]
fn observe_trace_counts_llm_skipped_tick_ratio() {
    let mut report = DemoRunReport::new("llm_bootstrap".to_string(), 4);
    report.active_ticks = 4;

    let skipped = make_trace(
        None,
        "execute_until_continue",
        "skip_llm_with_active_execute_until",
    );
    let normal = make_trace(Some("prompt"), "dialogue_turn", "turn=0");

    report.observe_trace(&skipped);
    report.observe_trace(&normal);
    report.finalize();

    assert_eq!(report.trace_counts.llm_skipped_ticks, 1);
    assert_eq!(report.trace_counts.llm_skipped_tick_ratio_ppm, 250_000);
}

#[test]
fn parse_options_defaults() {
    let options = parse_options([].into_iter()).expect("defaults");
    assert_eq!(options.scenario, WorldScenario::LlmBootstrap);
    assert_eq!(options.ticks, 20);
    assert_eq!(
        options.coverage_bootstrap_profile,
        CoverageBootstrapProfile::None
    );
    assert!(options.runtime_gameplay_bridge);
    assert_eq!(options.runtime_gameplay_preset, RuntimeGameplayPreset::None);
    assert_eq!(options.load_state_dir, None);
    assert_eq!(options.save_state_dir, None);
    assert!(!options.print_llm_io);
    assert_eq!(options.llm_io_max_chars, None);
    assert_eq!(options.llm_system_prompt, None);
    assert_eq!(options.llm_short_term_goal, None);
    assert_eq!(options.llm_long_term_goal, None);
    assert_eq!(options.prompt_switch_tick, None);
    assert_eq!(options.switch_llm_system_prompt, None);
    assert_eq!(options.switch_llm_short_term_goal, None);
    assert_eq!(options.switch_llm_long_term_goal, None);
    assert_eq!(options.prompt_switches_json, None);
    assert!(options.prompt_switches.is_empty());
    assert_eq!(options.decision_source, DecisionSource::LlmResponses);
    assert_eq!(options.agent_provider_url, None);
    assert_eq!(options.agent_provider_connect_timeout_ms, 60_000);
    assert_eq!(options.agent_provider_decision_timeout_ms, 60_000);
    assert_eq!(options.agent_provider_profile, "oasis7_p0_low_freq_npc");
}

#[test]
fn parse_options_accepts_alias_scenario() {
    let options = parse_options(["llm"].into_iter()).expect("scenario alias");
    assert_eq!(options.scenario, WorldScenario::LlmBootstrap);
}

#[test]
fn parse_options_accepts_ticks() {
    let options = parse_options(["--ticks", "12"].into_iter()).expect("ticks");
    assert_eq!(options.ticks, 12);
}

#[test]
fn parse_options_accepts_report_json_path() {
    let options =
        parse_options(["--report-json", ".tmp/report.json"].into_iter()).expect("report json path");
    assert_eq!(options.report_json.as_deref(), Some(".tmp/report.json"));
}

#[test]
fn parse_options_accepts_provider_loopback_http() {
    let options = parse_options(
        [
            "--decision-source",
            "provider_loopback_http",
            "--agent-provider-url",
            "http://127.0.0.1:5841",
            "--agent-provider-connect-timeout-ms",
            "1234",
            "--agent-provider-decision-timeout-ms",
            "2345",
            "--agent-provider-profile",
            "oasis7_p0_low_freq_npc",
        ]
        .into_iter(),
    )
    .expect("provider loopback options");

    assert_eq!(
        options.decision_source,
        DecisionSource::ProviderLoopbackHttp
    );
    assert_eq!(
        options.agent_provider_url.as_deref(),
        Some("http://127.0.0.1:5841")
    );
    assert_eq!(options.agent_provider_connect_timeout_ms, 1234);
    assert_eq!(options.agent_provider_decision_timeout_ms, 2345);
    assert_eq!(options.agent_provider_profile, "oasis7_p0_low_freq_npc");
}

#[test]
fn provider_loopback_demo_selects_legacy_compatibility_lane() {
    let source = include_str!("oasis7_llm_agent_demo_runtime.rs");
    let provider_branch = source
        .split_once("DecisionSource::ProviderLoopbackHttp")
        .map(|(_, branch)| branch)
        .expect("provider loopback demo branch");
    assert!(
        provider_branch.contains(".legacy_compatibility()"),
        "the synchronous demo AgentRunner must opt into the compatibility lane"
    );
}

#[test]
fn provider_loopback_transport_timeout_preserves_decision_budget() {
    let options = parse_options(
        [
            "--decision-source",
            "provider_loopback_http",
            "--agent-provider-url",
            "http://127.0.0.1:5841",
            "--agent-provider-connect-timeout-ms",
            "1000",
            "--agent-provider-decision-timeout-ms",
            "60000",
        ]
        .into_iter(),
    )
    .expect("provider loopback options");

    assert_eq!(
        resolve_provider_loopback_transport_timeout_ms(&options),
        60_000
    );
}

#[test]
fn provider_prompt_context_partial_switch_preserves_existing_parts() {
    let options = parse_options(
        [
            "--llm-system-prompt",
            "sys",
            "--llm-short-term-goal",
            "short",
            "--llm-long-term-goal",
            "long",
        ]
        .into_iter(),
    )
    .expect("initial provider prompt context");
    let mut context = ProviderPromptContext::from_options(&options);

    context.apply_switch(&PromptSwitchSpec {
        tick: 2,
        llm_system_prompt: None,
        llm_short_term_goal: Some("short2".to_string()),
        llm_long_term_goal: None,
    });

    let summary = context.memory_summary().expect("summary");
    assert!(summary.contains("system_hint=sys"));
    assert!(summary.contains("short_term_goal=short2"));
    assert!(summary.contains("long_term_goal=long"));
}

#[test]
fn parse_options_rejects_provider_loopback_http_without_url() {
    let err = parse_options(["--decision-source", "provider_loopback_http"].into_iter())
        .expect_err("provider loopback requires url");
    assert!(err.contains("--agent-provider-url"));
}

#[test]
fn parse_options_accepts_state_dir_paths() {
    let options = parse_options(
        [
            "--load-state-dir",
            ".tmp/state/in",
            "--save-state-dir",
            ".tmp/state/out",
        ]
        .into_iter(),
    )
    .expect("state dirs");
    assert_eq!(options.load_state_dir.as_deref(), Some(".tmp/state/in"));
    assert_eq!(options.save_state_dir.as_deref(), Some(".tmp/state/out"));
}

#[test]
fn parse_options_rejects_missing_load_state_dir_path() {
    let err =
        parse_options(["--load-state-dir"].into_iter()).expect_err("missing load state dir path");
    assert!(err.contains("directory path"));
}

#[test]
fn parse_options_disables_runtime_gameplay_bridge() {
    let options =
        parse_options(["--no-runtime-gameplay-bridge"].into_iter()).expect("disable bridge");
    assert!(!options.runtime_gameplay_bridge);
}

#[test]
fn parse_options_accepts_runtime_gameplay_preset() {
    let options = parse_options(["--runtime-gameplay-preset", "civic_hotspot_v1"].into_iter())
        .expect("runtime gameplay preset");
    assert_eq!(
        options.runtime_gameplay_preset,
        RuntimeGameplayPreset::CivicHotspotV1
    );
}

#[test]
fn parse_options_accepts_coverage_bootstrap_profile() {
    let options = parse_options(["--coverage-bootstrap-profile", "hybrid"].into_iter())
        .expect("coverage bootstrap profile");
    assert_eq!(
        options.coverage_bootstrap_profile,
        CoverageBootstrapProfile::Hybrid
    );
}

#[test]
fn parse_options_rejects_invalid_coverage_bootstrap_profile() {
    let err = parse_options(["--coverage-bootstrap-profile", "invalid"].into_iter())
        .expect_err("invalid coverage bootstrap profile");
    assert!(err.contains("invalid --coverage-bootstrap-profile"));
}

#[test]
fn parse_options_rejects_invalid_runtime_gameplay_preset() {
    let err = parse_options(["--runtime-gameplay-preset", "invalid"].into_iter())
        .expect_err("invalid runtime gameplay preset");
    assert!(err.contains("invalid --runtime-gameplay-preset"));
}

#[test]
fn parse_options_rejects_runtime_gameplay_preset_when_bridge_disabled() {
    let err = parse_options(
        [
            "--no-runtime-gameplay-bridge",
            "--runtime-gameplay-preset",
            "civic_hotspot_v1",
        ]
        .into_iter(),
    )
    .expect_err("runtime gameplay preset requires bridge");
    assert!(err.contains("requires --runtime-gameplay-bridge"));
}

#[test]
fn parse_options_rejects_runtime_bridge_dependent_coverage_profile_when_bridge_disabled() {
    let err = parse_options(
        [
            "--no-runtime-gameplay-bridge",
            "--coverage-bootstrap-profile",
            "gameplay",
        ]
        .into_iter(),
    )
    .expect_err("coverage gameplay profile requires bridge");
    assert!(err.contains("requires --runtime-gameplay-bridge"));
}

#[test]
fn parse_options_enables_print_llm_io() {
    let options = parse_options(["--print-llm-io"].into_iter()).expect("llm io option");
    assert!(options.print_llm_io);
}

#[test]
fn parse_options_accepts_llm_io_max_chars() {
    let options =
        parse_options(["--llm-io-max-chars", "256"].into_iter()).expect("llm io max chars option");
    assert_eq!(options.llm_io_max_chars, Some(256));
}

#[test]
fn parse_options_accepts_initial_prompt_overrides() {
    let options = parse_options(
        [
            "--llm-system-prompt",
            "sys",
            "--llm-short-term-goal",
            "short",
            "--llm-long-term-goal",
            "long",
        ]
        .into_iter(),
    )
    .expect("prompt overrides");
    assert_eq!(options.llm_system_prompt.as_deref(), Some("sys"));
    assert_eq!(options.llm_short_term_goal.as_deref(), Some("short"));
    assert_eq!(options.llm_long_term_goal.as_deref(), Some("long"));
}

#[test]
fn parse_options_accepts_switch_prompt_overrides() {
    let options = parse_options(
        [
            "--prompt-switch-tick",
            "9",
            "--switch-llm-system-prompt",
            "sys2",
            "--switch-llm-short-term-goal",
            "short2",
            "--switch-llm-long-term-goal",
            "long2",
        ]
        .into_iter(),
    )
    .expect("switch prompt overrides");
    assert_eq!(options.prompt_switch_tick, Some(9));
    assert_eq!(options.switch_llm_system_prompt.as_deref(), Some("sys2"));
    assert_eq!(
        options.switch_llm_short_term_goal.as_deref(),
        Some("short2")
    );
    assert_eq!(options.switch_llm_long_term_goal.as_deref(), Some("long2"));
    assert_eq!(options.prompt_switches.len(), 1);
    assert_eq!(options.prompt_switches[0].tick, 9);
}

#[test]
fn parse_options_accepts_prompt_switches_json() {
    let options = parse_options(
        [
            "--prompt-switches-json",
            r#"[{"tick":12,"llm_short_term_goal":"mid"},{"tick":24,"llm_long_term_goal":"late"}]"#,
        ]
        .into_iter(),
    )
    .expect("prompt switches json");
    assert_eq!(options.prompt_switches.len(), 2);
    assert_eq!(options.prompt_switches[0].tick, 12);
    assert_eq!(
        options.prompt_switches[0].llm_short_term_goal.as_deref(),
        Some("mid")
    );
    assert_eq!(options.prompt_switches[1].tick, 24);
    assert_eq!(
        options.prompt_switches[1].llm_long_term_goal.as_deref(),
        Some("late")
    );
}

#[test]
fn parse_options_rejects_invalid_prompt_switches_json() {
    let err = parse_options(["--prompt-switches-json", "not-json"].into_iter())
        .expect_err("invalid prompt switches json");
    assert!(err.contains("invalid --prompt-switches-json"));
}

#[test]
fn parse_options_rejects_prompt_switches_json_without_override_fields() {
    let err = parse_options(["--prompt-switches-json", r#"[{"tick":12}]"#].into_iter())
        .expect_err("missing switch override fields");
    assert!(err.contains("requires at least one llm_* override"));
}

#[test]
fn parse_options_rejects_mixed_legacy_and_prompt_switches_json() {
    let err = parse_options(
        [
            "--prompt-switch-tick",
            "9",
            "--switch-llm-short-term-goal",
            "short2",
            "--prompt-switches-json",
            r#"[{"tick":12,"llm_short_term_goal":"mid"}]"#,
        ]
        .into_iter(),
    )
    .expect_err("mixed legacy and json switch options");
    assert!(err.contains("cannot combine --prompt-switches-json"));
}

#[test]
fn parse_options_rejects_missing_report_json_path() {
    let err = parse_options(["--report-json"].into_iter()).expect_err("missing report path");
    assert!(err.contains("file path"));
}

#[test]
fn parse_options_rejects_zero_ticks() {
    let err = parse_options(["--ticks", "0"].into_iter()).expect_err("reject zero");
    assert!(err.contains("positive integer"));
}

#[test]
fn parse_options_rejects_invalid_llm_io_max_chars() {
    let err = parse_options(["--llm-io-max-chars", "0"].into_iter())
        .expect_err("reject zero llm io max chars");
    assert!(err.contains("positive integer"));
}

#[test]
fn parse_options_rejects_switch_prompt_without_tick() {
    let err = parse_options(["--switch-llm-system-prompt", "sys2"].into_iter())
        .expect_err("switch prompt without tick");
    assert!(err.contains("--prompt-switch-tick"));
}

#[test]
fn parse_options_rejects_switch_tick_without_switch_prompt() {
    let err = parse_options(["--prompt-switch-tick", "4"].into_iter())
        .expect_err("switch tick without switch prompt");
    assert!(err.contains("--switch-llm-"));
}

#[test]
fn truncate_for_llm_io_log_marks_truncation() {
    let truncated = truncate_for_llm_io_log("abcdef", Some(3));
    assert!(truncated.starts_with("abc"));
    assert!(truncated.contains("truncated"));
}

#[test]
fn reject_reason_metric_key_uses_serde_tag_name() {
    let key = reject_reason_metric_key(&RejectReason::InvalidAmount { amount: 0 });
    assert_eq!(key, "invalid_amount");
}

#[test]
fn action_metric_key_uses_serde_tag_name() {
    let key = action_metric_key(&Action::BuildFactory {
        owner: oasis7_simulator::ResourceOwner::Agent {
            agent_id: "agent-0".to_string(),
        },
        location_id: "loc-0".to_string(),
        factory_id: "factory.alpha".to_string(),
        factory_kind: "factory.assembler.mk1".to_string(),
    });
    assert_eq!(key, "build_factory");
}

#[test]
fn observe_action_result_counts_reject_reason_breakdown() {
    let mut report = DemoRunReport::new("llm_bootstrap".to_string(), 1);
    let action_result = ActionResult {
        action: Action::HarvestRadiation {
            agent_id: "agent-0".to_string(),
            max_amount: 1,
        },
        action_id: 1,
        success: false,
        event: WorldEvent {
            id: 1,
            time: 1,
            kind: WorldEventKind::ActionRejected {
                reason: RejectReason::InvalidAmount { amount: 0 },
            },
            runtime_event: None,
        },
    };

    report.observe_action_result(3, &action_result);

    assert_eq!(report.action_success, 0);
    assert_eq!(report.action_failure, 1);
    assert_eq!(report.action_kind_counts.get("harvest_radiation"), Some(&1));
    assert_eq!(
        report.action_kind_failure_counts.get("harvest_radiation"),
        Some(&1)
    );
    assert_eq!(report.first_action_tick.get("harvest_radiation"), Some(&3));
    assert_eq!(
        report.action_reject_reason_counts.get("invalid_amount"),
        Some(&1)
    );
}

#[test]
fn observe_action_result_counts_success_and_first_tick_per_action_kind() {
    let mut report = DemoRunReport::new("llm_bootstrap".to_string(), 1);
    let success = ActionResult {
        action: Action::BuildFactory {
            owner: oasis7_simulator::ResourceOwner::Agent {
                agent_id: "agent-0".to_string(),
            },
            location_id: "loc-0".to_string(),
            factory_id: "factory.alpha".to_string(),
            factory_kind: "factory.assembler.mk1".to_string(),
        },
        action_id: 7,
        success: true,
        event: WorldEvent {
            id: 7,
            time: 7,
            kind: WorldEventKind::FactoryBuilt {
                owner: oasis7_simulator::ResourceOwner::Agent {
                    agent_id: "agent-0".to_string(),
                },
                location_id: "loc-0".to_string(),
                factory_id: "factory.alpha".to_string(),
                factory_kind: "factory.assembler.mk1".to_string(),
                electricity_cost: 10,
                hardware_cost: 2,
            },
            runtime_event: None,
        },
    };
    let failure = ActionResult {
        action: Action::ScheduleRecipe {
            owner: oasis7_simulator::ResourceOwner::Agent {
                agent_id: "agent-0".to_string(),
            },
            factory_id: "factory.alpha".to_string(),
            recipe_id: "recipe.assembler.logistics_drone".to_string(),
            batches: 1,
        },
        action_id: 8,
        success: false,
        event: WorldEvent {
            id: 8,
            time: 8,
            kind: WorldEventKind::ActionRejected {
                reason: RejectReason::FacilityNotFound {
                    facility_id: "factory.alpha".to_string(),
                },
            },
            runtime_event: None,
        },
    };

    report.observe_action_result(5, &success);
    report.observe_action_result(9, &failure);

    assert_eq!(report.action_kind_counts.get("build_factory"), Some(&1));
    assert_eq!(report.action_kind_counts.get("schedule_recipe"), Some(&1));
    assert_eq!(
        report.action_kind_success_counts.get("build_factory"),
        Some(&1)
    );
    assert_eq!(
        report.action_kind_failure_counts.get("schedule_recipe"),
        Some(&1)
    );
    assert_eq!(report.first_action_tick.get("build_factory"), Some(&5));
    assert_eq!(report.first_action_tick.get("schedule_recipe"), Some(&9));
}

#[test]
fn industrial_coverage_bootstrap_records_required_action_kinds() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::LlmBootstrap, &config);
    let (mut kernel, _) = initialize_kernel(config, init).expect("initialize kernel");
    let mut report = DemoRunReport::new("llm_bootstrap".to_string(), 20);

    let action_count =
        run_industrial_coverage_bootstrap(&mut kernel, &mut report).expect("industrial bootstrap");
    assert_eq!(action_count, 5);
    assert_eq!(
        report.action_kind_success_counts.get("harvest_radiation"),
        Some(&1)
    );
    assert_eq!(
        report.action_kind_success_counts.get("mine_compound"),
        Some(&1)
    );
    assert_eq!(
        report.action_kind_success_counts.get("refine_compound"),
        Some(&1)
    );
    assert_eq!(
        report.action_kind_success_counts.get("build_factory"),
        Some(&1)
    );
    assert_eq!(
        report.action_kind_success_counts.get("schedule_recipe"),
        Some(&1)
    );
}

#[test]
fn gameplay_coverage_bootstrap_records_required_action_kinds() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::LlmBootstrap, &config);
    let (mut kernel, _) = initialize_kernel(config, init).expect("initialize kernel");
    let mut report = DemoRunReport::new("llm_bootstrap".to_string(), 20);
    let mut runtime_bridge =
        RuntimeGameplayBridge::from_kernel(&kernel).expect("bootstrap runtime gameplay bridge");
    let mut handles = RuntimeGameplayPresetHandles::default();

    let action_count = run_gameplay_coverage_bootstrap(
        &mut kernel,
        &mut runtime_bridge,
        &mut handles,
        &mut report,
    )
    .expect("gameplay bootstrap");
    assert_eq!(action_count, 4);
    assert_eq!(
        report
            .action_kind_success_counts
            .get("open_governance_proposal"),
        Some(&1)
    );
    assert_eq!(
        report
            .action_kind_success_counts
            .get("cast_governance_vote"),
        Some(&1)
    );
    assert_eq!(
        report.action_kind_success_counts.get("resolve_crisis"),
        Some(&1)
    );
    assert_eq!(
        report.action_kind_success_counts.get("grant_meta_progress"),
        Some(&1)
    );
    assert!(report.runtime_bridge_actions >= 4);
}

#[cfg(feature = "test_tier_full")]
#[test]
fn runtime_bridge_continues_governance_from_tracked_baseline_fixture() {
    let fixture_dir = tracked_baseline_fixture_dir();
    let mut kernel = oasis7::simulator::WorldKernel::load_from_dir(&fixture_dir)
        .expect("load tracked baseline fixture");
    let mut runtime_bridge =
        RuntimeGameplayBridge::from_kernel(&kernel).expect("bootstrap runtime gameplay bridge");

    let mut agent_ids: Vec<String> = kernel.model().agents.keys().cloned().collect();
    agent_ids.sort();
    assert!(
        agent_ids.len() >= 4,
        "tracked baseline fixture should contain at least 4 agents"
    );

    let proposer = agent_ids[0].clone();
    let voter = agent_ids[1].clone();
    let progress_target = agent_ids[2].clone();
    let contract_counterparty = agent_ids[3].clone();
    let proposal_key = "fixture.governance.smoke";
    let contract_id = "fixture.contract.smoke";
    let track = "civic";
    let achievement_id = "fixture-governance-smoke";
    let mut tick = kernel.time().saturating_add(1);
    let baseline_progress = runtime_bridge
        .state()
        .meta_progress
        .get(progress_target.as_str())
        .cloned();
    let baseline_total_points = baseline_progress
        .as_ref()
        .map(|state| state.total_points)
        .unwrap_or(0);
    let baseline_track_points = baseline_progress
        .as_ref()
        .and_then(|state| state.track_points.get(track))
        .copied()
        .unwrap_or(0);
    let baseline_achievement_known = baseline_progress
        .as_ref()
        .map(|state| {
            state
                .achievements
                .iter()
                .any(|item| item.as_str() == achievement_id)
        })
        .unwrap_or(false);
    runtime_bridge
        .seed_agent_resource_balance(proposer.as_str(), ResourceKind::Electricity, 10)
        .expect("seed proposer electricity for atomic exchange settlement");

    let open_proposal = runtime_bridge
        .execute(
            tick,
            proposer.as_str(),
            Action::OpenGovernanceProposal {
                proposer_agent_id: proposer.clone(),
                proposal_key: proposal_key.to_string(),
                title: "fixture governance smoke".to_string(),
                description: "verify governance continuation from tracked baseline".to_string(),
                options: vec!["approve".to_string(), "reject".to_string()],
                voting_window_ticks: 8,
                quorum_weight: 1,
                pass_threshold_bps: 5_000,
            },
        )
        .expect("runtime bridge open governance proposal");
    assert!(open_proposal.success, "open proposal should succeed");

    tick = tick.saturating_add(1);
    let cast_vote = runtime_bridge
        .execute(
            tick,
            voter.as_str(),
            Action::CastGovernanceVote {
                voter_agent_id: voter.clone(),
                proposal_key: proposal_key.to_string(),
                option: "approve".to_string(),
                weight: 1,
            },
        )
        .expect("runtime bridge cast governance vote");
    assert!(cast_vote.success, "cast vote should succeed");

    tick = tick.saturating_add(1);
    let grant_progress = runtime_bridge
        .execute(
            tick,
            proposer.as_str(),
            Action::GrantMetaProgress {
                operator_agent_id: proposer.clone(),
                target_agent_id: progress_target.clone(),
                track: track.to_string(),
                points: 5,
                achievement_id: Some(achievement_id.to_string()),
            },
        )
        .expect("runtime bridge grant meta progress");
    assert!(grant_progress.success, "grant meta progress should succeed");

    tick = tick.saturating_add(1);
    let open_contract = runtime_bridge
        .execute(
            tick,
            proposer.as_str(),
            Action::OpenEconomicContract {
                creator_agent_id: proposer.clone(),
                contract_id: contract_id.to_string(),
                counterparty_agent_id: contract_counterparty.clone(),
                settlement_kind: ResourceKind::Electricity,
                settlement_amount: 10,
                reputation_stake: 3,
                expires_at: tick.saturating_add(20),
                description: "fixture governance continuation contract smoke".to_string(),
            },
        )
        .expect("runtime bridge open economic contract");
    assert!(
        open_contract.success,
        "open economic contract should succeed"
    );

    tick = tick.saturating_add(1);
    let accept_contract = runtime_bridge
        .execute(
            tick,
            contract_counterparty.as_str(),
            Action::AcceptEconomicContract {
                accepter_agent_id: contract_counterparty.clone(),
                contract_id: contract_id.to_string(),
            },
        )
        .expect("runtime bridge accept economic contract");
    assert!(
        accept_contract.success,
        "accept economic contract should succeed"
    );

    tick = tick.saturating_add(1);
    let settle_contract = runtime_bridge
        .execute(
            tick,
            proposer.as_str(),
            Action::SettleEconomicContract {
                operator_agent_id: proposer.clone(),
                contract_id: contract_id.to_string(),
                success: true,
                notes: "atomic exchange smoke settlement".to_string(),
            },
        )
        .expect("runtime bridge settle economic contract");
    assert!(
        settle_contract.success,
        "settle economic contract should succeed"
    );

    let runtime_state = runtime_bridge.state();
    let proposal = runtime_state
        .governance_proposals
        .get(proposal_key)
        .expect("proposal state should exist");
    assert_eq!(proposal.status, GovernanceProposalStatus::Open);

    let votes = runtime_state
        .governance_votes
        .get(proposal_key)
        .expect("governance vote state should exist");
    assert_eq!(votes.total_weight, 1);
    assert_eq!(votes.tallies.get("approve"), Some(&1_u64));
    let ballot = votes
        .votes_by_agent
        .get(voter.as_str())
        .expect("voter ballot should exist");
    assert_eq!(ballot.option, "approve");
    assert_eq!(ballot.weight, 1);

    let progress = runtime_state
        .meta_progress
        .get(progress_target.as_str())
        .expect("meta progress state should exist");
    assert_eq!(progress.total_points, baseline_total_points + 5);
    assert_eq!(
        progress.track_points.get(track).copied(),
        Some(baseline_track_points + 5)
    );
    assert!(
        progress
            .achievements
            .iter()
            .any(|item| item.as_str() == achievement_id),
        "meta progress should contain smoke achievement id"
    );
    if !baseline_achievement_known {
        let baseline_achievement_len = baseline_progress
            .as_ref()
            .map(|state| state.achievements.len())
            .unwrap_or(0);
        assert_eq!(
            progress.achievements.len(),
            baseline_achievement_len + 1,
            "smoke achievement should append once when absent in baseline"
        );
    }

    let contract = runtime_state
        .economic_contracts
        .get(contract_id)
        .expect("economic contract state should exist");
    assert_eq!(contract.status, EconomicContractStatus::Settled);
    assert_eq!(contract.settlement_success, Some(true));
    assert_eq!(contract.transfer_amount, 10);
    assert_eq!(contract.tax_amount, 0);
    assert_eq!(
        contract.settlement_notes.as_deref(),
        Some("atomic exchange smoke settlement")
    );
    assert!(contract.accepted_at.is_some());
    assert!(contract.settled_at.is_some());

    advance_kernel_time_with_noop_move(&mut kernel, proposer.as_str());
}

#[cfg(feature = "test_tier_full")]
#[test]
fn runtime_bridge_civic_hotspot_preset_seeds_followup_handles() {
    let fixture_dir = tracked_baseline_fixture_dir();
    let kernel = oasis7::simulator::WorldKernel::load_from_dir(&fixture_dir)
        .expect("load tracked baseline fixture");
    let mut runtime_bridge =
        RuntimeGameplayBridge::from_kernel(&kernel).expect("bootstrap runtime gameplay bridge");

    let handles = runtime_bridge
        .apply_preset(RuntimeGameplayPreset::CivicHotspotV1)
        .expect("seed runtime gameplay preset");

    assert_eq!(
        handles.governance_proposal_key.as_deref(),
        Some("preset.governance.civic_hotspot_v1")
    );
    assert_eq!(handles.governance_vote_option.as_deref(), Some("approve"));
    assert_eq!(handles.crisis_id.as_deref(), Some("crisis.auto.8"));
    assert_eq!(
        handles.economic_contract_id.as_deref(),
        Some("preset.contract.civic_hotspot_v1")
    );

    let state = runtime_bridge.state();
    assert!(
        state
            .governance_proposals
            .contains_key("preset.governance.civic_hotspot_v1"),
        "preset governance proposal should exist"
    );
    assert!(
        state
            .economic_contracts
            .contains_key("preset.contract.civic_hotspot_v1"),
        "preset economic contract should exist"
    );
    assert!(
        state
            .crises
            .contains_key(handles.crisis_id.as_deref().unwrap_or("")),
        "preset crisis id should exist"
    );
}
