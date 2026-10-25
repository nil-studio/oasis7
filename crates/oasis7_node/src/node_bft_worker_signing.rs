use super::*;

impl Worker {
    pub(super) fn finish_effects(
        &mut self,
        gossip: Option<&GossipEndpoint>,
    ) -> Result<(), NodeError> {
        while let Some(effect) = self.effects.pop_front() {
            match effect {
                BftConsensusActionV1::SignVote(intent) => {
                    if intent.height != self.machine.height()
                        || intent.round != self.machine.round()
                    {
                        continue;
                    }
                    if self
                        .machine
                        .vote_intent(
                            intent.validator_id.clone(),
                            intent.phase,
                            intent.value.clone(),
                            intent.round,
                        )
                        .map_err(error)?
                        .is_none()
                    {
                        continue;
                    }
                    self.setup.wal.reserve_vote_intent(intent.clone())?;
                    let vote = intent
                        .sign(&self.setup.set, &self.setup.key)
                        .map_err(error)?;
                    let now = self.now()?;
                    self.setup
                        .wal
                        .append_local_vote(&intent, vote.clone(), now)?;
                    self.seen
                        .insert(super::events::message_key(&BftMessageV1::Vote(
                            vote.clone(),
                        ))?);
                    let more = self
                        .machine
                        .record_local_vote(vote.clone(), now)
                        .map_err(error)?;
                    if let Some(endpoint) = gossip {
                        self.broadcast(endpoint, &BftMessageV1::Vote(vote));
                    }
                    self.effects.extend(more);
                }
                BftConsensusActionV1::Commit(qc) => {
                    if qc.proposal.block.height != self.machine.height() {
                        continue;
                    }
                    let token = verify_commit_certificate_token_v1(
                        &self.setup.set,
                        &self.setup.schedule,
                        &qc,
                    )
                    .map_err(error)?;
                    self.apply_commit(token, gossip)?;
                }
                BftConsensusActionV1::RoundAdvanced { .. } => {}
            }
        }
        Ok(())
    }

    pub(super) fn resume_pending_signatures(
        &mut self,
        gossip: Option<&GossipEndpoint>,
    ) -> Result<(), NodeError> {
        if let Some(intent) = self.setup.wal.state().pending_proposal_intent.clone() {
            self.sign_proposal(intent, gossip)?;
            self.finish_effects(gossip)?;
        }
        if let Some(intent) = self.setup.wal.state().pending_vote_intent.clone() {
            self.effects
                .push_back(BftConsensusActionV1::SignVote(intent));
            self.finish_effects(gossip)?;
        }
        Ok(())
    }

    pub(super) fn maybe_propose(
        &mut self,
        gossip: Option<&GossipEndpoint>,
    ) -> Result<(), NodeError> {
        if self
            .last_commit
            .is_some_and(|t| t.elapsed() < self.setup.config.tick_interval)
        {
            return Ok(());
        }
        if self.machine.step() != BftStepV1::Propose
            || self.machine.current_proposer_id() != Some(self.setup.bft.local_validator_id.as_str())
            || self.setup.wal.state().events.iter().any(|event| matches!(event,
                BftWalEventV1::Proposal { proposal, local: true, .. } if proposal.header.round == self.machine.round())) {
            return Ok(());
        }
        let block = if let Some((id, _)) = self.machine.valid() {
            self.setup
                .wal
                .state()
                .events
                .iter()
                .rev()
                .find_map(|event| match event {
                    BftWalEventV1::Proposal { proposal, .. } if proposal.header.block_id == id => {
                        Some(proposal.block.clone())
                    }
                    _ => None,
                })
                .ok_or_else(|| error("BFT valid value lacks its durable original block"))?
        } else {
            let actions = {
                let pending = self
                    .setup
                    .pending
                    .lock()
                    .map_err(|_| error("BFT pending action queue lock poisoned"))?;
                pending
                    .iter()
                    .take(self.setup.config.max_engine_pending_consensus_actions)
                    .cloned()
                    .collect::<Vec<_>>()
            };
            NodeRuntime::validate_bft_proposal_actions_with_hook(
                &self.setup.config,
                &Some(Arc::clone(&self.setup.hook)),
                &actions,
                self.machine.height(),
            )?;
            let context = lock_hook(&self.setup.hook)?
                .current_bft_execution_context(
                    &self.setup.bft.scope,
                    self.setup.schedule.height(),
                    self.setup.schedule.parent_commit_ref(),
                    self.setup.bft.authority_epoch,
                )
                .map_err(error)?;
            lock_hook(&self.setup.hook)?
                .build_bft_candidate(crate::NodeBftBuildContextV1 {
                    execution: context,
                    actions,
                })
                .map_err(error)?
        };
        self.validate_candidate(&block)?;
        let intent = self
            .machine
            .proposal_intent(block, self.setup.bft.local_validator_id.as_str())
            .map_err(error)?;
        self.setup.wal.reserve_proposal_intent(intent.clone())?;
        self.sign_proposal(intent, gossip)
    }

    fn sign_proposal(
        &mut self,
        intent: oasis7_consensus::tendermint_v1::BftProposalIntentV1,
        gossip: Option<&GossipEndpoint>,
    ) -> Result<(), NodeError> {
        let proposal = self
            .machine
            .sign_proposal_intent(&intent, &self.setup.key)
            .map_err(error)?;
        self.validate_candidate(&proposal.block)?;
        let now = self.now()?;
        self.setup
            .wal
            .append_local_proposal(&intent, proposal.clone(), now)?;
        let mut next = self.machine.clone();
        let effects = next
            .receive_proposal(proposal.clone(), true, now)
            .map_err(error)?;
        self.machine = next;
        self.seen
            .insert(super::events::message_key(&BftMessageV1::Proposal(
                Box::new(proposal.clone()),
            ))?);
        if let Some(endpoint) = gossip {
            self.broadcast(endpoint, &BftMessageV1::Proposal(Box::new(proposal)));
        }
        self.effects.extend(effects);
        Ok(())
    }
}
