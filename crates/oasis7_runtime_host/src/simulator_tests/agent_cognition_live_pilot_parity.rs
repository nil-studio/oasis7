//! LIVE-6 RED contract for the deterministic low-frequency NPC parity pilot.
//!
//! This is a required-tier fixture contract, not evidence from a live remote
//! provider.  It requires one target async actor lifecycle for Builtin and
//! ProviderBacked runs, the exact same seed/observation fixture, and a replay
//! artifact that preserves outcome and diagnostics.  The full Local HTTP
//! rollout and producer/QA subjective scorecard remain separate gates.

use crate::simulator_tests::AsyncAgentRunner;
use serde_json::Value;

// This is P2 pilot infrastructure exercising the currently approved P0,
// single low-frequency-NPC scope.  It is not a claim of the P2 multi-actor
// (P2-001/2/3) rollout tier.
const ROLLOUT_STAGE: &str = "P2";
const PARITY_TIER: &str = "P0";
const PROFILE: &str = "oasis7_p0_low_freq_npc";
const FIXTURE_ID: &str = "p2-low-frequency-npc-pilot-001";
const SEED: u64 = 0x0A51_7007;

fn run_pilot() -> Value {
    // Target API: the harness must construct the same fixed observation and
    // seed for Builtin and ProviderBacked async actors, collect their
    // normalized lifecycle/outcome evidence, and persist enough identity to
    // replay it without another provider invocation.
    AsyncAgentRunner::run_p2_low_frequency_npc_parity(PROFILE, FIXTURE_ID, SEED)
        .expect("deterministic low-frequency NPC pilot")
}

fn assert_outcome_parity(report: &Value) {
    for field in ["lifecycle", "feedback", "world_effect", "outcome"] {
        assert_eq!(
            report["builtin"][field], report["provider"][field],
            "Builtin and ProviderBacked {field} must match exactly"
        );
    }
    assert_eq!(report["outcome_parity"], true);
    assert_eq!(report["target_async_actor_lifecycle"], true);
}

#[test]
fn p2_low_frequency_npc_pilot_replay_is_deterministic_and_exactly_parity_bound() {
    let report = run_pilot();

    assert_eq!(report["rollout_stage"], ROLLOUT_STAGE);
    assert_eq!(report["parity_tier"], PARITY_TIER);
    assert_eq!(report["scope"], "single_low_frequency_npc");
    assert_eq!(report["profile"], PROFILE);
    assert_eq!(report["fixture_id"], FIXTURE_ID);
    assert_eq!(report["seed"], SEED);
    assert_outcome_parity(&report);

    // Required-tier behavior gates from provider-agent-experience-parity:
    // completion gap <= 5pp, invalid actions <= 3%, timeout <= 2%, trace
    // completeness >= 95%, and recoverable errors resolved >= 90%.
    assert!(report["completion_gap_pp"].as_u64().unwrap_or(u64::MAX) <= 5);
    assert!(
        report["invalid_action_rate_ppm"]
            .as_u64()
            .unwrap_or(u64::MAX)
            <= 30_000
    );
    assert!(
        report["timeout_rate_ppm"].as_u64().unwrap_or(u64::MAX) <= 20_000,
        "pilot timeout rate exceeds the behavior gate"
    );
    assert!(
        report["trace_completeness_ppm"]
            .as_u64()
            .unwrap_or_default()
            >= 950_000
    );
    assert!(
        report["recoverable_error_resolution_rate_ppm"]
            .as_u64()
            .unwrap_or_default()
            >= 900_000
    );

    // Evidence must make a later diagnosis possible without treating a
    // provider transcript as world authority.
    assert!(report["provider_diagnostics"].is_object());
    assert!(report["error_codes"].is_array());
    assert!(report["action_outcomes"].is_array());
    assert!(report["evidence_digest"].as_str().is_some());
}

#[test]
fn p2_low_frequency_npc_pilot_replay_does_not_redrive_provider_or_change_outcome() {
    let first = run_pilot();
    let replay = first["replay"].clone();

    assert_eq!(replay["profile"], PROFILE);
    assert_eq!(replay["fixture_id"], FIXTURE_ID);
    assert_eq!(replay["seed"], SEED);
    assert_eq!(replay["deterministic"], true);
    assert_eq!(replay["provider_invocation_count"], 0);
    assert_eq!(replay["effect_count"], 0);
    assert_eq!(replay["debit_count"], 0);
    assert_eq!(replay["outcome"], first["outcome"]);
    assert_eq!(replay["evidence_digest"], first["evidence_digest"]);
}
