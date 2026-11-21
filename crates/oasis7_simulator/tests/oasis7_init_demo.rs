#![cfg(any(feature = "test_tier_required", feature = "test_tier_full"))]

use std::process::{Command, Output};

fn run_init_demo(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_oasis7_init_demo"))
        .args(args)
        .output()
        .expect("run oasis7_init_demo")
}

fn successful_stdout(output: Output) -> String {
    assert!(
        output.status.success(),
        "oasis7_init_demo failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("UTF-8 stdout")
}

fn assert_summary(stdout: &str, scenario: &str) {
    let mut lines = stdout.lines();
    let label = format!("scenario: {scenario}");
    assert_eq!(lines.next(), Some(label.as_str()));
    for prefix in [
        "seed: ",
        "locations: ",
        "agents: ",
        "power_plants: ",
        "asteroid_fragment_fragments: ",
    ] {
        let line = lines.next().expect("missing summary line");
        let value = line.strip_prefix(prefix).expect("unexpected summary field");
        assert!(value.parse::<u64>().is_ok(), "not a summary count: {line}");
    }
    assert_eq!(lines.next(), None, "summary mode printed extra detail");
}

#[cfg(feature = "test_tier_required")]
#[test]
fn oasis7_init_demo_runs_summary_only() {
    let summary = successful_stdout(run_init_demo(&["--summary-only", "minimal"]));
    assert_summary(&summary, "minimal");

    let normal = successful_stdout(run_init_demo(&["minimal"]));
    let detail = normal
        .strip_prefix(summary.as_str())
        .expect("normal mode must print the same summary before its details");
    for section in [
        "location_resources:",
        "location_facilities:",
        "agent_resources:",
    ] {
        assert!(detail.lines().any(|line| line == section));
    }
}

#[cfg(feature = "test_tier_full")]
fn assert_builtin_summary(scenario: &str) {
    let summary = successful_stdout(run_init_demo(&["--summary-only", scenario]));
    assert_summary(&summary, scenario);
}

#[cfg(feature = "test_tier_full")]
#[test]
fn oasis7_init_demo_runs_llm_bootstrap_summary() {
    assert_builtin_summary("llm_bootstrap");
}

#[cfg(feature = "test_tier_full")]
#[test]
fn oasis7_init_demo_runs_asteroid_fragment_summary() {
    assert_builtin_summary("asteroid_fragment_bootstrap");
}

#[cfg(feature = "test_tier_full")]
#[test]
fn oasis7_init_demo_runs_asteroid_fragment_detail_summary() {
    assert_builtin_summary("asteroid_fragment_detail_bootstrap");
}

#[cfg(feature = "test_tier_full")]
#[test]
fn oasis7_init_demo_runs_asteroid_fragment_twin_summary() {
    assert_builtin_summary("asteroid_fragment_twin_region_bootstrap");
}

#[cfg(feature = "test_tier_full")]
#[test]
fn oasis7_init_demo_runs_asteroid_fragment_triad_summary() {
    assert_builtin_summary("asteroid_fragment_triad_region_bootstrap");
}

#[cfg(feature = "test_tier_full")]
#[test]
fn oasis7_init_demo_runs_triad_summary() {
    assert_builtin_summary("triad_region_bootstrap");
}

#[cfg(feature = "test_tier_full")]
#[test]
fn oasis7_init_demo_runs_from_scenario_file() {
    for (scenario, seed) in [("minimal", "seed: 1"), ("llm_bootstrap", "seed: 5")] {
        let scenario_path = format!("{}/scenarios/{scenario}.json", env!("CARGO_MANIFEST_DIR"));
        let summary = successful_stdout(run_init_demo(&[
            "--summary-only",
            "--scenario-file",
            &scenario_path,
        ]));
        assert_summary(&summary, scenario);
        assert_eq!(summary.lines().nth(1), Some(seed));
    }
}

#[cfg(feature = "test_tier_full")]
#[test]
fn oasis7_init_demo_runs_with_invalid_scenario_file_and_rejects_it() {
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
    let output = run_init_demo(&["--summary-only", "--scenario-file", manifest]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("Failed to load scenario file:"));
}
