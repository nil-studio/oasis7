use super::*;
use oasis7_proto::distributed_bft::{
    BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1, BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1,
};

impl Worker {
    pub(super) fn apply_commit(
        &mut self,
        token: VerifiedBftCommitCertificateV2,
        gossip: Option<&GossipEndpoint>,
    ) -> Result<(), NodeError> {
        if token.schedule() != &self.setup.schedule
            || token.block().height != self.setup.machine.height()
        {
            return Err(error(
                "BFT V2 commit token does not extend the active machine",
            ));
        }
        let certificate = token.certificate().clone();
        let certificate_bytes = startup::canonical_certificate_bytes(&certificate)?.len();
        let next_history_count = self
            .setup
            .history
            .len()
            .checked_add(1)
            .ok_or_else(|| error("BFT V2 history height overflow"))?;
        let next_history_bytes = self
            .setup
            .history_bytes
            .checked_add(certificate_bytes)
            .ok_or_else(|| error("BFT V2 history byte count overflow"))?;
        if next_history_count > BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1
            || next_history_bytes > BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1
        {
            return Err(error(
                "BFT V2 commit history capacity exhausted; stop without signing",
            ));
        }

        // Every QC, including one just formed locally, must pass actual
        // deterministic application execution before the protected history
        // can advance. The provider's QC validation alone is not valid(v).
        startup::validate_candidate_with_hook(
            &self.setup.config,
            &self.setup.hook,
            &self.setup.schedule,
            token.block(),
        )?;

        self.reserve_commit(&certificate)?;
        self.setup.checkpoint = self
            .setup
            .lease
            .append_committed_certificate(
                self.setup.checkpoint.state.revision,
                &self.setup.checkpoint.state.state_digest,
                certificate.clone(),
            )
            .map_err(|reason| {
                error(format!(
                    "append protected BFT V2 commit certificate: {reason}"
                ))
            })?;

        let result = lock_hook(&self.setup.hook)?
            .commit_bft_v2_certificate(token.clone())
            .map_err(error)?;
        startup::validate_execution_result_v2(&token, &result)?;

        let now = self.protected_now()?;
        let mut transition_check = self.setup.machine.clone();
        transition_check
            .mark_commit_applied(&token, now, self.setup.selected.binding.timeouts)
            .map_err(error)?;
        let next_schedule = token.next_height_schedule().clone();
        if transition_check.schedule() != &next_schedule {
            return Err(error(
                "BFT V2 machine failed to advance to the verified schedule",
            ));
        }

        // A transition ACK must attest to the provider's durable journal
        // replay, not a volatile machine advanced by the local commit callback.
        // The provider appended the QC above, so restore from that checkpoint
        // before allowing an installation receipt.
        let next_machine = startup::restore_machine(
            &self.setup.selected,
            &next_schedule,
            &self.setup.checkpoint,
            now,
        )?;

        if let Some(reference) = token.block().transition_ref.as_ref() {
            startup::maybe_install_transition(
                &self.setup.selected,
                &next_schedule,
                reference,
                &certificate,
                &result,
                &next_machine,
                &mut *self.setup.lease,
                &mut self.setup.checkpoint,
            )?;
        }
        let next_local_key = startup::provisioned_active_key(&self.setup.selected, &next_schedule);
        if next_local_key.is_some()
            && next_schedule.last_activation_height() > 0
            && !self
                .setup
                .checkpoint
                .state
                .install_receipts
                .iter()
                .any(|receipt| {
                    receipt.body.activation_height == next_schedule.last_activation_height()
                        && receipt.body.local_validator_id
                            == self.setup.selected.binding.local_validator_id
                })
        {
            return Err(error(
                "active BFT V2 validator has no durable receipt for the activated set",
            ));
        }

        self.setup.schedule = next_schedule;
        self.setup.set = self.setup.schedule.active_set().clone();
        self.setup.local_key = next_local_key;
        let needs_timer = self
            .setup
            .checkpoint
            .state
            .journal
            .as_ref()
            .is_some_and(|journal| journal.height_started_at_ms().is_none());
        if needs_timer {
            self.setup.checkpoint = self
                .setup
                .lease
                .start_height_timer(self.setup.checkpoint.state.revision)
                .map_err(|reason| {
                    error(format!(
                        "start next protected BFT V2 height timer: {reason}"
                    ))
                })?;
        }
        self.setup.machine = if self.setup.checkpoint.state.journal.is_some() {
            let journal = self
                .setup
                .checkpoint
                .state
                .journal
                .as_ref()
                .ok_or_else(|| error("protected signer omitted successor BFT V2 journal"))?;
            replay_bft_signing_journal_v2(
                journal,
                &self.setup.set,
                &self.setup.schedule,
                self.setup.selected.binding.timeouts,
            )
            .map_err(|reason| error(format!("replay successor BFT V2 journal: {reason}")))?
        } else {
            BftConsensusMachineV2::new(
                self.setup.schedule.clone(),
                self.setup.selected.binding.local_validator_id.clone(),
                now,
                self.setup.selected.binding.timeouts,
            )
            .map_err(error)?
        };

        self.setup.history.push(certificate);
        self.setup.history_bytes = next_history_bytes;
        self.setup.tip = Some(token.clone());
        self.effects.clear();
        self.seen.clear();
        self.last_commit = Some(Instant::now());
        {
            let mut pending = self
                .setup
                .pending
                .lock()
                .map_err(|_| error("BFT V2 pending action queue lock poisoned"))?;
            let committed_actions: Vec<NodeConsensusAction> =
                serde_cbor::from_slice(&token.block().actions_cbor)
                    .map_err(|err| error(format!("decode committed BFT V2 actions: {err}")))?;
            pending.retain(|action| !committed_actions.iter().any(|item| item == action));
            let bytes = pending.iter().try_fold(0usize, |total, action| {
                total
                    .checked_add(action.payload_cbor.len())
                    .ok_or_else(|| error("BFT V2 action queue byte count overflow"))
            })?;
            self.setup.pending_bytes.store(bytes, Ordering::Release);
        }
        self.publish_status(Some(&token));
        lock_state(&self.setup.state).consensus.last_committed_at_ms = Some(now_unix_ms());
        if let Some(endpoint) = gossip {
            self.broadcast(endpoint, &BftMessageV2::Commit(token.certificate().clone()));
        }
        Ok(())
    }

    fn reserve_commit(&mut self, certificate: &BftCommitCertificateV2) -> Result<(), NodeError> {
        let Some(previous) = self.setup.checkpoint.state.journal.as_ref() else {
            // An inactive follower has no signing journal and cannot sign. Its
            // raw QC still advances only through the provider's set verifier.
            return Ok(());
        };
        if previous.pending_commit().is_some() {
            if previous.pending_commit() == Some(certificate) {
                return Ok(());
            }
            return Err(error(
                "a different protected BFT V2 commit is already pending",
            ));
        }
        if previous.pending_proposal_intent().is_some() || previous.pending_vote_intent().is_some()
        {
            return Err(error(
                "cannot reserve BFT V2 commit while a signer decision is pending",
            ));
        }
        let mut next = previous.clone();
        next.pending_commit = Some(certificate.clone());
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
            .map_err(|reason| error(format!("reserve protected BFT V2 commit: {reason}")))?;
        Ok(())
    }
}
