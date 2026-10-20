use std::collections::BTreeMap;

use oasis7::network_tier_manifest::{
    LoadedNetworkTierManifest, NETWORK_TIER_MANIFEST_SCHEMA_V1, NetworkTierClaimsPolicy,
    NetworkTierEndpointPolicy, NetworkTierManifest, NetworkTierPromotionPolicy,
    NetworkTierRuntimeRefs, NetworkTierTokenPolicy, NetworkTierValidatorPolicy,
};
use oasis7::simulator::{RuntimePerfBottleneck, RuntimePerfHealth};
use oasis7_node::NodeRole;

use super::super::status_payload::{RuntimePerfGateTier, runtime_perf_gate_tier_from_manifest};

fn loaded_network_tier_manifest(tier: &str) -> LoadedNetworkTierManifest {
    LoadedNetworkTierManifest {
        source_path: "test-network-tier.json".to_string(),
        manifest: NetworkTierManifest {
            release_policy: None,
            world_policy: None,
            authority_policy: None,
            schema_version: NETWORK_TIER_MANIFEST_SCHEMA_V1.to_string(),
            tier: tier.to_string(),
            status: "rehearsal".to_string(),
            network_id: format!("oasis7-{tier}"),
            chain_id: format!("oasis7-{tier}"),
            runtime_refs: NetworkTierRuntimeRefs {
                release_candidate_bundle_ref: "bundle.json".to_string(),
                genesis_ref: "genesis.json".to_string(),
                bootstrap_peer_ref: "peers.txt".to_string(),
            },
            endpoint_policy: NetworkTierEndpointPolicy {
                rpc_ref: "http://127.0.0.1:6631".to_string(),
                explorer_ref: "http://127.0.0.1:6632/explorer".to_string(),
                faucet_ref: Some("none".to_string()),
            },
            validator_policy: NetworkTierValidatorPolicy {
                governance_mode: "governance_registry".to_string(),
                validator_admission: "allowlist_or_governed_candidate".to_string(),
                target_validator_count: 2,
                allow_observer_nodes: true,
            },
            token_policy: NetworkTierTokenPolicy {
                symbol: "OC".to_string(),
                faucet_mode: "guarded_testnet_faucet".to_string(),
                reset_policy: Some("resettable".to_string()),
                value_semantics: "testnet".to_string(),
            },
            claims_policy: NetworkTierClaimsPolicy {
                allowed_claims: vec!["public_testnet".to_string()],
                denied_claims: vec!["mainnet_live".to_string()],
            },
            promotion_policy: NetworkTierPromotionPolicy {
                promote_from: vec!["local_devnet".to_string()],
                required_gates: vec!["public_testnet_rehearsal_pass".to_string()],
            },
            evidence_refs: vec![],
        },
        bootstrap_peers: vec![],
    }
}

#[test]
fn runtime_perf_gate_tier_defaults_to_strict_without_loaded_manifest() {
    assert_eq!(
        runtime_perf_gate_tier_from_manifest(None, NodeRole::Storage),
        RuntimePerfGateTier::Strict
    );
}

#[test]
fn runtime_perf_gate_tier_selects_low_resource_only_for_trusted_public_testnet_manifest() {
    let manifest = loaded_network_tier_manifest("public_testnet");

    assert_eq!(
        runtime_perf_gate_tier_from_manifest(Some(&manifest), NodeRole::Storage),
        RuntimePerfGateTier::LowResourceValidatorV1
    );
}

#[test]
fn runtime_perf_gate_tier_keeps_non_storage_and_non_testnet_paths_strict() {
    let mainnet_manifest = loaded_network_tier_manifest("mainnet");
    let local_devnet_manifest = loaded_network_tier_manifest("local_devnet");

    assert_eq!(
        runtime_perf_gate_tier_from_manifest(Some(&mainnet_manifest), NodeRole::Storage),
        RuntimePerfGateTier::Strict
    );
    assert_eq!(
        runtime_perf_gate_tier_from_manifest(Some(&local_devnet_manifest), NodeRole::Storage),
        RuntimePerfGateTier::Strict
    );
    assert_eq!(
        runtime_perf_gate_tier_from_manifest(
            Some(&loaded_network_tier_manifest("public_testnet")),
            NodeRole::Sequencer
        ),
        RuntimePerfGateTier::Strict
    );
    assert_eq!(
        runtime_perf_gate_tier_from_manifest(
            Some(&loaded_network_tier_manifest("public_testnet")),
            NodeRole::Observer
        ),
        RuntimePerfGateTier::Strict
    );
}

fn runtime_perf_for_steady_window(
    tier: RuntimePerfGateTier,
    recent_over_budget_count: u64,
    p95_total_ms: u64,
    latest_total_ms: u64,
    max_total_ms: u64,
) -> oasis7::simulator::RuntimePerfSnapshot {
    let timing = super::super::execution_bridge::ExecutionBridgeCommitTimingSnapshot {
        window_capacity: 128,
        recent_commit_count: 128,
        recent_over_budget_count,
        recent_over_budget_ratio_ppm: recent_over_budget_count * 1_000_000 / 128,
        p50_total_ms: Some(780),
        p95_total_ms: Some(p95_total_ms),
        latest_total_ms: Some(latest_total_ms),
        max_total_ms: Some(max_total_ms),
        slow_count: recent_over_budget_count,
        last_slow_stage: Some("cas_put".to_string()),
        stages: BTreeMap::new(),
    };

    super::super::status_payload::build_runtime_perf_snapshot_from_execution_bridge_timing(
        &timing, tier,
    )
    .expect("runtime perf snapshot")
}

#[test]
fn runtime_perf_snapshot_from_execution_bridge_timing_accepts_one_isolated_steady_window_jitter() {
    let runtime_perf =
        runtime_perf_for_steady_window(RuntimePerfGateTier::Strict, 1, 900, 900, 1_100);

    assert_eq!(runtime_perf.health, RuntimePerfHealth::Healthy);
    assert_eq!(runtime_perf.bottleneck, RuntimePerfBottleneck::None);
    assert_eq!(runtime_perf.action_execution.samples_window, 128);
    assert_eq!(runtime_perf.action_execution.p95_ms, 900.0);
    assert_eq!(runtime_perf.action_execution.over_budget_total, 1);
    assert_eq!(runtime_perf.action_execution.last_ms, 900.0);
    assert_eq!(runtime_perf.action_execution.max_ms, 1_100.0);
}

#[test]
fn runtime_perf_snapshot_from_execution_bridge_timing_accepts_bounded_low_resource_validator_window()
 {
    let runtime_perf = runtime_perf_for_steady_window(
        RuntimePerfGateTier::LowResourceValidatorV1,
        4,
        1_380,
        1_452,
        1_452,
    );

    assert_eq!(runtime_perf.health, RuntimePerfHealth::Healthy);
    assert_eq!(runtime_perf.bottleneck, RuntimePerfBottleneck::None);
    assert_eq!(runtime_perf.action_execution.samples_window, 128);
    assert_eq!(runtime_perf.action_execution.p95_ms, 1_380.0);
    assert_eq!(runtime_perf.action_execution.over_budget_total, 4);
    assert_eq!(runtime_perf.action_execution.last_ms, 1_452.0);
    assert_eq!(runtime_perf.action_execution.max_ms, 1_452.0);
}

#[test]
fn runtime_perf_snapshot_from_execution_bridge_timing_accepts_low_resource_validator_window_at_60_over_budget_samples_and_2999_ms_max()
 {
    let runtime_perf = runtime_perf_for_steady_window(
        RuntimePerfGateTier::LowResourceValidatorV1,
        60,
        1_258,
        1_499,
        2_999,
    );

    assert_eq!(runtime_perf.health, RuntimePerfHealth::Healthy);
    assert_eq!(runtime_perf.bottleneck, RuntimePerfBottleneck::None);
    assert_eq!(runtime_perf.action_execution.samples_window, 128);
    assert_eq!(runtime_perf.action_execution.p95_ms, 1_258.0);
    assert_eq!(runtime_perf.action_execution.over_budget_total, 60);
    assert_eq!(runtime_perf.action_execution.last_ms, 1_499.0);
    assert_eq!(runtime_perf.action_execution.max_ms, 2_999.0);
}

#[test]
fn runtime_perf_snapshot_from_execution_bridge_timing_projects_latest_sample_separately_from_maximum()
 {
    let runtime_perf =
        runtime_perf_for_steady_window(RuntimePerfGateTier::Strict, 1, 900, 900, 1_100);

    assert_eq!(runtime_perf.action_execution.last_ms, 900.0);
    assert_eq!(runtime_perf.action_execution.max_ms, 1_100.0);
}

#[test]
fn runtime_perf_snapshot_from_execution_bridge_timing_rejects_second_steady_window_breach() {
    let runtime_perf =
        runtime_perf_for_steady_window(RuntimePerfGateTier::Strict, 2, 900, 1_100, 1_100);

    assert_eq!(runtime_perf.health, RuntimePerfHealth::Warn);
    assert_eq!(
        runtime_perf.bottleneck,
        RuntimePerfBottleneck::ActionExecution
    );
    assert_eq!(runtime_perf.action_execution.over_budget_total, 2);
}

#[test]
fn runtime_perf_snapshot_from_execution_bridge_timing_rejects_catastrophic_steady_window_outlier() {
    let runtime_perf =
        runtime_perf_for_steady_window(RuntimePerfGateTier::Strict, 1, 900, 1_250, 1_250);

    assert_eq!(runtime_perf.health, RuntimePerfHealth::Warn);
    assert_eq!(
        runtime_perf.bottleneck,
        RuntimePerfBottleneck::ActionExecution
    );
    assert_eq!(runtime_perf.action_execution.max_ms, 1_250.0);
}

#[test]
fn runtime_perf_snapshot_from_execution_bridge_timing_rejects_low_resource_p95_at_boundary() {
    let runtime_perf = runtime_perf_for_steady_window(
        RuntimePerfGateTier::LowResourceValidatorV1,
        4,
        1_500,
        1_452,
        1_452,
    );

    assert_eq!(runtime_perf.health, RuntimePerfHealth::Warn);
    assert_eq!(
        runtime_perf.bottleneck,
        RuntimePerfBottleneck::ActionExecution
    );
    assert_eq!(runtime_perf.action_execution.p95_ms, 1_500.0);
}

#[test]
fn runtime_perf_snapshot_from_execution_bridge_timing_rejects_low_resource_over_budget_count_at_61_samples()
 {
    let runtime_perf = runtime_perf_for_steady_window(
        RuntimePerfGateTier::LowResourceValidatorV1,
        61,
        1_380,
        1_452,
        2_999,
    );

    assert_eq!(runtime_perf.health, RuntimePerfHealth::Warn);
    assert_eq!(
        runtime_perf.bottleneck,
        RuntimePerfBottleneck::ActionExecution
    );
    assert_eq!(runtime_perf.action_execution.over_budget_total, 61);
}

#[test]
fn runtime_perf_snapshot_from_execution_bridge_timing_rejects_low_resource_max_at_boundary() {
    let runtime_perf = runtime_perf_for_steady_window(
        RuntimePerfGateTier::LowResourceValidatorV1,
        60,
        1_380,
        1_452,
        3_000,
    );

    assert_eq!(runtime_perf.health, RuntimePerfHealth::Warn);
    assert_eq!(
        runtime_perf.bottleneck,
        RuntimePerfBottleneck::ActionExecution
    );
    assert_eq!(runtime_perf.action_execution.last_ms, 1_452.0);
    assert_eq!(runtime_perf.action_execution.max_ms, 3_000.0);
}

#[test]
fn runtime_perf_snapshot_from_execution_bridge_timing_keeps_low_resource_candidate_out_of_strict_tier()
 {
    let runtime_perf =
        runtime_perf_for_steady_window(RuntimePerfGateTier::Strict, 4, 1_380, 1_452, 1_452);

    assert_eq!(runtime_perf.health, RuntimePerfHealth::Warn);
    assert_eq!(
        runtime_perf.bottleneck,
        RuntimePerfBottleneck::ActionExecution
    );
}

#[test]
fn runtime_perf_snapshot_from_execution_bridge_timing_does_not_qualify_an_incomplete_window() {
    let timing = super::super::execution_bridge::ExecutionBridgeCommitTimingSnapshot {
        window_capacity: 128,
        recent_commit_count: 127,
        recent_over_budget_count: 0,
        recent_over_budget_ratio_ppm: 0,
        p50_total_ms: Some(780),
        p95_total_ms: Some(900),
        latest_total_ms: Some(900),
        max_total_ms: Some(900),
        slow_count: 0,
        last_slow_stage: None,
        stages: BTreeMap::new(),
    };

    let runtime_perf =
        super::super::status_payload::build_runtime_perf_snapshot_from_execution_bridge_timing(
            &timing,
            RuntimePerfGateTier::LowResourceValidatorV1,
        )
        .expect("runtime perf snapshot");

    assert_eq!(runtime_perf.health, RuntimePerfHealth::Warn);
    assert_eq!(
        runtime_perf.bottleneck,
        RuntimePerfBottleneck::ActionExecution
    );
    assert_eq!(runtime_perf.action_execution.samples_window, 127);
}
