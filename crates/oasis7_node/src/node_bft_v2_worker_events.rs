use super::*;

impl Worker {
    pub(super) fn index_journal_events(&mut self) -> Result<(), NodeError> {
        let Some(journal) = self.setup.checkpoint.state.journal.as_ref() else {
            return Ok(());
        };
        for event in journal.events() {
            let message = match event {
                BftSigningJournalEventV2::Proposal { proposal, .. } => {
                    Some(BftMessageV2::Proposal(proposal.clone()))
                }
                BftSigningJournalEventV2::Vote { vote, .. } => {
                    Some(BftMessageV2::Vote(vote.as_ref().clone()))
                }
                BftSigningJournalEventV2::Timeout { .. }
                | BftSigningJournalEventV2::ProposalEquivocation { .. } => None,
            };
            if let Some(message) = message {
                self.seen.insert(message_digest(&message)?);
            }
        }
        Ok(())
    }

    pub(super) fn receive(
        &mut self,
        message: BftMessageV2,
        endpoint: &GossipEndpoint,
    ) -> Result<(), NodeError> {
        if let BftMessageV2::HistoryRequest(request) = message {
            return self.serve_history(request, endpoint);
        }
        if let BftMessageV2::Commit(certificate) = message {
            let height = certificate.proposal.block.height;
            if height != self.setup.schedule.height() {
                return Ok(());
            }
            match verify_commit_certificate_token_v2(&self.setup.schedule, &certificate) {
                Ok(token) => {
                    if let Err(reason) = startup::validate_candidate_with_hook(
                        &self.setup.config,
                        &self.setup.hook,
                        &self.setup.schedule,
                        token.block(),
                    ) {
                        self.reject_inbound(reason);
                        return Ok(());
                    }
                    self.apply_commit(token, Some(endpoint))?;
                }
                Err(reason) => self.reject_inbound(reason),
            }
            return Ok(());
        }

        let digest = message_digest(&message)?;
        if self.seen.contains(&digest) {
            return Ok(());
        }
        if self.seen.len() >= MAX_SEEN_MESSAGES_V2 {
            self.reject_inbound("BFT V2 per-height duplicate-message index is full");
            return Ok(());
        }
        let height = match &message {
            BftMessageV2::Proposal(proposal) => proposal.header.height,
            BftMessageV2::Vote(vote) => vote.height,
            _ => return Ok(()),
        };
        if height != self.setup.schedule.height() {
            return Ok(());
        }
        let now = self.protected_now()?;
        let mut next = self.setup.machine.clone();
        let effects = match &message {
            BftMessageV2::Proposal(proposal) => {
                match next.receive_proposal(proposal.as_ref().clone(), true, now) {
                    Ok(effects) => {
                        if let Err(reason) = startup::validate_candidate_with_hook(
                            &self.setup.config,
                            &self.setup.hook,
                            &self.setup.schedule,
                            &proposal.block,
                        ) {
                            self.reject_inbound(reason);
                            return Ok(());
                        }
                        effects
                    }
                    Err(reason) => {
                        self.retain_proposal_equivocation(&next, digest, now)?;
                        self.reject_inbound(reason);
                        return Ok(());
                    }
                }
            }
            BftMessageV2::Vote(vote) => match next.receive_vote(vote.clone(), now) {
                Ok(effects) => effects,
                Err(reason) => {
                    self.reject_inbound(reason);
                    return Ok(());
                }
            },
            _ => return Ok(()),
        };

        let event = match message {
            BftMessageV2::Proposal(proposal) => BftSigningJournalEventV2::Proposal {
                proposal,
                local: false,
                now_ms: now,
            },
            BftMessageV2::Vote(vote) => BftSigningJournalEventV2::Vote {
                vote: Box::new(vote),
                local: false,
                now_ms: now,
            },
            _ => unreachable!("history and commit messages returned above"),
        };
        self.append_event(event)?;
        self.seen.insert(digest);
        self.setup.machine = next;
        self.effects.extend(effects);
        Ok(())
    }

    fn retain_proposal_equivocation(
        &mut self,
        observed: &BftConsensusMachineV2,
        message_digest: [u8; 32],
        now_ms: i64,
    ) -> Result<(), NodeError> {
        let evidence = observed
            .proposal_equivocation_evidence()
            .find_map(|(first, second)| {
                let already_retained = self.setup.machine.proposal_equivocation_evidence().any(
                    |(known_first, known_second)| known_first == first && known_second == second,
                );
                (!already_retained).then(|| (first.clone(), second.clone()))
            });
        let Some((first, second)) = evidence else {
            return Ok(());
        };

        self.append_event(BftSigningJournalEventV2::ProposalEquivocation {
            first: Box::new(first.clone()),
            second: Box::new(second.clone()),
            now_ms,
        })?;
        self.setup
            .machine
            .record_proposal_equivocation_evidence(&first, &second)
            .map_err(error)?;
        self.seen.insert(message_digest);
        self.publish_status(None);
        Ok(())
    }

    pub(super) fn append_event(
        &mut self,
        event: BftSigningJournalEventV2,
    ) -> Result<(), NodeError> {
        let Some(previous) = self.setup.checkpoint.state.journal.as_ref() else {
            return Ok(());
        };
        let evidence_only = matches!(
            &event,
            BftSigningJournalEventV2::ProposalEquivocation { .. }
        );
        if !evidence_only
            && (previous.pending_proposal_intent().is_some()
                || previous.pending_vote_intent().is_some()
                || previous.pending_commit().is_some())
        {
            return Err(error("protected BFT V2 signer has an unresolved decision"));
        }
        let mut next = previous.clone();
        next.events.push(event);
        if next
            .encoded_len()
            .map_err(|reason| error(format!("BFT V2 journal bound: {reason}")))?
            > oasis7_consensus::tendermint_v2::BFT_MAX_SIGNING_JOURNAL_BYTES_V2
        {
            return Err(error("protected BFT V2 journal capacity exhausted"));
        }
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
            .map_err(|reason| error(format!("append protected BFT V2 event: {reason}")))?;
        Ok(())
    }

    pub(super) fn on_timeout(&mut self, now: i64) -> Result<(), NodeError> {
        let mut next = self.setup.machine.clone();
        let effects = next
            .on_timeout(now, &self.setup.selected.binding.local_validator_id)
            .map_err(error)?;
        self.append_event(BftSigningJournalEventV2::Timeout { now_ms: now })?;
        self.setup.machine = next;
        self.effects.extend(effects);
        Ok(())
    }

    fn serve_history(
        &mut self,
        request: BftHistoryRequestV2,
        endpoint: &GossipEndpoint,
    ) -> Result<(), NodeError> {
        if request.validate().is_err()
            || request.scope != self.setup.selected.binding.scope
            || self
                .last_response
                .is_some_and(|last| last.elapsed() < Duration::from_secs(1))
        {
            return Ok(());
        }
        let Ok(after) = usize::try_from(request.after_height) else {
            // A foreign cursor is an invalid request, not a local failure.
            return Ok(());
        };
        let Some(certificate) = self.setup.history.get(after) else {
            return Ok(());
        };
        // Startup independently verifies the complete history and every append
        // extends its verified cursor. The next retained QC therefore already
        // records the set/policy at this exact request cursor. Replaying the
        // whole prefix per request would let polling monopolize the worker.
        let next_block = &certificate.proposal.block;
        let expected_cursor = if after == 0 {
            self.setup.selected.binding.scope.genesis_hash.clone()
        } else {
            self.setup.history[after - 1]
                .proposal
                .block
                .block_id()
                .map_err(error)?
        };
        if request.after_block_id != expected_cursor
            || request.active_validator_set_hash != next_block.active_validator_set_hash
            || request.schedule_policy_hash != next_block.schedule_policy_hash
        {
            return Ok(());
        }
        self.last_response = Some(Instant::now());
        self.broadcast(endpoint, &BftMessageV2::Commit(certificate.clone()));
        Ok(())
    }

    pub(super) fn retransmit(&mut self, gossip: Option<&GossipEndpoint>) -> Result<(), NodeError> {
        let Some(endpoint) = gossip else {
            return Ok(());
        };
        if self
            .last_broadcast
            .is_none_or(|last| last.elapsed() >= Duration::from_millis(200))
        {
            self.last_broadcast = Some(Instant::now());
            if let Some(journal) = self.setup.checkpoint.state.journal.as_ref() {
                for event in journal.events() {
                    match event {
                        BftSigningJournalEventV2::Proposal {
                            proposal,
                            local: true,
                            ..
                        } if proposal.header.round == self.setup.machine.round() => {
                            self.broadcast(endpoint, &BftMessageV2::Proposal(proposal.clone()));
                        }
                        BftSigningJournalEventV2::Vote {
                            vote, local: true, ..
                        } if vote.round == self.setup.machine.round() => {
                            self.broadcast(endpoint, &BftMessageV2::Vote(vote.as_ref().clone()));
                        }
                        _ => {}
                    }
                }
            }
            if let Some(token) = &self.setup.tip {
                self.broadcast(endpoint, &BftMessageV2::Commit(token.certificate().clone()));
            }
        }
        if self
            .last_request
            .is_none_or(|last| last.elapsed() >= Duration::from_secs(1))
        {
            self.last_request = Some(Instant::now());
            self.broadcast(
                endpoint,
                &BftMessageV2::HistoryRequest(BftHistoryRequestV2 {
                    scope: self.setup.schedule.scope().clone(),
                    active_validator_set_hash: self.setup.set.set_hash().into(),
                    schedule_policy_hash: self.setup.schedule.policy_hash().into(),
                    after_height: self.setup.schedule.height().saturating_sub(1),
                    after_block_id: self.setup.schedule.parent_commit_ref().into(),
                }),
            );
        }
        Ok(())
    }

    pub(super) fn broadcast(&self, endpoint: &GossipEndpoint, message: &BftMessageV2) {
        if let Err(reason) = endpoint.broadcast_bft_v2(message) {
            self.set_transport_error(reason);
        }
    }
}

pub(super) fn message_digest(message: &BftMessageV2) -> Result<[u8; 32], NodeError> {
    let bytes = serde_cbor::to_vec(message)
        .map_err(|err| error(format!("encode BFT V2 message identity: {err}")))?;
    let encoded = crate::blake3_hex(&bytes);
    let mut digest = [0_u8; 32];
    hex::decode_to_slice(encoded.as_bytes(), &mut digest)
        .map_err(|err| error(format!("decode BFT V2 message identity digest: {err}")))?;
    Ok(digest)
}
