#![expect(clippy::field_reassign_with_default)]

use super::build_chain_status_payload;
use super::cli::parse_options;
use oasis7::network_tier_manifest::{LoadedNetworkTierManifest, NETWORK_TIER_MANIFEST_SCHEMA_V1};
use oasis7_node::{
    Libp2pReachabilitySnapshot, NodeConsensusSnapshot, NodeNetworkPolicy, NodePeerCommittedHead,
    NodeReachabilityAutoDetection, NodeRole, NodeSnapshot, NodeUserMode,
    NodeValidatorStakeProofSnapshot,
};
use oasis7_proto::distributed_dht::{PeerDeploymentMode, PeerNodeRole};
use oasis7_proto::storage_profile::{StorageProfile, StorageProfileConfig};
use oasis7_runtime::ReleaseSecurityPolicy;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "oasis7_chain_runtime_network_tier_generated_manifest_tests.rs"]
mod generated_manifest_tests;

static TEST_TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_dir(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let count = TEST_TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "oasis7-chain-runtime-{label}-{}-{nonce}-{count}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn current_test_binary_sha256() -> String {
    let current_exe = std::env::current_exe().expect("current exe path");
    let current_bytes = fs::read(current_exe).expect("read current exe bytes");
    sha256_hex(current_bytes.as_slice())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TestDirTreeMeta {
    sha256_tree: String,
    file_count: u64,
    total_bytes: u64,
}

fn test_dir_tree_meta(path: &std::path::Path) -> TestDirTreeMeta {
    let mut files = Vec::new();
    collect_test_files(path, &mut files);
    files.sort();

    let mut combined = Sha256::new();
    let mut total_bytes = 0_u64;
    for file in files.iter() {
        let rel = file
            .strip_prefix(path)
            .expect("file under tree")
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = fs::read(file).expect("read tree file");
        let digest = sha256_hex(bytes.as_slice());
        let size = bytes.len() as u64;
        combined.update(rel.as_bytes());
        combined.update(b"\0");
        combined.update(digest.as_bytes());
        combined.update(b"\0");
        combined.update(size.to_string().as_bytes());
        combined.update(b"\n");
        total_bytes += size;
    }

    TestDirTreeMeta {
        sha256_tree: hex::encode(combined.finalize()),
        file_count: files.len() as u64,
        total_bytes,
    }
}

fn collect_test_files(path: &std::path::Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(path).expect("read tree dir") {
        let entry = entry.expect("read tree entry");
        let child = entry.path();
        if child.is_dir() {
            collect_test_files(child.as_path(), files);
        } else if child.is_file() {
            files.push(child);
        }
    }
}

fn write_test_network_tier_manifest(runtime_sha256: &str) -> (PathBuf, PathBuf) {
    write_test_network_tier_manifest_for_tier(runtime_sha256, "public_testnet")
}

fn write_test_network_tier_manifest_for_tier(
    runtime_sha256: &str,
    tier: &str,
) -> (PathBuf, PathBuf) {
    let dir = temp_dir("manifest");
    let peers_path = dir.join("bootstrap.txt");
    let genesis_path = dir.join("genesis.json");
    let bundle_path = dir.join("public-testnet.bundle.json");
    let sidecar_dir = dir.join("generated-scenario-world");
    let provenance_path = dir.join("world-generation-provenance.json");
    fs::create_dir_all(&sidecar_dir).expect("create generated sidecar dir");
    fs::write(sidecar_dir.join("snapshot.json"), "{}\n").expect("write sidecar snapshot");
    fs::write(sidecar_dir.join("journal.json"), "{}\n").expect("write sidecar journal");
    fs::write(
        &provenance_path,
        r#"{"scenario_id":"asteroid_fragment_bootstrap"}"#,
    )
    .expect("write sidecar provenance");
    let sidecar_meta = test_dir_tree_meta(sidecar_dir.as_path());
    let provenance_sha256 = sha256_hex(
        fs::read(&provenance_path)
            .expect("read provenance")
            .as_slice(),
    );
    fs::write(
        &peers_path,
        "/ip4/127.0.0.1/tcp/4100\n/dns4/bootstrap.example/tcp/4101\n",
    )
    .expect("write peers");
    fs::write(&genesis_path, "{}\n").expect("write genesis");
    fs::write(
        &bundle_path,
        format!(
            r#"{{
  "runtime_build": {{
    "sha256": "{runtime_sha256}"
  }},
  "generated_world_sidecar": {{
    "kind": "directory",
    "resolved_path": "{}",
    "sha256_tree": "{}",
    "file_count": {},
    "total_bytes": {}
  }},
  "world_generation_provenance": {{
    "kind": "file",
    "resolved_path": "{}",
    "sha256": "{}"
  }}
}}"#,
            sidecar_dir.display(),
            sidecar_meta.sha256_tree,
            sidecar_meta.file_count,
            sidecar_meta.total_bytes,
            provenance_path.display(),
            provenance_sha256,
        ),
    )
    .expect("write bundle");
    let manifest_path = dir.join("manifest.json");
    fs::write(
        &manifest_path,
        format!(
            r#"{{
  "schema_version": "{NETWORK_TIER_MANIFEST_SCHEMA_V1}",
  "tier": "{tier}",
  "status": "rehearsal",
  "network_id": "oasis7-{tier}",
  "chain_id": "oasis7-{tier}",
  "runtime_refs": {{
    "release_candidate_bundle_ref": "{}",
    "genesis_ref": "{}",
    "bootstrap_peer_ref": "{}"
  }},
  "endpoint_policy": {{
    "rpc_ref": "https://public-testnet.example.invalid/rpc",
    "explorer_ref": "https://public-testnet.example.invalid/explorer",
    "faucet_ref": {}
  }},
  "validator_policy": {{
    "governance_mode": "{}",
    "validator_admission": "{}",
    "target_validator_count": 4,
    "allow_observer_nodes": true
  }},
  "token_policy": {{
    "symbol": "OC",
    "faucet_mode": "{}",
    "reset_policy": "{}",
    "value_semantics": "{}"
  }},
  "claims_policy": {{
    "allowed_claims": [{}],
    "denied_claims": [{}]
  }},
  "promotion_policy": {{
    "promote_from": ["local_devnet"],
    "required_gates": [{}]
  }},
  "evidence_refs": ["doc/testing/evidence/public-testnet.md"]
}}"#,
            bundle_path.display(),
            genesis_path.display(),
            peers_path.display(),
            if tier == "mainnet" {
                "null".to_string()
            } else {
                r#""https://public-testnet.example.invalid/faucet""#.to_string()
            },
            if tier == "mainnet" {
                "governance_registry"
            } else {
                "shared_ops"
            },
            if tier == "mainnet" {
                "governance_registry_only"
            } else if tier == "local_devnet" {
                "local_only"
            } else {
                "allowlist_or_governed_candidate"
            },
            if tier == "mainnet" {
                "none"
            } else {
                "guarded_testnet_faucet"
            },
            if tier == "mainnet" {
                "frozen"
            } else if tier == "local_devnet" {
                "ephemeral"
            } else {
                "resettable"
            },
            if tier == "mainnet" {
                "production"
            } else if tier == "local_devnet" {
                "preview"
            } else {
                "testnet"
            },
            if tier == "mainnet" {
                r#""mainnet_live""#.to_string()
            } else if tier == "local_devnet" {
                r#""local_devnet""#.to_string()
            } else {
                r#""public_testnet""#.to_string()
            },
            if tier == "mainnet" {
                r#""faucet_claims""#.to_string()
            } else {
                r#""mainnet_live", "production_oc_settlement""#.to_string()
            },
            if tier == "mainnet" {
                r#""MAINNET-1", "MAINNET-2", "MAINNET-3", "MAINNET-4""#.to_string()
            } else {
                r#""public_testnet_rehearsal_pass", "public_rpc_ready", "faucet_guard_ready", "reset_policy_announced""#.to_string()
            },
        ),
    )
    .expect("write manifest");
    (dir, manifest_path)
}

fn minimal_storage_metrics() -> super::storage_metrics::StorageMetricsSnapshot {
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

fn minimal_reward_metrics() -> super::reward_runtime_worker::RewardRuntimeMetricsSnapshot {
    super::reward_runtime_worker::RewardRuntimeMetricsSnapshot {
        enabled: false,
        metrics_available: true,
        report_dir: String::new(),
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

fn public_p2p_status() -> super::status_payload::ChainP2pStatus {
    super::status_payload::ChainP2pStatus {
        requested_user_mode: "auto_join".to_string(),
        recommended_user_mode: "public_entry".to_string(),
        effective_user_mode: "public_entry".to_string(),
        applied_effective_user_mode: Some("public_entry".to_string()),
        requires_explicit_public_entry_confirmation: false,
        detected_reachability: Some("public".to_string()),
        hole_punch_viability: "viable".to_string(),
        autonat_status: "public".to_string(),
        public_port_reachability: "reachable".to_string(),
        observed_public_addr: Some("/ip4/203.0.113.10/tcp/4001".to_string()),
        confirmed_external_direct_addrs: vec!["/ip4/203.0.113.10/tcp/4001".to_string()],
        active_transport_kind: Some("direct".to_string()),
        active_transport_kind_since_unix_ms: Some(1_700_000_000_000),
        active_direct_path_count: 1,
        active_hole_punch_path_count: 0,
        active_relay_path_count: 0,
        transport_transition_count: 0,
        transport_transitions: super::status_payload::ChainP2pTransportTransitionCounters::default(
        ),
        last_transport_transition: None,
        relay_available: false,
        probe_stable: true,
        deployment_mode: "public".to_string(),
        node_role_claim: "validator_core".to_string(),
        rationale: Vec::new(),
    }
}

fn replication_with_active_peers(peer_count: usize) -> super::ChainReplicationDebugStatus {
    super::ChainReplicationDebugStatus {
        local_peer_id: "peer-local".to_string(),
        connected_peers: (0..peer_count)
            .map(|index| format!("peer-{index}"))
            .collect(),
        peer_healths: (0..peer_count)
            .map(|index| super::ChainPeerHealthStatus {
                peer_id: format!("peer-{index}"),
                status: "active".to_string(),
                issues: Vec::new(),
                discovery_sources: vec!["static_bootstrap".to_string()],
                active_path_kind: Some("direct".to_string()),
                source_operator: None,
                source_asn: None,
            })
            .collect(),
        registered_protocols: Vec::new(),
        protocol_retry_cooldown_peers: BTreeMap::new(),
        transport_retry_cooldown_peers: Vec::new(),
        request_peer_scores: BTreeMap::new(),
        connection_events: Vec::new(),
        recent_errors: Vec::new(),
    }
}

fn peer_head(
    node_id: &str,
    validator_id: Option<&str>,
    height: u64,
    observed_at_ms: i64,
) -> NodePeerCommittedHead {
    NodePeerCommittedHead {
        node_id: node_id.to_string(),
        validator_id: validator_id.map(str::to_string),
        height,
        block_hash: format!("block-{height}"),
        committed_at_ms: observed_at_ms,
        observed_at_ms,
        execution_block_hash: Some(format!("execution-{height}")),
        execution_state_root: Some(format!("state-{height}")),
    }
}

fn add_test_stake_proof_chain(consensus: &mut NodeConsensusSnapshot) {
    consensus.validator_set_hash = "test-validator-set-hash".to_string();
    consensus.validator_stake_root = "test-validator-stake-root".to_string();
    consensus.validator_stake_proofs = consensus
        .validator_stakes
        .iter()
        .map(|(validator_id, stake)| NodeValidatorStakeProofSnapshot {
            validator_id: validator_id.clone(),
            player_id: format!("player-{validator_id}"),
            stake: *stake,
            signer_public_key_hex: None,
            leaf_hash: format!("leaf-{validator_id}-{stake}"),
            proof: Vec::new(),
        })
        .collect();
}

#[test]
fn parse_options_loads_network_tier_manifest_and_bootstrap_peers() {
    let runtime_sha256 = current_test_binary_sha256();
    let (dir, manifest_path) = write_test_network_tier_manifest(runtime_sha256.as_str());
    let options = parse_options(
        [
            "--network-tier-manifest",
            manifest_path.to_string_lossy().as_ref(),
        ]
        .into_iter(),
    )
    .expect("parse should succeed");

    let loaded = options
        .loaded_network_tier_manifest
        .as_ref()
        .expect("manifest should load");
    assert_eq!(loaded.manifest.tier, "public_testnet");
    assert_eq!(loaded.bootstrap_peers.len(), 2);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn status_payload_exposes_loaded_network_tier_manifest() {
    let runtime_sha256 = current_test_binary_sha256();
    let (dir, manifest_path) = write_test_network_tier_manifest(runtime_sha256.as_str());
    let loaded = LoadedNetworkTierManifest::load(manifest_path.as_path()).expect("load manifest");
    let snapshot = NodeSnapshot {
        node_id: "node-a".to_string(),
        player_id: "player-a".to_string(),
        world_id: "live-a".to_string(),
        role: NodeRole::Observer,
        replication_enabled: false,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: Some(1_700_000_000_000),
        consensus: NodeConsensusSnapshot::default(),
        consensus_progress_observer_error: None,
        last_error: None,
    };
    let reward_runtime = super::reward_runtime_worker::RewardRuntimeMetricsSnapshot {
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
    };
    let storage = super::storage_metrics::StorageMetricsSnapshot {
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
    };
    let recommendation = NodeNetworkPolicy::recommend_for_user_mode(
        NodeRole::Observer,
        NodeUserMode::PrivateSafe,
        NodeReachabilityAutoDetection::default(),
        false,
    )
    .expect("recommendation");
    let execution_world_dir = dir.join("execution-world");
    let payload = build_chain_status_payload(
        snapshot,
        execution_world_dir.as_path(),
        None,
        Some(&loaded),
        &recommendation,
        None,
        NodeNetworkPolicy {
            deployment_mode: PeerDeploymentMode::Public,
            node_role_claim: PeerNodeRole::Relay,
        },
        &Libp2pReachabilitySnapshot::default(),
        NodeReachabilityAutoDetection::default(),
        ReleaseSecurityPolicy::default(),
        reward_runtime,
        storage,
        super::wasm_status::ChainWasmStatus {
            metrics_available: false,
            observed_since_unix_ms: None,
            degraded_reason: None,
            build: super::wasm_status::ChainWasmBuildStatus {
                metrics_available: false,
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
        },
        None,
        super::ChainTrafficStatus {
            udp_gossip: None,
            libp2p_replication: oasis7_node::Libp2pTrafficMetricsSnapshot::default(),
        },
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
        },
        super::ChainReplicationDebugStatus::default(),
    );
    let tier = payload.network_tier.expect("network tier should exist");
    assert_eq!(tier.tier, "public_testnet");
    assert_eq!(tier.bootstrap_peer_count, 2);
    assert_eq!(tier.token_symbol, "OC");
    assert_eq!(
        payload.world_resource.schema_version,
        oasis7_runtime::CHAIN_RESOURCE_MANIFEST_SCHEMA_V1
    );
    assert_eq!(
        payload.world_resource.delta_schema_version,
        oasis7_runtime::CHAIN_RESOURCE_DELTA_SCHEMA_V1
    );
    assert_eq!(
        payload.world_resource.chunk_generation_schema_version,
        oasis7_runtime::CHUNK_GENERATION_SCHEMA_V1
    );
    assert_eq!(payload.world_resource.chain_id, "oasis7-public_testnet");
    assert_eq!(payload.world_resource.readiness_status, "not_ready");
    assert!(
        payload
            .world_resource
            .failed_gates
            .iter()
            .any(|gate| gate.starts_with("world_resource_snapshot_unavailable"))
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn public_testnet_validator_network_head_uses_manifest_quorum() {
    let runtime_sha256 = current_test_binary_sha256();
    let (dir, manifest_path) = write_test_network_tier_manifest(runtime_sha256.as_str());
    let loaded = LoadedNetworkTierManifest::load(manifest_path.as_path()).expect("load manifest");
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 10;
    consensus.network_committed_height = 10;
    consensus.replication_persisted_height = 10;
    consensus.known_peer_heads = 1;
    consensus.peer_heads = vec![NodePeerCommittedHead {
        node_id: "peer-a".to_string(),
        validator_id: None,
        height: 10,
        block_hash: "block-a".to_string(),
        committed_at_ms: 1_000,
        observed_at_ms: i64::MAX,
        execution_block_hash: Some("execution-a".to_string()),
        execution_state_root: Some("state-a".to_string()),
    }];
    let snapshot = NodeSnapshot {
        node_id: "node-a".to_string(),
        player_id: "player-a".to_string(),
        world_id: "live-a".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: Some(1_700_000_000_000),
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };

    let network_head = super::status_payload::build_network_head_status(
        &snapshot,
        1_700_000_000_000,
        Some(&loaded),
    );

    assert_eq!(network_head.required_peer_count, 2);
    assert_eq!(network_head.fresh_peer_count, 1);
    assert_eq!(network_head.decision, "degraded");

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn public_testnet_validator_genesis_cold_start_uses_self_head() {
    let runtime_sha256 = current_test_binary_sha256();
    let (dir, manifest_path) = write_test_network_tier_manifest(runtime_sha256.as_str());
    let loaded = LoadedNetworkTierManifest::load(manifest_path.as_path()).expect("load manifest");
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 0;
    consensus.network_committed_height = 0;
    consensus.replication_persisted_height = 0;
    consensus.known_peer_heads = 0;
    consensus.peer_heads = Vec::new();
    let snapshot = NodeSnapshot {
        node_id: "node-a".to_string(),
        player_id: "player-a".to_string(),
        world_id: "live-a".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: Some(1_700_000_000_000),
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };

    let network_head = super::status_payload::build_network_head_status(
        &snapshot,
        1_700_000_000_000,
        Some(&loaded),
    );

    assert_eq!(network_head.required_peer_count, 0);
    assert_eq!(network_head.fresh_peer_count, 0);
    assert_eq!(network_head.source, "self_only");
    assert_eq!(network_head.decision, "ready");

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn public_testnet_observer_clean_start_waits_for_peer_head() {
    let runtime_sha256 = current_test_binary_sha256();
    let (dir, manifest_path) = write_test_network_tier_manifest(runtime_sha256.as_str());
    let loaded = LoadedNetworkTierManifest::load(manifest_path.as_path()).expect("load manifest");
    let snapshot = NodeSnapshot {
        node_id: "observer-a".to_string(),
        player_id: "player-observer-a".to_string(),
        world_id: "live-a".to_string(),
        role: NodeRole::Observer,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: Some(1_700_000_000_000),
        consensus: NodeConsensusSnapshot::default(),
        consensus_progress_observer_error: None,
        last_error: None,
    };

    let network_head = super::status_payload::build_network_head_status(
        &snapshot,
        1_700_000_000_000,
        Some(&loaded),
    );

    assert_eq!(network_head.required_peer_count, 1);
    assert_eq!(network_head.fresh_peer_count, 0);
    assert_eq!(network_head.source, "unknown");
    assert_eq!(network_head.decision, "degraded");

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn public_testnet_validator_network_head_prefers_highest_quorum_bucket() {
    let runtime_sha256 = current_test_binary_sha256();
    let (dir, manifest_path) = write_test_network_tier_manifest(runtime_sha256.as_str());
    let loaded = LoadedNetworkTierManifest::load(manifest_path.as_path()).expect("load manifest");
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 10;
    consensus.network_committed_height = 11;
    consensus.replication_persisted_height = 10;
    consensus.known_peer_heads = 3;
    consensus.peer_heads = vec![
        peer_head("node-b", None, 10, i64::MAX),
        peer_head("node-c", None, 10, i64::MAX),
        peer_head("node-d", None, 11, i64::MAX),
    ];
    let snapshot = NodeSnapshot {
        node_id: "node-a".to_string(),
        player_id: "player-a".to_string(),
        world_id: "live-a".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: Some(1_700_000_000_000),
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };

    let network_head = super::status_payload::build_network_head_status(
        &snapshot,
        1_700_000_000_000,
        Some(&loaded),
    );

    assert_eq!(network_head.required_peer_count, 2);
    assert_eq!(network_head.fresh_peer_count, 3);
    assert_eq!(network_head.source, "peer_quorum");
    assert_eq!(network_head.decision, "ready");
    assert_eq!(network_head.height, Some(10));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn public_testnet_validator_accepts_old_peer_heads_that_match_current_local_head() {
    let runtime_sha256 = current_test_binary_sha256();
    let (dir, manifest_path) = write_test_network_tier_manifest(runtime_sha256.as_str());
    let loaded = LoadedNetworkTierManifest::load(manifest_path.as_path()).expect("load manifest");
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 10;
    consensus.network_committed_height = 10;
    consensus.replication_persisted_height = 10;
    consensus.last_block_hash = Some("block-10".to_string());
    consensus.last_execution_block_hash = Some("execution-10".to_string());
    consensus.last_execution_state_root = Some("state-10".to_string());
    consensus.known_peer_heads = 2;
    consensus.peer_heads = vec![
        peer_head("node-b", None, 10, 1_000),
        peer_head("node-c", None, 10, 1_000),
    ];
    let snapshot = NodeSnapshot {
        node_id: "node-a".to_string(),
        player_id: "player-a".to_string(),
        world_id: "live-a".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: Some(1_700_000_000_000),
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };

    let network_head = super::status_payload::build_network_head_status(
        &snapshot,
        1_700_000_000_000,
        Some(&loaded),
    );

    assert_eq!(network_head.required_peer_count, 2);
    assert_eq!(network_head.fresh_peer_count, 2);
    assert_eq!(network_head.stale_peer_count, 0);
    assert_eq!(network_head.source, "peer_quorum");
    assert_eq!(network_head.decision, "ready");
    assert_eq!(network_head.height, Some(10));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn public_testnet_validator_keeps_old_peer_heads_stale_when_local_head_differs() {
    let runtime_sha256 = current_test_binary_sha256();
    let (dir, manifest_path) = write_test_network_tier_manifest(runtime_sha256.as_str());
    let loaded = LoadedNetworkTierManifest::load(manifest_path.as_path()).expect("load manifest");
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 10;
    consensus.network_committed_height = 10;
    consensus.replication_persisted_height = 10;
    consensus.last_block_hash = Some("different-block-10".to_string());
    consensus.last_execution_block_hash = Some("execution-10".to_string());
    consensus.last_execution_state_root = Some("state-10".to_string());
    consensus.known_peer_heads = 2;
    consensus.peer_heads = vec![
        peer_head("node-b", None, 10, 1_000),
        peer_head("node-c", None, 10, 1_000),
    ];
    let snapshot = NodeSnapshot {
        node_id: "node-a".to_string(),
        player_id: "player-a".to_string(),
        world_id: "live-a".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: Some(1_700_000_000_000),
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };

    let network_head = super::status_payload::build_network_head_status(
        &snapshot,
        1_700_000_000_000,
        Some(&loaded),
    );

    assert_eq!(network_head.required_peer_count, 2);
    assert_eq!(network_head.fresh_peer_count, 0);
    assert_eq!(network_head.stale_peer_count, 2);
    assert_eq!(network_head.source, "unknown");
    assert_eq!(network_head.decision, "degraded");

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn public_testnet_validator_keeps_old_peer_heads_stale_when_execution_binding_differs() {
    let runtime_sha256 = current_test_binary_sha256();
    let (dir, manifest_path) = write_test_network_tier_manifest(runtime_sha256.as_str());
    let loaded = LoadedNetworkTierManifest::load(manifest_path.as_path()).expect("load manifest");
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 10;
    consensus.network_committed_height = 10;
    consensus.replication_persisted_height = 10;
    consensus.last_block_hash = Some("block-10".to_string());
    consensus.last_execution_block_hash = Some("different-execution-10".to_string());
    consensus.last_execution_state_root = Some("different-state-10".to_string());
    consensus.known_peer_heads = 2;
    consensus.peer_heads = vec![
        peer_head("node-b", None, 10, 1_000),
        peer_head("node-c", None, 10, 1_000),
    ];
    let snapshot = NodeSnapshot {
        node_id: "node-a".to_string(),
        player_id: "player-a".to_string(),
        world_id: "live-a".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: Some(1_700_000_000_000),
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };

    let network_head = super::status_payload::build_network_head_status(
        &snapshot,
        1_700_000_000_000,
        Some(&loaded),
    );

    assert_eq!(network_head.required_peer_count, 2);
    assert_eq!(network_head.fresh_peer_count, 0);
    assert_eq!(network_head.stale_peer_count, 2);
    assert_eq!(network_head.source, "unknown");
    assert_eq!(network_head.decision, "degraded");

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn public_testnet_validator_keeps_old_peer_heads_stale_while_local_node_is_catching_up() {
    let runtime_sha256 = current_test_binary_sha256();
    let (dir, manifest_path) = write_test_network_tier_manifest(runtime_sha256.as_str());
    let loaded = LoadedNetworkTierManifest::load(manifest_path.as_path()).expect("load manifest");
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 9;
    consensus.network_committed_height = 10;
    consensus.replication_persisted_height = 9;
    consensus.last_block_hash = Some("block-9".to_string());
    consensus.last_execution_block_hash = Some("execution-9".to_string());
    consensus.last_execution_state_root = Some("state-9".to_string());
    consensus.known_peer_heads = 2;
    consensus.peer_heads = vec![
        peer_head("node-b", None, 10, 1_000),
        peer_head("node-c", None, 10, 1_000),
    ];
    let snapshot = NodeSnapshot {
        node_id: "node-a".to_string(),
        player_id: "player-a".to_string(),
        world_id: "live-a".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: Some(1_700_000_000_000),
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };

    let network_head = super::status_payload::build_network_head_status(
        &snapshot,
        1_700_000_000_000,
        Some(&loaded),
    );

    assert_eq!(network_head.required_peer_count, 2);
    assert_eq!(network_head.fresh_peer_count, 0);
    assert_eq!(network_head.stale_peer_count, 2);
    assert_eq!(network_head.source, "unknown");
    assert_eq!(network_head.decision, "degraded");

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn local_devnet_network_head_honors_zero_peer_quorum() {
    let runtime_sha256 = current_test_binary_sha256();
    let (dir, manifest_path) =
        write_test_network_tier_manifest_for_tier(runtime_sha256.as_str(), "local_devnet");
    let loaded = LoadedNetworkTierManifest::load(manifest_path.as_path()).expect("load manifest");
    let snapshot = NodeSnapshot {
        node_id: "node-a".to_string(),
        player_id: "player-a".to_string(),
        world_id: "local-a".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: Some(1_700_000_000_000),
        consensus: NodeConsensusSnapshot::default(),
        consensus_progress_observer_error: None,
        last_error: None,
    };

    let network_head = super::status_payload::build_network_head_status(
        &snapshot,
        1_700_000_000_000,
        Some(&loaded),
    );

    assert_eq!(network_head.required_peer_count, 0);
    assert_eq!(network_head.source, "self_only");
    assert_eq!(network_head.decision, "ready");

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn mainnet_validator_network_head_uses_stake_weighted_quorum() {
    let runtime_sha256 = current_test_binary_sha256();
    let (dir, manifest_path) =
        write_test_network_tier_manifest_for_tier(runtime_sha256.as_str(), "mainnet");
    let loaded = LoadedNetworkTierManifest::load(manifest_path.as_path()).expect("load manifest");
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 10;
    consensus.network_committed_height = 10;
    consensus.replication_persisted_height = 10;
    consensus.known_peer_heads = 2;
    consensus.validator_stakes = BTreeMap::from([
        ("validator-a".to_string(), 40),
        ("validator-b".to_string(), 34),
        ("validator-c".to_string(), 26),
    ]);
    consensus.required_stake = 67;
    consensus.total_stake = 100;
    add_test_stake_proof_chain(&mut consensus);
    consensus.peer_heads = vec![
        peer_head("node-b", Some("validator-b"), 10, i64::MAX),
        peer_head("node-c", Some("validator-c"), 10, i64::MAX),
    ];
    let snapshot = NodeSnapshot {
        node_id: "validator-a".to_string(),
        player_id: "player-a".to_string(),
        world_id: "mainnet-a".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: Some(1_700_000_000_000),
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };

    let network_head = super::status_payload::build_network_head_status(
        &snapshot,
        1_700_000_000_000,
        Some(&loaded),
    );

    assert_eq!(network_head.quorum_mode, "stake_weighted");
    assert_eq!(network_head.observed_stake, 60);
    assert_eq!(network_head.required_stake, 67);
    assert!(!network_head.stake_quorum_met);
    assert_eq!(network_head.decision, "degraded");

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn mainnet_validator_keeps_old_peer_heads_stale_even_when_they_match_current_local_head() {
    let runtime_sha256 = current_test_binary_sha256();
    let (dir, manifest_path) =
        write_test_network_tier_manifest_for_tier(runtime_sha256.as_str(), "mainnet");
    let loaded = LoadedNetworkTierManifest::load(manifest_path.as_path()).expect("load manifest");
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 10;
    consensus.network_committed_height = 10;
    consensus.replication_persisted_height = 10;
    consensus.last_block_hash = Some("block-10".to_string());
    consensus.last_execution_block_hash = Some("execution-10".to_string());
    consensus.last_execution_state_root = Some("state-10".to_string());
    consensus.known_peer_heads = 2;
    consensus.validator_stakes = BTreeMap::from([
        ("validator-a".to_string(), 40),
        ("validator-b".to_string(), 34),
        ("validator-c".to_string(), 26),
    ]);
    consensus.required_stake = 67;
    consensus.total_stake = 100;
    add_test_stake_proof_chain(&mut consensus);
    consensus.peer_heads = vec![
        peer_head("node-b", Some("validator-b"), 10, 1_000),
        peer_head("node-c", Some("validator-c"), 10, 1_000),
    ];
    let snapshot = NodeSnapshot {
        node_id: "validator-a".to_string(),
        player_id: "player-a".to_string(),
        world_id: "mainnet-a".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: Some(1_700_000_000_000),
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };

    let network_head = super::status_payload::build_network_head_status(
        &snapshot,
        1_700_000_000_000,
        Some(&loaded),
    );

    assert_eq!(network_head.quorum_mode, "stake_weighted");
    assert_eq!(network_head.fresh_peer_count, 0);
    assert_eq!(network_head.stale_peer_count, 2);
    assert_eq!(network_head.observed_stake, 0);
    assert_eq!(network_head.source, "unknown");
    assert_eq!(network_head.decision, "degraded");

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn mainnet_validator_readiness_blocks_when_stake_mapping_is_unavailable() {
    let runtime_sha256 = current_test_binary_sha256();
    let (dir, manifest_path) =
        write_test_network_tier_manifest_for_tier(runtime_sha256.as_str(), "mainnet");
    let loaded = LoadedNetworkTierManifest::load(manifest_path.as_path()).expect("load manifest");
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 10;
    consensus.network_committed_height = 10;
    consensus.replication_persisted_height = 10;
    consensus.known_peer_heads = 3;
    consensus.peer_heads = vec![
        peer_head("node-b", None, 10, i64::MAX),
        peer_head("node-c", None, 10, i64::MAX),
        peer_head("node-d", None, 10, i64::MAX),
    ];
    let snapshot = NodeSnapshot {
        node_id: "validator-a".to_string(),
        player_id: "player-a".to_string(),
        world_id: "mainnet-a".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: Some(1_700_000_000_000),
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };
    let p2p = public_p2p_status();
    let policy = super::status_payload::readiness_policy(&snapshot, Some(&loaded));
    let network_head = super::status_payload::build_network_head_status(
        &snapshot,
        1_700_000_000_000,
        Some(&loaded),
    );
    let status = super::status_payload::build_chain_node_observability_status(
        &snapshot,
        &minimal_storage_metrics(),
        &minimal_reward_metrics(),
        &replication_with_active_peers(3),
        &network_head,
        &p2p,
        &policy,
        None,
        1_700_000_000_000,
    );

    assert_eq!(policy.quorum_mode, "count_fallback_stake_unavailable");
    assert!(!status.ready);
    assert!(
        status
            .alerts
            .iter()
            .any(|alert| alert.code == "consensus_stake_quorum_unavailable")
    );

    let _ = fs::remove_dir_all(dir);
}

#[path = "oasis7_chain_runtime_network_tier_generated_world_tests.rs"]
mod generated_world_tests;
#[path = "oasis7_chain_runtime_network_tier_mainnet_policy_tests.rs"]
mod mainnet_policy_tests;

#[path = "oasis7_chain_runtime_planned_policy_tests.rs"]
mod planned_policy_tests;
