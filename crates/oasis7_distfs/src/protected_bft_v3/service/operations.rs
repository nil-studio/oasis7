use super::*;

impl BftSignerServiceV3 {
    // Keep the validated lease, snapshot, intent, and signed-request context
    // explicit at this one-shot signing boundary.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn sign_proposal(
        &self,
        runtime: &mut BftSignerServiceRuntimeV3,
        credential: &BftSignerLeaseCredentialV3,
        token_digest: &str,
        snapshot: BftSignerBackendSnapshotV3,
        expected_revision: u64,
        intent: &BftProposalIntentV3,
        request: &SignedBftSignerRequestV3,
        request_digest: &str,
    ) -> Result<ReplyDataV3, BftSignerErrorV3> {
        let state = &snapshot.state;
        if expected_revision <= state.revision
            && let Some(proposal) = state.journal.as_ref().and_then(|journal| {
                journal.events.iter().find_map(|event| {
                    if let oasis7_consensus::tendermint_v3::BftSigningJournalEventV3::Proposal {
                        proposal,
                        local: true,
                        ..
                    } = event
                    {
                        let mut header = proposal.header.clone();
                        header.signature_hex.clear();
                        if header == intent.header
                            && proposal.block == intent.block
                            && proposal.valid_round_evidence == intent.valid_round_evidence
                        {
                            return Some(proposal.as_ref().clone());
                        }
                    }
                    None
                })
            })
        {
            return self.state_reply(
                snapshot,
                request,
                request_digest,
                BftSignerReplyKindV3::ProposalSigned {
                    proposal: Box::new(proposal),
                },
            );
        }
        if expected_revision != state.revision {
            return Err(invalid("BFT V3 proposal signing revision is stale"));
        }
        let verified = validate_state(
            &self.binding,
            Some(&self.provider_key.public_key_hex()),
            state,
        )?;
        validate_signing_install_gate(&self.binding, state, &verified.schedule)?;
        let journal = state
            .journal
            .as_ref()
            .ok_or_else(|| invalid("BFT V3 proposal signing journal is missing"))?;
        if journal.pending_proposal_intent() != Some(intent) {
            return Err(invalid("BFT V3 proposal differs from its reserved intent"));
        }
        let key = self.signing_key_for_active_set(verified.schedule.active_set())?;
        let header = BftProposalHeaderV3 {
            protocol_version: BFT_PROTOCOL_VERSION_V3,
            scope: intent.header.scope.clone(),
            height: intent.header.height,
            round: intent.header.round,
            active_validator_set_hash: intent.header.active_validator_set_hash.clone(),
            schedule_policy_hash: intent.header.schedule_policy_hash.clone(),
            proposer_id: intent.header.proposer_id.clone(),
            block_id: intent.header.block_id.clone(),
            valid_round: intent.header.valid_round,
            public_key_hex: key.public_key_hex(),
            signature_hex: String::new(),
        };
        if header.public_key_hex != intent.header.public_key_hex {
            return Err(invalid("BFT V3 proposal intent selects an unpinned key"));
        }
        let mut proposal = BftPublicProposalV3 {
            header,
            block: intent.block.clone(),
            valid_round_evidence: intent.valid_round_evidence.clone(),
        };
        let preimage = proposal
            .header
            .signing_payload()
            .map_err(|error| invalid(format!("BFT V3 proposal signing preimage: {error}")))?;
        proposal.header.signature_hex = hex::encode(key.sign_preimage(&preimage)?);
        proposal
            .validate_binding()
            .map_err(|error| invalid(format!("BFT V3 signed proposal validation: {error}")))?;
        let now = self.sample_time(runtime, credential, token_digest)?;
        let next_journal = complete_reserved_bft_proposal_v3(
            journal,
            proposal.clone(),
            now,
            verified.schedule.active_set(),
            &verified.schedule,
            self.binding.timeouts,
        )
        .map_err(|error| invalid(format!("BFT V3 proposal intent completion: {error}")))?;
        let mut candidate = state.candidate();
        candidate.journal = Some(next_journal);
        let mut committed = self.commit_candidate(
            runtime,
            credential,
            token_digest,
            state,
            candidate,
            Some(now),
            request,
            request_digest,
            BftSignerReplyKindV3::ProposalSigned {
                proposal: Box::new(proposal.clone()),
            },
        )?;
        committed.result = BftSignerReplyKindV3::ProposalSigned {
            proposal: Box::new(proposal),
        };
        Ok(committed)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn sign_vote(
        &self,
        runtime: &mut BftSignerServiceRuntimeV3,
        credential: &BftSignerLeaseCredentialV3,
        token_digest: &str,
        snapshot: BftSignerBackendSnapshotV3,
        expected_revision: u64,
        intent: &BftVoteIntentV3,
        request: &SignedBftSignerRequestV3,
        request_digest: &str,
    ) -> Result<ReplyDataV3, BftSignerErrorV3> {
        let state = &snapshot.state;
        if expected_revision <= state.revision
            && let Some(vote) = state.journal.as_ref().and_then(|journal| {
                journal.events.iter().find_map(|event| {
                    if let oasis7_consensus::tendermint_v3::BftSigningJournalEventV3::Vote {
                        vote,
                        local: true,
                        ..
                    } = event
                        && vote.height == intent.height
                        && vote.round == intent.round
                        && vote.phase == intent.phase
                        && vote.value == intent.value
                        && vote.validator_id == intent.validator_id
                        && vote.schedule_policy_hash == intent.schedule_policy_hash
                    {
                        return Some(vote.as_ref().clone());
                    }
                    None
                })
            })
        {
            return self.state_reply(
                snapshot,
                request,
                request_digest,
                BftSignerReplyKindV3::VoteSigned {
                    vote: Box::new(vote),
                },
            );
        }
        if expected_revision != state.revision {
            return Err(invalid("BFT V3 vote signing revision is stale"));
        }
        let verified = validate_state(
            &self.binding,
            Some(&self.provider_key.public_key_hex()),
            state,
        )?;
        validate_signing_install_gate(&self.binding, state, &verified.schedule)?;
        let journal = state
            .journal
            .as_ref()
            .ok_or_else(|| invalid("BFT V3 vote signing journal is missing"))?;
        if journal.pending_vote_intent() != Some(intent) {
            return Err(invalid("BFT V3 vote differs from its reserved intent"));
        }
        let key = self.signing_key_for_active_set(verified.schedule.active_set())?;
        let mut vote = BftVoteV3 {
            protocol_version: BFT_PROTOCOL_VERSION_V3,
            scope: verified.schedule.scope().clone(),
            height: intent.height,
            round: intent.round,
            phase: intent.phase,
            active_validator_set_hash: verified.schedule.active_set().set_hash().to_string(),
            schedule_policy_hash: verified.schedule.policy_hash().to_string(),
            value: intent.value.clone(),
            validator_id: intent.validator_id.clone(),
            public_key_hex: key.public_key_hex(),
            signature_hex: String::new(),
        };
        let preimage = vote
            .signing_payload()
            .map_err(|error| invalid(format!("BFT V3 vote signing preimage: {error}")))?;
        vote.signature_hex = hex::encode(key.sign_preimage(&preimage)?);
        vote.verify_signature()
            .map_err(|error| invalid(format!("BFT V3 signed vote validation: {error}")))?;
        let now = self.sample_time(runtime, credential, token_digest)?;
        let next_journal = complete_reserved_bft_vote_v3(
            journal,
            vote.clone(),
            now,
            verified.schedule.active_set(),
            &verified.schedule,
            self.binding.timeouts,
        )
        .map_err(|error| invalid(format!("BFT V3 vote intent completion: {error}")))?;
        let mut candidate = state.candidate();
        candidate.journal = Some(next_journal);
        self.commit_candidate(
            runtime,
            credential,
            token_digest,
            state,
            candidate,
            Some(now),
            request,
            request_digest,
            BftSignerReplyKindV3::VoteSigned {
                vote: Box::new(vote),
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn commit_candidate(
        &self,
        runtime: &mut BftSignerServiceRuntimeV3,
        credential: &BftSignerLeaseCredentialV3,
        token_digest: &str,
        previous: &BftSignerStateV3,
        candidate: BftSignerCandidateV3,
        minimum_event_time_ms: Option<i64>,
        request: &SignedBftSignerRequestV3,
        request_digest: &str,
        result: BftSignerReplyKindV3,
    ) -> Result<ReplyDataV3, BftSignerErrorV3> {
        let previous_verified = runtime
            .verified
            .as_ref()
            .ok_or_else(|| invalid("BFT V3 verified state cache is unavailable"))?;
        if previous_verified.state != *previous {
            runtime.poisoned = true;
            return Err(invalid(
                "BFT V3 mutation snapshot differs from verified cache",
            ));
        }
        let next_verified = validate_candidate_transition(
            &self.binding,
            previous_verified,
            &candidate,
            &self.provider_key.public_key_hex(),
        )?;
        let now = self.sample_time(runtime, credential, token_digest)?;
        if minimum_event_time_ms.is_some_and(|time| time < 0 || time > now) {
            return Err(invalid("BFT V3 event timestamp exceeds protected time"));
        }
        let next_state = next_verified.state.clone();
        let response = runtime.backend.compare_and_swap(
            credential.generation,
            token_digest,
            previous.revision,
            &previous.state_digest,
            minimum_event_time_ms,
            next_state,
        );
        let snapshot = match response {
            Ok(snapshot) => snapshot,
            Err(error) => {
                runtime.poisoned = true;
                return Err(invalid(format!(
                    "BFT V3 state CAS outcome is uncertain; provider poisoned: {error}"
                )));
            }
        };
        if snapshot.lease_generation != credential.generation
            || snapshot.lease_token_digest != token_digest
            || snapshot.state != next_verified.state
            || snapshot.protected_time_floor_ms < runtime.observed_time_floor_ms
        {
            runtime.poisoned = true;
            return Err(invalid(
                "BFT V3 state CAS returned an invalid snapshot; provider poisoned",
            ));
        }
        let verified = match validate_state(
            &self.binding,
            Some(&self.provider_key.public_key_hex()),
            &snapshot.state,
        ) {
            Ok(verified) => verified,
            Err(error) => {
                runtime.poisoned = true;
                return Err(invalid(format!(
                    "BFT V3 state CAS committed invalid state; provider poisoned: {error}"
                )));
            }
        };
        runtime.verified = Some(verified);
        runtime.observed_time_floor_ms = snapshot.protected_time_floor_ms;
        self.state_reply(snapshot, request, request_digest, result)
    }

    pub(super) fn sample_time(
        &self,
        runtime: &mut BftSignerServiceRuntimeV3,
        credential: &BftSignerLeaseCredentialV3,
        token_digest: &str,
    ) -> Result<i64, BftSignerErrorV3> {
        let now = match runtime
            .backend
            .protected_time_ms(credential.generation, token_digest)
        {
            Ok(now) => now,
            Err(error) => {
                runtime.poisoned = true;
                return Err(invalid(format!(
                    "BFT V3 protected clock is unavailable; provider poisoned: {error}"
                )));
            }
        };
        if now < runtime.observed_time_floor_ms || now < 0 {
            runtime.poisoned = true;
            return Err(invalid("BFT V3 backend protected clock rollback"));
        }
        runtime.observed_time_floor_ms = now;
        Ok(now)
    }

    pub(super) fn state_reply(
        &self,
        snapshot: BftSignerBackendSnapshotV3,
        request: &SignedBftSignerRequestV3,
        request_digest: &str,
        result: BftSignerReplyKindV3,
    ) -> Result<ReplyDataV3, BftSignerErrorV3> {
        let checkpoint = self.make_checkpoint(
            snapshot.state,
            snapshot.lease_generation,
            request.body.challenge.clone(),
            request_digest.to_string(),
        )?;
        Ok(ReplyDataV3 {
            result,
            generation: checkpoint.lease_generation,
            revision: checkpoint.state.revision,
            state_digest: checkpoint.state.state_digest.clone(),
            checkpoint: Some(Box::new(checkpoint)),
            lease_token: None,
        })
    }

    pub(super) fn make_checkpoint(
        &self,
        state: BftSignerStateV3,
        generation: u64,
        challenge: String,
        request_digest: String,
    ) -> Result<BftSignerCheckpointV3, BftSignerErrorV3> {
        let mut checkpoint = BftSignerCheckpointV3 {
            state,
            lease_generation: generation,
            challenge,
            request_digest,
            provider_public_key: self.provider_key.public_key_hex(),
            provider_signature_hex: String::new(),
        };
        checkpoint.provider_signature_hex = sign_preimage(
            self.provider_key.as_ref(),
            BFT_SIGNER_CHECKPOINT_DOMAIN_V3,
            &checkpoint.signable_body(),
        )?;
        Ok(checkpoint)
    }

    pub(super) fn signing_key_for_active_set(
        &self,
        set: &oasis7_consensus::tendermint_v3::BftValidatorSetV2,
    ) -> Result<&dyn BftSigningKeyHandleV3, BftSignerErrorV3> {
        let validator = set
            .validator(&self.binding.local_validator_id)
            .ok_or_else(|| invalid("BFT V3 local validator is not an active member"))?;
        let configured = self
            .binding
            .provisioned_signing_keys
            .iter()
            .find(|identity| {
                identity.validator_id == validator.validator_id
                    && identity.public_key_hex == validator.public_key_hex
            })
            .ok_or_else(|| invalid("BFT V3 active validator key is not provisioned"))?;
        self.validator_keys
            .get(&configured.public_key_hex)
            .map(|key| key.as_ref())
            .ok_or_else(|| invalid("BFT V3 active validator key handle is unavailable"))
    }

    pub(super) fn verify_observed_snapshot(
        &self,
        runtime: &mut BftSignerServiceRuntimeV3,
        snapshot: &BftSignerBackendSnapshotV3,
        credential: &BftSignerLeaseCredentialV3,
    ) -> Result<(), BftSignerErrorV3> {
        let expected_token_digest = if credential.token.is_empty() {
            None
        } else {
            Some(lease_digest(&credential.token)?)
        };
        validate_snapshot_shape(snapshot, Some(credential), expected_token_digest.as_deref())?;
        let verified = match self.verify_snapshot_against_cache(runtime, snapshot) {
            Ok(verified) => verified,
            Err(error) => {
                runtime.poisoned = true;
                return Err(error);
            }
        };
        if snapshot.protected_time_floor_ms < runtime.observed_time_floor_ms
            || snapshot.lease_generation < runtime.observed_generation
        {
            runtime.poisoned = true;
            return Err(invalid(
                "BFT V3 backend monotonic floor or generation regressed",
            ));
        }
        runtime.observed_time_floor_ms = snapshot.protected_time_floor_ms;
        runtime.observed_generation = snapshot.lease_generation;
        runtime.verified = Some(verified);
        Ok(())
    }

    pub(super) fn verify_snapshot_against_cache(
        &self,
        runtime: &BftSignerServiceRuntimeV3,
        snapshot: &BftSignerBackendSnapshotV3,
    ) -> Result<VerifiedBftSignerStateV3, BftSignerErrorV3> {
        let verified = validate_state(
            &self.binding,
            Some(&self.provider_key.public_key_hex()),
            &snapshot.state,
        )?;
        if let Some(previous) = &runtime.verified {
            if snapshot.state.revision < previous.state.revision {
                return Err(invalid("BFT V3 backend state revision regressed"));
            }
            if snapshot.state.revision == previous.state.revision {
                if snapshot.state != previous.state {
                    return Err(invalid("BFT V3 backend changed state at the same revision"));
                }
            } else {
                let candidate = snapshot.state.candidate();
                let monotonic = validate_candidate_transition(
                    &self.binding,
                    previous,
                    &candidate,
                    &self.provider_key.public_key_hex(),
                )?;
                if monotonic.state != snapshot.state {
                    return Err(invalid("BFT V3 backend state skipped its legal transition"));
                }
            }
        }
        Ok(verified)
    }

    pub(super) fn release_session(
        &self,
        session: &mut BftSignerSessionV3,
    ) -> Result<(), BftSignerErrorV3> {
        session.prepared = None;
        let Some(credential) = session.lease.take() else {
            return Ok(());
        };
        let token_digest = lease_digest(&credential.token)?;
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| invalid("BFT V3 provider mutex poisoned"))?;
        if runtime.poisoned {
            return Err(invalid("BFT V3 provider is poisoned"));
        }
        let snapshot = match runtime
            .backend
            .release_lease(credential.generation, &token_digest)
        {
            Ok(snapshot) => snapshot,
            Err(error) => {
                runtime.poisoned = true;
                return Err(invalid(format!(
                    "BFT V3 lease release is uncertain; provider poisoned: {error}"
                )));
            }
        };
        if let Err(error) = validate_snapshot_shape(&snapshot, None, None) {
            runtime.poisoned = true;
            return Err(invalid(format!(
                "BFT V3 lease release returned an invalid snapshot; provider poisoned: {error}"
            )));
        }
        let verified = match self.verify_snapshot_against_cache(&runtime, &snapshot) {
            Ok(verified) => verified,
            Err(error) => {
                runtime.poisoned = true;
                return Err(invalid(format!(
                    "BFT V3 lease release snapshot is invalid; provider poisoned: {error}"
                )));
            }
        };
        if snapshot.protected_time_floor_ms < runtime.observed_time_floor_ms {
            runtime.poisoned = true;
            return Err(invalid("BFT V3 lease release clock floor regressed"));
        }
        runtime.verified = Some(verified.clone());
        runtime.observed_generation = snapshot.lease_generation;
        runtime.observed_time_floor_ms = snapshot.protected_time_floor_ms;
        if snapshot.lease_token_digest.is_empty() {
            runtime.active_lease = None;
        } else {
            runtime.active_lease = Some(ActiveLeaseV3 {
                generation: snapshot.lease_generation,
                token_digest: snapshot.lease_token_digest.clone(),
            });
        }
        if snapshot.lease_generation == credential.generation.saturating_add(1)
            && snapshot.lease_token_digest.is_empty()
        {
            return Ok(());
        }
        if snapshot.lease_generation > credential.generation
            && snapshot.lease_token_digest != token_digest
        {
            return Ok(());
        }
        runtime.poisoned = true;
        Err(invalid(
            "BFT V3 backend did not durably fence the released lease",
        ))
    }
}
