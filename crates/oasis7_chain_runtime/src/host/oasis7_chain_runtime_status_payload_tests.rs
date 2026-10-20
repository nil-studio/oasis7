use super::cli::TrafficProfile;
use super::{build_chain_status_payload, release_security_policy_for_storage_profile};
use ed25519_dalek::{Signer, SigningKey};
use oasis7::simulator::RuntimePerfSnapshot;
use oasis7_node::{
    GossipTrafficMetricsSnapshot, Libp2pReachabilitySnapshot, NodeConsensusSnapshot,
    NodeFinalityLatencySnapshot, NodeNetworkPolicy, NodeReachabilityAutoDetection, NodeRole,
    NodeSnapshot, NodeUserMode, NodeValidatorStakeProofSnapshot,
};
use oasis7_proto::distributed_dht::{PeerDeploymentMode, PeerNodeRole};
use oasis7_proto::storage_profile::{StorageProfile, StorageProfileConfig};
use oasis7_runtime::{
    Manifest, ModuleAbiContract, ModuleActivation, ModuleArtifactIdentity, ModuleChangeSet,
    ModuleKind, ModuleLimits, ModuleManifest, ModuleRole, ModuleSubscription,
    ModuleSubscriptionStage, PolicySet, ProposalDecision, ReleaseSecurityPolicy, World,
};
use oasis7_wasm_abi::{
    ModuleCallFailure, ModuleCallRequest, ModuleOutput, ModuleSandbox, ModuleTickLifecycleDirective,
};
use sha2::Digest;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
#[path = "oasis7_chain_runtime_status_payload_gate_policy_tests.rs"]
mod gate_policy_tests;
#[path = "status_payload_module_tick_isolation_tests.rs"]
mod module_tick_isolation_tests;
#[path = "oasis7_chain_runtime_status_payload_runtime_perf_tests.rs"]
mod runtime_perf_tests;

const TEST_MODULE_ARTIFACT_SIGNER_NODE_ID: &str = "test.module.release.signer";

#[test]
fn validator_status_projection_uses_authoritative_consensus_identity() {
    let node_id = "triad-testnet-validator-47".to_string();
    let signer = "cf8c9c2b5637d20d0efa585f0fb7f503b19a1aaba02fb807637e67ed40919fc2".to_string();
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.validator_stakes.insert(node_id.clone(), 100);
    consensus.validator_set_hash = "validator-set-hash".to_string();
    consensus.validator_stake_root = "validator-stake-root".to_string();
    consensus.validator_stake_proofs = vec![NodeValidatorStakeProofSnapshot {
        validator_id: node_id.clone(),
        player_id: node_id.clone(),
        stake: 100,
        signer_public_key_hex: Some(signer.clone()),
        leaf_hash: "validator-leaf-hash".to_string(),
        proof: Vec::new(),
    }];
    let snapshot = NodeSnapshot {
        node_id: node_id.clone(),
        player_id: node_id.clone(),
        world_id: "oasis7-public-testnet-governed-20260606".to_string(),
        role: NodeRole::Storage,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: Some(1_700_000_000_000),
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };
    let mut validator_signer_public_keys = BTreeMap::new();
    validator_signer_public_keys.insert(node_id.clone(), signer.clone());
    let mut validator_stakes = BTreeMap::new();
    validator_stakes.insert(node_id.clone(), 100);
    let authority = super::runtime_authority::RuntimeAuthorityBinding {
        registry_ref: "config/validator-registry.json".to_string(),
        registry_sha256: "registry-sha256".to_string(),
        registry_semantic_sha256: "registry-semantic-sha256".to_string(),
        inventory_ref: "scripts/validator-inventory.json".to_string(),
        inventory_sha256: "inventory-sha256".to_string(),
        validator_signer_public_keys,
        validator_stakes,
        validator_47_provider_peer_id: Some("12D3KooWValidator47".to_string()),
    };

    let projected =
        super::status_payload::build_chain_validator_status(&snapshot, Some(&authority));

    assert_eq!(
        projected.schema_version,
        "oasis7.chain_validator_provider_status.v1"
    );
    assert_eq!(projected.role, "validator");
    assert_eq!(projected.membership, "active");
    assert_eq!(projected.stake, Some(100));
    assert_eq!(projected.signer_binding.as_deref(), Some(node_id.as_str()));
    assert_eq!(
        projected.signer_public_key_hex.as_deref(),
        Some(signer.as_str())
    );
    assert_eq!(
        projected.registry_ref.as_deref(),
        Some("config/validator-registry.json")
    );
    assert_eq!(
        projected.inventory_ref.as_deref(),
        Some("scripts/validator-inventory.json")
    );
}

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("oasis7-status-payload-{prefix}-{unique}"))
}

pub(super) fn minimal_reward_runtime_metrics()
-> super::reward_runtime_worker::RewardRuntimeMetricsSnapshot {
    super::reward_runtime_worker::RewardRuntimeMetricsSnapshot {
        enabled: true,
        metrics_available: true,
        report_dir: "/tmp/reports".to_string(),
        report_count: 0,
        latest_epoch_index: 0,
        latest_report_observed_at_unix_ms: 0,
        latest_total_distributed_points: 0,
        latest_minted_record_count: 0,
        cumulative_minted_record_count: 0,
        distfs_total_checks: 0,
        distfs_failed_checks: 0,
        distfs_failure_ratio: 0.0,
        settlement_apply_attempts_total: 0,
        settlement_apply_failures_total: 0,
        settlement_apply_failure_ratio: 0.0,
        invariant_ok: true,
        last_error: None,
    }
}

pub(super) fn minimal_storage_metrics() -> super::storage_metrics::StorageMetricsSnapshot {
    super::storage_metrics::StorageMetricsSnapshot {
        storage_profile: "dev_local".to_string(),
        effective_budget: StorageProfileConfig::from(StorageProfile::DevLocal),
        bytes_by_dir: BTreeMap::new(),
        blob_counts: BTreeMap::new(),
        ref_count: 0,
        pin_count: 0,
        retained_heights: Vec::new(),
        checkpoint_count: 0,
        replay_summary: super::storage_metrics::StorageReplaySummary::default(),
        orphan_blob_count: 0,
        last_gc_at_ms: None,
        last_gc_result: "not_available".to_string(),
        last_gc_error: None,
        degraded_reason: None,
    }
}

pub(super) fn minimal_wasm_status() -> super::wasm_status::ChainWasmStatus {
    super::wasm_status::ChainWasmStatus {
        metrics_available: true,
        observed_since_unix_ms: None,
        degraded_reason: None,
        build: super::wasm_status::ChainWasmBuildStatus {
            metrics_available: true,
            observed_since_unix_ms: None,
            degraded_reason: None,
            total_build_wall_ms: None,
            cargo_build_ms: None,
            canonicalize_ms: None,
            hash_ms: None,
            receipt_write_ms: None,
            metadata_write_ms: None,
            wasm_size_bytes: None,
        },
        executor: oasis7_wasm_executor::WasmExecutorMetricsSnapshot::empty(),
        router: oasis7_wasm_router::WasmRouterMetricsSnapshot::empty(),
    }
}

pub(super) fn minimal_transfer_status() -> super::transfer_submit_api::ChainTransferMetricsStatus {
    super::transfer_submit_api::ChainTransferMetricsStatus {
        tracked_records: 0,
        accepted_count: 0,
        pending_count: 0,
        confirmed_count: 0,
        failed_count: 0,
        timeout_count: 0,
        inflight_count: 0,
        oldest_inflight_age_ms: None,
        recent_confirmation_latency:
            super::transfer_submit_api::ChainTransferLatencySummaryStatus {
                sample_count: 0,
                avg_latency_ms: None,
                max_latency_ms: None,
                p50_latency_ms: None,
                p95_latency_ms: None,
            },
    }
}

pub(super) fn build_minimal_status_payload(
    execution_records_dir: Option<&Path>,
) -> super::status_payload::ChainStatusResponse {
    build_minimal_status_payload_with_world_dir(
        Path::new("/tmp/execution-world"),
        execution_records_dir,
    )
}

pub(super) fn build_minimal_status_payload_with_storage_root(
    execution_records_dir: Option<&Path>,
    storage_root: Option<&Path>,
) -> super::status_payload::ChainStatusResponse {
    let snapshot = NodeSnapshot {
        node_id: "node-a".to_string(),
        player_id: "player-a".to_string(),
        world_id: "live-a".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: false,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: Some(1_700_000_000_000),
        consensus: NodeConsensusSnapshot::default(),
        consensus_progress_observer_error: None,
        last_error: None,
    };
    let recommendation = NodeNetworkPolicy::recommend_for_user_mode(
        NodeRole::Sequencer,
        NodeUserMode::PrivateSafe,
        NodeReachabilityAutoDetection::default(),
        false,
    )
    .expect("recommendation");

    super::status_payload::build_chain_status_payload_with_storage_root(
        snapshot,
        Path::new("/tmp/execution-world"),
        execution_records_dir,
        storage_root,
        None,
        &recommendation,
        None,
        NodeNetworkPolicy {
            deployment_mode: PeerDeploymentMode::Private,
            node_role_claim: PeerNodeRole::ValidatorCore,
        },
        &Libp2pReachabilitySnapshot::default(),
        NodeReachabilityAutoDetection::default(),
        ReleaseSecurityPolicy::default(),
        minimal_reward_runtime_metrics(),
        minimal_storage_metrics(),
        minimal_wasm_status(),
        None,
        super::ChainTrafficStatus {
            udp_gossip: None,
            libp2p_replication: oasis7_node::Libp2pTrafficMetricsSnapshot::default(),
        },
        minimal_transfer_status(),
        super::ChainReplicationDebugStatus::default(),
    )
}

fn build_minimal_status_payload_with_world_dir(
    execution_world_dir: &Path,
    execution_records_dir: Option<&Path>,
) -> super::status_payload::ChainStatusResponse {
    build_minimal_status_payload_with_world_dir_and_runtime_perf(
        execution_world_dir,
        execution_records_dir,
        None,
    )
}

fn build_minimal_status_payload_with_runtime_perf(
    runtime_perf: RuntimePerfSnapshot,
) -> super::status_payload::ChainStatusResponse {
    build_minimal_status_payload_with_world_dir_and_runtime_perf(
        Path::new("/tmp/execution-world"),
        None,
        Some(runtime_perf),
    )
}

fn build_minimal_status_payload_with_world_dir_and_runtime_perf(
    execution_world_dir: &Path,
    execution_records_dir: Option<&Path>,
    runtime_perf: Option<RuntimePerfSnapshot>,
) -> super::status_payload::ChainStatusResponse {
    build_minimal_status_payload_with_world_dir_runtime_perf_and_wasm(
        execution_world_dir,
        execution_records_dir,
        runtime_perf,
        minimal_wasm_status(),
    )
}

fn build_minimal_status_payload_with_world_dir_runtime_perf_and_wasm(
    execution_world_dir: &Path,
    execution_records_dir: Option<&Path>,
    runtime_perf: Option<RuntimePerfSnapshot>,
    wasm: super::wasm_status::ChainWasmStatus,
) -> super::status_payload::ChainStatusResponse {
    build_minimal_status_payload_with_world_dir_runtime_perf_wasm_and_traffic(
        execution_world_dir,
        execution_records_dir,
        runtime_perf,
        wasm,
        super::ChainTrafficStatus {
            udp_gossip: None,
            libp2p_replication: oasis7_node::Libp2pTrafficMetricsSnapshot::default(),
        },
    )
}

fn build_minimal_status_payload_with_world_dir_runtime_perf_wasm_and_traffic(
    execution_world_dir: &Path,
    execution_records_dir: Option<&Path>,
    runtime_perf: Option<RuntimePerfSnapshot>,
    wasm: super::wasm_status::ChainWasmStatus,
    traffic: super::ChainTrafficStatus,
) -> super::status_payload::ChainStatusResponse {
    super::status_payload_observer_tests::build_minimal_status_payload_with_world_dir_runtime_perf_wasm_traffic_and_observer_error(
        execution_world_dir,
        execution_records_dir,
        runtime_perf,
        wasm,
        traffic,
        None,
    )
}

fn build_minimal_status_payload_for_observability_contract(
    consensus: NodeConsensusSnapshot,
    transactions: super::transfer_submit_api::ChainTransferMetricsStatus,
) -> super::status_payload::ChainStatusResponse {
    let snapshot = NodeSnapshot {
        node_id: "node-a".to_string(),
        player_id: "player-a".to_string(),
        world_id: "live-a".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: false,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: Some(1_700_000_000_000),
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };
    let recommendation = NodeNetworkPolicy::recommend_for_user_mode(
        NodeRole::Sequencer,
        NodeUserMode::PrivateSafe,
        NodeReachabilityAutoDetection::default(),
        false,
    )
    .expect("recommendation");

    build_chain_status_payload(
        snapshot,
        Path::new("/tmp/execution-world"),
        None,
        None,
        &recommendation,
        None,
        NodeNetworkPolicy {
            deployment_mode: PeerDeploymentMode::Private,
            node_role_claim: PeerNodeRole::ValidatorCore,
        },
        &Libp2pReachabilitySnapshot::default(),
        NodeReachabilityAutoDetection::default(),
        ReleaseSecurityPolicy::default(),
        minimal_reward_runtime_metrics(),
        minimal_storage_metrics(),
        minimal_wasm_status(),
        None,
        super::ChainTrafficStatus {
            udp_gossip: None,
            libp2p_replication: oasis7_node::Libp2pTrafficMetricsSnapshot::default(),
        },
        transactions,
        super::ChainReplicationDebugStatus::default(),
    )
}

#[derive(Default)]
struct CountingSandbox {
    calls: usize,
}

impl ModuleSandbox for CountingSandbox {
    fn call(&mut self, _request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        self.calls += 1;
        Ok(ModuleOutput {
            new_state: None,
            effects: Vec::new(),
            emits: Vec::new(),
            tick_lifecycle: None,
            output_bytes: 0,
        })
    }
}

#[derive(Default)]
struct SlowRecurringTickSandbox {
    calls: usize,
}

impl ModuleSandbox for SlowRecurringTickSandbox {
    fn call(&mut self, _request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        self.calls += 1;
        std::thread::sleep(Duration::from_millis(105));
        Ok(ModuleOutput {
            new_state: None,
            effects: Vec::new(),
            emits: Vec::new(),
            tick_lifecycle: Some(ModuleTickLifecycleDirective::WakeAfterTicks { ticks: 1 }),
            output_bytes: 0,
        })
    }
}

fn install_tick_module(world: &mut World) {
    world.set_policy(PolicySet::allow_all());
    world
        .bind_node_identity(
            TEST_MODULE_ARTIFACT_SIGNER_NODE_ID,
            test_module_artifact_signer_public_key_hex().as_str(),
        )
        .expect("bind test module signer identity");
    let wasm_bytes = b"status-payload-tick-routing-observability";
    let wasm_hash = sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .expect("register module artifact");
    let module_manifest = ModuleManifest {
        module_id: "m.status.tick".to_string(),
        name: "Status Tick".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: Vec::new(),
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::Tick),
            filters: None,
        }],
        required_caps: Vec::new(),
        artifact_identity: Some(signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits::unbounded(),
    };
    let changes = ModuleChangeSet {
        register: vec![module_manifest.clone()],
        activate: vec![ModuleActivation {
            module_id: module_manifest.module_id.clone(),
            version: module_manifest.version.clone(),
        }],
        ..ModuleChangeSet::default()
    };
    let proposal_id = world
        .propose_manifest_update(
            Manifest {
                version: 2,
                content: serde_json::json!({ "module_changes": changes }),
            },
            "alice",
        )
        .expect("propose manifest");
    world.shadow_proposal(proposal_id).expect("shadow proposal");
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .expect("approve proposal");
    world.apply_proposal(proposal_id).expect("apply proposal");
}

fn signed_test_artifact_identity(wasm_hash: &str) -> ModuleArtifactIdentity {
    let source_hash = sha256_hex(format!("test-src:{wasm_hash}").as_bytes());
    let build_manifest_hash = sha256_hex(b"test-build-manifest-v1");
    let payload = ModuleArtifactIdentity::signing_payload_v1(
        wasm_hash,
        source_hash.as_str(),
        build_manifest_hash.as_str(),
        TEST_MODULE_ARTIFACT_SIGNER_NODE_ID,
    );
    let signing_key = test_module_artifact_signing_key();
    let signature = signing_key.sign(payload.as_slice());
    ModuleArtifactIdentity {
        source_hash,
        build_manifest_hash,
        signer_node_id: TEST_MODULE_ARTIFACT_SIGNER_NODE_ID.to_string(),
        signature_scheme: ModuleArtifactIdentity::SIGNATURE_SCHEME_ED25519.to_string(),
        artifact_signature: format!(
            "{}{}",
            ModuleArtifactIdentity::SIGNATURE_PREFIX_ED25519_V1,
            hex::encode(signature.to_bytes())
        ),
    }
}

fn test_module_artifact_signing_key() -> SigningKey {
    let seed = sha256_hex(b"oasis7-test-module-artifact-signer-v1");
    let seed_bytes = hex::decode(seed).expect("decode test module signing seed");
    let private_key_bytes: [u8; 32] = seed_bytes
        .as_slice()
        .try_into()
        .expect("test module signing seed is 32 bytes");
    SigningKey::from_bytes(&private_key_bytes)
}

fn test_module_artifact_signer_public_key_hex() -> String {
    hex::encode(
        test_module_artifact_signing_key()
            .verifying_key()
            .to_bytes(),
    )
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = sha2::Sha256::digest(bytes);
    hex::encode(digest)
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn build_chain_status_payload_warns_when_finality_p95_exceeds_budget() {
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.recent_finality_latency = NodeFinalityLatencySnapshot {
        sample_count: 8,
        avg_latency_ms: Some(1_200),
        max_latency_ms: Some(2_200),
        p50_latency_ms: Some(1_100),
        p95_latency_ms: Some(2_000),
    };

    let payload = build_minimal_status_payload_for_observability_contract(
        consensus,
        minimal_transfer_status(),
    );

    let alert = payload
        .observability
        .alerts
        .iter()
        .find(|alert| alert.code == "consensus_finality_latency_degraded")
        .expect("finality latency degradation alert");
    assert_eq!(alert.severity, "warn");
    assert!(alert.summary.contains("sample_count=8"));
    assert!(alert.summary.contains("finality_p95_ms=2000"));
    assert!(alert.summary.contains("finality_budget_ms="));
}

#[test]
fn build_chain_status_payload_warns_when_transfer_lifecycle_failures_cross_threshold() {
    let transactions = super::transfer_submit_api::ChainTransferMetricsStatus {
        tracked_records: 20,
        accepted_count: 0,
        pending_count: 0,
        confirmed_count: 8,
        failed_count: 2,
        timeout_count: 10,
        inflight_count: 0,
        oldest_inflight_age_ms: None,
        recent_confirmation_latency:
            super::transfer_submit_api::ChainTransferLatencySummaryStatus {
                sample_count: 8,
                avg_latency_ms: Some(400),
                max_latency_ms: Some(500),
                p50_latency_ms: Some(400),
                p95_latency_ms: Some(500),
            },
    };

    let payload = build_minimal_status_payload_for_observability_contract(
        NodeConsensusSnapshot::default(),
        transactions,
    );

    let alert = payload
        .observability
        .alerts
        .iter()
        .find(|alert| alert.code == "transfer_lifecycle_degraded")
        .expect("transfer lifecycle degradation alert");
    assert_eq!(alert.severity, "warn");
    assert!(alert.summary.contains("failure_ratio_ppm=600000"));
    assert!(
        alert
            .summary
            .contains("dominant_error_code=transfer_timeout")
    );
}

#[test]
fn build_chain_status_payload_surfaces_persisted_module_tick_routing_metrics() {
    let dir = temp_dir("module-routing-status");
    let mut world = oasis7::runtime_host::world::new();
    install_tick_module(&mut world);
    let mut sandbox = CountingSandbox::default();
    world
        .route_tick_to_modules(&mut sandbox)
        .expect("route tick module");
    assert_eq!(sandbox.calls, 1);
    world.save_to_dir(&dir).expect("save execution world");
    let snapshot_path = dir.join("snapshot.json");
    let mut snapshot_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(snapshot_path.as_path()).expect("read snapshot"))
            .expect("parse snapshot");
    snapshot_json["module_tick_routing_metrics"] = serde_json::json!({
        "last_due_count": 1,
        "last_invoked_count": 1,
        "missing_invocation_count": 0,
        "last_missing_invocation_count": 0,
        "oldest_overdue_ticks": 0,
        "routing_count": 1
    });
    fs::write(
        snapshot_path.as_path(),
        serde_json::to_vec_pretty(&snapshot_json).expect("encode snapshot"),
    )
    .expect("write snapshot with module routing metrics");

    let payload = build_minimal_status_payload_with_world_dir(dir.as_path(), None);
    assert!(payload.module_tick_routing.available);
    assert_eq!(payload.module_tick_routing.source, "execution_world");
    assert!(payload.module_tick_routing.load_error.is_none());
    let metrics = payload
        .module_tick_routing
        .metrics
        .as_ref()
        .expect("module tick routing metrics");
    assert_eq!(metrics["routing_count"], 1);
    assert_eq!(metrics["last_due_count"], 1);
    assert_eq!(metrics["last_invoked_count"], 1);
    assert!(
        metrics.get("duration_buckets").is_none(),
        "persisted module routing metrics must stay deterministic"
    );
    assert!(
        metrics.get("last_route_duration_ms").is_none(),
        "wall-clock route duration must not enter canonical snapshots"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn build_chain_status_payload_warns_when_module_tick_routing_is_degraded() {
    let dir = temp_dir("module-routing-observability-alert");
    let mut world = oasis7::runtime_host::world::new();
    install_tick_module(&mut world);
    world.save_to_dir(&dir).expect("save execution world");
    let snapshot_path = dir.join("snapshot.json");
    let mut snapshot_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(snapshot_path.as_path()).expect("read snapshot"))
            .expect("parse snapshot");
    snapshot_json["module_tick_routing_metrics"] = serde_json::json!({
        "last_due_count": 10,
        "last_invoked_count": 6,
        "missing_invocation_count": 4,
        "last_missing_invocation_count": 4,
        "oldest_overdue_ticks": 3,
        "routing_count": 10
    });
    fs::write(
        snapshot_path.as_path(),
        serde_json::to_vec_pretty(&snapshot_json).expect("encode snapshot"),
    )
    .expect("write snapshot with degraded module routing metrics");

    let payload = build_minimal_status_payload_with_world_dir(dir.as_path(), None);

    assert_eq!(payload.observability.status, "warn");
    let alert = payload
        .observability
        .alerts
        .iter()
        .find(|alert| alert.code == "module_tick_routing_degraded")
        .expect("module tick routing degradation alert");
    assert_eq!(alert.severity, "warn");
    assert!(alert.summary.contains("missing_invocation_count=4"));
    assert!(alert.summary.contains("oldest_overdue_ticks=3"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn build_chain_status_payload_does_not_warn_for_historical_module_tick_miss_after_recovery() {
    let dir = temp_dir("module-routing-observability-historical-miss");
    let mut world = oasis7::runtime_host::world::new();
    install_tick_module(&mut world);
    world.save_to_dir(&dir).expect("save execution world");
    let snapshot_path = dir.join("snapshot.json");
    let mut snapshot_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(snapshot_path.as_path()).expect("read snapshot"))
            .expect("parse snapshot");
    snapshot_json["module_tick_routing_metrics"] = serde_json::json!({
        "last_due_count": 1,
        "last_invoked_count": 1,
        "missing_invocation_count": 4,
        "last_missing_invocation_count": 0,
        "oldest_overdue_ticks": 0,
        "routing_count": 10
    });
    fs::write(
        snapshot_path.as_path(),
        serde_json::to_vec_pretty(&snapshot_json).expect("encode snapshot"),
    )
    .expect("write recovered module routing metrics");

    let payload = build_minimal_status_payload_with_world_dir(dir.as_path(), None);

    assert!(
        !payload
            .observability
            .alerts
            .iter()
            .any(|alert| alert.code == "module_tick_routing_degraded"),
        "historical missing invocations must not keep a recovered module route in warn state"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn build_chain_status_payload_does_not_claim_persisted_module_tick_duration_degradation() {
    let dir = temp_dir("module-routing-observability-one-slow-route");
    let mut world = oasis7::runtime_host::world::new();
    install_tick_module(&mut world);
    world.save_to_dir(&dir).expect("save execution world");
    let snapshot_path = dir.join("snapshot.json");
    let mut snapshot_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(snapshot_path.as_path()).expect("read snapshot"))
            .expect("parse snapshot");
    // Production snapshots intentionally omit wall-clock duration buckets, so a
    // healthy deterministic snapshot must not produce a synthetic slow-route alert.
    snapshot_json["module_tick_routing_metrics"] = serde_json::json!({
        "last_due_count": 1,
        "last_invoked_count": 1,
        "missing_invocation_count": 0,
        "last_missing_invocation_count": 0,
        "oldest_overdue_ticks": 0,
        "routing_count": 1
    });
    fs::write(
        snapshot_path.as_path(),
        serde_json::to_vec_pretty(&snapshot_json).expect("encode snapshot"),
    )
    .expect("write deterministic healthy module routing metrics");

    let payload = build_minimal_status_payload_with_world_dir(dir.as_path(), None);

    assert!(
        !payload
            .observability
            .alerts
            .iter()
            .any(|alert| alert.code == "module_tick_routing_degraded"),
        "persisted deterministic routing metrics do not contain duration buckets"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn build_chain_status_payload_warns_for_sustained_slow_live_module_tick_routes() {
    let dir = temp_dir("module-routing-observability-sustained-slow-live");
    let mut world = oasis7::runtime_host::world::new();
    install_tick_module(&mut world);
    let mut sandbox = SlowRecurringTickSandbox::default();
    for _ in 0..4 {
        world
            .step_with_modules(&mut sandbox)
            .expect("route recurring tick module");
    }
    assert_eq!(sandbox.calls, 4);
    let live_metrics = world.module_tick_routing_metrics_snapshot();
    assert_eq!(live_metrics.routing_count, 4);
    assert!(
        live_metrics.duration_buckets.ge_100ms >= 4,
        "each real in-process route must be represented in the slow duration bucket"
    );

    world.save_to_dir(&dir).expect("save execution world");
    // Status must consume the process-local execution-driver publication, not
    // inject wall-clock buckets into the deterministic persisted snapshot.
    super::execution_bridge::record_execution_bridge_module_tick_routing_metrics(live_metrics);

    let payload = build_minimal_status_payload_with_world_dir(dir.as_path(), None);
    assert_eq!(payload.observability.status, "warn");
    let alert = payload
        .observability
        .alerts
        .iter()
        .find(|alert| alert.code == "module_tick_routing_degraded")
        .expect("sustained slow module tick routing alert");
    assert_eq!(alert.severity, "warn");
    assert!(alert.summary.contains("slow"));

    super::execution_bridge::reset_execution_bridge_commit_timing_for_tests();

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn build_chain_status_payload_warns_for_udp_gossip_send_failures() {
    let mut gossip = GossipTrafficMetricsSnapshot::default();
    gossip.totals.outbound.attempted_datagrams = 4;
    gossip.totals.outbound.succeeded_datagrams = 3;
    gossip.totals.outbound.failed_datagrams = 1;
    gossip.totals.outbound.failure_ratio_ppm = 250_000;
    gossip
        .by_error_kind
        .insert("permission_denied".to_string(), 1);

    let payload = build_minimal_status_payload_with_world_dir_runtime_perf_wasm_and_traffic(
        Path::new("/tmp/execution-world"),
        None,
        None,
        minimal_wasm_status(),
        super::ChainTrafficStatus {
            udp_gossip: Some(gossip),
            libp2p_replication: oasis7_node::Libp2pTrafficMetricsSnapshot::default(),
        },
    );

    assert_eq!(payload.observability.status, "warn");
    let alert = payload
        .observability
        .alerts
        .iter()
        .find(|alert| alert.code == "udp_gossip_send_failures")
        .expect("UDP gossip failure alert");
    assert_eq!(alert.severity, "warn");
    assert!(alert.summary.contains("attempted=4 succeeded=3 failed=1"));
    assert!(alert.summary.contains("failure_ratio_ppm=250000"));
    assert!(
        alert
            .summary
            .contains("dominant_error=permission_denied count=1")
    );
}

#[test]
fn build_chain_status_payload_does_not_warn_for_transient_udp_gossip_send_failure() {
    let mut gossip = GossipTrafficMetricsSnapshot::default();
    gossip.totals.outbound.attempted_datagrams = 1;
    gossip.totals.outbound.failed_datagrams = 1;
    gossip.totals.outbound.failure_ratio_ppm = 1_000_000;
    gossip
        .by_error_kind
        .insert("permission_denied".to_string(), 1);

    let payload = build_minimal_status_payload_with_world_dir_runtime_perf_wasm_and_traffic(
        Path::new("/tmp/execution-world"),
        None,
        None,
        minimal_wasm_status(),
        super::ChainTrafficStatus {
            udp_gossip: Some(gossip),
            libp2p_replication: oasis7_node::Libp2pTrafficMetricsSnapshot::default(),
        },
    );

    assert!(
        payload
            .observability
            .alerts
            .iter()
            .all(|alert| alert.code != "udp_gossip_send_failures"),
        "a single transient UDP gossip send failure must not degrade node observability"
    );
    assert_ne!(payload.observability.status, "warn");
}

#[test]
fn build_chain_status_payload_warns_when_only_wasm_is_degraded() {
    let mut wasm = minimal_wasm_status();
    wasm.degraded_reason = Some("executor unavailable".to_string());

    let payload = build_minimal_status_payload_with_world_dir_runtime_perf_and_wasm(
        Path::new("/tmp/execution-world"),
        None,
        None,
        wasm,
    );

    assert_eq!(payload.observability.status, "warn");
    let alert = payload
        .observability
        .alerts
        .iter()
        .find(|alert| alert.code == "wasm_observability_degraded")
        .expect("WASM observability degradation alert");
    assert_eq!(alert.severity, "warn");
    assert!(alert.summary.contains("executor unavailable"));
}

// Kept separate to keep status-payload test responsibilities and file size bounded.
mod status_payload_release_policy_tests;

#[test]
fn feedback_p2p_is_disabled_for_observer_role() {
    assert!(
        super::feedback_p2p_config_for_role(NodeRole::Observer, TrafficProfile::Default).is_none()
    );
    assert!(
        super::feedback_p2p_config_for_role(NodeRole::Sequencer, TrafficProfile::Default).is_some()
    );
    assert!(
        super::feedback_p2p_config_for_role(NodeRole::Storage, TrafficProfile::Default).is_some()
    );
}
