use oasis7_node::NodeSnapshot;

pub(super) fn consensus_participation_hold_reason(
    snapshot: &NodeSnapshot,
    network_height_lag: u64,
    replication_state_gap: u64,
    max_network_height_lag: u64,
) -> Option<String> {
    if !snapshot.replication_enabled {
        return None;
    }
    if let Some(height) = snapshot.consensus.replication_gap_sync_blocked_height {
        return Some(format!("replication_gap_sync_blocked_height={height}"));
    }
    if replication_state_gap > 0 {
        return Some(format!("replication_state_gap={replication_state_gap}"));
    }
    if network_height_lag > max_network_height_lag {
        return Some(format!(
            "network_height_lag={network_height_lag} allowed={}",
            max_network_height_lag
        ));
    }
    None
}

pub(super) fn state_sync_trusted_checkpoint_required_height(
    snapshot: &NodeSnapshot,
    replication_state_gap: u64,
    network_height_lag: u64,
    max_network_height_lag: u64,
) -> Option<u64> {
    if !snapshot.replication_enabled {
        return None;
    }
    let mut required_height = snapshot.consensus.replication_gap_sync_blocked_height;
    if replication_state_gap > 0 {
        required_height = Some(
            required_height
                .unwrap_or(0)
                .max(snapshot.consensus.committed_height),
        );
    }
    if network_height_lag > max_network_height_lag {
        required_height = Some(
            required_height
                .unwrap_or(0)
                .max(snapshot.consensus.network_committed_height),
        );
    }
    required_height
}

pub(super) fn state_sync_fallback_reason(
    snapshot: &NodeSnapshot,
    replication_state_gap: u64,
    network_height_lag: u64,
    max_network_height_lag: u64,
) -> Option<String> {
    let required_height = state_sync_trusted_checkpoint_required_height(
        snapshot,
        replication_state_gap,
        network_height_lag,
        max_network_height_lag,
    )?;
    let mut reasons = Vec::new();
    if let Some(height) = snapshot.consensus.replication_gap_sync_blocked_height {
        reasons.push(format!("replication_gap_sync_blocked_height={height}"));
    }
    if replication_state_gap > 0 {
        reasons.push(format!("replication_state_gap={replication_state_gap}"));
    }
    if network_height_lag > max_network_height_lag {
        reasons.push(format!("network_height_lag={network_height_lag}"));
    }
    Some(format!(
        "trusted checkpoint and verified snapshot/state-sync required at height >= {required_height}: {}",
        reasons.join(",")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_node::{NodeConsensusSnapshot, NodeRole};

    fn replicated_snapshot(committed_height: u64, network_committed_height: u64) -> NodeSnapshot {
        let consensus = NodeConsensusSnapshot {
            committed_height,
            network_committed_height,
            replication_persisted_height: committed_height,
            ..NodeConsensusSnapshot::default()
        };
        NodeSnapshot {
            node_id: "node-a".to_string(),
            player_id: "node-a".to_string(),
            world_id: "world-a".to_string(),
            role: NodeRole::Sequencer,
            replication_enabled: true,
            running: true,
            tick_count: 1,
            last_tick_unix_ms: None,
            consensus,
            consensus_progress_observer_error: None,
            last_error: None,
        }
    }

    #[test]
    fn in_policy_network_lag_does_not_require_state_sync_fallback() {
        let snapshot = replicated_snapshot(10, 12);
        assert_eq!(
            state_sync_trusted_checkpoint_required_height(&snapshot, 0, 2, 2),
            None
        );
        assert_eq!(state_sync_fallback_reason(&snapshot, 0, 2, 2), None);
    }

    #[test]
    fn over_policy_network_lag_requires_state_sync_fallback() {
        let snapshot = replicated_snapshot(10, 13);
        assert_eq!(
            state_sync_trusted_checkpoint_required_height(&snapshot, 0, 3, 2),
            Some(13)
        );
        assert!(
            state_sync_fallback_reason(&snapshot, 0, 3, 2)
                .as_deref()
                .is_some_and(|reason| reason.contains("network_height_lag=3"))
        );
    }
}
