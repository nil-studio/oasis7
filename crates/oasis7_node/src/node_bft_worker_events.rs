use super::*;

impl Worker {
    pub(super) fn replay(&mut self) -> Result<(), NodeError> {
        for event in self.setup.wal.state().events.clone() {
            let effects = match event {
                BftWalEventV1::Proposal {
                    proposal, now_ms, ..
                } => {
                    self.clock_base = self.clock_base.max(now_ms);
                    self.validate_candidate(&proposal.block)?;
                    self.seen
                        .insert(message_key(&BftMessageV1::Proposal(Box::new(
                            proposal.clone(),
                        )))?);
                    self.machine
                        .receive_proposal(proposal, true, now_ms)
                        .map_err(error)?
                }
                BftWalEventV1::Vote {
                    vote,
                    local,
                    now_ms,
                } => {
                    self.clock_base = self.clock_base.max(now_ms);
                    self.seen
                        .insert(message_key(&BftMessageV1::Vote(vote.clone()))?);
                    if local {
                        self.machine.record_local_vote(vote, now_ms)
                    } else {
                        self.machine.receive_vote(vote, now_ms)
                    }
                    .map_err(error)?
                }
                BftWalEventV1::Timeout { now_ms } => {
                    self.clock_base = self.clock_base.max(now_ms);
                    self.machine
                        .on_timeout(now_ms, &self.setup.bft.local_validator_id)
                        .map_err(error)?
                }
            };
            self.effects.extend(effects);
        }
        Ok(())
    }

    pub(super) fn receive(
        &mut self,
        message: BftMessageV1,
        endpoint: &GossipEndpoint,
    ) -> Result<(), NodeError> {
        if let BftMessageV1::HistoryRequest(request) = message {
            return self.serve_history(request, endpoint);
        }
        if let BftMessageV1::Commit(qc) = message {
            if qc.proposal.block.height != self.machine.height() {
                return Ok(());
            }
            let verified =
                verify_commit_certificate_token_v1(&self.setup.set, &self.setup.schedule, &qc);
            match verified {
                Ok(token) => {
                    if let Err(reason) = self.validate_candidate(token.block()) {
                        self.reject_inbound(reason);
                        return Ok(());
                    }
                    self.apply_commit(token, Some(endpoint))?;
                }
                Err(reason) => self.reject_inbound(reason),
            }
            return Ok(());
        }
        let height = match &message {
            BftMessageV1::Proposal(proposal) => proposal.block.height,
            BftMessageV1::Vote(vote) => vote.height,
            _ => return Ok(()),
        };
        if height != self.machine.height() {
            return Ok(());
        }
        let key = message_key(&message)?;
        if self.seen.contains(&key) {
            return Ok(());
        }
        let now = self.now()?;
        let mut next = self.machine.clone();
        let transition = match &message {
            BftMessageV1::Proposal(proposal) => {
                // Authenticate protocol evidence on a detached machine before
                // spending application replay work or persisting any input.
                let transition = next.receive_proposal(proposal.as_ref().clone(), true, now);
                match transition {
                    Ok(effects) => match self.validate_candidate(&proposal.block) {
                        Ok(()) => Ok(effects),
                        Err(reason) => Err(reason.to_string()),
                    },
                    Err(reason) => Err(reason),
                }
            }
            BftMessageV1::Vote(vote) => next.receive_vote(vote.clone(), now),
            _ => return Ok(()),
        };
        let effects = match transition {
            Ok(effects) => effects,
            Err(reason) => {
                self.reject_inbound(reason);
                return Ok(());
            }
        };
        let event = match message {
            BftMessageV1::Proposal(proposal) => BftWalEventV1::Proposal {
                proposal: *proposal,
                local: false,
                now_ms: now,
            },
            BftMessageV1::Vote(vote) => BftWalEventV1::Vote {
                vote,
                local: false,
                now_ms: now,
            },
            _ => return Ok(()),
        };
        self.setup.wal.append_event(event)?;
        self.seen.insert(key);
        self.machine = next;
        self.effects.extend(effects);
        Ok(())
    }

    pub(super) fn validate_candidate(
        &self,
        block: &oasis7_proto::distributed_bft::BftBlockV1,
    ) -> Result<(), NodeError> {
        block.validate().map_err(error)?;
        super::transport::validate_capacity(block, &self.setup.set)?;
        let actions: Vec<NodeConsensusAction> =
            serde_cbor::from_slice(&block.actions_cbor).map_err(|e| error(e.to_string()))?;
        if serde_cbor::to_vec(&actions).map_err(|e| error(e.to_string()))? != block.actions_cbor {
            return Err(error("BFT candidate actions are not canonical"));
        }
        NodeRuntime::validate_bft_proposal_actions_with_hook(
            &self.setup.config,
            &Some(Arc::clone(&self.setup.hook)),
            &actions,
            block.height,
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
            .preview_bft_candidate(&context, block)
            .map_err(error)
    }
}

pub(super) fn message_key(message: &BftMessageV1) -> Result<Vec<u8>, NodeError> {
    serde_cbor::to_vec(message).map_err(|e| error(format!("encode BFT message key: {e}")))
}
