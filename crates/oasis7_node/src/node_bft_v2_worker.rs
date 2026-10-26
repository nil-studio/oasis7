use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use oasis7_consensus::tendermint_v2::{
    BFT_MAX_ROUNDS_PER_HEIGHT_V2, BftConsensusActionV2, BftConsensusMachineV2,
    BftHeightProposerScheduleV2, BftSigningJournalEventV2, BftStepV2, BftValidatorSetV2,
    VerifiedBftCommitCertificateV2, replay_bft_signing_journal_v2,
    verify_bft_signing_journal_transition_v2, verify_commit_certificate_token_v2,
};
use oasis7_distfs::protected_bft_v2::{BftSignerCheckpointV2, BftSignerLeaseV2};
use oasis7_proto::distributed_bft::v2::{
    BFT_MAX_VALIDATORS_V2, BftCommitCertificateV2, BftHistoryRequestV2, BftMessageV2, BftVoteV2,
};

use super::SelectedNodeBftConfigV2;
use super::startup::{self, Hook, error, lock_hook};
use crate::gossip_udp::{GossipEndpoint, GossipMessage};
use crate::node_runtime_core::RuntimeState;
use crate::runtime_util::{lock_state, now_unix_ms};
use crate::{NodeConfig, NodeConsensusAction, NodeConsensusMode, NodeError, NodeRuntime};

#[path = "node_bft_v2_worker_commit.rs"]
mod commit;
#[path = "node_bft_v2_worker_events.rs"]
mod events;
#[path = "node_bft_v2_worker_signing.rs"]
mod signing;

const MAX_SEEN_MESSAGES_V2: usize = (BFT_MAX_ROUNDS_PER_HEIGHT_V2 as usize + 1)
    .saturating_mul(BFT_MAX_VALIDATORS_V2.saturating_mul(2).saturating_add(1));

pub(super) struct WorkerSetup {
    pub config: NodeConfig,
    pub selected: SelectedNodeBftConfigV2,
    pub set: BftValidatorSetV2,
    pub schedule: BftHeightProposerScheduleV2,
    pub machine: BftConsensusMachineV2,
    pub lease: Box<dyn BftSignerLeaseV2>,
    pub checkpoint: BftSignerCheckpointV2,
    pub local_key: Option<String>,
    pub hook: Hook,
    pub pending: Arc<Mutex<Vec<NodeConsensusAction>>>,
    pub pending_bytes: Arc<AtomicUsize>,
    pub state: Arc<Mutex<RuntimeState>>,
    pub tip: Option<VerifiedBftCommitCertificateV2>,
    pub history_bytes: usize,
    pub history: Vec<BftCommitCertificateV2>,
}

pub(super) struct Worker {
    setup: WorkerSetup,
    effects: VecDeque<BftConsensusActionV2>,
    seen: HashSet<[u8; 32]>,
    clock_base: i64,
    last_broadcast: Option<Instant>,
    last_request: Option<Instant>,
    last_response: Option<Instant>,
    last_commit: Option<Instant>,
}

impl Worker {
    pub(super) fn restore(mut setup: WorkerSetup) -> Result<Self, NodeError> {
        if setup.machine.schedule() != &setup.schedule
            || setup.machine.height() != setup.schedule.height()
        {
            return Err(error(
                "BFT V2 startup machine differs from verified schedule",
            ));
        }
        let protected_now = setup
            .lease
            .protected_time_ms()
            .map_err(|err| error(format!("read protected BFT V2 time: {err}")))?;
        let mut this = Self {
            setup,
            effects: VecDeque::new(),
            seen: HashSet::new(),
            clock_base: protected_now,
            last_broadcast: None,
            last_request: None,
            last_response: None,
            last_commit: None,
        };
        this.index_journal_events()?;
        this.publish_status(None);
        Ok(this)
    }

    pub(super) fn run(
        mut self,
        stop: mpsc::Receiver<()>,
        gossip: Option<Arc<GossipEndpoint>>,
    ) -> Result<(), NodeError> {
        if let Some(certificate) = self
            .setup
            .checkpoint
            .state
            .journal
            .as_ref()
            .and_then(|journal| journal.pending_commit().cloned())
        {
            self.effects.clear();
            self.effects
                .push_back(BftConsensusActionV2::Commit(certificate));
        }
        self.resume_pending_signatures(gossip.as_deref())?;
        self.finish_effects(gossip.as_deref())?;

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
                    if let GossipMessage::BftV2 { message } = message.message {
                        self.receive(message, endpoint)?;
                        self.finish_effects(Some(endpoint))?;
                    }
                }
            }
            let now = self.protected_now()?;
            if self.local_signer_enabled()
                && self
                    .setup
                    .machine
                    .step_deadline_ms()
                    .is_some_and(|deadline| now >= deadline)
            {
                self.on_timeout(now)?;
                self.finish_effects(gossip.as_deref())?;
            }
            self.maybe_propose(gossip.as_deref())?;
            self.finish_effects(gossip.as_deref())?;
            self.retransmit(gossip.as_deref())?;
            self.publish_status(None);
        }
    }

    fn protected_now(&mut self) -> Result<i64, NodeError> {
        let now = self
            .setup
            .lease
            .protected_time_ms()
            .map_err(|err| error(format!("read protected BFT V2 time: {err}")))?;
        if now < self.clock_base {
            return Err(error("protected BFT V2 time moved backwards"));
        }
        self.clock_base = now;
        Ok(now)
    }

    fn local_signer_enabled(&self) -> bool {
        self.setup.local_key.is_some()
            && self
                .setup
                .checkpoint
                .state
                .journal
                .as_ref()
                .is_some_and(|journal| {
                    journal.local_validator_id() == self.setup.selected.binding.local_validator_id
                        && journal.height() == self.setup.schedule.height()
                })
    }

    fn journal_event_count(&self) -> usize {
        self.setup
            .checkpoint
            .state
            .journal
            .as_ref()
            .map_or(0, |journal| journal.events().len())
    }

    pub(super) fn publish_status(&self, committed: Option<&VerifiedBftCommitCertificateV2>) {
        let mut state = lock_state(&self.setup.state);
        let status = state.bft_v2_status.get_or_insert_with(Default::default);
        status.running = true;
        status.height = self.setup.machine.height();
        status.round = self.setup.machine.round();
        status.step = format!("{:?}", self.setup.machine.step());
        status.committed_height = self.setup.schedule.height().saturating_sub(1);
        status.active_validator_set_hash = self.setup.set.set_hash().to_string();
        status.pending_transition_height = self
            .setup
            .schedule
            .pending_transition()
            .map(|transition| transition.activation_height);
        status.local_validator_active = self.local_signer_enabled();
        status.durable_event_count = self.journal_event_count();
        status.proposal_equivocation_evidence = self
            .setup
            .machine
            .proposal_equivocation_evidence()
            .map(|(first, second)| (first.clone(), second.clone()))
            .collect();
        state.tick_count = state.tick_count.saturating_add(1);
        state.last_tick_unix_ms = Some(now_unix_ms());

        let snapshot = &mut state.consensus;
        snapshot.mode = NodeConsensusMode::TendermintV2;
        snapshot.slot = self.setup.machine.round();
        snapshot.epoch = self.setup.schedule.authority_epoch();
        snapshot.latest_height = self.setup.schedule.height().saturating_sub(1);
        snapshot.committed_height = snapshot.latest_height;
        snapshot.network_committed_height = snapshot.committed_height;
        snapshot.replication_persisted_height = snapshot.committed_height;
        snapshot.validator_set_hash = self.setup.set.set_hash().to_string();
        snapshot.required_stake = self.setup.set.quorum_voting_power();
        snapshot.total_stake = self.setup.set.total_voting_power();
        snapshot.validator_stakes = self
            .setup
            .set
            .validators()
            .values()
            .map(|validator| (validator.validator_id.clone(), validator.voting_power))
            .collect();
        snapshot.last_status = None;
        snapshot.validator_stake_root.clear();
        snapshot.validator_stake_proofs.clear();
        snapshot.pending_proposal = None;
        if let Some(token) = committed.or(self.setup.tip.as_ref()) {
            let block = token.block();
            snapshot.last_block_hash = Some(token.block_id().to_string());
            snapshot.last_execution_height = block.height;
            snapshot.last_execution_block_hash = Some(block.execution_block_hash.clone());
            snapshot.last_execution_state_root = Some(block.execution_state_root.clone());
        }
    }

    fn reject_inbound(&self, reason: impl std::fmt::Display) {
        let mut state = lock_state(&self.setup.state);
        let status = state.bft_v2_status.get_or_insert_with(Default::default);
        status.rejected_inbound_messages = status.rejected_inbound_messages.saturating_add(1);
        status.last_inbound_rejection = Some(reason.to_string());
    }

    fn set_transport_error(&self, reason: impl std::fmt::Display) {
        lock_state(&self.setup.state)
            .bft_v2_status
            .get_or_insert_with(Default::default)
            .last_transport_error = Some(reason.to_string());
    }
}
