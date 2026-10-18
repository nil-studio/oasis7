//! Host Node snapshots and filesystem observations for deterministic reward accounting.
use crate::runtime::NodePointsRuntimeObservation;
use oasis7_agent_api::node_contribution::{
    DistFsChallengeFailureReason, DistFsChallengeProofHint, DistFsChallengeSampleSource,
    NodePointsRole,
};
use oasis7_node::{NodeRole, NodeSnapshot};
use std::{fs, path::Path};
pub fn node_points_observation_from_snapshot(
    snapshot: &NodeSnapshot,
    effective_storage_bytes: u64,
    observed_at_unix_ms: i64,
) -> NodePointsRuntimeObservation {
    NodePointsRuntimeObservation {
        node_id: snapshot.node_id.clone(),
        role: match snapshot.role {
            NodeRole::Sequencer => NodePointsRole::Sequencer,
            NodeRole::Storage => NodePointsRole::Storage,
            NodeRole::Observer => NodePointsRole::Observer,
        },
        tick_count: snapshot.tick_count,
        running: snapshot.running,
        uptime_checks_passed: if snapshot.running { 1 } else { 0 },
        uptime_checks_total: 1,
        storage_checks_passed: if snapshot.role == NodeRole::Storage && snapshot.running {
            1
        } else {
            0
        },
        storage_checks_total: if snapshot.role == NodeRole::Storage {
            1
        } else {
            0
        },
        staked_storage_bytes: effective_storage_bytes,
        observed_at_unix_ms,
        has_error: snapshot.last_error.is_some(),
        effective_storage_bytes,
        storage_challenge_proof_hint: if snapshot.role == NodeRole::Storage {
            Some(DistFsChallengeProofHint {
                sample_source: DistFsChallengeSampleSource::LocalStoreIndex,
                sample_reference: format!(
                    "distfs://{}/tick/{}",
                    snapshot.node_id, snapshot.tick_count
                ),
                failure_reason: snapshot
                    .last_error
                    .as_deref()
                    .map(classify_storage_failure_reason),
                proof_kind_hint: "reserved".to_string(),
                vrf_seed_hint: None,
                post_commitment_hint: None,
            })
        } else {
            None
        },
    }
}
fn classify_storage_failure_reason(error: &str) -> DistFsChallengeFailureReason {
    let lower = error.to_ascii_lowercase();
    if lower.contains("hash") || lower.contains("integrity") {
        DistFsChallengeFailureReason::HashMismatch
    } else if lower.contains("timeout") {
        DistFsChallengeFailureReason::Timeout
    } else if lower.contains("not found") || lower.contains("missing") {
        DistFsChallengeFailureReason::MissingSample
    } else if lower.contains("io") || lower.contains("read") {
        DistFsChallengeFailureReason::ReadIoError
    } else {
        DistFsChallengeFailureReason::Unknown
    }
}
pub fn measure_directory_storage_bytes(path: &Path) -> u64 {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return 0,
    };

    if metadata.is_file() {
        return metadata.len();
    }
    if !metadata.is_dir() {
        return 0;
    }

    let mut total = 0_u64;
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(_) => return 0,
    };
    for entry in entries.flatten() {
        total = total.saturating_add(measure_directory_storage_bytes(entry.path().as_path()));
    }
    total
}
#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_node::NodeConsensusSnapshot;
    fn temp_dir(prefix: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    #[test]
    fn observation_from_snapshot_sets_distfs_proof_semantics() {
        let snapshot = NodeSnapshot {
            node_id: "node-storage".to_string(),
            player_id: "node-storage".to_string(),
            world_id: "world-1".to_string(),
            role: NodeRole::Storage,
            replication_enabled: false,
            running: true,
            tick_count: 42,
            last_tick_unix_ms: Some(1_000),
            consensus: NodeConsensusSnapshot::default(),
            consensus_progress_observer_error: None,
            last_error: Some("hash mismatch for sampled chunk".to_string()),
        };

        let observation = node_points_observation_from_snapshot(&snapshot, 1024, 2_000);
        let hint = observation
            .storage_challenge_proof_hint
            .expect("storage proof hint");
        assert_eq!(
            hint.sample_source,
            DistFsChallengeSampleSource::LocalStoreIndex
        );
        assert_eq!(hint.sample_reference, "distfs://node-storage/tick/42");
        assert_eq!(
            hint.failure_reason,
            Some(DistFsChallengeFailureReason::HashMismatch)
        );
        assert_eq!(hint.proof_kind_hint, "reserved");
        assert!(hint.vrf_seed_hint.is_none());
        assert!(hint.post_commitment_hint.is_none());
    }
    #[test]
    fn observation_from_snapshot_non_storage_has_no_distfs_proof_hint() {
        let snapshot = NodeSnapshot {
            node_id: "node-observer".to_string(),
            player_id: "node-observer".to_string(),
            world_id: "world-1".to_string(),
            role: NodeRole::Observer,
            replication_enabled: false,
            running: true,
            tick_count: 7,
            last_tick_unix_ms: Some(50),
            consensus: NodeConsensusSnapshot::default(),
            consensus_progress_observer_error: None,
            last_error: None,
        };

        let observation = node_points_observation_from_snapshot(&snapshot, 0, 100);
        assert!(observation.storage_challenge_proof_hint.is_none());
    }
    #[test]
    fn measure_directory_storage_counts_nested_files() {
        let root = temp_dir("measure-storage");
        let nested = root.join("a").join("b");
        fs::create_dir_all(&nested).expect("nested");
        fs::write(root.join("root.bin"), vec![1_u8; 11]).expect("root file");
        fs::write(nested.join("leaf.bin"), vec![1_u8; 29]).expect("leaf file");

        let bytes = measure_directory_storage_bytes(root.as_path());
        assert_eq!(bytes, 40);

        let _ = fs::remove_dir_all(root);
    }
}
