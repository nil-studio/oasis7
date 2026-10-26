use super::node_engine_network_hash::{
    peer_commit_heads_conflict, validated_commits_share_identity_block_action,
};
use super::*;

impl PosNodeEngine {
    pub(super) fn observe_network_replication_commit(
        &mut self,
        peer_node_id: &str,
        payload: &ReplicationCommitPayloadView,
    ) {
        if payload.height == 0 {
            return;
        }
        if self
            .validator_id_for_peer_head(peer_node_id)
            .map(|validator_id| self.quarantined_validators.contains(&validator_id))
            .unwrap_or(false)
        {
            return;
        }
        self.network_committed_height = self.network_committed_height.max(payload.height);
        if self
            .peer_heads
            .get(peer_node_id)
            .map(|head| payload.height >= head.height)
            .unwrap_or(true)
        {
            self.peer_heads.insert(
                peer_node_id.to_string(),
                PeerCommittedHead {
                    height: payload.height,
                    block_hash: payload.block_hash.clone(),
                    committed_at_ms: payload.committed_at_ms,
                    observed_at_ms: crate::runtime_util::now_unix_ms(),
                    execution_block_hash: payload.execution_block_hash.clone(),
                    execution_state_root: payload.execution_state_root.clone(),
                    action_root: String::new(),
                    public_key_hex: None,
                    signature_hex: None,
                },
            );
        }
    }

    pub(super) fn observe_peer_commit_message(&mut self, commit: &GossipCommitMessage) {
        if commit.height == 0 {
            return;
        }
        let next_head = PeerCommittedHead {
            height: commit.height,
            block_hash: commit.block_hash.clone(),
            committed_at_ms: commit.committed_at_ms,
            observed_at_ms: crate::runtime_util::now_unix_ms(),
            execution_block_hash: commit.execution_block_hash.clone(),
            execution_state_root: commit.execution_state_root.clone(),
            action_root: commit.action_root.clone(),
            public_key_hex: commit.public_key_hex.clone(),
            signature_hex: commit.signature_hex.clone(),
        };
        self.observe_peer_committed_head(commit.node_id.as_str(), next_head);
        if self
            .peer_heads
            .get(commit.node_id.as_str())
            .is_some_and(|head| {
                head.height == commit.height && head.block_hash == commit.block_hash
            })
        {
            self.remember_latest_validated_peer_commit(commit);
        }
    }

    fn remember_latest_validated_peer_commit(&mut self, commit: &GossipCommitMessage) {
        let replace = match self.latest_validated_peer_commit.as_ref() {
            None => true,
            Some(previous) if commit.height > previous.height => true,
            Some(previous) if commit.height < previous.height => false,
            Some(previous) => {
                validated_commits_share_identity_block_action(previous, commit)
                    && commit.execution_block_hash.is_some()
                    && commit.execution_state_root.is_some()
                    && (previous.execution_block_hash.is_none()
                        || previous.execution_state_root.is_none())
            }
        };
        if replace {
            self.latest_validated_peer_commit = Some(commit.clone());
        }
    }

    pub(super) fn observe_peer_committed_head(
        &mut self,
        peer_node_id: &str,
        mut next_head: PeerCommittedHead,
    ) {
        if next_head.height == 0 {
            return;
        }
        let validator_id = self.validator_id_for_peer_head(peer_node_id);
        if validator_id
            .as_deref()
            .map(|id| self.quarantined_validators.contains(id))
            .unwrap_or(false)
        {
            return;
        }
        let now_ms = crate::runtime_util::now_unix_ms();
        if let Some(previous) = self.peer_heads.get(peer_node_id) {
            if next_head.height < previous.height {
                return;
            }
            if next_head.height == previous.height
                && peer_commit_heads_conflict(previous, &next_head)
            {
                if let Some(validator_id) = validator_id {
                    self.record_commit_equivocation_evidence(
                        validator_id,
                        peer_node_id.to_string(),
                        previous.clone(),
                        next_head,
                        now_ms,
                    );
                    self.peer_heads.remove(peer_node_id);
                }
                return;
            }
            if next_head.height == previous.height && next_head.block_hash == previous.block_hash {
                if next_head.execution_block_hash.is_none() {
                    next_head.execution_block_hash = previous.execution_block_hash.clone();
                }
                if next_head.execution_state_root.is_none() {
                    next_head.execution_state_root = previous.execution_state_root.clone();
                }
                if next_head.action_root.is_empty() {
                    next_head.action_root = previous.action_root.clone();
                }
            }
        }
        self.network_committed_height = self.network_committed_height.max(next_head.height);
        self.peer_heads.insert(peer_node_id.to_string(), next_head);
    }

    fn record_commit_equivocation_evidence(
        &mut self,
        validator_id: String,
        node_id: String,
        first: PeerCommittedHead,
        second: PeerCommittedHead,
        observed_at_ms: i64,
    ) {
        let key = format!("commit_equivocation:{validator_id}:{}", first.height);
        self.quarantined_validators.insert(validator_id.clone());
        if self
            .latest_validated_peer_commit
            .as_ref()
            .is_some_and(|commit| {
                commit.node_id == node_id
                    || self
                        .validator_id_for_peer_head(commit.node_id.as_str())
                        .as_deref()
                        == Some(validator_id.as_str())
            })
        {
            self.latest_validated_peer_commit = None;
        }
        self.misbehavior_evidence
            .entry(key)
            .or_insert_with(|| ConsensusMisbehaviorEvidence {
                kind: "commit_equivocation".to_string(),
                validator_id: validator_id.clone(),
                node_id,
                height: first.height,
                observed_at_ms,
                first_block_hash: first.block_hash,
                second_block_hash: second.block_hash,
                first_execution_block_hash: first.execution_block_hash,
                second_execution_block_hash: second.execution_block_hash,
                first_execution_state_root: first.execution_state_root,
                second_execution_state_root: second.execution_state_root,
                first_action_root: first.action_root,
                second_action_root: second.action_root,
                first_public_key_hex: first.public_key_hex,
                second_public_key_hex: second.public_key_hex,
                first_signature_hex: first.signature_hex,
                second_signature_hex: second.signature_hex,
                slashable_stake: self
                    .validators
                    .get(validator_id.as_str())
                    .copied()
                    .unwrap_or(0),
                total_stake: self.total_stake,
                validator_stake_root: self.validator_stake_root.clone(),
                quarantined: true,
            });
    }
}
