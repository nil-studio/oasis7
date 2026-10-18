use std::env;
use std::process;

use oasis7::viewer::generate_viewer_demo;
use oasis7_simulator::WorldScenario;

fn main() {
    let mut args = env::args().skip(1);
    let mut scenario: Option<String> = None;
    let mut out_dir = ".data/world_viewer_data".to_string();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                print_help();
                return;
            }
            "--out" => {
                out_dir = match args.next() {
                    Some(dir) => dir,
                    None => {
                        eprintln!("--out requires a directory path");
                        process::exit(1);
                    }
                };
            }
            "--scenario" => {
                scenario = match args.next() {
                    Some(name) => Some(name),
                    None => {
                        eprintln!("--scenario requires a name");
                        process::exit(1);
                    }
                };
            }
            _ => {
                if scenario.is_none() {
                    scenario = Some(arg);
                }
            }
        }
    }

    let scenario = match scenario {
        Some(name) => match WorldScenario::parse(&name) {
            Some(scenario) => scenario,
            None => {
                eprintln!("Unknown scenario: {name}");
                eprintln!(
                    "Available scenarios: {}",
                    WorldScenario::variants().join(", ")
                );
                process::exit(1);
            }
        },
        None => WorldScenario::TwinRegionBootstrap,
    };

    let summary = match generate_viewer_demo(&out_dir, scenario) {
        Ok(summary) => summary,
        Err(err) => {
            eprintln!("failed to generate viewer demo data: {err:?}");
            process::exit(1);
        }
    };

    println!("scenario: {}", scenario.as_str());
    println!("seed: {}", summary.init.seed);
    if let Some(asteroid_fragment_seed) = summary.init.asteroid_fragment_seed {
        println!("asteroid_fragment_seed: {asteroid_fragment_seed}");
    }
    println!("locations: {}", summary.init.locations);
    println!("agents: {}", summary.init.agents);
    println!("actions: {}", summary.actions);
    println!("events: {}", summary.events);
    println!("output_dir: {}", out_dir);
    println!("next: run oasis7_viewer_server and oasis7 Viewer to connect");
}

fn print_help() {
    println!("Usage: oasis7_viewer_demo [scenario] [--out <dir>]");
    println!("Options:");
    println!("  --out <dir>       Output directory (default: .data/world_viewer_data)");
    println!("  --scenario <name> Scenario name (default: twin_region_bootstrap)");
    println!(
        "Available scenarios: {}",
        WorldScenario::variants().join(", ")
    );
}
