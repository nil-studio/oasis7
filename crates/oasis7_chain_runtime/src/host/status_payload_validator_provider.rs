use super::super::ChainReplicationDebugStatus;
use super::super::execution_bridge::ExecutionBridgeCommitTimingSnapshot;
use super::super::runtime_authority::RuntimeAuthorityBinding;
use super::super::traffic_status::ChainTrafficStatus;
use super::super::wasm_status::ChainWasmStatus;
use super::ChainNodeObservabilityStatus;
use super::status_payload_chain_proof::ChainProofStatus;
use super::status_payload_consensus::ChainConsensusStatus;
use super::status_payload_lifecycle::{ChainLivenessStatus, ChainReadinessStatus, ChainSyncStatus};
use super::status_payload_module_tick_routing::ChainModuleTickRoutingStatus;
use super::status_payload_network_tier::ChainNetworkTierStatus;
use super::status_payload_p2p::ChainP2pStatus;
use super::status_payload_world_resource::ChainWorldResourceStatus;
use super::storage_metrics;
use oasis7::simulator::RuntimePerfSnapshot;
use oasis7_node::{NodeRole, NodeSnapshot, NodeValidatorStakeProofSnapshot};
use oasis7_runtime::ReleaseSecurityPolicy;
use serde::Serialize;
use std::path::Path;

const TRIAD_STATUS_PROJECTION_SCHEMA: &str = "oasis7.chain_validator_provider_status.v1";

pub(crate) fn build_chain_validator_status(
    snapshot: &NodeSnapshot,
    authority_binding: Option<&RuntimeAuthorityBinding>,
) -> ChainValidatorStatus {
    let local_validator = snapshot
        .consensus
        .validator_stake_proofs
        .iter()
        .find(|proof| proof.validator_id == snapshot.node_id);
    let stake = snapshot
        .consensus
        .validator_stakes
        .get(snapshot.node_id.as_str())
        .copied();
    let authority_identity_matches = authority_binding.is_none_or(|binding| {
        binding
            .validator_signer_public_keys
            .get(snapshot.node_id.as_str())
            .is_some_and(|expected| {
                local_validator
                    .and_then(|proof| proof.signer_public_key_hex.as_deref())
                    .is_some_and(|actual| actual.eq_ignore_ascii_case(expected))
                    && stake
                        == binding
                            .validator_stakes
                            .get(snapshot.node_id.as_str())
                            .copied()
            })
    });
    let membership_active = local_validator.is_some()
        && stake.is_some_and(|stake| stake > 0)
        && authority_identity_matches
        && !snapshot
            .consensus
            .quarantined_validators
            .iter()
            .any(|validator| validator == &snapshot.node_id)
        && !snapshot.consensus.validator_set_hash.is_empty()
        && !snapshot.consensus.validator_stake_root.is_empty();
    ChainValidatorStatus {
        schema_version: TRIAD_STATUS_PROJECTION_SCHEMA.to_string(),
        role: if membership_active {
            "validator".to_string()
        } else {
            "non_validator".to_string()
        },
        membership: if membership_active {
            "active".to_string()
        } else {
            "inactive".to_string()
        },
        stake,
        signer_binding: local_validator.map(|proof| proof.validator_id.clone()),
        signer_public_key_hex: local_validator
            .and_then(|proof| proof.signer_public_key_hex.clone()),
        stake_proof: local_validator.cloned(),
        validator_set_hash: snapshot.consensus.validator_set_hash.clone(),
        stake_root: snapshot.consensus.validator_stake_root.clone(),
        registry_ref: authority_binding.map(|binding| binding.registry_ref.clone()),
        registry_sha256: authority_binding.map(|binding| binding.registry_sha256.clone()),
        registry_semantic_sha256: authority_binding
            .map(|binding| binding.registry_semantic_sha256.clone()),
        inventory_ref: authority_binding.and_then(|binding| {
            (!binding.inventory_ref.is_empty()).then(|| binding.inventory_ref.clone())
        }),
        inventory_sha256: authority_binding.and_then(|binding| {
            (!binding.inventory_sha256.is_empty()).then(|| binding.inventory_sha256.clone())
        }),
    }
}

pub(crate) fn build_chain_provider_status(
    snapshot: &NodeSnapshot,
    world_resource: &ChainWorldResourceStatus,
    chain_proof: &ChainProofStatus,
    storage_metrics: &storage_metrics::StorageMetricsSnapshot,
    replication: &ChainReplicationDebugStatus,
    authority_binding: Option<&RuntimeAuthorityBinding>,
) -> ChainProviderStatus {
    let observed_provider_id =
        (!replication.local_peer_id.trim().is_empty()).then(|| replication.local_peer_id.clone());
    let provider_id = match (
        observed_provider_id,
        authority_binding.and_then(|binding| binding.validator_47_provider_peer_id.as_deref()),
    ) {
        (Some(actual), Some(expected)) if snapshot.node_id == "triad-testnet-validator-47" => {
            (actual == expected).then_some(actual)
        }
        (Some(actual), _) => Some(actual),
        (None, _) => None,
    };
    let storage_runtime_healthy = matches!(snapshot.role, NodeRole::Storage)
        && snapshot.replication_enabled
        && provider_id.is_some()
        && storage_metrics.degraded_reason.is_none()
        && world_resource.failed_gates.is_empty()
        && storage_metrics.checkpoint_count > 0
        && storage_metrics
            .replay_summary
            .latest_checkpoint_height
            .is_some();
    let checkpoint = chain_proof
        .latest_execution_checkpoint
        .as_ref()
        .zip(chain_proof.latest_world_head_proof.as_ref())
        .is_some_and(|(checkpoint, proof)| {
            let checkpoint_identity_matches = chain_proof
                .source_record_path
                .as_deref()
                .and_then(|source_record_path| Path::new(source_record_path).parent())
                .and_then(|records_dir| {
                    super::super::execution_bridge::load_latest_execution_checkpoint_status_evidence(
                        records_dir,
                    )
                    .ok()
                    .flatten()
                })
                .is_some_and(|authoritative| {
                    authoritative.schema_version == checkpoint.schema_version
                        && authoritative.checkpoint_id == checkpoint.checkpoint_id
                        && authoritative.manifest_hash == checkpoint.manifest_hash
                        && authoritative.world_id == proof.world_id
                        && authoritative.height == proof.height
                        && authoritative.execution_block_hash == proof.execution_block_hash
                        && authoritative.execution_state_root == proof.execution_state_root
                });
            let expected_checkpoint_ref = format!("{:020}/manifest.json", proof.height);
            storage_runtime_healthy
                && chain_proof.status == "available"
                && checkpoint.schema_version >= 2
                && checkpoint.height > 0
                && storage_metrics.replay_summary.latest_checkpoint_height
                    == Some(checkpoint.height)
                && checkpoint.height == proof.height
                && !checkpoint.checkpoint_id.trim().is_empty()
                && !checkpoint.manifest_hash.trim().is_empty()
                && !proof.proof_hash.trim().is_empty()
                && proof.world_id == snapshot.world_id
                && proof.world_id == world_resource.world_id
                && !world_resource.chain_id.trim().is_empty()
                && proof
                    .checkpoint_ref
                    .as_deref()
                    .is_some_and(|value| value == expected_checkpoint_ref)
                && checkpoint_identity_matches
        });
    let (checkpoint_proof, full_storage_proof) = if checkpoint {
        let checkpoint = chain_proof
            .latest_execution_checkpoint
            .as_ref()
            .expect("checkpoint proof checked above");
        let proof = chain_proof
            .latest_world_head_proof
            .as_ref()
            .expect("world head proof checked above");
        let provider_id = provider_id.clone().expect("provider id checked above");
        let checkpoint_proof = ChainProviderCheckpointProof {
            schema_version: checkpoint.schema_version,
            checkpoint_id: checkpoint.checkpoint_id.clone(),
            height: checkpoint.height,
            manifest_hash: checkpoint.manifest_hash.clone(),
            proof_hash: proof.proof_hash.clone(),
            world_id: proof.world_id.clone(),
            chain_id: world_resource.chain_id.clone(),
        };
        let full_storage_proof = ChainProviderFullStorageProof {
            status: "ready".to_string(),
            provider_id,
            world_id: proof.world_id.clone(),
            chain_id: world_resource.chain_id.clone(),
            manifest_hash: checkpoint.manifest_hash.clone(),
            height: checkpoint.height,
        };
        (Some(checkpoint_proof), Some(full_storage_proof))
    } else {
        (None, None)
    };
    ChainProviderStatus {
        schema_version: TRIAD_STATUS_PROJECTION_SCHEMA.to_string(),
        node_id: snapshot.node_id.clone(),
        provider_id,
        checkpoint,
        // Full-storage readiness is only claimable once the local runtime has
        // an actual retained checkpoint that is bound to the world-head proof.
        full_storage: checkpoint,
        checkpoint_proof,
        full_storage_proof,
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct ChainStatusResponse {
    pub(crate) ok: bool,
    pub(crate) observed_at_unix_ms: i64,
    pub(crate) node_id: String,
    pub(crate) world_id: String,
    pub(crate) role: String,
    pub(crate) running: bool,
    pub(crate) liveness: ChainLivenessStatus,
    pub(crate) readiness: ChainReadinessStatus,
    pub(crate) sync: ChainSyncStatus,
    pub(crate) worker_poll_count: u64,
    pub(crate) tick_count: u64,
    pub(crate) last_tick_unix_ms: Option<i64>,
    pub(crate) consensus: ChainConsensusStatus,
    pub(crate) chain_proof: ChainProofStatus,
    pub(crate) consensus_progress_observer_error: Option<String>,
    pub(crate) last_error: Option<String>,
    pub(crate) execution_world_dir: String,
    pub(crate) network_tier: Option<ChainNetworkTierStatus>,
    pub(crate) world_resource: ChainWorldResourceStatus,
    pub(crate) p2p: ChainP2pStatus,
    pub(crate) observability: ChainNodeObservabilityStatus,
    pub(crate) release_security_policy: ReleaseSecurityPolicy,
    pub(crate) reward_runtime: super::super::reward_runtime_worker::RewardRuntimeMetricsSnapshot,
    pub(crate) storage: storage_metrics::StorageMetricsSnapshot,
    pub(crate) wasm: ChainWasmStatus,
    pub(crate) runtime_perf: Option<RuntimePerfSnapshot>,
    pub(crate) traffic: ChainTrafficStatus,
    pub(crate) transactions: super::super::transfer_submit_api::ChainTransferMetricsStatus,
    pub(crate) replication: ChainReplicationDebugStatus,
    pub(crate) execution_bridge_commit_timing: ExecutionBridgeCommitTimingSnapshot,
    pub(crate) module_tick_routing: ChainModuleTickRoutingStatus,
    /// Effective local validator admission data. This is derived only from
    /// the node's consensus snapshot; collectors must not infer it when the
    /// runtime cannot provide it.
    pub(crate) validator: ChainValidatorStatus,
    /// Effective local provider data. Provider readiness requires a real
    /// local peer id, storage capability, and an authenticated checkpoint.
    pub(crate) provider: ChainProviderStatus,
}

#[derive(Debug, Serialize)]
pub(crate) struct ChainValidatorStatus {
    pub(crate) schema_version: String,
    pub(crate) role: String,
    pub(crate) membership: String,
    pub(crate) stake: Option<u64>,
    pub(crate) signer_binding: Option<String>,
    pub(crate) signer_public_key_hex: Option<String>,
    pub(crate) stake_proof: Option<NodeValidatorStakeProofSnapshot>,
    pub(crate) validator_set_hash: String,
    pub(crate) stake_root: String,
    /// These bindings remain optional until deployment supplies immutable
    /// runtime paths/digests. Fleet health treats absence as a hard failure.
    pub(crate) registry_ref: Option<String>,
    pub(crate) registry_sha256: Option<String>,
    pub(crate) registry_semantic_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) inventory_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) inventory_sha256: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ChainProviderStatus {
    pub(crate) schema_version: String,
    pub(crate) node_id: String,
    pub(crate) provider_id: Option<String>,
    pub(crate) checkpoint: bool,
    pub(crate) full_storage: bool,
    pub(crate) checkpoint_proof: Option<ChainProviderCheckpointProof>,
    pub(crate) full_storage_proof: Option<ChainProviderFullStorageProof>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ChainProviderCheckpointProof {
    pub(crate) schema_version: u32,
    pub(crate) checkpoint_id: String,
    pub(crate) height: u64,
    pub(crate) manifest_hash: String,
    pub(crate) proof_hash: String,
    pub(crate) world_id: String,
    pub(crate) chain_id: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ChainProviderFullStorageProof {
    pub(crate) status: String,
    pub(crate) provider_id: String,
    pub(crate) world_id: String,
    pub(crate) chain_id: String,
    pub(crate) manifest_hash: String,
    pub(crate) height: u64,
}

#[cfg(test)]
mod tests {
    use super::super::status_payload_chain_proof::{
        ChainProofStatus, LatestExecutionCheckpointStatus, LatestWorldHeadProofStatus,
    };
    use super::super::status_payload_world_resource::ChainWorldResourceStatus;
    use super::*;
    use oasis7_node::{NodeConsensusSnapshot, NodeRole, NodeSnapshot};
    use oasis7_proto::storage_profile::{StorageProfile, StorageProfileConfig};
    use oasis7_runtime::blake3_hex;
    use serde::Serialize;
    use std::collections::BTreeMap;

    #[derive(Serialize)]
    struct ManifestHashPayload<'a> {
        schema_version: u32,
        checkpoint_id: &'a str,
        world_id: &'a str,
        height: u64,
        execution_block_hash: &'a str,
        execution_state_root: &'a str,
        predecessor_execution_block_hash: Option<&'a str>,
        latest_state_ref: &'a str,
        snapshot_ref: Option<&'a str>,
        journal_ref: Option<&'a str>,
        pinned_refs: &'a [String],
        created_at_ms: i64,
    }

    fn temp_records_dir() -> std::path::PathBuf {
        let unique = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock after epoch")
                .as_nanos()
        );
        let dir = std::env::temp_dir().join(format!("oasis7-provider-status-{unique}"));
        std::fs::create_dir_all(dir.as_path()).expect("create records dir");
        dir
    }

    fn write_checkpoint_fixture(records_dir: &std::path::Path) -> String {
        let checkpoint_id = "checkpoint-00000000000000000042-execution-a";
        let pinned_refs = vec!["journal-ref".to_string(), "snapshot-ref".to_string()];
        let hash_bytes = serde_cbor::to_vec(&ManifestHashPayload {
            schema_version: 2,
            checkpoint_id,
            world_id: "live-a",
            height: 42,
            execution_block_hash: "execution-a",
            execution_state_root: "state-a",
            predecessor_execution_block_hash: Some("previous-execution"),
            latest_state_ref: "snapshot-ref",
            snapshot_ref: Some("snapshot-ref"),
            journal_ref: Some("journal-ref"),
            pinned_refs: pinned_refs.as_slice(),
            created_at_ms: 1_700_000_000_000,
        })
        .expect("encode manifest hash payload");
        let manifest_hash = blake3_hex(hash_bytes.as_slice());
        let manifest_rel_path = "00000000000000000042/manifest.json";
        let manifest_path = records_dir.join("checkpoints").join(manifest_rel_path);
        std::fs::create_dir_all(manifest_path.parent().expect("manifest parent"))
            .expect("create checkpoint dir");
        let manifest = serde_json::json!({
            "schema_version": 2,
            "checkpoint_id": checkpoint_id,
            "world_id": "live-a",
            "height": 42,
            "execution_block_hash": "execution-a",
            "execution_state_root": "state-a",
            "predecessor_execution_block_hash": "previous-execution",
            "latest_state_ref": "snapshot-ref",
            "snapshot_ref": "snapshot-ref",
            "journal_ref": "journal-ref",
            "pinned_refs": pinned_refs,
            "manifest_hash": manifest_hash,
            "created_at_ms": 1_700_000_000_000i64
        });
        std::fs::write(
            manifest_path,
            serde_json::to_vec_pretty(&manifest).expect("encode manifest"),
        )
        .expect("write manifest");
        std::fs::write(
            records_dir.join("checkpoints/latest.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema_version": 1,
                "checkpoint_id": checkpoint_id,
                "height": 42,
                "manifest_hash": manifest_hash,
                "manifest_rel_path": manifest_rel_path,
                "updated_at_ms": 1_700_000_000_000i64
            }))
            .expect("encode latest pointer"),
        )
        .expect("write latest pointer");
        manifest_hash
    }

    fn storage_metrics() -> storage_metrics::StorageMetricsSnapshot {
        storage_metrics::StorageMetricsSnapshot {
            storage_profile: "dev_local".to_string(),
            effective_budget: StorageProfileConfig::from(StorageProfile::DevLocal),
            bytes_by_dir: BTreeMap::new(),
            blob_counts: BTreeMap::new(),
            ref_count: 0,
            pin_count: 0,
            retained_heights: Vec::new(),
            checkpoint_count: 1,
            replay_summary: storage_metrics::StorageReplaySummary {
                latest_checkpoint_height: Some(42),
                ..storage_metrics::StorageReplaySummary::default()
            },
            orphan_blob_count: 0,
            last_gc_at_ms: None,
            last_gc_result: "not_available".to_string(),
            last_gc_error: None,
            degraded_reason: None,
        }
    }

    #[test]
    fn observer_status_omits_inventory_authority_when_not_bound() {
        let snapshot = NodeSnapshot {
            node_id: "triad-testnet-local".to_string(),
            player_id: "observer-player".to_string(),
            world_id: "oasis7-public-testnet-governed-20260606".to_string(),
            role: NodeRole::Observer,
            replication_enabled: false,
            running: true,
            tick_count: 1,
            last_tick_unix_ms: Some(1_700_000_000_000),
            consensus: NodeConsensusSnapshot::default(),
            consensus_progress_observer_error: None,
            last_error: None,
        };
        let authority = RuntimeAuthorityBinding {
            registry_ref: "config/observer-registry.json".to_string(),
            registry_sha256: "registry-sha256".to_string(),
            registry_semantic_sha256: "registry-semantic-sha256".to_string(),
            inventory_ref: String::new(),
            inventory_sha256: String::new(),
            validator_signer_public_keys: BTreeMap::new(),
            validator_stakes: BTreeMap::new(),
            validator_47_provider_peer_id: None,
        };

        let projected = build_chain_validator_status(&snapshot, Some(&authority));
        assert_eq!(projected.inventory_ref, None);
        assert_eq!(projected.inventory_sha256, None);
        let serialized = serde_json::to_value(&projected).expect("serialize observer status");
        assert!(serialized.get("inventory_ref").is_none());
        assert!(serialized.get("inventory_sha256").is_none());
    }

    #[test]
    #[expect(
        clippy::field_reassign_with_default,
        reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
    )]
    fn provider_status_rejects_same_height_unrelated_checkpoint_identity() {
        let records_dir = temp_records_dir();
        let manifest_hash = write_checkpoint_fixture(records_dir.as_path());
        let snapshot = NodeSnapshot {
            node_id: "node-a".to_string(),
            player_id: "player-a".to_string(),
            world_id: "live-a".to_string(),
            role: NodeRole::Storage,
            replication_enabled: true,
            running: true,
            tick_count: 1,
            last_tick_unix_ms: Some(1_700_000_000_000),
            consensus: NodeConsensusSnapshot::default(),
            consensus_progress_observer_error: None,
            last_error: None,
        };
        let world_resource = ChainWorldResourceStatus {
            schema_version: "oasis7.chain_resource_manifest.v1".to_string(),
            delta_schema_version: "oasis7.chain_resource_delta.v1".to_string(),
            world_id: "live-a".to_string(),
            chain_id: "live-a".to_string(),
            world_seed: 1,
            chunk_generation_schema_version: "oasis7.chunk_generation.v1".to_string(),
            seed_manifest_hash: "seed-manifest".to_string(),
            starter_chunk_manifest_hash: Some("starter-manifest".to_string()),
            latest_resource_commit_height: 42,
            latest_resource_commit_hash: Some("resource-commit".to_string()),
            committed_chunk_count: 1,
            provisional_chunk_count: 0,
            pending_delta_count: 0,
            last_delta_id: Some("delta-42".to_string()),
            last_delta_commit_height: Some(42),
            readiness_status: "ready".to_string(),
            failed_gates: Vec::new(),
        };
        let checkpoint = LatestExecutionCheckpointStatus {
            schema_version: 2,
            checkpoint_id: "checkpoint-00000000000000000042-execution-a".to_string(),
            height: 42,
            manifest_hash,
        };
        let proof = LatestWorldHeadProofStatus {
            schema_version: 1,
            world_id: "live-a".to_string(),
            height: 42,
            execution_block_hash: "execution-b".to_string(),
            execution_state_root: "state-b".to_string(),
            node_block_hash: "node-block-b".to_string(),
            action_root: "action-b".to_string(),
            world_head_proof_ref: "cas:proof-b".to_string(),
            proof_hash: "proof-hash-b".to_string(),
            checkpoint_ref: Some("00000000000000000042/manifest.json".to_string()),
        };
        let mut chain_proof = ChainProofStatus {
            schema_version: "oasis7.chain_proof_status.v1".to_string(),
            proof_contract: "WorldHeadProofV1".to_string(),
            claim_boundary:
                "head_execution_checkpoint_evidence_only_not_light_client_or_mainnet_readiness"
                    .to_string(),
            status: "available".to_string(),
            latest_world_head_proof: Some(proof),
            latest_execution_checkpoint: Some(checkpoint),
            source_record_path: Some(records_dir.join("latest.json").display().to_string()),
            load_error: None,
            does_not_claim: Vec::new(),
        };
        let mut replication = ChainReplicationDebugStatus::default();
        replication.local_peer_id = "12D3KooWProviderA".to_string();

        let projected = build_chain_provider_status(
            &snapshot,
            &world_resource,
            &chain_proof,
            &storage_metrics(),
            &replication,
            None,
        );

        assert!(
            !projected.checkpoint && !projected.full_storage,
            "provider readiness must reject same-height checkpoint/proof identity drift"
        );
        assert!(projected.checkpoint_proof.is_none());
        assert!(projected.full_storage_proof.is_none());

        {
            let proof = chain_proof
                .latest_world_head_proof
                .as_mut()
                .expect("world head proof fixture");
            proof.execution_block_hash = "execution-a".to_string();
            proof.execution_state_root = "state-a".to_string();
            proof.checkpoint_ref = Some("other-checkpoint/manifest.json".to_string());
        }
        let projected = build_chain_provider_status(
            &snapshot,
            &world_resource,
            &chain_proof,
            &storage_metrics(),
            &replication,
            None,
        );
        assert!(
            !projected.checkpoint && !projected.full_storage,
            "provider readiness must reject a non-canonical checkpoint reference"
        );

        chain_proof
            .latest_world_head_proof
            .as_mut()
            .expect("world head proof fixture")
            .checkpoint_ref = Some("00000000000000000042/manifest.json".to_string());
        let projected = build_chain_provider_status(
            &snapshot,
            &world_resource,
            &chain_proof,
            &storage_metrics(),
            &replication,
            None,
        );
        assert!(projected.checkpoint && projected.full_storage);
        assert!(projected.checkpoint_proof.is_some());
        assert!(projected.full_storage_proof.is_some());
        std::fs::remove_dir_all(records_dir).expect("remove records dir");
    }
}
