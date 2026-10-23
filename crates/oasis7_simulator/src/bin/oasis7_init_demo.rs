use oasis7_agent_api::world_action::ResourceKind;
use oasis7_simulator::WorldScenarioSpec;
use oasis7_simulator::{WorldConfig, WorldInitConfig, WorldScenario, build_world_model};
use std::collections::BTreeMap;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if matches!(args.get(1).map(|s| s.as_str()), Some("--help") | Some("-h")) {
        println!("Usage: oasis7_init_demo [scenario]");
        println!("Options:");
        println!("  --summary-only  Only print counts (omit per-location/agent details)");
        println!("  --scenario-file Load scenario spec from JSON file");
        println!(
            "Available scenarios: {}",
            WorldScenario::variants().join(", ")
        );
        return;
    }

    let mut summary_only = false;
    let mut scenario_arg: Option<&str> = None;
    let mut scenario_file: Option<&str> = None;
    let mut arg_iter = args.iter().skip(1);
    while let Some(arg) = arg_iter.next() {
        if arg == "--summary-only" {
            summary_only = true;
        } else if arg == "--scenario-file" {
            scenario_file = match arg_iter.next() {
                Some(path) => Some(path.as_str()),
                None => {
                    eprintln!("--scenario-file requires a path");
                    std::process::exit(1);
                }
            };
        } else if scenario_arg.is_none() {
            scenario_arg = Some(arg);
        }
    }

    let config = WorldConfig::default();
    let (scenario_label, init) = match (scenario_file, scenario_arg) {
        (Some(_), Some(_)) => {
            eprintln!("--scenario-file cannot be combined with a scenario name");
            std::process::exit(1);
        }
        (Some(path), None) => {
            let spec = match WorldScenarioSpec::load_from_path(Path::new(path)) {
                Ok(spec) => spec,
                Err(err) => {
                    eprintln!("Failed to load scenario file: {err}");
                    std::process::exit(1);
                }
            };
            let label = if spec.id.is_empty() {
                "custom".to_string()
            } else {
                spec.id.clone()
            };
            (label, spec.into_init_config(&config))
        }
        (None, Some(name)) => {
            let scenario = match WorldScenario::parse(name) {
                Some(scenario) => scenario,
                None => {
                    eprintln!("Unknown scenario: {name}");
                    eprintln!(
                        "Available scenarios: {}",
                        WorldScenario::variants().join(", ")
                    );
                    std::process::exit(1);
                }
            };
            (
                scenario.as_str().to_string(),
                WorldInitConfig::from_scenario(scenario, &config),
            )
        }
        (None, None) => {
            let scenario = WorldScenario::Minimal;
            (
                scenario.as_str().to_string(),
                WorldInitConfig::from_scenario(scenario, &config),
            )
        }
    };
    let (model, report) = build_world_model(&config, &init).expect("init should succeed");
    let asteroid_fragment_fragments = model.locations.len().saturating_sub(report.locations);

    println!("scenario: {}", scenario_label);
    println!("seed: {}", report.seed);
    println!("locations: {}", report.locations);
    println!("agents: {}", report.agents);
    println!("power_plants: {}", model.power_plants.len());
    println!(
        "asteroid_fragment_fragments: {}",
        asteroid_fragment_fragments
    );
    if !summary_only {
        println!("location_resources:");

        let mut location_ids: Vec<_> = model.locations.keys().collect();
        location_ids.sort();
        for location_id in location_ids {
            let location = &model.locations[location_id];
            let electricity = location.resources.get(ResourceKind::Electricity);
            let data = location.resources.get(ResourceKind::Data);
            println!(
                "- {}: electricity={} data={}",
                location_id, electricity, data
            );
        }

        let mut facility_counts: BTreeMap<String, usize> = BTreeMap::new();
        for plant in model.power_plants.values() {
            let entry = facility_counts
                .entry(plant.location_id.clone())
                .or_insert(0);
            *entry += 1;
        }
        println!("location_facilities:");
        let mut facility_locations: Vec<_> = facility_counts.keys().collect();
        facility_locations.sort();
        for location_id in facility_locations {
            let plants = facility_counts[location_id];
            println!("- {}: power_plants={}", location_id, plants);
        }

        println!("agent_resources:");
        let mut agent_ids: Vec<_> = model.agents.keys().collect();
        agent_ids.sort();
        for agent_id in agent_ids {
            let agent = &model.agents[agent_id];
            let electricity = agent.resources.get(ResourceKind::Electricity);
            let data = agent.resources.get(ResourceKind::Data);
            println!("- {}: electricity={} data={}", agent_id, electricity, data);
        }
    }
}
