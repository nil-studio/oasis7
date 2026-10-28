use super::startup::{self, Hook, lock_hook};
use super::{NodeBftStatusV3, SelectedNodeBftConfigV3, error};
use crate::{
    NodeBftBuildContextV3, NodeBftExecutionContextV3, NodeCommittedActionBatch, NodeConfig,
    NodeConsensusAction, NodeError,
};
use oasis7_consensus::tendermint_v3::*;
use oasis7_distfs::protected_bft_v3::{BftSignerCheckpointV3, BftSignerLeaseV3};
#[path = "node_bft_v3_worker_peer.rs"]
pub(super) mod peer;
use std::collections::VecDeque;
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};
pub(super) struct Worker {
    pub config: NodeConfig,
    pub selected: SelectedNodeBftConfigV3,
    pub hook: Hook,
    pub lease: super::private_exchange::NodeBftSharedSignerV3,
    pub peers: peer::PeerRuntime,
    pub checkpoint: BftSignerCheckpointV3,
    pub schedule: BftHeaderHeightScheduleV3,
    pub machine: BftConsensusMachineV3,
    pub effects: VecDeque<BftConsensusActionV3>,
    pub pending: Arc<Mutex<Vec<NodeConsensusAction>>>,
    pub pending_bytes: Arc<AtomicUsize>,
    pub state: Arc<Mutex<crate::node_runtime_core::RuntimeState>>,
    pub last_commit: Instant,
    pub recovery_batches: Arc<Mutex<Vec<NodeCommittedActionBatch>>>,
    pub batches: Arc<(Mutex<Vec<NodeCommittedActionBatch>>, Condvar)>,
}
impl Worker {
    pub(super) fn run(mut self, stop: mpsc::Receiver<()>) -> Result<(), NodeError> {
        self.publish_recovered_completions()?;
        self.publish_status();
        self.publish_peer_view()?;
        loop {
            match stop.try_recv() {
                Ok(()) | Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
                Err(mpsc::TryRecvError::Empty) => {}
            }
            if self.checkpoint.state.history.len() >= 64 {
                return Err(error("wire-3 H prefix capacity exhausted"));
            }
            // Recover every original durable local decision before accepting
            // peer input that could select a different QC or signing slot.
            if let Some(c) = self
                .checkpoint
                .state
                .journal
                .as_ref()
                .and_then(|j| j.pending_commit.clone())
            {
                self.apply_commit(c)?;
                continue;
            }
            if let Some(p) = self
                .checkpoint
                .state
                .journal
                .as_ref()
                .and_then(|j| j.pending_proposal_intent.clone())
            {
                self.sign_proposal(p)?;
            }
            if let Some(v) = self
                .checkpoint
                .state
                .journal
                .as_ref()
                .and_then(|j| j.pending_vote_intent.clone())
            {
                self.sign_vote(v)?;
            }
            while let Some(effect) = self.effects.pop_front() {
                match effect {
                    BftConsensusActionV3::SignVote(v) => self.sign_vote(v)?,
                    BftConsensusActionV3::Commit(c) => self.apply_commit(*c)?,
                    BftConsensusActionV3::RoundAdvanced { .. } => {}
                }
            }
            if self.checkpoint.state.history.len() >= 64 {
                continue;
            }
            self.poll_peers()?;
            if self.checkpoint.state.history.len() >= 64 {
                continue;
            }
            if super::membership::active_key(&self.selected, &self.schedule).is_none() {
                self.publish_status();
                match stop.recv_timeout(Duration::from_millis(10)) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                }
            }
            if !self.peers.is_catching_up() {
                self.propose()?;
            }
            let now = self
                .signer()?
                .protected_time_ms()
                .map_err(|_| error("wire-3 protected clock unavailable"))?;
            let mut next = self.machine.clone();
            let effects = next
                .on_timeout(now, &self.selected.binding.local_validator_id)
                .map_err(error)?;
            if !effects.is_empty() {
                let mut journal = self.journal()?;
                journal
                    .events
                    .push(BftSigningJournalEventV3::Timeout { now_ms: now });
                self.advance(journal)?;
                self.machine = next;
                self.effects.extend(effects);
            }
            self.publish_status();
            match stop.recv_timeout(Duration::from_millis(10)) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }
    fn publish_recovered_completions(&self) -> Result<(), NodeError> {
        super::completions::publish(
            &self.recovery_batches,
            &self.pending,
            &self.pending_bytes,
            &self.batches,
            self.config.max_committed_action_batches,
            self.config.max_committed_action_batch_bytes,
        )
    }
    fn signer(&self) -> Result<std::sync::MutexGuard<'_, Box<dyn BftSignerLeaseV3>>, NodeError> {
        self.lease
            .lock()
            .map_err(|_| error("wire-3 shared signer poisoned"))
    }
    fn publish_peer_view(&self) -> Result<(), NodeError> {
        if let Some(network) = &self.selected.network {
            network
                .execution
                .publish(self.schedule.clone(), self.checkpoint.state.history.clone())
                .map_err(error)?;
        }
        Ok(())
    }
    fn journal(&self) -> Result<BftSigningJournalV3, NodeError> {
        self.checkpoint
            .state
            .journal
            .clone()
            .ok_or_else(|| error("wire-3 journal missing"))
    }
    fn advance(&mut self, journal: BftSigningJournalV3) -> Result<(), NodeError> {
        verify_bft_signing_journal_transition_v3(
            &self.journal()?,
            &journal,
            self.schedule.active_set(),
            &self.schedule,
            self.selected.binding.timeouts,
        )
        .map_err(error)?;
        let mut candidate = self.checkpoint.state.candidate();
        candidate.journal = Some(journal);
        let checkpoint = self
            .signer()?
            .advance(
                self.checkpoint.state.revision,
                &self.checkpoint.state.state_digest,
                candidate,
            )
            .map_err(|_| error("wire-3 journal reservation rejected"))?;
        self.checkpoint = checkpoint;
        Ok(())
    }
    fn propose(&mut self) -> Result<(), NodeError> {
        if self
            .schedule
            .expected_proposer_for_round(self.machine.round())
            .map_err(error)?
            != self.selected.binding.local_validator_id
        {
            return Ok(());
        }
        if self
            .machine
            .proposal_for_round(self.machine.round())
            .is_some()
            || self.last_commit.elapsed() < self.config.tick_interval
        {
            return Ok(());
        }
        let value = if let Some((id, _)) = self.machine.valid() {
            self.journal()?
                .events
                .into_iter()
                .find_map(|event| match event {
                    BftSigningJournalEventV3::Proposal { proposal, .. }
                        if proposal.header.block_id == id =>
                    {
                        Some(proposal.block)
                    }
                    _ => None,
                })
                .ok_or_else(|| error("wire-3 valid value lacks original public proposal"))?
        } else {
            let mut hook = lock_hook(&self.hook)?;
            let expected = NodeBftExecutionContextV3::from_schedule(&self.schedule);
            if hook
                .current_bft_v3_execution_context(&self.schedule)
                .map_err(|_| error("wire-3 current parent unavailable"))?
                != expected
            {
                return Err(error("wire-3 build parent mismatch"));
            }
            let private = match hook
                .pending_bft_v3_private_candidate(&expected)
                .map_err(|_| error("wire-3 original private reservation recovery failed"))?
            {
                Some(original) => original,
                None => {
                    let actions = self
                        .pending
                        .lock()
                        .map_err(|_| error("wire-3 pending queue poisoned"))?
                        .clone();
                    let actions = super::actions::select_prefix(
                        actions,
                        self.config.max_engine_pending_consensus_actions,
                    )?;
                    hook.validate_bft_v3_action_batch(&self.schedule, &actions)
                        .map_err(|_| error("wire-3 queued parent-policy admission rejected"))?;
                    hook.build_bft_v3_candidate(NodeBftBuildContextV3 {
                        execution: expected.clone(),
                        actions,
                    })
                    .map_err(|_| error("wire-3 candidate construction failed"))?
                }
            };
            startup::validate_private(
                &self.config,
                &mut **hook,
                &self.schedule,
                &private,
                &self.selected,
            )?;
            let digest = hook
                .retain_bft_v3_private_candidate(&expected, &private)
                .map_err(|_| error("wire-3 original private candidate retention failed"))?;
            if !canonical_digest(&digest) {
                return Err(error("wire-3 private retention receipt malformed"));
            }
            private.public_value
        };
        let intent = self
            .machine
            .proposal_intent(value, &self.selected.binding.local_validator_id)
            .map_err(error)?;
        self.sign_proposal(intent)
    }
    fn sign_proposal(&mut self, intent: BftProposalIntentV3) -> Result<(), NodeError> {
        if self
            .machine
            .proposal_intent(
                intent.block.clone(),
                &self.selected.binding.local_validator_id,
            )
            .map_err(error)?
            != intent
        {
            return Err(error("wire-3 proposal intent changed"));
        }
        startup::candidate(
            &self.config,
            &self.hook,
            &self.schedule,
            &intent.block,
            &self.selected,
        )?;
        let mut journal = self.journal()?;
        if let Some(pending) = &journal.pending_proposal_intent {
            if pending != &intent {
                return Err(error("different wire-3 proposal already reserved"));
            }
        } else {
            journal.pending_proposal_intent = Some(intent.clone());
            self.advance(journal)?;
        }
        let (proposal, checkpoint) = self
            .signer()?
            .sign_proposal(self.checkpoint.state.revision, &intent)
            .map_err(|_| error("wire-3 protected proposal signing failed"))?;
        let mut unsigned = proposal.header.clone();
        unsigned.signature_hex.clear();
        if unsigned != intent.header
            || proposal.block != intent.block
            || proposal.valid_round_evidence != intent.valid_round_evidence
        {
            return Err(error("wire-3 provider signed a different proposal"));
        }
        self.checkpoint = checkpoint;
        startup::candidate(
            &self.config,
            &self.hook,
            &self.schedule,
            &proposal.block,
            &self.selected,
        )?;
        let now = self
            .journal()?
            .events
            .iter()
            .rev()
            .find_map(|e| match e {
                BftSigningJournalEventV3::Proposal {
                    proposal: p,
                    local: true,
                    now_ms,
                } if **p == proposal => Some(*now_ms),
                _ => None,
            })
            .ok_or_else(|| error("wire-3 signed proposal missing from protected journal"))?;
        self.effects.extend(
            self.machine
                .receive_proposal(proposal.clone(), true, now)
                .map_err(error)?,
        );
        self.broadcast(BftPublicPeerMessageV3::Proposal(Box::new(proposal)));
        Ok(())
    }
    fn sign_vote(&mut self, intent: BftVoteIntentV3) -> Result<(), NodeError> {
        let mismatched_position =
            intent.height != self.schedule.height() || intent.round != self.machine.round();
        if mismatched_position
            && retired_unreserved_vote(
                &intent,
                self.schedule.height(),
                self.machine.round(),
                self.journal()?.pending_vote_intent.as_ref(),
            )
        {
            return Ok(());
        }
        let exact = self
            .machine
            .vote_intent(
                intent.validator_id.clone(),
                intent.phase,
                intent.value.clone(),
                intent.round,
            )
            .map_err(error)?;
        if exact.is_none() {
            return Ok(());
        }
        if exact.as_ref() != Some(&intent) {
            return Err(error("wire-3 vote intent changed"));
        }
        if let oasis7_proto::distributed_bft::v3::BftVoteValueV3::Block { block_id } = &intent.value
        {
            let value = self
                .journal()?
                .events
                .into_iter()
                .rev()
                .find_map(|event| match event {
                    BftSigningJournalEventV3::Proposal { proposal, .. }
                        if &proposal.header.block_id == block_id =>
                    {
                        Some(proposal.block)
                    }
                    _ => None,
                })
                .ok_or_else(|| error("wire-3 vote lacks its original proposal"))?;
            startup::candidate(
                &self.config,
                &self.hook,
                &self.schedule,
                &value,
                &self.selected,
            )?;
        }
        let mut journal = self.journal()?;
        if let Some(pending) = &journal.pending_vote_intent {
            if pending != &intent {
                return Err(error("different wire-3 vote already reserved"));
            }
        } else {
            journal.pending_vote_intent = Some(intent.clone());
            self.advance(journal)?;
        }
        let (vote, checkpoint) = self
            .signer()?
            .sign_vote(self.checkpoint.state.revision, &intent)
            .map_err(|_| error("wire-3 protected vote signing failed"))?;
        if vote.height != intent.height
            || vote.round != intent.round
            || vote.phase != intent.phase
            || vote.value != intent.value
            || vote.validator_id != intent.validator_id
        {
            return Err(error("wire-3 provider signed a different vote"));
        }
        self.checkpoint = checkpoint;
        let now = self
            .journal()?
            .events
            .iter()
            .rev()
            .find_map(|e| match e {
                BftSigningJournalEventV3::Vote {
                    vote: v,
                    local: true,
                    now_ms,
                } if **v == vote => Some(*now_ms),
                _ => None,
            })
            .ok_or_else(|| error("wire-3 signed vote missing from journal"))?;
        self.effects.extend(
            self.machine
                .record_local_vote(vote.clone(), now)
                .map_err(error)?,
        );
        self.broadcast(BftPublicPeerMessageV3::Vote(Box::new(vote)));
        Ok(())
    }
    fn apply_commit(&mut self, certificate: FormalBftCertifiedHeaderV3) -> Result<(), NodeError> {
        let token =
            verify_commit_certificate_token_v3(&self.schedule, &certificate).map_err(error)?;
        let committed = certificate.clone();
        let value = BftHeaderValueV3 {
            header: certificate.header.clone(),
            execution_commitment: certificate.execution_commitment.clone(),
        };
        let private = startup::candidate(
            &self.config,
            &self.hook,
            &self.schedule,
            &value,
            &self.selected,
        )?;
        let actions = oasis7_consensus::formal_bft_payload_v3::decode_private_actions_v3(
            &private.actions_cbor,
        )
        .map_err(|_| error("wire-3 committed private bytes rejected"))?;
        if let Some(network) = &self.selected.network {
            network.execution.invalidate_parent().map_err(error)?;
        }
        if super::membership::active_key(&self.selected, &self.schedule).is_some() {
            let mut journal = self.journal()?;
            if let Some(pending) = &journal.pending_commit {
                if pending != &certificate {
                    return Err(error("different wire-3 commit already reserved"));
                }
            } else {
                journal.pending_commit = Some(certificate.clone());
                self.advance(journal)?;
            }
        }
        let checkpoint = self
            .signer()?
            .append_committed_certificate(
                self.checkpoint.state.revision,
                &self.checkpoint.state.state_digest,
                certificate,
            )
            .map_err(|_| error("wire-3 protected QC append failed"))?;
        self.checkpoint = checkpoint;
        self.selected
            .capacity_exhausted
            .store(self.checkpoint.state.history.len() >= 64, Ordering::Release);
        let result = lock_hook(&self.hook)?
            .commit_bft_v3_certificate(token.clone())
            .map_err(|_| error("wire-3 durable actual commit failed"))?;
        startup::validate_result(&token, &result)?;
        let checkpoint = self
            .signer()?
            .confirm_applied(
                self.checkpoint.state.revision,
                &self.checkpoint.state.state_digest,
                result.height,
                &result.header_id,
                &result.driver_install_digest,
            )
            .map_err(|_| error("wire-3 applied install confirmation failed"))?;
        self.checkpoint = checkpoint;
        if self.checkpoint.state.pending_driver_apply.is_some()
            || self
                .checkpoint
                .state
                .applied_installs
                .last()
                .is_none_or(|r| {
                    r.height != result.height
                        || r.header_id != result.header_id
                        || r.driver_install_digest != result.driver_install_digest
                })
        {
            return Err(error("wire-3 provider omitted exact applied receipt"));
        }
        let now = self
            .signer()?
            .protected_time_ms()
            .map_err(|_| error("wire-3 protected clock unavailable"))?;
        let c = token.bound_header().commitment();
        {
            let mut pending = self
                .pending
                .lock()
                .map_err(|_| error("wire-3 pending queue poisoned"))?;
            pending.retain(|a| !actions.contains(a));
            let bytes = crate::action_payload_bytes(pending.iter());
            self.pending_bytes.store(bytes, Ordering::Release);
        }
        crate::push_committed_action_batch(
            &self.batches,
            NodeCommittedActionBatch {
                height: c.height,
                slot: token.certificate().proposal.round,
                epoch: c.authority_epoch,
                block_hash: result.header_id.clone(),
                action_root: c.action_root.clone(),
                committed_at_unix_ms: now,
                actions,
            },
            self.config.max_committed_action_batches,
            self.config.max_committed_action_batch_bytes,
        );
        self.broadcast(BftPublicPeerMessageV3::Commit(Box::new(committed)));
        self.peers.height_advanced();
        self.schedule = token.next_height_schedule().clone();
        startup::validate_schedule(&self.schedule, &self.selected)?;
        {
            let lease = self.lease.clone();
            let mut signer = lease.lock().map_err(|_| error("wire-3 signer poisoned"))?;
            super::membership::install(
                &self.selected,
                &self.schedule,
                &result,
                &mut **signer,
                &mut self.checkpoint,
            )?;
        }
        self.publish_peer_view()?;
        self.last_commit = Instant::now();
        if self.checkpoint.state.history.len() >= 64 {
            self.publish_status();
            return Ok(());
        }
        if super::membership::active_key(&self.selected, &self.schedule).is_none() {
            let (machine, _) = startup::restore(
                &self.selected,
                &self.schedule,
                &self.checkpoint,
                &self.hook,
                &self.config,
            )?;
            self.machine = machine;
            self.effects.clear();
            self.publish_status();
            return Ok(());
        }
        let checkpoint = self
            .signer()?
            .start_height_timer(self.checkpoint.state.revision)
            .map_err(|_| error("wire-3 successor timer rejected"))?;
        self.checkpoint = checkpoint;
        let (machine, effects) = startup::restore(
            &self.selected,
            &self.schedule,
            &self.checkpoint,
            &self.hook,
            &self.config,
        )?;
        self.machine = machine;
        self.effects = effects.into();
        self.publish_status();
        Ok(())
    }
    fn publish_status(&self) {
        let mut state = crate::runtime_util::lock_state(&self.state);
        state.bft_v3_status = Some(NodeBftStatusV3 {
            running: true,
            height: self.schedule.height(),
            round: self.machine.round(),
            committed_height: self.schedule.height().saturating_sub(1),
            durable_event_count: self
                .checkpoint
                .state
                .journal
                .as_ref()
                .map_or(0, |j| j.events.len()),
            blocked_reason: None,
        });
        state.tick_count = state.tick_count.saturating_add(1);
        state.last_tick_unix_ms = Some(crate::runtime_util::now_unix_ms());
        let s = &mut state.consensus;
        s.mode = crate::NodeConsensusMode::TendermintV3;
        s.latest_height = self.schedule.height().saturating_sub(1);
        s.committed_height = s.latest_height;
        s.network_committed_height = s.latest_height;
        s.replication_persisted_height = s.latest_height;
        s.validator_set_hash = self.schedule.active_set().set_hash().into();
        if let Some(last) = self.checkpoint.state.history.last() {
            s.last_block_hash = last.header.block_id().ok();
            s.last_execution_height = last.execution_commitment.height;
            s.last_execution_block_hash = Some(last.header.execution_commitment_hash.clone());
            s.last_execution_state_root =
                Some(last.execution_commitment.execution_state_root.clone());
        }
    }
}
fn canonical_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.peers.cancel();
        if let Some(network) = &self.selected.network {
            network.execution.stop_serving();
        }
    }
}

// Volatile effects can be overtaken by a later event in one inbound batch.
// The protected original intent is never retired; replay/recovery must finish
// it or fail-stop. This predicate is deliberately specific to SignVote, so an
// older-round independently verified Commit remains admissible.
fn retired_unreserved_vote(
    intent: &BftVoteIntentV3,
    height: u64,
    round: u64,
    reserved: Option<&BftVoteIntentV3>,
) -> bool {
    (intent.height != height || intent.round != round) && reserved != Some(intent)
}
