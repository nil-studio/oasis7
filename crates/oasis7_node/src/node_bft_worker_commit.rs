use super::*;

impl Worker {
    pub(super) fn apply_commit(
        &mut self,
        token: VerifiedBftCommitCertificateV1,
        gossip: Option<&GossipEndpoint>,
    ) -> Result<(), NodeError> {
        self.validate_candidate(token.block())?;
        self.setup.wal.reserve_commit(&token)?;
        let result = lock_hook(&self.setup.hook)?
            .commit_bft_certificate(token.clone())
            .map_err(error)?;
        validate_execution_result(&token, &result)?;
        self.setup.wal.complete_commit(&token)?;
        let actions: Vec<NodeConsensusAction> = serde_cbor::from_slice(&token.block().actions_cbor)
            .map_err(|e| error(e.to_string()))?;
        {
            let mut pending = self
                .setup
                .pending
                .lock()
                .map_err(|_| error("BFT pending action queue lock poisoned"))?;
            pending.retain(|action| !actions.iter().any(|committed| committed == action));
            let bytes = pending.iter().try_fold(0usize, |n, action| {
                n.checked_add(action.payload_cbor.len())
                    .ok_or_else(|| error("BFT action queue bytes overflow"))
            })?;
            self.setup.pending_bytes.store(bytes, Ordering::Release);
        }
        self.setup.schedule = token.next_height_schedule().clone();
        self.setup.tip = Some(token.clone());
        self.last_commit = Some(Instant::now());
        self.effects.clear();
        self.seen.clear();
        let now = self.now()?;
        let origin = self.setup.wal.ensure_height_started_at(now)?;
        self.machine = BftConsensusMachineV1::new(
            self.setup.set.clone(),
            self.setup.bft.local_validator_id.clone(),
            self.setup.schedule.height(),
            Some(self.setup.schedule.starting_priority().clone()),
            origin,
            self.setup.bft.timeouts,
        )
        .map_err(error)?;
        self.publish_status(Some(&token));
        lock_state(&self.setup.state).consensus.last_committed_at_ms = Some(now_unix_ms());
        // No legacy NodeCommittedActionBatch/PosCommitted receipt is emitted.
        if let Some(endpoint) = gossip {
            self.broadcast(endpoint, &BftMessageV1::Commit(token.certificate().clone()));
        }
        if self.setup.schedule.height()
            > oasis7_proto::distributed_bft::BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1 as u64
        {
            return Err(error(
                "BFT engineering commit-history height capacity exhausted; stop signing",
            ));
        }
        Ok(())
    }

    pub(super) fn serve_history(
        &mut self,
        request: BftHistoryRequestV1,
        endpoint: &GossipEndpoint,
    ) -> Result<(), NodeError> {
        if request.validate().is_err()
            || request.scope != self.setup.bft.scope
            || request.validator_set_hash != self.setup.set.set_hash()
            || self
                .last_response
                .is_some_and(|t| t.elapsed() < Duration::from_secs(1))
        {
            return Ok(());
        }
        let local_committed_height = self
            .setup
            .schedule
            .height()
            .checked_sub(1)
            .ok_or_else(|| error("BFT next-height schedule has no committed predecessor"))?;
        if request.after_height > local_committed_height {
            // The cursor is an unauthenticated hint. Never ask durable history
            // for a cursor beyond the locally verified committed tip.
            return Ok(());
        }
        self.last_response = Some(Instant::now());
        // A hint never establishes a trusted head. Match its cursor against
        // our own immutable certificate history before answering one page.
        let config = self.execution_config();
        let expected = if request.after_height == 0 {
            self.setup.bft.scope.genesis_hash.clone()
        } else {
            let page = lock_hook(&self.setup.hook)?
                .load_bft_commit_history_page(&config, request.after_height - 1, 1)
                .map_err(error)?;
            let Some(cert) = page.first() else {
                return Ok(());
            };
            if page.len() != 1 || cert.proposal.block.height != request.after_height {
                return Err(error("BFT local history returned an invalid cursor page"));
            }
            cert.proposal.block.block_id().map_err(error)?
        };
        if expected != request.after_block_id {
            return Ok(());
        }
        let page = lock_hook(&self.setup.hook)?
            .load_bft_commit_history_page(&config, request.after_height, 1)
            .map_err(error)?;
        if page.len() > 1 {
            return Err(error("BFT history response exceeds one certificate"));
        }
        if let Some(cert) = page.into_iter().next() {
            if cert.proposal.block.height
                != request
                    .after_height
                    .checked_add(1)
                    .ok_or_else(|| error("BFT history cursor overflow"))?
            {
                return Err(error("BFT history response is not contiguous"));
            }
            self.last_response = Some(Instant::now());
            self.broadcast(endpoint, &BftMessageV1::Commit(cert));
        }
        Ok(())
    }

    pub(super) fn execution_config(&self) -> crate::NodeBftExecutionConfigV1 {
        crate::NodeBftExecutionConfigV1 {
            scope: self.setup.bft.scope.clone(),
            authority_epoch: self.setup.bft.authority_epoch,
            validators: self.setup.bft.validators.clone(),
            validator_set_hash: self.setup.set.set_hash().into(),
        }
    }

    pub(super) fn broadcast(&self, endpoint: &GossipEndpoint, message: &BftMessageV1) {
        if let Err(reason) = endpoint.broadcast_bft(message) {
            // Transport loss never revokes a durable signing decision/QC.
            lock_state(&self.setup.state)
                .bft_status
                .get_or_insert_with(Default::default)
                .last_transport_error = Some(reason.to_string());
        }
    }

    pub(super) fn retransmit(&mut self, gossip: Option<&GossipEndpoint>) -> Result<(), NodeError> {
        let Some(endpoint) = gossip else {
            return Ok(());
        };
        if self
            .last_broadcast
            .is_none_or(|t| t.elapsed() >= Duration::from_millis(200))
        {
            self.last_broadcast = Some(Instant::now());
            for event in &self.setup.wal.state().events {
                match event {
                    BftWalEventV1::Proposal {
                        proposal,
                        local: true,
                        ..
                    } if proposal.header.round == self.machine.round() => {
                        self.broadcast(endpoint, &BftMessageV1::Proposal(proposal.clone()))
                    }
                    BftWalEventV1::Vote {
                        vote, local: true, ..
                    } if vote.round == self.machine.round() => {
                        self.broadcast(endpoint, &BftMessageV1::Vote(vote.as_ref().clone()))
                    }
                    _ => {}
                }
            }
            if let Some(token) = &self.setup.tip {
                self.broadcast(endpoint, &BftMessageV1::Commit(token.certificate().clone()));
            }
        }
        if self
            .last_request
            .is_none_or(|t| t.elapsed() >= Duration::from_secs(1))
        {
            self.last_request = Some(Instant::now());
            self.broadcast(
                endpoint,
                &BftMessageV1::HistoryRequest(BftHistoryRequestV1 {
                    scope: self.setup.bft.scope.clone(),
                    validator_set_hash: self.setup.set.set_hash().into(),
                    after_height: self.setup.schedule.height() - 1,
                    after_block_id: self.setup.schedule.parent_commit_ref().into(),
                }),
            );
        }
        Ok(())
    }
}
