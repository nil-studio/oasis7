use crate::device_client::{Client, pending, print};
use crate::operation_store::ClientStateKind;
use oasis7_client_api::world_service::*;
use serde_json::json;
use std::{collections::BTreeMap, path::Path};
const HELP: &str = "oasis7_game_client --config <device.json> <command> [options]\nCommands:\n  schedule-essential --context <context-id> --factory <factory-id> --recipe <recipe-id> --batches <positive-u32> --obligation <registered-id>\n  world-rule-key --request <stable-id>\n  world-rule --context <context-id> --request <stable-id> --command <typed-json>\n  describe\n  task\n  capabilities\n  industry\n  context --operation <stable-prepare-id>\n  move --context <prepare-id> --to <x,y,z>\n  transfer --context <prepare-id> --from <ledger> --to <ledger> --material <kind> --amount <positive-i64> --distance-km <nonnegative-i64> --priority <standard|urgent> --routes <route-id,...|direct>\n  quote-build --context <prepare-id> --site <site-id> --factory <factory-id> --kind <factory.smelter.mk1|factory.assembler.mk1>\n  build --context <prepare-id> --site <site-id> --factory <factory-id> --kind <factory.smelter.mk1|factory.assembler.mk1>\n  quote-schedule --context <prepare-id> --factory <factory-id> --recipe <canonical-recipe-id> --batches <positive-u32>\n  schedule --context <prepare-id> --factory <factory-id> --recipe <canonical-recipe-id> --batches <positive-u32>\n  outcome --context <prepare-id> --reason <executor_aborted|provider_failed|transport_exhausted|context_expired>\n  lookup --context <prepare-id> --kind <prefix|action|outcome>\nConfiguration: endpoint, world_id, genesis_digest, agent_id, service_public_key, signer_file, state_directory.\nThe signer is an operator-owned mode-0600 raw 32-byte file; state_directory is a private encrypted operation cache.\nAuthenticated executor: --executor-config <executor.json> <same-command> [same-options]. Public config contains identity, login_operation, executor_vault_directory, service_public_key, full fence and state_directory; private unlock only through trusted local stdin. No owner signing or Runtime launch.\nHTTP loopback requires all local listening processes to be trusted. Use authenticated TLS for untrusted multi-user hosts.";

pub fn run(args: Vec<String>) -> Result<(), String> {
    #[cfg(unix)]
    if args
        .first()
        .is_some_and(|v| ["broker-serve", "broker-profile", "broker-call"].contains(&v.as_str()))
    {
        return crate::runtime_broker_cli::run(&args);
    }
    if args == ["identity-operation-id"] {
        use base64::Engine;
        let mut bytes = [0; 32];
        getrandom::fill(&mut bytes).map_err(|_| "identity randomness unavailable")?;
        return print(
            &json!({"operation_id":base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)}),
        );
    }
    #[cfg(unix)]
    if args.first().is_some_and(|v| v == "--identity-config") {
        return crate::identity_cli::run(args);
    }
    #[cfg(unix)]
    if args.first().is_some_and(|v| v == "--entry-config") {
        return crate::participant_cli::run(args);
    }
    if args.is_empty() || args == ["--help"] {
        println!("{HELP}");
        #[cfg(unix)]
        println!("{}", crate::identity_cli::HELP);
        #[cfg(unix)]
        println!("{}", crate::participant_cli::HELP);
        #[cfg(unix)]
        println!("{}", crate::runtime_broker_cli::HELP);
        return Ok(());
    }
    if args.len() < 3 || !["--config", "--executor-config"].contains(&args[0].as_str()) {
        return Err(HELP.into());
    }
    let command = &args[2];
    let allowed: &[&str] = match command.as_str() {
        "describe" | "task" | "capabilities" | "industry" => &[],
        "quote-build" | "build" => &["--context", "--site", "--factory", "--kind"],
        "quote-schedule" | "schedule" => &["--context", "--factory", "--recipe", "--batches"],
        "transfer" => &[
            "--context",
            "--from",
            "--to",
            "--material",
            "--amount",
            "--distance-km",
            "--priority",
            "--routes",
        ],
        "world-rule" => &["--context", "--request", "--command"],
        "world-rule-key" => &["--request"],
        "schedule-essential" => &[
            "--context",
            "--factory",
            "--recipe",
            "--batches",
            "--obligation",
        ],
        "context" => &["--operation"],
        "move" => &["--context", "--to"],
        "outcome" => &["--context", "--reason"],
        "lookup" => &["--context", "--kind"],
        _ => return Err("unknown client command; use --help".into()),
    };
    if !(args.len() - 3).is_multiple_of(2) {
        return Err("options require a value".into());
    }
    let mut options = BTreeMap::new();
    for pair in args[3..].chunks_exact(2) {
        if !allowed.contains(&pair[0].as_str())
            || options.insert(pair[0].as_str(), pair[1].as_str()).is_some()
        {
            return Err("unsupported or duplicate option".into());
        }
    }
    let required = |name| {
        options
            .get(name)
            .copied()
            .ok_or_else(|| format!("missing {name}"))
    };
    let client = if args[0] == "--executor-config" {
        #[cfg(unix)]
        {
            use std::io::{IsTerminal, Read};
            if std::io::stdin().is_terminal() {
                return Err("action_required: executor unlock requires trusted local private stdin; never place unlock in model context".into());
            }
            let mut password = zeroize::Zeroizing::new(Vec::new());
            std::io::stdin()
                .take(1025)
                .read_to_end(&mut password)
                .map_err(|_| "executor private unlock unavailable")?;
            if !(12..=1024).contains(&password.len()) {
                return Err("executor private unlock requires 12 to 1024 bytes".into());
            }
            Client::open_authenticated(Path::new(&args[1]), &password)?
        }
        #[cfg(not(unix))]
        {
            return Err("protected executor gateway unavailable on this platform".into());
        }
    } else {
        Client::open(Path::new(&args[1]))?
    };
    match command.as_str() {
        "describe" => print(
            &client
                .remote
                .describe()
                .map_err(|_| "signed service discovery unavailable")?,
        ),
        "task" => print(&client.task_fence()?),
        "capabilities" => print(&client.capabilities()?),
        "industry" => print(&client.industry()?),
        "context" => {
            let operation = required("--operation")?;
            let prefix = client.prefix(operation)?;
            let response = client.submit_and_poll(&prefix)?;
            if !matches!(response.outcome, IntentOutcome::Committed { .. }) {
                return print(&pending(&response));
            }
            let captured = client.captured(operation)?;
            let prepared = &captured.prepared;
            print(&json!({
                "api_revision":1,"context":prepared.identity,
                "goal":{"revision":prepared.snapshot.control.active_goal_revision,"content":prepared.snapshot.active_goal.owner_proof.request.content},
                "actor_observation_schema":prepared.snapshot.actor_observation_schema,
                "actor_observation":prepared.snapshot.actor_observation,
                "capabilities":captured.current_public_context.provider_authority.capability_catalog,
                "capture_commit":captured.capture_commit,
                "current_read_commit":captured.current_public_context.read_commit,
                "prepare_operation_id":operation,
                "action_correlation_identity":prepared.identity.decision_request_id
            }))
        }
        "world-rule-key" => {
            let request_id = required("--request")?;
            if request_id.is_empty() || request_id.len() > 128 || request_id.trim() != request_id {
                return Err(
                    "world-rule request must contain 1 to 128 bytes without outer whitespace"
                        .into(),
                );
            }
            print(&json!({
                "actor_id": client.agent,
                "request_id": request_id,
                "effect_ref": oasis7_agent_api::world_action::world_rule_request_key_v1(&client.agent, request_id)
            }))
        }
        "schedule-essential" => {
            let batches = required("--batches")?
                .parse::<u32>()
                .map_err(|_| "invalid batches")?;
            if batches == 0 {
                return Err("batches must be positive".into());
            }
            let action = oasis7_agent_api::world_action::Action::ScheduleEssentialRecipe {
                requester_agent_id: client.agent.clone(),
                factory_id: required("--factory")?.into(),
                recipe_id: required("--recipe")?.into(),
                batches,
                obligation_id: required("--obligation")?.into(),
            };
            print(&client.submit_action(required("--context")?, action)?)
        }
        "world-rule" => {
            let command =
                serde_json::from_str::<oasis7_agent_api::world_action::WorldRuleCommandV1>(
                    required("--command")?,
                )
                .map_err(|_| "invalid typed world-rule command")?;
            let action = oasis7_agent_api::world_action::Action::WorldRule {
                actor_id: client.agent.clone(),
                request_id: required("--request")?.into(),
                command,
            };
            print(&client.submit_action(required("--context")?, action)?)
        }
        "move" => {
            let operation = required("--context")?;
            let values = required("--to")?
                .split(',')
                .map(str::parse::<i64>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "invalid coordinates")?;
            let [x, y, z] = values.as_slice() else {
                return Err("--to requires x,y,z".into());
            };
            let action = oasis7_agent_api::world_action::Action::MoveAgent {
                agent_id: client.agent.clone(),
                to: oasis7_agent_api::world_action::GeoPos::new(*x, *y, *z),
            };
            print(&client.submit_action(operation, action)?)
        }
        "transfer" => {
            let operation = required("--context")?;
            let action = transfer_action(&client.agent, &options)?;
            print(&client.submit_action(operation, action)?)
        }
        "quote-build" | "build" => {
            let operation = required("--context")?;
            let (site, factory, kind) = build_selection(&options)?;
            if command == "quote-build" {
                print(&client.quote_build(operation, site, factory, kind)?)
            } else {
                print(&client.build(operation, site, factory, kind)?)
            }
        }
        "quote-schedule" | "schedule" => {
            let operation = required("--context")?;
            let (factory, recipe, batches) = schedule_selection(&options)?;
            if command == "quote-schedule" {
                print(&client.quote_schedule(operation, factory, recipe, batches)?)
            } else {
                print(&client.schedule(operation, factory, recipe, batches)?)
            }
        }
        "outcome" => {
            let operation = required("--context")?;
            let reason = required("--reason")?;
            validate_outcome_reason(reason)?;
            print(&client.outcome(operation, reason)?)
        }
        "lookup" => {
            let operation = required("--context")?;
            let kind = match required("--kind")? {
                "prefix" => ClientStateKind::Prefix,
                "action" => ClientStateKind::Action,
                "outcome" => ClientStateKind::Outcome,
                _ => return Err("invalid lookup kind".into()),
            };
            let request = client
                .cached_intent(kind, operation)?
                .ok_or("original operation is not in this device cache")?;
            print(&client.lookup(&request)?)
        }
        _ => unreachable!(),
    }
}

fn validate_outcome_reason(reason: &str) -> Result<(), String> {
    if ![
        "executor_aborted",
        "provider_failed",
        "transport_exhausted",
        "context_expired",
    ]
    .contains(&reason)
    {
        return Err("unsupported context outcome".into());
    }
    Ok(())
}

fn build_selection<'a>(
    options: &BTreeMap<&str, &'a str>,
) -> Result<(&'a str, &'a str, &'a str), String> {
    let required = |key| {
        options
            .get(key)
            .copied()
            .ok_or_else(|| format!("missing {key}"))
    };
    let site = required("--site")?;
    let factory = required("--factory")?;
    let kind = required("--kind")?;
    bounded_id(site)?;
    bounded_id(factory)?;
    if oasis7_agent_api::world_quote::builtin_factory_spec(kind, factory).is_none() {
        return Err("factory kind requires an exact built-in ID".into());
    }
    Ok((site, factory, kind))
}

fn schedule_selection<'a>(
    options: &BTreeMap<&str, &'a str>,
) -> Result<(&'a str, &'a str, u32), String> {
    let required = |key| {
        options
            .get(key)
            .copied()
            .ok_or_else(|| format!("missing {key}"))
    };
    let factory = required("--factory")?;
    let recipe = required("--recipe")?;
    bounded_id(factory)?;
    bounded_id(recipe)?;
    let batches = required("--batches")?
        .parse::<u32>()
        .map_err(|_| "invalid batches")?;
    if oasis7_agent_api::world_quote::builtin_recipe_plan(recipe, batches).is_none() {
        return Err("recipe requires an exact built-in ID and positive u32 batches".into());
    }
    Ok((factory, recipe, batches))
}

fn transfer_action(
    agent: &str,
    options: &BTreeMap<&str, &str>,
) -> Result<oasis7_agent_api::world_action::Action, String> {
    use oasis7_agent_api::world_action::{Action, MaterialTransitPriority};
    let required = |name| {
        options
            .get(name)
            .copied()
            .ok_or_else(|| format!("missing {name}"))
    };
    let ledger = |name| {
        let value = required(name)?;
        bounded_id(value)?;
        oasis7_agent_api::world_action::MaterialLedgerId::try_from(value.to_owned())
            .map_err(|_| "invalid material ledger".to_owned())
    };
    let from_ledger = ledger("--from")?;
    let to_ledger = ledger("--to")?;
    if from_ledger == to_ledger {
        return Err("source and destination must differ".into());
    }
    let kind = required("--material")?;
    bounded_id(kind)?;
    let amount = required("--amount")?
        .parse::<i64>()
        .map_err(|_| "invalid amount")?;
    let distance_km = required("--distance-km")?
        .parse::<i64>()
        .map_err(|_| "invalid distance")?;
    if amount <= 0 || distance_km < 0 {
        return Err("amount must be positive and distance nonnegative".into());
    }
    let priority = match required("--priority")? {
        "standard" => MaterialTransitPriority::Standard,
        "urgent" => MaterialTransitPriority::Urgent,
        _ => return Err("invalid priority".into()),
    };
    let routes = required("--routes")?;
    let route_ids = if routes == "direct" {
        Vec::new()
    } else {
        let values: Vec<_> = routes.split(',').map(str::to_owned).collect();
        if values.len() > 16 {
            return Err("route path exceeds 16 hops".into());
        }
        for value in &values {
            bounded_id(value)?;
        }
        let unique: std::collections::BTreeSet<_> = values.iter().collect();
        if unique.len() != values.len() {
            return Err("route path repeats a route".into());
        }
        values
    };
    Ok(Action::TransferMaterial {
        requester_agent_id: agent.to_owned(),
        from_ledger,
        to_ledger,
        kind: kind.to_owned(),
        amount,
        distance_km,
        priority: Some(priority),
        route_id: None,
        route_ids,
        auto_reroute: false,
    })
}
fn bounded_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 256
        || value
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == ',')
    {
        return Err("invalid bounded identifier".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    #[test]
    fn expired_context_terminal_reason_is_available_to_cli() {
        assert!(validate_outcome_reason("context_expired").is_ok());
        assert!(validate_outcome_reason("pretend_expired").is_err());
    }

    #[test]
    fn factory_commands_reject_injected_authority_and_costs_before_configuration() {
        for command in ["quote-build", "build"] {
            for option in [
                "--spec",
                "--grant",
                "--cost",
                "--power",
                "--builder",
                "--action",
                "--authorization",
            ] {
                let args = [
                    "--config",
                    "/nonexistent-config",
                    command,
                    option,
                    "injected",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect();
                assert_eq!(
                    super::run(args).unwrap_err(),
                    "unsupported or duplicate option"
                );
            }
        }
    }
    #[test]
    fn factory_selection_requires_exact_kind_and_bounded_business_ids() {
        let mut options = std::collections::BTreeMap::from([
            ("--site", "ready-site"),
            ("--factory", "new-factory"),
            ("--kind", "factory.smelter.mk1"),
        ]);
        assert_eq!(
            super::build_selection(&options).unwrap(),
            ("ready-site", "new-factory", "factory.smelter.mk1")
        );
        for kind in ["smelter", "factory.custom", "FACTORY.SMELTER.MK1"] {
            options.insert("--kind", kind);
            assert!(super::build_selection(&options).is_err());
        }
        options.insert("--kind", "factory.assembler.mk1");
        assert!(super::build_selection(&options).is_ok());
        for (key, value) in [("--site", ""), ("--site", "bad site"), ("--factory", "a,b")] {
            let mut invalid = options.clone();
            invalid.insert(key, value);
            assert!(super::build_selection(&invalid).is_err());
        }
    }
    #[test]
    fn recipe_commands_reject_raw_plan_grant_and_cost_before_configuration() {
        for command in ["quote-schedule", "schedule"] {
            for option in [
                "--plan",
                "--grant",
                "--cost",
                "--action",
                "--requester",
                "--authorization",
            ] {
                let args = [
                    "--config",
                    "/nonexistent-config",
                    command,
                    option,
                    "injected",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect();
                assert_eq!(
                    super::run(args).unwrap_err(),
                    "unsupported or duplicate option"
                );
            }
        }
    }
    #[test]
    fn recipe_selection_requires_exact_builtin_and_positive_u32_batches() {
        let mut options = std::collections::BTreeMap::from([
            ("--factory", "smelter"),
            ("--recipe", "recipe.smelter.iron_ingot"),
            ("--batches", "1"),
        ]);
        assert_eq!(
            super::schedule_selection(&options).unwrap(),
            ("smelter", "recipe.smelter.iron_ingot", 1)
        );
        for batches in ["0", "-1", "4294967296", "1.5", "unknown"] {
            options.insert("--batches", batches);
            assert!(super::schedule_selection(&options).is_err());
        }
        options.insert("--batches", "4294967295");
        assert!(super::schedule_selection(&options).is_ok());
        for recipe in [
            "recipe.iron_ingot",
            "recipe.custom",
            "RECIPE.SMELTER.IRON_INGOT",
        ] {
            options.insert("--recipe", recipe);
            assert!(super::schedule_selection(&options).is_err());
        }
    }

    use super::*;
    fn options() -> BTreeMap<&'static str, &'static str> {
        BTreeMap::from([
            ("--from", "agent:configured"),
            ("--to", "factory:smelter"),
            ("--material", "iron_ore"),
            ("--amount", "2"),
            ("--distance-km", "1"),
            ("--priority", "standard"),
            ("--routes", "route-a,route-b"),
        ])
    }
    #[test]
    fn transfer_is_typed_and_requester_is_configured_identity() {
        let oasis7_agent_api::world_action::Action::TransferMaterial {
            requester_agent_id,
            route_ids,
            route_id,
            auto_reroute,
            ..
        } = transfer_action("configured", &options()).unwrap()
        else {
            panic!("wrong action")
        };
        assert_eq!(requester_agent_id, "configured");
        assert_eq!(route_ids, ["route-a", "route-b"]);
        assert!(route_id.is_none());
        assert!(!auto_reroute);
    }
    #[test]
    fn transfer_cli_rejects_raw_action_and_authority_options_before_configuration() {
        for option in [
            "--action",
            "--requester",
            "--grant",
            "--quote",
            "--plan",
            "--auto-reroute",
        ] {
            let error = run(vec![
                "--config".into(),
                "/unopened-config".into(),
                "transfer".into(),
                option.into(),
                "{}".into(),
            ])
            .unwrap_err();
            assert_eq!(error, "unsupported or duplicate option");
        }
        let error = run(vec![
            "--config".into(),
            "/unopened-config".into(),
            "transfer".into(),
            "--amount".into(),
            "1".into(),
            "--amount".into(),
            "2".into(),
        ])
        .unwrap_err();
        assert_eq!(error, "unsupported or duplicate option");
    }
    #[test]
    fn transfer_rejects_invalid_business_arguments() {
        for (key, value) in [
            ("--amount", "0"),
            ("--amount", "9223372036854775808"),
            ("--distance-km", "-1"),
            ("--priority", "admin"),
            ("--from", "unknown:x"),
            ("--to", "agent:configured"),
            ("--routes", "a,a"),
            ("--routes", "a,,b"),
            ("--material", "iron ore"),
        ] {
            let mut input = options();
            input.insert(key, value);
            assert!(transfer_action("configured", &input).is_err(), "{key}");
        }
    }
}
