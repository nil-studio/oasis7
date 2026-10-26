use super::events::message_digest;
use super::*;
use oasis7_consensus::tendermint_v2::{BftProposalIntentV2, BftVoteIntentV2};
use oasis7_proto::distributed_bft::v2::BftProposalV2;

impl Worker {
    pub(super) fn finish_effects(
        &mut self,
        gossip: Option<&GossipEndpoint>,
    ) -> Result<(), NodeError> {
        while let Some(effect) = self.effects.pop_front() {
            match effect {
                BftConsensusActionV2::SignVote(intent) => {
                    self.sign_vote(intent, gossip)?;
                }
                BftConsensusActionV2::Commit(certificate) => {
                    if certificate.proposal.block.height != self.setup.schedule.height() {
                        continue;
                    }
                    let token =
                        verify_commit_certificate_token_v2(&self.setup.schedule, &certificate)
                            .map_err(error)?;
                    self.apply_commit(token, gossip)?;
                }
                BftConsensusActionV2::RoundAdvanced { .. } => {}
            }
        }
        Ok(())
    }

    pub(super) fn resume_pending_signatures(
        &mut self,
        gossip: Option<&GossipEndpoint>,
    ) -> Result<(), NodeError> {
        let pending_proposal = self
            .setup
            .checkpoint
            .state
            .journal
            .as_ref()
            .and_then(|journal| journal.pending_proposal_intent().cloned());
        if let Some(intent) = pending_proposal {
            self.sign_proposal(intent, gossip)?;
            self.finish_effects(gossip)?;
        }
        let pending_vote = self
            .setup
            .checkpoint
            .state
            .journal
            .as_ref()
            .and_then(|journal| journal.pending_vote_intent().cloned());
        if let Some(intent) = pending_vote {
            self.sign_vote(intent, gossip)?;
            self.finish_effects(gossip)?;
        }
        Ok(())
    }

    pub(super) fn maybe_propose(
        &mut self,
        gossip: Option<&GossipEndpoint>,
    ) -> Result<(), NodeError> {
        if !self.local_signer_enabled()
            || self
                .last_commit
                .is_some_and(|last| last.elapsed() < self.setup.config.tick_interval)
            || self.setup.machine.step() != BftStepV2::Propose
            || self.setup.machine.current_proposer_id()
                != Some(self.setup.selected.binding.local_validator_id.as_str())
        {
            return Ok(());
        }
        let round = self.setup.machine.round();
        if self
            .setup
            .checkpoint
            .state
            .journal
            .as_ref()
            .is_some_and(|journal| {
                journal.events().iter().any(|event| {
                    matches!(event,
                        BftSigningJournalEventV2::Proposal {
                            proposal,
                            local: true,
                            ..
                        } if proposal.header.round == round)
                })
            })
        {
            return Ok(());
        }

        let block = if let Some((block_id, _)) = self.setup.machine.valid() {
            self.setup
                .checkpoint
                .state
                .journal
                .as_ref()
                .and_then(|journal| {
                    journal.events().iter().rev().find_map(|event| match event {
                        BftSigningJournalEventV2::Proposal { proposal, .. }
                            if proposal.header.block_id == block_id =>
                        {
                            Some(proposal.block.clone())
                        }
                        _ => None,
                    })
                })
                .ok_or_else(|| error("BFT V2 valid value lacks its durable proposal block"))?
        } else {
            self.build_candidate()?
        };
        let intent = self
            .setup
            .machine
            .proposal_intent(
                block,
                self.setup.selected.binding.local_validator_id.as_str(),
            )
            .map_err(error)?;
        self.sign_proposal(intent, gossip)
    }

    fn build_candidate(
        &mut self,
    ) -> Result<oasis7_proto::distributed_bft::v2::BftBlockV2, NodeError> {
        let hook_handle = Arc::clone(&self.setup.hook);
        let mut execution_hook = lock_hook(&hook_handle)?;
        let (actions, _) = {
            let mut pending = self
                .setup
                .pending
                .lock()
                .map_err(|_| error("BFT V2 pending action queue lock poisoned"))?;
            NodeRuntime::canonicalize_bft_v2_local_actions(
                &mut pending,
                &self.setup.pending_bytes,
                &mut **execution_hook,
                &self.setup.schedule,
            )?
        };
        let actions = actions
            .into_iter()
            .take(self.setup.config.max_engine_pending_consensus_actions)
            .collect::<Vec<_>>();
        execution_hook
            .validate_bft_v2_action_batch(&self.setup.schedule, &actions)
            .map_err(error)?;
        let expected = execution_hook
            .current_bft_v2_execution_context(&self.setup.schedule)
            .map_err(error)?;
        if expected != crate::NodeBftExecutionContextV2::from_schedule(&self.setup.schedule) {
            return Err(error(
                "BFT V2 local execution context differs from its verified schedule",
            ));
        }
        let block = execution_hook
            .build_bft_v2_candidate(crate::NodeBftBuildContextV2 {
                execution: expected,
                actions,
            })
            .map_err(error)?;
        drop(execution_hook);
        startup::validate_candidate_with_hook(
            &self.setup.config,
            &self.setup.hook,
            &self.setup.schedule,
            &block,
        )?;
        Ok(block)
    }

    fn sign_proposal(
        &mut self,
        intent: BftProposalIntentV2,
        gossip: Option<&GossipEndpoint>,
    ) -> Result<(), NodeError> {
        if !self.local_signer_enabled() {
            return Err(error("BFT V2 proposal signer is not active and installed"));
        }
        if self
            .setup
            .machine
            .proposal_intent(
                intent.block.clone(),
                self.setup.selected.binding.local_validator_id.as_str(),
            )
            .map_err(error)?
            != intent
        {
            return Err(error(
                "BFT V2 proposal intent changed before protected signing",
            ));
        }
        startup::validate_candidate_with_hook(
            &self.setup.config,
            &self.setup.hook,
            &self.setup.schedule,
            &intent.block,
        )?;
        self.reserve_proposal(&intent)?;
        let (proposal, checkpoint) = self
            .setup
            .lease
            .sign_proposal(self.setup.checkpoint.state.revision, &intent)
            .map_err(|reason| error(format!("protected BFT V2 proposal signing: {reason}")))?;
        self.setup.checkpoint = checkpoint;
        let now = self
            .signed_proposal_time(&proposal)
            .ok_or_else(|| error("protected signer omitted the exact local proposal event"))?;
        let mut next = self.setup.machine.clone();
        let effects = next
            .receive_proposal(proposal.clone(), true, now)
            .map_err(error)?;
        self.setup.machine = next;
        self.seen
            .insert(message_digest(&BftMessageV2::Proposal(Box::new(
                proposal.clone(),
            )))?);
        if let Some(endpoint) = gossip {
            self.broadcast(endpoint, &BftMessageV2::Proposal(Box::new(proposal)));
        }
        self.effects.extend(effects);
        Ok(())
    }

    fn sign_vote(
        &mut self,
        intent: BftVoteIntentV2,
        gossip: Option<&GossipEndpoint>,
    ) -> Result<(), NodeError> {
        if intent.height != self.setup.machine.height()
            || intent.round != self.setup.machine.round()
        {
            return Ok(());
        }
        if !self.local_signer_enabled() {
            return Ok(());
        }
        let exact = self
            .setup
            .machine
            .vote_intent(
                intent.validator_id.clone(),
                intent.phase,
                intent.value.clone(),
                intent.round,
            )
            .map_err(error)?;
        if exact.as_ref() != Some(&intent) {
            return Err(error("BFT V2 vote intent changed before protected signing"));
        }
        self.reserve_vote(&intent)?;
        let (vote, checkpoint) = self
            .setup
            .lease
            .sign_vote(self.setup.checkpoint.state.revision, &intent)
            .map_err(|reason| error(format!("protected BFT V2 vote signing: {reason}")))?;
        self.setup.checkpoint = checkpoint;
        let now = self
            .signed_vote_time(&vote)
            .ok_or_else(|| error("protected signer omitted the exact local vote event"))?;
        let mut next = self.setup.machine.clone();
        let effects = next.record_local_vote(vote.clone(), now).map_err(error)?;
        self.setup.machine = next;
        self.seen
            .insert(message_digest(&BftMessageV2::Vote(vote.clone()))?);
        if let Some(endpoint) = gossip {
            self.broadcast(endpoint, &BftMessageV2::Vote(vote));
        }
        self.effects.extend(effects);
        Ok(())
    }

    fn reserve_proposal(&mut self, intent: &BftProposalIntentV2) -> Result<(), NodeError> {
        let previous = self
            .setup
            .checkpoint
            .state
            .journal
            .as_ref()
            .ok_or_else(|| error("active BFT V2 signer journal is missing"))?;
        if previous.pending_proposal_intent().is_some() {
            if previous.pending_proposal_intent() == Some(intent) {
                return Ok(());
            }
            return Err(error("another protected BFT V2 proposal is pending"));
        }
        if previous.pending_vote_intent().is_some() || previous.pending_commit().is_some() {
            return Err(error("another protected BFT V2 decision is pending"));
        }
        let mut next = previous.clone();
        next.pending_proposal_intent = Some(intent.clone());
        verify_bft_signing_journal_transition_v2(
            previous,
            &next,
            &self.setup.set,
            &self.setup.schedule,
            self.setup.selected.binding.timeouts,
        )
        .map_err(error)?;
        let mut candidate = self.setup.checkpoint.state.candidate();
        candidate.journal = Some(next);
        self.setup.checkpoint = self
            .setup
            .lease
            .advance(
                self.setup.checkpoint.state.revision,
                &self.setup.checkpoint.state.state_digest,
                candidate,
            )
            .map_err(|reason| error(format!("reserve protected BFT V2 proposal: {reason}")))?;
        Ok(())
    }

    fn reserve_vote(&mut self, intent: &BftVoteIntentV2) -> Result<(), NodeError> {
        let previous = self
            .setup
            .checkpoint
            .state
            .journal
            .as_ref()
            .ok_or_else(|| error("active BFT V2 signer journal is missing"))?;
        if previous.pending_vote_intent().is_some() {
            if previous.pending_vote_intent() == Some(intent) {
                return Ok(());
            }
            return Err(error("another protected BFT V2 vote is pending"));
        }
        if previous.pending_proposal_intent().is_some() || previous.pending_commit().is_some() {
            return Err(error("another protected BFT V2 decision is pending"));
        }
        let mut next = previous.clone();
        next.pending_vote_intent = Some(intent.clone());
        verify_bft_signing_journal_transition_v2(
            previous,
            &next,
            &self.setup.set,
            &self.setup.schedule,
            self.setup.selected.binding.timeouts,
        )
        .map_err(error)?;
        let mut candidate = self.setup.checkpoint.state.candidate();
        candidate.journal = Some(next);
        self.setup.checkpoint = self
            .setup
            .lease
            .advance(
                self.setup.checkpoint.state.revision,
                &self.setup.checkpoint.state.state_digest,
                candidate,
            )
            .map_err(|reason| error(format!("reserve protected BFT V2 vote: {reason}")))?;
        Ok(())
    }

    fn signed_proposal_time(&self, proposal: &BftProposalV2) -> Option<i64> {
        self.setup
            .checkpoint
            .state
            .journal
            .as_ref()?
            .events()
            .last()
            .and_then(|event| match event {
                BftSigningJournalEventV2::Proposal {
                    proposal: stored,
                    local: true,
                    now_ms,
                } if stored.as_ref() == proposal => Some(*now_ms),
                _ => None,
            })
    }

    fn signed_vote_time(&self, vote: &BftVoteV2) -> Option<i64> {
        self.setup
            .checkpoint
            .state
            .journal
            .as_ref()?
            .events()
            .last()
            .and_then(|event| match event {
                BftSigningJournalEventV2::Vote {
                    vote: stored,
                    local: true,
                    now_ms,
                } if stored.as_ref() == vote => Some(*now_ms),
                _ => None,
            })
    }
}
