use std::collections::{BTreeSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use oasis7_consensus::tendermint_v1::{
    BftConsensusActionV1, BftConsensusMachineV1, BftHeightProposerScheduleV1, BftStepV1,
    BftValidatorSetV1, VerifiedBftCommitCertificateV1, verify_commit_certificate_token_v1,
};
use oasis7_proto::distributed_bft::{
    BftCommitCertificateV1, BftHistoryRequestV1, BftMessageV1, BftProposalV1,
};

use super::NodeBftConfigV1;
use super::startup::{Hook, error, lock_hook};
use crate::gossip_udp::{GossipEndpoint, GossipMessage};
use crate::node_bft_wal::{BftWalEventV1, BftWalStore};
use crate::node_runtime_core::RuntimeState;
use crate::runtime_util::{lock_state, now_unix_ms};
use crate::{NodeConfig, NodeConsensusAction, NodeConsensusMode, NodeError, NodeRuntime};

#[path = "node_bft_worker_commit.rs"]
mod commit;
#[path = "node_bft_worker_events.rs"]
mod events;
#[path = "node_bft_worker_signing.rs"]
mod signing;
#[path = "node_bft_transport_capacity.rs"]
mod transport;

pub(super) struct WorkerSetup {
    pub config: NodeConfig,
    pub bft: NodeBftConfigV1,
    pub set: BftValidatorSetV1,
    pub schedule: BftHeightProposerScheduleV1,
    pub key: Option<SigningKey>,
    pub hook: Hook,
    pub wal: BftWalStore,
    pub pending: Arc<Mutex<Vec<NodeConsensusAction>>>,
    pub pending_bytes: Arc<AtomicUsize>,
    pub state: Arc<Mutex<RuntimeState>>,
    pub tip: Option<VerifiedBftCommitCertificateV1>,
}

pub(super) struct Worker {
    setup: WorkerSetup,
    machine: BftConsensusMachineV1,
    effects: VecDeque<BftConsensusActionV1>,
    seen: BTreeSet<Vec<u8>>,
    clock_base: i64,
    clock_origin: Instant,
    last_broadcast: Option<Instant>,
    last_request: Option<Instant>,
    last_response: Option<Instant>,
    last_commit: Option<Instant>,
}

impl Worker {
    pub(super) fn validate_configuration(
        set: &BftValidatorSetV1,
        epoch: u64,
    ) -> Result<(), NodeError> {
        transport::validate_configuration(set, epoch)
    }
    pub(super) fn restore(mut setup: WorkerSetup) -> Result<Self, NodeError> {
        let now = setup.wal.protected_time_ms()?.unwrap_or_else(now_unix_ms);
        let origin = setup.wal.ensure_height_started_at(now)?;
        let machine = BftConsensusMachineV1::new(
            setup.set.clone(),
            setup.bft.local_validator_id.clone(),
            setup.schedule.height(),
            Some(setup.schedule.starting_priority().clone()),
            origin,
            setup.bft.timeouts,
        )
        .map_err(error)?;
        let mut this = Self {
            setup,
            machine,
            effects: VecDeque::new(),
            seen: BTreeSet::new(),
            clock_base: origin,
            clock_origin: Instant::now(),
            last_broadcast: None,
            last_request: None,
            last_response: None,
            last_commit: None,
        };
        this.replay()?;
        if this.setup.key.is_some() {
            this.clock_base = this.clock_base.max(now_unix_ms());
        }
        this.clock_origin = Instant::now();
        this.publish_status(None);
        Ok(this)
    }

    pub(super) fn run(
        mut self,
        stop: mpsc::Receiver<()>,
        gossip: Option<Arc<GossipEndpoint>>,
    ) -> Result<(), NodeError> {
        // An original pending commit takes priority over any further signature.
        if let Some(qc) = self.setup.wal.state().pending_commit.clone() {
            self.effects.clear();
            self.effects.push_back(BftConsensusActionV1::Commit(qc));
        }
        self.finish_effects(gossip.as_deref())?;
        self.resume_pending_signatures(gossip.as_deref())?;
        let interval = self
            .setup
            .config
            .tick_interval
            .clamp(Duration::from_millis(5), Duration::from_millis(100));
        loop {
            match stop.recv_timeout(interval) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            if let Some(endpoint) = gossip.as_deref() {
                for message in endpoint.drain_messages_limited(64)? {
                    if let GossipMessage::Bft { message } = message.message {
                        self.receive(message, endpoint)?;
                        self.finish_effects(Some(endpoint))?;
                    }
                }
            }
            let now = self.now()?;
            if self
                .machine
                .step_deadline_ms()
                .is_some_and(|deadline| now >= deadline)
            {
                let mut next = self.machine.clone();
                let effects = next
                    .on_timeout(now, &self.setup.bft.local_validator_id)
                    .map_err(error)?;
                self.setup
                    .wal
                    .append_event(BftWalEventV1::Timeout { now_ms: now })?;
                self.machine = next;
                self.effects.extend(effects);
                self.finish_effects(gossip.as_deref())?;
            }
            self.maybe_propose(gossip.as_deref())?;
            self.finish_effects(gossip.as_deref())?;
            self.retransmit(gossip.as_deref())?;
            self.publish_status(None);
        }
    }

    fn now(&mut self) -> Result<i64, NodeError> {
        if let Some(now) = self.setup.wal.protected_time_ms()? {
            return Ok(now);
        }
        let elapsed = i64::try_from(self.clock_origin.elapsed().as_millis())
            .map_err(|_| error("BFT elapsed clock overflow"))?;
        self.clock_base
            .checked_add(elapsed)
            .ok_or_else(|| error("BFT logical clock overflow"))
    }

    fn publish_status(&self, committed: Option<&VerifiedBftCommitCertificateV1>) {
        let mut state = lock_state(&self.setup.state);
        let status = state.bft_status.get_or_insert_with(Default::default);
        status.height = self.machine.height();
        status.round = self.machine.round();
        status.step = format!("{:?}", self.machine.step());
        status.committed_height = self.machine.height().saturating_sub(1);
        status.validator_set_hash = self.setup.set.set_hash().into();
        status.locked_block_id = self.machine.locked().map(|(id, _)| id.to_string());
        status.locked_round = self.machine.locked().map(|(_, round)| round);
        status.valid_block_id = self.machine.valid().map(|(id, _)| id.to_string());
        status.valid_round = self.machine.valid().map(|(_, round)| round);
        status.durable_event_count = self.setup.wal.state().events.len();
        state.tick_count = state.tick_count.saturating_add(1);
        state.last_tick_unix_ms = Some(now_unix_ms());
        let snapshot = &mut state.consensus;
        snapshot.mode = NodeConsensusMode::TendermintV1;
        // In this explicit mode the legacy slot projection is the BFT round;
        // no PosConsensusStatus or legacy committed batch is manufactured.
        snapshot.slot = self.machine.round();
        snapshot.epoch = self.setup.bft.authority_epoch;
        snapshot.latest_height = self.machine.height().saturating_sub(1);
        snapshot.committed_height = snapshot.latest_height;
        snapshot.network_committed_height = snapshot.committed_height;
        snapshot.replication_persisted_height = snapshot.committed_height;
        snapshot.validator_set_hash = self.setup.set.set_hash().into();
        snapshot.required_stake = self.setup.set.quorum_voting_power();
        snapshot.total_stake = self.setup.set.total_voting_power();
        snapshot.validator_stakes = self
            .setup
            .set
            .validators()
            .values()
            .map(|v| (v.validator_id.clone(), v.voting_power))
            .collect();
        snapshot.last_status = None;
        snapshot.validator_stake_root.clear();
        snapshot.validator_stake_proofs.clear();
        snapshot.pending_proposal = None;
        if let Some(token) = committed.or(self.setup.tip.as_ref()) {
            let block = token.block();
            snapshot.last_block_hash = Some(token.block_id().into());
            snapshot.last_execution_height = block.height;
            snapshot.last_execution_block_hash = Some(block.execution_block_hash.clone());
            snapshot.last_execution_state_root = Some(block.execution_state_root.clone());
        }
    }

    fn reject_inbound(&self, reason: impl std::fmt::Display) {
        let mut state = lock_state(&self.setup.state);
        let status = state.bft_status.get_or_insert_with(Default::default);
        status.rejected_inbound_messages = status.rejected_inbound_messages.saturating_add(1);
        status.last_inbound_rejection = Some(reason.to_string());
    }
}

pub(super) fn validate_execution_result(
    token: &VerifiedBftCommitCertificateV1,
    result: &crate::NodeBftExecutionCommitResultV1,
) -> Result<(), NodeError> {
    let block = token.block();
    if result.height != block.height
        || result.bft_commit_ref != token.block_id()
        || result.execution_block_hash != block.execution_block_hash
        || result.execution_state_root != block.execution_state_root
        || result.state_root != block.state_root
    {
        return Err(error(
            "BFT execution hook returned a result outside the verified certificate",
        ));
    }
    Ok(())
}
