use super::*;
use crate::gossip_udp::GossipCheckpointLineageVoteMessage;

pub(crate) struct ConsensusNetworkEndpoint {
    network: Arc<dyn DistributedNetwork<WorldError> + Send + Sync>,
    network_policy: NodeNetworkPolicy,
    proposal_topic: String,
    attestation_topic: String,
    commit_topic: String,
    lineage_topic: String,
    proposal_subscription: Option<NetworkSubscription>,
    attestation_subscription: Option<NetworkSubscription>,
    commit_subscription: Option<NetworkSubscription>,
    lineage_subscription: Option<NetworkSubscription>,
}

impl ConsensusNetworkEndpoint {
    pub(crate) fn new(
        handle: &NodeReplicationNetworkHandle,
        world_id: &str,
        subscribe: bool,
        network_policy: &NodeNetworkPolicy,
    ) -> Result<Self, NodeError> {
        let registry = handle.resolved_lane_registry(world_id);
        let proposal_topic = registry.consensus_proposal_topic;
        let attestation_topic = registry.consensus_attestation_topic;
        let commit_topic = registry.consensus_commit_topic;
        let lineage_topic = default_consensus_lineage_topic(world_id);
        let subscribe_topic = |topic: &str| -> Result<Option<NetworkSubscription>, NodeError> {
            if !subscribe {
                return Ok(None);
            }
            Ok(Some(handle.network.subscribe(topic).map_err(network_err)?))
        };
        if subscribe {
            validate_lane_access(
                network_policy,
                NetworkLane::ConsensusGossip,
                NetworkLaneOperation::Subscribe,
                proposal_topic.as_str(),
            )?;
        }
        Ok(Self {
            network: Arc::clone(&handle.network),
            network_policy: network_policy.clone(),
            proposal_topic: proposal_topic.clone(),
            attestation_topic: attestation_topic.clone(),
            commit_topic: commit_topic.clone(),
            proposal_subscription: subscribe_topic(proposal_topic.as_str())?,
            attestation_subscription: subscribe_topic(attestation_topic.as_str())?,
            commit_subscription: subscribe_topic(commit_topic.as_str())?,
            lineage_topic: lineage_topic.clone(),
            lineage_subscription: subscribe_topic(lineage_topic.as_str())?,
        })
    }
    pub(crate) fn publish_proposal(
        &self,
        message: &GossipProposalMessage,
    ) -> Result<(), NodeError> {
        self.publish_json(self.proposal_topic.as_str(), message)
    }
    pub(crate) fn publish_attestation(
        &self,
        message: &GossipAttestationMessage,
    ) -> Result<(), NodeError> {
        self.publish_json(self.attestation_topic.as_str(), message)
    }
    pub(crate) fn publish_commit(&self, message: &GossipCommitMessage) -> Result<(), NodeError> {
        self.publish_json(self.commit_topic.as_str(), message)
    }
    pub(crate) fn publish_checkpoint_lineage_vote(
        &self,
        message: &GossipCheckpointLineageVoteMessage,
    ) -> Result<(), NodeError> {
        self.publish_json(
            self.lineage_topic.as_str(),
            &GossipMessage::CheckpointLineageVote(message.clone()),
        )
    }
    pub(crate) fn drain_messages(&self) -> Result<Vec<GossipMessage>, NodeError> {
        let mut out = Vec::new();
        Self::drain_subscription(self.proposal_subscription.as_ref(), &mut out);
        Self::drain_subscription(self.attestation_subscription.as_ref(), &mut out);
        Self::drain_subscription(self.commit_subscription.as_ref(), &mut out);
        Self::drain_subscription(self.lineage_subscription.as_ref(), &mut out);
        Ok(out)
    }
    pub(crate) fn allows_publish(&self) -> bool {
        self.network_policy
            .allows_lane_operation(NetworkLane::ConsensusGossip, NetworkLaneOperation::Publish)
    }
    fn publish_json<T: Serialize>(&self, topic: &str, message: &T) -> Result<(), NodeError> {
        validate_lane_access(
            &self.network_policy,
            NetworkLane::ConsensusGossip,
            NetworkLaneOperation::Publish,
            topic,
        )?;
        let payload = serde_json::to_vec(message).map_err(|err| NodeError::Replication {
            reason: format!("serialize consensus network message failed: {}", err),
        })?;
        self.network
            .publish_best_effort(topic, payload.as_slice())
            .map_err(network_err)
    }
    fn drain_subscription(
        subscription: Option<&NetworkSubscription>,
        out: &mut Vec<GossipMessage>,
    ) {
        let Some(subscription) = subscription else {
            return;
        };
        for payload in subscription.drain() {
            if let Some(message) = decode_consensus_message(payload.as_slice()) {
                out.push(message);
            }
        }
    }
}

fn decode_consensus_message(payload: &[u8]) -> Option<GossipMessage> {
    if let Ok(message) = serde_json::from_slice::<GossipMessage>(payload) {
        match message {
            GossipMessage::Proposal(_)
            | GossipMessage::Attestation(_)
            | GossipMessage::Commit(_)
            | GossipMessage::CheckpointLineageVote(_) => return Some(message),
            GossipMessage::Hello(_)
            | GossipMessage::Replication(_)
            | GossipMessage::Bft { .. }
            | GossipMessage::BftV2 { .. } => {}
        }
    }
    if let Ok(message) = serde_json::from_slice::<GossipProposalMessage>(payload) {
        return Some(GossipMessage::Proposal(message));
    }
    if let Ok(message) = serde_json::from_slice::<GossipAttestationMessage>(payload) {
        return Some(GossipMessage::Attestation(message));
    }
    if let Ok(message) = serde_json::from_slice::<GossipCommitMessage>(payload) {
        return Some(GossipMessage::Commit(message));
    }
    None
}
