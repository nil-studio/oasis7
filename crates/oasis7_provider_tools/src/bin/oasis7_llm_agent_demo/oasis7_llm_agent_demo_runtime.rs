fn main() {
    let args: Vec<String> = env::args().collect();
    let options = match parse_options(args.iter().skip(1).map(|arg| arg.as_str())) {
        Ok(options) => options,
        Err(err) => {
            eprintln!("{err}");
            print_help();
            process::exit(1);
        }
    };

    let config = WorldConfig::default();
    let (mut kernel, seed_label) = if let Some(state_dir) = options.load_state_dir.as_ref() {
        match WorldKernel::load_from_dir(state_dir) {
            Ok(kernel) => (kernel, "loaded".to_string()),
            Err(err) => {
                eprintln!("failed to load world state from {}: {err:?}", state_dir);
                process::exit(1);
            }
        }
    } else {
        let init = WorldInitConfig::from_scenario(options.scenario, &config);
        match initialize_kernel(config, init) {
            Ok((kernel, report)) => (kernel, report.seed.to_string()),
            Err(err) => {
                eprintln!("failed to initialize world: {err:?}");
                process::exit(1);
            }
        }
    };

    let mut runner: AgentRunner<DemoAgentBehavior> = AgentRunner::new();
    let mut agent_ids: Vec<String> = kernel.model().agents.keys().cloned().collect();
    agent_ids.sort();

    if agent_ids.is_empty() {
        eprintln!("no agents in scenario {}", options.scenario.as_str());
        process::exit(1);
    }

    for agent_id in &agent_ids {
        let behavior = match build_demo_behavior(agent_id, &options) {
            Ok(behavior) => behavior,
            Err(err) => {
                eprintln!("failed to create {} behavior for {agent_id}: {err}", options.decision_source.as_str());
                process::exit(1);
            }
        };
        runner.register(behavior);
    }

    println!("scenario: {}", options.scenario.as_str());
    println!("seed: {}", seed_label);
    println!("agents: {}", agent_ids.len());
    if let Some(path) = options.load_state_dir.as_ref() {
        println!("state_dir_loaded: {path}");
    }
    println!("ticks: {}", options.ticks);
    println!("decision_source: {}", options.decision_source.as_str());
    if let Some(url) = options.agent_provider_url.as_ref() {
        println!("agent_provider_url: {url}");
    }
    println!(
        "runtime_gameplay_bridge: {}",
        if options.runtime_gameplay_bridge {
            1
        } else {
            0
        }
    );
    println!(
        "runtime_gameplay_preset: {}",
        options.runtime_gameplay_preset.as_str()
    );
    println!(
        "coverage_bootstrap_profile: {}",
        options.coverage_bootstrap_profile.as_str()
    );

    let mut runtime_gameplay_bridge = if options.runtime_gameplay_bridge {
        match RuntimeGameplayBridge::from_kernel(&kernel) {
            Ok(bridge) => Some(bridge),
            Err(err) => {
                eprintln!("failed to initialize runtime gameplay bridge: {err}");
                process::exit(1);
            }
        }
    } else {
        None
    };
    let mut runtime_gameplay_preset_handles = if let Some(bridge) = runtime_gameplay_bridge.as_mut()
    {
        match bridge.apply_preset(options.runtime_gameplay_preset) {
            Ok(handles) => handles,
            Err(err) => {
                eprintln!(
                    "failed to apply runtime gameplay preset {}: {}",
                    options.runtime_gameplay_preset.as_str(),
                    err
                );
                process::exit(1);
            }
        }
    } else {
        RuntimeGameplayPresetHandles::default()
    };
    if let Some(proposal_key) = runtime_gameplay_preset_handles
        .governance_proposal_key
        .as_ref()
    {
        println!("runtime_gameplay_preset_proposal_key: {proposal_key}");
    }
    if let Some(vote_option) = runtime_gameplay_preset_handles
        .governance_vote_option
        .as_ref()
    {
        println!("runtime_gameplay_preset_vote_option: {vote_option}");
    }
    if let Some(crisis_id) = runtime_gameplay_preset_handles.crisis_id.as_ref() {
        println!("runtime_gameplay_preset_crisis_id: {crisis_id}");
    }
    if let Some(contract_id) = runtime_gameplay_preset_handles
        .economic_contract_id
        .as_ref()
    {
        println!("runtime_gameplay_preset_contract_id: {contract_id}");
    }
    if let Some(counterparty) = runtime_gameplay_preset_handles
        .economic_contract_counterparty
        .as_ref()
    {
        println!("runtime_gameplay_preset_counterparty: {counterparty}");
    }

    let mut run_report = DemoRunReport::new(options.scenario.as_str().to_string(), options.ticks);
    let coverage_bootstrap_actions =
        if options.coverage_bootstrap_profile == CoverageBootstrapProfile::None {
            0
        } else {
            match run_coverage_bootstrap(
                options.coverage_bootstrap_profile,
                &mut kernel,
                &mut runtime_gameplay_bridge,
                &mut runtime_gameplay_preset_handles,
                &mut run_report,
            ) {
                Ok(action_count) => action_count,
                Err(err) => {
                    eprintln!("failed to apply coverage bootstrap profile: {err}");
                    process::exit(1);
                }
            }
        };
    if coverage_bootstrap_actions > 0 {
        println!("coverage_bootstrap_actions: {coverage_bootstrap_actions}");
    }
    let mut next_prompt_switch_idx = 0usize;

    for idx in 0..options.ticks {
        let tick = idx + 1;
        while next_prompt_switch_idx < options.prompt_switches.len()
            && tick >= options.prompt_switches[next_prompt_switch_idx].tick
        {
            let switch = options.prompt_switches[next_prompt_switch_idx].clone();
            for agent_id in runner.agent_ids() {
                if let Some(agent) = runner.get_mut(agent_id.as_str()) {
                    agent.behavior.apply_prompt_switch(&switch);
                }
            }
            println!(
                "tick={} prompt_switch_applied=true switch_index={} switch_tick={}",
                tick,
                next_prompt_switch_idx + 1,
                switch.tick
            );
            next_prompt_switch_idx += 1;
        }

        match runner.tick_decide_only(&mut kernel) {
            Some(result) => {
                run_report.active_ticks += 1;
                run_report.observe_decision(&result.decision);

                if let Some(trace) = result.decision_trace.as_ref() {
                    run_report.observe_trace(trace);
                    if options.print_llm_io {
                        print_llm_io_trace(
                            tick,
                            result.agent_id.as_str(),
                            trace,
                            options.llm_io_max_chars,
                        );
                    }
                }

                let action_result = if let AgentDecision::Act(action) = &result.decision {
                    let mut used_runtime_bridge = false;
                    let action_execution_started_at = Instant::now();
                    let executed = if let Some(bridge) = runtime_gameplay_bridge.as_mut() {
                        if is_bridgeable_action(action) {
                            match bridge.execute(tick, result.agent_id.as_str(), action.clone()) {
                                Ok(bridged) => {
                                    used_runtime_bridge = true;
                                    run_report.observe_runtime_bridge_result(&bridged);
                                    bridged
                                }
                                Err(err) => {
                                    eprintln!(
                                        "runtime gameplay bridge execute failed at tick {} agent {}: {}",
                                        tick, result.agent_id, err
                                    );
                                    execute_action_in_kernel(
                                        &mut kernel,
                                        result.agent_id.as_str(),
                                        action.clone(),
                                    )
                                }
                            }
                        } else {
                            execute_action_in_kernel(
                                &mut kernel,
                                result.agent_id.as_str(),
                                action.clone(),
                            )
                        }
                    } else {
                        execute_action_in_kernel(
                            &mut kernel,
                            result.agent_id.as_str(),
                            action.clone(),
                        )
                    };
                    runner.record_external_action_execution_duration(
                        action_execution_started_at.elapsed(),
                    );

                    let _ = runner.notify_action_result(result.agent_id.as_str(), &executed);
                    if used_runtime_bridge {
                        advance_kernel_time_with_noop_move(&mut kernel, result.agent_id.as_str());
                    }
                    Some(executed)
                } else {
                    None
                };

                if let Some(action_result) = action_result.as_ref() {
                    run_report.observe_action_result(idx + 1, action_result);
                    println!(
                        "tick={} agent={} success={} action={:?}",
                        tick, result.agent_id, action_result.success, action_result.action
                    );
                    if let Some(reason) = action_result.reject_reason() {
                        println!(
                            "tick={} agent={} reject_reason={:?}",
                            tick, result.agent_id, reason
                        );
                    }
                } else {
                    println!(
                        "tick={} agent={} decision={:?}",
                        tick, result.agent_id, result.decision
                    );
                }
            }
            None => {
                println!("tick={} idle", tick);
                break;
            }
        }
    }

    let metrics = runner.metrics();
    run_report.total_actions = metrics.total_actions + coverage_bootstrap_actions;
    run_report.total_decisions = metrics.total_decisions;
    run_report.runtime_perf = metrics.runtime_perf;
    run_report.world_time = kernel.time();
    run_report.journal_events = kernel.journal().len();
    run_report.finalize();

    if let Some(path) = options.report_json.as_ref() {
        if let Err(err) = write_report_json(path, &run_report) {
            eprintln!("failed to write report json: {err}");
            process::exit(1);
        }
        println!("report_json: {path}");
    }
    if let Some(path) = options.save_state_dir.as_ref() {
        if let Err(err) = kernel.save_to_dir(path) {
            eprintln!("failed to save world state to {}: {err:?}", path);
            process::exit(1);
        }
        println!("state_dir_saved: {path}");
    }

    println!("active_ticks: {}", run_report.active_ticks);
    println!("total_actions: {}", run_report.total_actions);
    println!("total_decisions: {}", run_report.total_decisions);
    println!("world_time: {}", run_report.world_time);
    println!("journal_events: {}", run_report.journal_events);
    println!("action_success: {}", run_report.action_success);
    println!("action_failure: {}", run_report.action_failure);
    if !run_report.action_reject_reason_counts.is_empty() {
        for (reason, count) in &run_report.action_reject_reason_counts {
            println!("action_reject_reason_{}: {}", reason, count);
        }
    }
    if !run_report.action_kind_counts.is_empty() {
        for (kind, count) in &run_report.action_kind_counts {
            println!("action_kind_{}: {}", kind, count);
        }
    }
    if !run_report.first_action_tick.is_empty() {
        for (kind, tick) in &run_report.first_action_tick {
            println!("first_action_tick_{}: {}", kind, tick);
        }
    }
    println!("decision_wait: {}", run_report.decision_counts.wait);
    println!(
        "decision_wait_ticks: {}",
        run_report.decision_counts.wait_ticks
    );
    println!("decision_act: {}", run_report.decision_counts.act);
    println!(
        "decision_module_command: {}",
        run_report.decision_counts.module_command
    );
    println!("trace_count: {}", run_report.trace_counts.traces);
    println!(
        "llm_skipped_ticks: {}",
        run_report.trace_counts.llm_skipped_ticks
    );
    println!(
        "llm_skipped_tick_ratio_ppm: {}",
        run_report.trace_counts.llm_skipped_tick_ratio_ppm
    );
    println!("llm_errors: {}", run_report.trace_counts.llm_errors);
    println!("parse_errors: {}", run_report.trace_counts.parse_errors);
    println!(
        "repair_rounds_total: {}",
        run_report.trace_counts.repair_rounds_total
    );
    println!(
        "repair_rounds_max: {}",
        run_report.trace_counts.repair_rounds_max
    );
    println!(
        "llm_input_chars_avg: {}",
        run_report.trace_counts.llm_input_chars_avg
    );
    println!(
        "llm_input_chars_max: {}",
        run_report.trace_counts.llm_input_chars_max
    );
    println!(
        "runtime_bridge_actions: {}",
        run_report.runtime_bridge_actions
    );
    println!(
        "runtime_bridge_action_success: {}",
        run_report.runtime_bridge_action_success
    );
    println!(
        "runtime_bridge_action_failure: {}",
        run_report.runtime_bridge_action_failure
    );
    println!(
        "runtime_perf_health: {}",
        run_report.runtime_perf.health.as_str()
    );
    println!(
        "runtime_perf_bottleneck: {}",
        run_report.runtime_perf.bottleneck.as_str()
    );
    println!(
        "runtime_perf_tick_p95_ms: {:.3}",
        run_report.runtime_perf.tick.p95_ms
    );
    println!(
        "runtime_perf_llm_api_p95_ms: {:.3}",
        run_report.runtime_perf.llm_api.p95_ms
    );
    println!(
        "runtime_perf_tick_over_budget_ratio_ppm: {}",
        run_report.runtime_perf.tick.over_budget_ratio_ppm
    );
}

fn build_demo_behavior(
    agent_id: &str,
    options: &CliOptions,
) -> Result<DemoAgentBehavior, String> {
    match options.decision_source {
        DecisionSource::LlmResponses => {
            let mut behavior =
                LlmAgentBehavior::from_env(agent_id.to_string()).map_err(|err| err.to_string())?;
            if options.has_initial_prompt_override() {
                behavior.apply_prompt_overrides(
                    options.llm_system_prompt.clone(),
                    options.llm_short_term_goal.clone(),
                    options.llm_long_term_goal.clone(),
                );
            }
            Ok(DemoAgentBehavior::Llm(behavior))
        }
        DecisionSource::ProviderLoopbackHttp => {
            let base_url = options.agent_provider_url.as_deref().ok_or_else(|| {
                "--agent-provider-url is required when --decision-source provider_loopback_http"
                    .to_string()
            })?;
            let adapter = ProviderLoopbackAdapter::new(
                base_url,
                options.agent_provider_auth_token.as_deref(),
                resolve_provider_loopback_transport_timeout_ms(options),
            )
            .map_err(|err| err.to_string())?;
            let mut behavior = ProviderBackedAgentBehavior::new(
                agent_id.to_string(),
                adapter,
                provider_phase1_action_catalog(),
            )
            .legacy_compatibility()
            .with_provider_config_ref(format!(
                "provider://loopback-http/llm-agent-demo/pid-{}/{}",
                process::id(),
                agent_id
            ))
            .with_agent_profile(options.agent_provider_profile.clone())
            .with_execution_mode(ProviderExecutionMode::HeadlessAgent)
            .with_timeout_budget_ms(options.agent_provider_decision_timeout_ms)
            .with_environment_class("llm_agent_demo");
            let prompt_context = ProviderPromptContext::from_options(options);
            if let Some(memory_summary) = prompt_context.memory_summary() {
                behavior = behavior.with_memory_summary(memory_summary);
            }
            Ok(DemoAgentBehavior::ProviderBacked {
                behavior,
                prompt_context,
            })
        }
    }
}

fn resolve_provider_loopback_transport_timeout_ms(options: &CliOptions) -> u64 {
    options
        .agent_provider_connect_timeout_ms
        .max(options.agent_provider_decision_timeout_ms)
}

fn write_report_json(path: &str, run_report: &DemoRunReport) -> Result<(), String> {
    let report_path = Path::new(path);
    if let Some(parent) = report_path.parent()
        && !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|err| {
                format!(
                    "failed to create report directory {}: {err}",
                    parent.display()
                )
            })?;
        }

    let content = serde_json::to_string_pretty(run_report)
        .map_err(|err| format!("failed to serialize report json: {err}"))?;
    fs::write(report_path, format!("{content}\n")).map_err(|err| {
        format!(
            "failed to write report file {}: {err}",
            report_path.display()
        )
    })
}

fn normalize_prompt_switches(
    mut switches: Vec<PromptSwitchSpec>,
    source_hint: &str,
) -> Result<Vec<PromptSwitchSpec>, String> {
    if switches.is_empty() {
        return Err(format!("{source_hint} requires at least one switch entry"));
    }

    switches.sort_by_key(|entry| entry.tick);
    let mut previous_tick: Option<u64> = None;
    for entry in &switches {
        if entry.tick == 0 {
            return Err(format!("{source_hint} tick must be a positive integer"));
        }
        if !entry.has_override() {
            return Err(format!(
                "{source_hint} tick={} requires at least one llm_* override field",
                entry.tick
            ));
        }
        if previous_tick == Some(entry.tick) {
            return Err(format!(
                "{source_hint} contains duplicated tick={}",
                entry.tick
            ));
        }
        previous_tick = Some(entry.tick);
    }
    Ok(switches)
}

fn parse_prompt_switches_json(raw: &str) -> Result<Vec<PromptSwitchSpec>, String> {
    let parsed: Vec<PromptSwitchSpec> = serde_json::from_str(raw)
        .map_err(|err| format!("invalid --prompt-switches-json: {err}"))?;
    normalize_prompt_switches(parsed, "--prompt-switches-json")
}

fn parse_options<'a>(args: impl Iterator<Item = &'a str>) -> Result<CliOptions, String> {
    let mut options = CliOptions::default();
    let mut scenario_arg: Option<&str> = None;

    let mut iter = args.peekable();
    while let Some(arg) = iter.next() {
        match arg {
            "--help" | "-h" => {
                print_help();
                process::exit(0);
            }
            "--ticks" => {
                let raw = iter
                    .next()
                    .ok_or_else(|| "--ticks requires a positive integer".to_string())?;
                options.ticks = raw
                    .parse::<u64>()
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or_else(|| "--ticks requires a positive integer".to_string())?;
            }
            "--scenario" => {
                scenario_arg = Some(
                    iter.next()
                        .ok_or_else(|| "--scenario requires a scenario name".to_string())?,
                );
            }
            "--report-json" => {
                options.report_json = Some(
                    iter.next()
                        .ok_or_else(|| "--report-json requires a file path".to_string())?
                        .to_string(),
                );
            }
            "--coverage-bootstrap-profile" => {
                let raw = iter
                    .next()
                    .ok_or_else(|| {
                        "--coverage-bootstrap-profile requires a profile name".to_string()
                    })?
                    .to_string();
                options.coverage_bootstrap_profile =
                    CoverageBootstrapProfile::parse(raw.as_str()).ok_or_else(|| {
                        format!(
                            "invalid --coverage-bootstrap-profile: {} (expected none|industrial|gameplay|hybrid)",
                            raw
                        )
                    })?;
            }
            "--runtime-gameplay-bridge" => {
                options.runtime_gameplay_bridge = true;
            }
            "--no-runtime-gameplay-bridge" => {
                options.runtime_gameplay_bridge = false;
            }
            "--runtime-gameplay-preset" => {
                let raw = iter
                    .next()
                    .ok_or_else(|| "--runtime-gameplay-preset requires a preset name".to_string())?
                    .to_string();
                options.runtime_gameplay_preset = RuntimeGameplayPreset::parse(raw.as_str())
                    .ok_or_else(|| {
                        format!(
                            "invalid --runtime-gameplay-preset: {} (expected none|civic_hotspot_v1)",
                            raw
                        )
                    })?;
            }
            "--decision-source" | "--agent-decision-source" => {
                let raw = iter
                    .next()
                    .ok_or_else(|| "--decision-source requires a source name".to_string())?
                    .to_string();
                options.decision_source = DecisionSource::parse(raw.as_str()).ok_or_else(|| {
                    format!(
                        "invalid --decision-source: {} (expected llm_responses|provider_loopback_http)",
                        raw
                    )
                })?;
            }
            "--agent-provider-url" => {
                options.agent_provider_url = Some(
                    iter.next()
                        .ok_or_else(|| "--agent-provider-url requires a URL".to_string())?
                        .to_string(),
                );
            }
            "--agent-provider-auth-token" => {
                options.agent_provider_auth_token = Some(
                    iter.next()
                        .ok_or_else(|| "--agent-provider-auth-token requires a token".to_string())?
                        .to_string(),
                );
            }
            "--agent-provider-connect-timeout-ms" => {
                let raw = iter.next().ok_or_else(|| {
                    "--agent-provider-connect-timeout-ms requires a positive integer".to_string()
                })?;
                options.agent_provider_connect_timeout_ms = raw
                    .parse::<u64>()
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or_else(|| {
                        "--agent-provider-connect-timeout-ms requires a positive integer"
                            .to_string()
                    })?;
            }
            "--agent-provider-decision-timeout-ms" => {
                let raw = iter.next().ok_or_else(|| {
                    "--agent-provider-decision-timeout-ms requires a positive integer".to_string()
                })?;
                options.agent_provider_decision_timeout_ms = raw
                    .parse::<u64>()
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or_else(|| {
                        "--agent-provider-decision-timeout-ms requires a positive integer"
                            .to_string()
                    })?;
            }
            "--agent-provider-profile" => {
                options.agent_provider_profile = iter
                    .next()
                    .ok_or_else(|| "--agent-provider-profile requires a profile name".to_string())?
                    .to_string();
            }
            "--load-state-dir" => {
                options.load_state_dir = Some(
                    iter.next()
                        .ok_or_else(|| "--load-state-dir requires a directory path".to_string())?
                        .to_string(),
                );
            }
            "--save-state-dir" => {
                options.save_state_dir = Some(
                    iter.next()
                        .ok_or_else(|| "--save-state-dir requires a directory path".to_string())?
                        .to_string(),
                );
            }
            "--print-llm-io" => {
                options.print_llm_io = true;
            }
            "--llm-io-max-chars" => {
                let raw = iter
                    .next()
                    .ok_or_else(|| "--llm-io-max-chars requires a positive integer".to_string())?;
                options.llm_io_max_chars = Some(
                    raw.parse::<usize>()
                        .ok()
                        .filter(|value| *value > 0)
                        .ok_or_else(|| {
                            "--llm-io-max-chars requires a positive integer".to_string()
                        })?,
                );
            }
            "--llm-system-prompt" => {
                options.llm_system_prompt = Some(
                    iter.next()
                        .ok_or_else(|| "--llm-system-prompt requires prompt text".to_string())?
                        .to_string(),
                );
            }
            "--llm-short-term-goal" => {
                options.llm_short_term_goal = Some(
                    iter.next()
                        .ok_or_else(|| "--llm-short-term-goal requires goal text".to_string())?
                        .to_string(),
                );
            }
            "--llm-long-term-goal" => {
                options.llm_long_term_goal = Some(
                    iter.next()
                        .ok_or_else(|| "--llm-long-term-goal requires goal text".to_string())?
                        .to_string(),
                );
            }
            "--prompt-switch-tick" => {
                let raw = iter.next().ok_or_else(|| {
                    "--prompt-switch-tick requires a positive integer".to_string()
                })?;
                options.prompt_switch_tick = Some(
                    raw.parse::<u64>()
                        .ok()
                        .filter(|value| *value > 0)
                        .ok_or_else(|| {
                            "--prompt-switch-tick requires a positive integer".to_string()
                        })?,
                );
            }
            "--switch-llm-system-prompt" => {
                options.switch_llm_system_prompt = Some(
                    iter.next()
                        .ok_or_else(|| {
                            "--switch-llm-system-prompt requires prompt text".to_string()
                        })?
                        .to_string(),
                );
            }
            "--switch-llm-short-term-goal" => {
                options.switch_llm_short_term_goal = Some(
                    iter.next()
                        .ok_or_else(|| {
                            "--switch-llm-short-term-goal requires goal text".to_string()
                        })?
                        .to_string(),
                );
            }
            "--switch-llm-long-term-goal" => {
                options.switch_llm_long_term_goal = Some(
                    iter.next()
                        .ok_or_else(|| {
                            "--switch-llm-long-term-goal requires goal text".to_string()
                        })?
                        .to_string(),
                );
            }
            "--prompt-switches-json" => {
                options.prompt_switches_json = Some(
                    iter.next()
                        .ok_or_else(|| "--prompt-switches-json requires a JSON string".to_string())?
                        .to_string(),
                );
            }
            _ => {
                if scenario_arg.is_none() {
                    scenario_arg = Some(arg);
                } else {
                    return Err(format!("unexpected argument: {arg}"));
                }
            }
        }
    }

    if let Some(name) = scenario_arg {
        options.scenario = WorldScenario::parse(name).ok_or_else(|| {
            format!(
                "unknown scenario: {name}. available: {}",
                WorldScenario::variants().join(", ")
            )
        })?;
    }

    if options.prompt_switches_json.is_some()
        && (options.prompt_switch_tick.is_some() || options.has_switch_prompt_override())
    {
        return Err(
            "cannot combine --prompt-switches-json with --prompt-switch-tick/--switch-llm-*"
                .to_string(),
        );
    }

    if let Some(raw_json) = options.prompt_switches_json.as_ref() {
        options.prompt_switches = parse_prompt_switches_json(raw_json)?;
    } else {
        if options.has_switch_prompt_override() && options.prompt_switch_tick.is_none() {
            return Err(
                "--prompt-switch-tick is required when switch prompt overrides are set".to_string(),
            );
        }
        if options.prompt_switch_tick.is_some() && !options.has_switch_prompt_override() {
            return Err(
                "--prompt-switch-tick requires at least one --switch-llm-* override".to_string(),
            );
        }
        if let Some(tick) = options.prompt_switch_tick {
            options.prompt_switches = normalize_prompt_switches(
                vec![PromptSwitchSpec {
                    tick,
                    llm_system_prompt: options.switch_llm_system_prompt.clone(),
                    llm_short_term_goal: options.switch_llm_short_term_goal.clone(),
                    llm_long_term_goal: options.switch_llm_long_term_goal.clone(),
                }],
                "legacy --prompt-switch-tick",
            )?;
        }
    }

    if !options.runtime_gameplay_bridge
        && options.runtime_gameplay_preset != RuntimeGameplayPreset::None
    {
        return Err(
            "--runtime-gameplay-preset requires --runtime-gameplay-bridge to be enabled"
                .to_string(),
        );
    }
    if !options.runtime_gameplay_bridge
        && options.coverage_bootstrap_profile.requires_runtime_bridge()
    {
        return Err(
            "--coverage-bootstrap-profile gameplay|hybrid requires --runtime-gameplay-bridge to be enabled"
                .to_string(),
        );
    }
    if options.decision_source == DecisionSource::ProviderLoopbackHttp
        && options.agent_provider_url.is_none()
    {
        return Err(
            "--agent-provider-url is required when --decision-source provider_loopback_http"
                .to_string(),
        );
    }

    Ok(options)
}

fn print_help() {
    println!(
        "Usage: oasis7_llm_agent_demo [scenario] [--ticks <n>] [--report-json <path>] [--print-llm-io] [--llm-io-max-chars <n>] [prompt overrides]"
    );
    println!("Options:");
    println!("  --scenario <name>  Scenario name (default: llm_bootstrap)");
    println!("  --ticks <n>        Max runner ticks (default: 20)");
    println!("  --report-json <path>  Persist run summary as JSON report");
    println!(
        "  --coverage-bootstrap-profile <name>  Run deterministic action bootstrap before LLM loop (none|industrial|gameplay|hybrid)"
    );
    println!("  --load-state-dir <path>  Load simulator state from directory");
    println!("  --save-state-dir <path>  Save simulator state to directory after run");
    println!(
        "  --runtime-gameplay-bridge / --no-runtime-gameplay-bridge  Enable or disable runtime bridge for gameplay/economic actions (default: enabled)"
    );
    println!(
        "  --runtime-gameplay-preset <name>  Seed runtime gameplay events before loop (none|civic_hotspot_v1)"
    );
    println!(
        "  --decision-source <name>  Decision backend: llm_responses|provider_loopback_http (default: llm_responses)"
    );
    println!("  --agent-provider-url <url>  Local provider bridge URL for provider_loopback_http");
    println!(
        "  --agent-provider-connect-timeout-ms <n>  Provider bridge connect timeout (default: 60000)"
    );
    println!(
        "  --agent-provider-decision-timeout-ms <n>  Provider decision budget (default: 60000)"
    );
    println!("  --agent-provider-profile <name>  Provider agent profile label");
    println!("  --print-llm-io     Print LLM input/output to stdout for each tick");
    println!("  --llm-io-max-chars <n>  Truncate each LLM input/output block to n chars");
    println!("  --llm-system-prompt <text>  Override default system prompt for this run");
    println!("  --llm-short-term-goal <text>  Override default short-term goal for this run");
    println!("  --llm-long-term-goal <text>  Override default long-term goal for this run");
    println!("  --prompt-switch-tick <n>  Apply switch prompt overrides at tick n (1-based)");
    println!("  --switch-llm-system-prompt <text>  System prompt used after --prompt-switch-tick");
    println!(
        "  --switch-llm-short-term-goal <text>  Short-term goal used after --prompt-switch-tick"
    );
    println!(
        "  --switch-llm-long-term-goal <text>  Long-term goal used after --prompt-switch-tick"
    );
    println!(
        "  --prompt-switches-json <json>  Multi-stage switch plan (array of {{\"tick\":n,\"llm_*\":...}}); cannot be mixed with legacy --prompt-switch-* options"
    );
    println!(
        "Available scenarios: {}",
        WorldScenario::variants().join(", ")
    );
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
