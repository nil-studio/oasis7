use super::super::crypto::digest_hex;
use super::*;

impl BftSignerServiceV2 {
    // Keep the validated lease, snapshot, intent, and signed-request context
    // explicit at this one-shot signing boundary.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn sign_proposal(
        &self,
        runtime: &mut BftSignerServiceRuntimeV2,
        credential: &BftSignerLeaseCredentialV2,
        token_digest: &str,
        snapshot: BftSignerBackendSnapshotV2,
        expected_revision: u64,
        intent: &BftProposalIntentV2,
        request: &SignedBftSignerRequestV2,
        request_digest: &str,
    ) -> Result<ReplyDataV2, BftSignerErrorV2> {
        let state = &snapshot.state;
        if expected_revision != state.revision {
            return Err(invalid("BFT V2 proposal signing revision is stale"));
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
            .ok_or_else(|| invalid("BFT V2 proposal signing journal is missing"))?;
        if journal.pending_proposal_intent() != Some(intent) {
            return Err(invalid("BFT V2 proposal differs from its reserved intent"));
        }
        let key = self.signing_key_for_active_set(verified.schedule.active_set())?;
        let header = BftProposalHeaderV2 {
            protocol_version: BFT_PROTOCOL_VERSION_V2,
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
            return Err(invalid("BFT V2 proposal intent selects an unpinned key"));
        }
        let mut proposal = BftProposalV2 {
            header,
            block: intent.block.clone(),
            valid_round_evidence: intent.valid_round_evidence.clone(),
        };
        let preimage = proposal
            .header
            .signing_payload()
            .map_err(|error| invalid(format!("BFT V2 proposal signing preimage: {error}")))?;
        proposal.header.signature_hex = hex::encode(key.sign_preimage(&preimage)?);
        proposal
            .validate_binding()
            .map_err(|error| invalid(format!("BFT V2 signed proposal validation: {error}")))?;
        let now = self.sample_time(runtime, credential, token_digest)?;
        let next_journal = complete_reserved_bft_proposal_v2(
            journal,
            proposal.clone(),
            now,
            verified.schedule.active_set(),
            &verified.schedule,
            self.binding.timeouts,
        )
        .map_err(|error| invalid(format!("BFT V2 proposal intent completion: {error}")))?;
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
            BftSignerReplyKindV2::ProposalSigned {
                proposal: Box::new(proposal.clone()),
            },
        )?;
        committed.result = BftSignerReplyKindV2::ProposalSigned {
            proposal: Box::new(proposal),
        };
        Ok(committed)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn sign_vote(
        &self,
        runtime: &mut BftSignerServiceRuntimeV2,
        credential: &BftSignerLeaseCredentialV2,
        token_digest: &str,
        snapshot: BftSignerBackendSnapshotV2,
        expected_revision: u64,
        intent: &BftVoteIntentV2,
        request: &SignedBftSignerRequestV2,
        request_digest: &str,
    ) -> Result<ReplyDataV2, BftSignerErrorV2> {
        let state = &snapshot.state;
        if expected_revision != state.revision {
            return Err(invalid("BFT V2 vote signing revision is stale"));
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
            .ok_or_else(|| invalid("BFT V2 vote signing journal is missing"))?;
        if journal.pending_vote_intent() != Some(intent) {
            return Err(invalid("BFT V2 vote differs from its reserved intent"));
        }
        let key = self.signing_key_for_active_set(verified.schedule.active_set())?;
        let mut vote = BftVoteV2 {
            protocol_version: BFT_PROTOCOL_VERSION_V2,
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
            .map_err(|error| invalid(format!("BFT V2 vote signing preimage: {error}")))?;
        vote.signature_hex = hex::encode(key.sign_preimage(&preimage)?);
        vote.verify_signature()
            .map_err(|error| invalid(format!("BFT V2 signed vote validation: {error}")))?;
        let now = self.sample_time(runtime, credential, token_digest)?;
        let next_journal = complete_reserved_bft_vote_v2(
            journal,
            vote.clone(),
            now,
            verified.schedule.active_set(),
            &verified.schedule,
            self.binding.timeouts,
        )
        .map_err(|error| invalid(format!("BFT V2 vote intent completion: {error}")))?;
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
            BftSignerReplyKindV2::VoteSigned {
                vote: Box::new(vote),
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_transition_install(
        &self,
        runtime: &mut BftSignerServiceRuntimeV2,
        credential: &BftSignerLeaseCredentialV2,
        token_digest: &str,
        snapshot: BftSignerBackendSnapshotV2,
        expected_revision: u64,
        expected_state_digest: &str,
        request_id: &str,
        source_height: u64,
        runtime_instance_id: &str,
        request: &SignedBftSignerRequestV2,
        request_digest: &str,
    ) -> Result<ReplyDataV2, BftSignerErrorV2> {
        if let Some(receipt) = snapshot
            .state
            .install_receipt_for_request(request_id)
            .cloned()
        {
            let original_revision = receipt.body.installed_revision.checked_sub(2);
            if original_revision != Some(expected_revision)
                || receipt.body.pre_install_state_digest != expected_state_digest
                || receipt.body.source_height != source_height
                || receipt.body.runtime_instance_id != runtime_instance_id
            {
                return Err(invalid(
                    "BFT V2 completed install request ID was reused with different inputs",
                ));
            }
            return self.state_reply(
                snapshot,
                request,
                request_digest,
                BftSignerReplyKindV2::InstallPreparedCompleted {
                    receipt: Box::new(receipt),
                },
            );
        }
        if let Some(intent) = snapshot.state.pending_install.clone() {
            if intent.body.request_id == request_id {
                if intent.body.expected_revision != expected_revision
                    || intent.body.expected_state_digest != expected_state_digest
                    || intent.body.transition_source_height != source_height
                    || intent.body.runtime_instance_id != runtime_instance_id
                {
                    return Err(invalid(
                        "BFT V2 pending install request ID was reused with different inputs",
                    ));
                }
                return self.state_reply(
                    snapshot,
                    request,
                    request_digest,
                    BftSignerReplyKindV2::InstallPreparedPending {
                        intent: Box::new(intent),
                    },
                );
            }
            return Err(invalid("BFT V2 another transition installation is pending"));
        }
        let state = &snapshot.state;
        if state.revision != expected_revision || state.state_digest != expected_state_digest {
            return Err(invalid("BFT V2 install preparation checkpoint is stale"));
        }
        if runtime_instance_id != self.binding.operator_instance_id {
            return Err(invalid(
                "BFT V2 install runtime instance differs from binding",
            ));
        }
        let contexts = install_contexts_for_state(&self.binding, state)?;
        let schedule = runtime
            .verified
            .as_ref()
            .filter(|verified| verified.state == *state)
            .map(|verified| &verified.schedule)
            .ok_or_else(|| invalid("BFT V2 install schedule cache is unavailable"))?;
        let context = contexts
            .iter()
            .find(|context| context.source_height == source_height)
            .ok_or_else(|| invalid("BFT V2 install source transition is not committed"))?;
        let tip = state
            .history
            .last()
            .ok_or_else(|| invalid("BFT V2 install requires a committed current head"))?;
        let tip_block = &tip.proposal.block;
        let tip_height = tip_block.height;
        let tip_id = tip_block
            .block_id()
            .map_err(|error| invalid(format!("BFT V2 install current head ID: {error}")))?;
        if context.activation_height != schedule.last_activation_height()
            || context.new_validator_set_hash != schedule.active_set().set_hash()
            || context.authority_epoch != schedule.authority_epoch()
            || tip_height.checked_add(1) != Some(schedule.height())
            || tip_id != schedule.parent_commit_ref()
            || tip_block.state_root != schedule.parent_state_root()
            || tip_block.post_manifest_hash != schedule.parent_manifest_hash()
        {
            return Err(invalid(
                "BFT V2 install transition is not the current active successor schedule",
            ));
        }
        if !context
            .active_set
            .validator(&self.binding.local_validator_id)
            .is_some_and(|validator| {
                self.binding
                    .provisioned_signing_keys
                    .iter()
                    .any(|identity| {
                        identity.validator_id == validator.validator_id
                            && identity.public_key_hex == validator.public_key_hex
                    })
            })
        {
            return Err(invalid("BFT V2 local identity is not in the activated set"));
        }
        if state
            .install_receipts
            .iter()
            .any(|receipt| receipt.body.source_height == source_height)
        {
            return Err(invalid("BFT V2 source transition was already installed"));
        }
        let nonce = random_nonce()?;
        let body = BftTransitionInstallIntentBodyV2 {
            schema_version: 2,
            binding_digest: self.binding.digest()?,
            expected_revision,
            expected_state_digest: expected_state_digest.to_string(),
            lease_generation: credential.generation,
            request_id: request_id.to_string(),
            intent_nonce: nonce,
            runtime_instance_id: runtime_instance_id.to_string(),
            transition_source_height: context.source_height,
            transition_source_parent_commit_ref: context.source_parent_commit_ref.clone(),
            transition_block_id: context.transition_block_id.clone(),
            transition_digest: context.transition_digest.clone(),
            confirmation_height: context.confirmation_height,
            confirmation_block_id: context.confirmation_block_id.clone(),
            activation_height: context.activation_height,
            install_head_height: tip_height,
            old_validator_set_hash: context.old_validator_set_hash.clone(),
            new_validator_set_hash: context.new_validator_set_hash.clone(),
            install_active_set_hash: schedule.active_set().set_hash().to_string(),
            target_authority_epoch: schedule.authority_epoch(),
            install_parent_commit_ref: tip_id,
            install_state_root: tip_block.state_root.clone(),
            install_manifest_hash: tip_block.post_manifest_hash.clone(),
            local_validator_id: self.binding.local_validator_id.clone(),
            local_signing_public_key: context
                .active_set
                .validator(&self.binding.local_validator_id)
                .ok_or_else(|| invalid("BFT V2 local key left activated set"))?
                .public_key_hex
                .clone(),
        };
        let intent = BftTransitionInstallIntentV2::sign(body, self.provider_key.as_ref())?;
        encode_bounded(&intent, BFT_SIGNER_MAX_INSTALL_RECORD_BYTES_V2)?;
        let mut candidate = state.candidate();
        candidate.pending_install = Some(intent.clone());
        self.commit_candidate(
            runtime,
            credential,
            token_digest,
            state,
            candidate,
            None,
            request,
            request_digest,
            BftSignerReplyKindV2::InstallPreparedPending {
                intent: Box::new(intent),
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn complete_transition_install(
        &self,
        runtime: &mut BftSignerServiceRuntimeV2,
        session: &mut BftSignerSessionV2,
        credential: &BftSignerLeaseCredentialV2,
        token_digest: &str,
        snapshot: BftSignerBackendSnapshotV2,
        intent: &BftTransitionInstallIntentV2,
        acknowledgement: &BftSignedTransitionInstallAckV2,
        request: &SignedBftSignerRequestV2,
        request_digest: &str,
    ) -> Result<ReplyDataV2, BftSignerErrorV2> {
        let state = &snapshot.state;
        intent.verify_for(&self.binding, &self.provider_key.public_key_hex())?;
        acknowledgement.verify_for(intent, &self.binding)?;
        let acknowledgement_digest = acknowledgement.digest()?;
        if state.pending_install.as_ref() != Some(intent) {
            if let Some(receipt) = state
                .install_receipt_for_request(&intent.body.request_id)
                .cloned()
                && receipt.body.intent_digest == intent.digest()?
                && receipt.body.acknowledgement_digest == acknowledgement_digest
            {
                return self.state_reply(
                    snapshot,
                    request,
                    request_digest,
                    BftSignerReplyKindV2::InstallPreparedCompleted {
                        receipt: Box::new(receipt),
                    },
                );
            }
            return Err(invalid(
                "BFT V2 install completion differs from durable intent",
            ));
        }
        if intent.body.lease_generation > credential.generation {
            return Err(invalid(
                "BFT V2 pending install belongs to a future lease generation",
            ));
        }
        if state.revision
            != intent
                .body
                .expected_revision
                .checked_add(1)
                .ok_or_else(|| invalid("BFT V2 pending install revision overflow"))?
        {
            return Err(invalid(
                "BFT V2 install completion differs from durable intent",
            ));
        }
        let evidence = &acknowledgement.body.evidence;
        let next_revision = state
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("BFT V2 install revision overflow"))?;
        let next_generation = credential
            .generation
            .checked_add(1)
            .ok_or_else(|| invalid("BFT V2 lease generation overflow"))?;
        let intent_digest = intent.digest()?;
        let receipt_body = BftTransitionInstallReceiptBodyV2 {
            schema_version: 2,
            binding_digest: self.binding.digest()?,
            request_id: intent.body.request_id.clone(),
            intent_digest,
            acknowledgement_digest,
            source_height: intent.body.transition_source_height,
            transition_source_parent_commit_ref: intent
                .body
                .transition_source_parent_commit_ref
                .clone(),
            transition_block_id: intent.body.transition_block_id.clone(),
            transition_digest: intent.body.transition_digest.clone(),
            confirmation_height: intent.body.confirmation_height,
            confirmation_block_id: intent.body.confirmation_block_id.clone(),
            activation_height: intent.body.activation_height,
            installed_head_height: evidence.installed_head_height,
            old_validator_set_hash: intent.body.old_validator_set_hash.clone(),
            new_validator_set_hash: intent.body.new_validator_set_hash.clone(),
            active_validator_set_hash: evidence.installed_active_set_hash.clone(),
            local_validator_id: evidence.local_validator_id.clone(),
            local_signing_public_key: evidence.local_signing_public_key.clone(),
            authority_epoch: evidence.installed_authority_epoch,
            runtime_instance_id: evidence.runtime_instance_id.clone(),
            installed_head_commit_ref: evidence.installed_head_commit_ref.clone(),
            installed_state_root: evidence.installed_state_root.clone(),
            installed_manifest_hash: evidence.installed_manifest_hash.clone(),
            wal_install_digest: evidence.wal_install_digest.clone(),
            driver_install_digest: evidence.driver_install_digest.clone(),
            lease_generation: next_generation,
            installed_revision: next_revision,
            pre_install_state_digest: intent.body.expected_state_digest.clone(),
        };
        let receipt =
            BftTransitionInstallReceiptV2::sign(receipt_body, self.provider_key.as_ref())?;
        encode_bounded(&receipt, BFT_SIGNER_MAX_INSTALL_RECORD_BYTES_V2)?;
        let mut candidate = state.candidate();
        candidate.pending_install = None;
        candidate.install_receipts.push(receipt.clone());
        let next_verified = validate_candidate_transition(
            &self.binding,
            runtime
                .verified
                .as_ref()
                .ok_or_else(|| invalid("BFT V2 verified state cache is unavailable"))?,
            &candidate,
            &self.provider_key.public_key_hex(),
        )?;
        let next_state = next_verified.state.clone();
        let next_token = random_nonce()?;
        let next_token_digest = digest_hex(BFT_SIGNER_LEASE_DOMAIN_V2, &next_token)?;
        let now = self.sample_time(runtime, credential, token_digest)?;
        let response = runtime.backend.complete_transition_install(
            credential.generation,
            token_digest,
            state.revision,
            &state.state_digest,
            now,
            &next_token_digest,
            next_state.clone(),
        );
        let snapshot = match response {
            Ok(snapshot) => snapshot,
            Err(error) => {
                runtime.poisoned = true;
                return Err(invalid(format!(
                    "BFT V2 install CAS outcome is uncertain; provider poisoned: {error}"
                )));
            }
        };
        if snapshot.state != next_state
            || snapshot.lease_generation != next_generation
            || snapshot.lease_token_digest != next_token_digest
            || snapshot.protected_time_floor_ms < runtime.observed_time_floor_ms
        {
            runtime.poisoned = true;
            return Err(invalid(
                "BFT V2 install CAS returned an invalid snapshot; provider poisoned",
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
                    "BFT V2 install committed invalid state; provider poisoned: {error}"
                )));
            }
        };
        runtime.verified = Some(verified);
        runtime.observed_generation = snapshot.lease_generation;
        runtime.observed_time_floor_ms = snapshot.protected_time_floor_ms;
        runtime.active_lease = Some(ActiveLeaseV2 {
            generation: next_generation,
            token_digest: next_token_digest,
        });
        session.lease = Some(BftSignerLeaseCredentialV2 {
            generation: next_generation,
            token: next_token.clone(),
        });
        let checkpoint = self.make_checkpoint(
            snapshot.state,
            next_generation,
            request.body.challenge.clone(),
            request_digest.to_string(),
        )?;
        Ok(ReplyDataV2 {
            result: BftSignerReplyKindV2::TransitionInstalled {
                receipt: Box::new(receipt),
            },
            checkpoint: Some(Box::new(checkpoint.clone())),
            lease_token: Some(next_token),
            generation: next_generation,
            revision: checkpoint.state.revision,
            state_digest: checkpoint.state.state_digest.clone(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn commit_candidate(
        &self,
        runtime: &mut BftSignerServiceRuntimeV2,
        credential: &BftSignerLeaseCredentialV2,
        token_digest: &str,
        previous: &BftSignerStateV2,
        candidate: BftSignerCandidateV2,
        minimum_event_time_ms: Option<i64>,
        request: &SignedBftSignerRequestV2,
        request_digest: &str,
        result: BftSignerReplyKindV2,
    ) -> Result<ReplyDataV2, BftSignerErrorV2> {
        let previous_verified = runtime
            .verified
            .as_ref()
            .ok_or_else(|| invalid("BFT V2 verified state cache is unavailable"))?;
        if previous_verified.state != *previous {
            runtime.poisoned = true;
            return Err(invalid(
                "BFT V2 mutation snapshot differs from verified cache",
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
            return Err(invalid("BFT V2 event timestamp exceeds protected time"));
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
                    "BFT V2 state CAS outcome is uncertain; provider poisoned: {error}"
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
                "BFT V2 state CAS returned an invalid snapshot; provider poisoned",
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
                    "BFT V2 state CAS committed invalid state; provider poisoned: {error}"
                )));
            }
        };
        runtime.verified = Some(verified);
        runtime.observed_time_floor_ms = snapshot.protected_time_floor_ms;
        self.state_reply(snapshot, request, request_digest, result)
    }

    pub(super) fn sample_time(
        &self,
        runtime: &mut BftSignerServiceRuntimeV2,
        credential: &BftSignerLeaseCredentialV2,
        token_digest: &str,
    ) -> Result<i64, BftSignerErrorV2> {
        let now = match runtime
            .backend
            .protected_time_ms(credential.generation, token_digest)
        {
            Ok(now) => now,
            Err(error) => {
                runtime.poisoned = true;
                return Err(invalid(format!(
                    "BFT V2 protected clock is unavailable; provider poisoned: {error}"
                )));
            }
        };
        if now < runtime.observed_time_floor_ms || now < 0 {
            runtime.poisoned = true;
            return Err(invalid("BFT V2 backend protected clock rollback"));
        }
        runtime.observed_time_floor_ms = now;
        Ok(now)
    }

    pub(super) fn state_reply(
        &self,
        snapshot: BftSignerBackendSnapshotV2,
        request: &SignedBftSignerRequestV2,
        request_digest: &str,
        result: BftSignerReplyKindV2,
    ) -> Result<ReplyDataV2, BftSignerErrorV2> {
        let checkpoint = self.make_checkpoint(
            snapshot.state,
            snapshot.lease_generation,
            request.body.challenge.clone(),
            request_digest.to_string(),
        )?;
        Ok(ReplyDataV2 {
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
        state: BftSignerStateV2,
        generation: u64,
        challenge: String,
        request_digest: String,
    ) -> Result<BftSignerCheckpointV2, BftSignerErrorV2> {
        let mut checkpoint = BftSignerCheckpointV2 {
            state,
            lease_generation: generation,
            challenge,
            request_digest,
            provider_public_key: self.provider_key.public_key_hex(),
            provider_signature_hex: String::new(),
        };
        checkpoint.provider_signature_hex = sign_preimage(
            self.provider_key.as_ref(),
            BFT_SIGNER_CHECKPOINT_DOMAIN_V2,
            &checkpoint.signable_body(),
        )?;
        Ok(checkpoint)
    }

    pub(super) fn signing_key_for_active_set(
        &self,
        set: &oasis7_consensus::tendermint_v2::BftValidatorSetV2,
    ) -> Result<&dyn BftSigningKeyHandleV2, BftSignerErrorV2> {
        let validator = set
            .validator(&self.binding.local_validator_id)
            .ok_or_else(|| invalid("BFT V2 local validator is not an active member"))?;
        let configured = self
            .binding
            .provisioned_signing_keys
            .iter()
            .find(|identity| {
                identity.validator_id == validator.validator_id
                    && identity.public_key_hex == validator.public_key_hex
            })
            .ok_or_else(|| invalid("BFT V2 active validator key is not provisioned"))?;
        self.validator_keys
            .get(&configured.public_key_hex)
            .map(|key| key.as_ref())
            .ok_or_else(|| invalid("BFT V2 active validator key handle is unavailable"))
    }

    pub(super) fn verify_observed_snapshot(
        &self,
        runtime: &mut BftSignerServiceRuntimeV2,
        snapshot: &BftSignerBackendSnapshotV2,
        credential: &BftSignerLeaseCredentialV2,
    ) -> Result<(), BftSignerErrorV2> {
        let expected_token_digest = if credential.token.is_empty() {
            None
        } else {
            Some(lease_digest(&credential.token)?)
        };
        validate_snapshot_shape(snapshot, Some(credential), expected_token_digest.as_deref())?;
        if snapshot
            .state
            .pending_install
            .as_ref()
            .is_some_and(|intent| intent.body.lease_generation > snapshot.lease_generation)
            || snapshot
                .state
                .install_receipts
                .iter()
                .any(|receipt| receipt.body.lease_generation > snapshot.lease_generation)
        {
            runtime.poisoned = true;
            return Err(invalid(
                "BFT V2 install evidence is from a future lease generation",
            ));
        }
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
                "BFT V2 backend monotonic floor or generation regressed",
            ));
        }
        runtime.observed_time_floor_ms = snapshot.protected_time_floor_ms;
        runtime.observed_generation = snapshot.lease_generation;
        runtime.verified = Some(verified);
        Ok(())
    }

    pub(super) fn verify_snapshot_against_cache(
        &self,
        runtime: &BftSignerServiceRuntimeV2,
        snapshot: &BftSignerBackendSnapshotV2,
    ) -> Result<VerifiedBftSignerStateV2, BftSignerErrorV2> {
        let verified = validate_state(
            &self.binding,
            Some(&self.provider_key.public_key_hex()),
            &snapshot.state,
        )?;
        if let Some(previous) = &runtime.verified {
            if snapshot.state.revision < previous.state.revision {
                return Err(invalid("BFT V2 backend state revision regressed"));
            }
            if snapshot.state.revision == previous.state.revision {
                if snapshot.state != previous.state {
                    return Err(invalid("BFT V2 backend changed state at the same revision"));
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
                    return Err(invalid("BFT V2 backend state skipped its legal transition"));
                }
            }
        }
        Ok(verified)
    }

    pub(super) fn release_session(
        &self,
        session: &mut BftSignerSessionV2,
    ) -> Result<(), BftSignerErrorV2> {
        session.prepared = None;
        let Some(credential) = session.lease.take() else {
            return Ok(());
        };
        let token_digest = lease_digest(&credential.token)?;
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| invalid("BFT V2 provider mutex poisoned"))?;
        if runtime.poisoned {
            return Err(invalid("BFT V2 provider is poisoned"));
        }
        let snapshot = match runtime
            .backend
            .release_lease(credential.generation, &token_digest)
        {
            Ok(snapshot) => snapshot,
            Err(error) => {
                runtime.poisoned = true;
                return Err(invalid(format!(
                    "BFT V2 lease release is uncertain; provider poisoned: {error}"
                )));
            }
        };
        if let Err(error) = validate_snapshot_shape(&snapshot, None, None) {
            runtime.poisoned = true;
            return Err(invalid(format!(
                "BFT V2 lease release returned an invalid snapshot; provider poisoned: {error}"
            )));
        }
        let verified = match self.verify_snapshot_against_cache(&runtime, &snapshot) {
            Ok(verified) => verified,
            Err(error) => {
                runtime.poisoned = true;
                return Err(invalid(format!(
                    "BFT V2 lease release snapshot is invalid; provider poisoned: {error}"
                )));
            }
        };
        if snapshot.protected_time_floor_ms < runtime.observed_time_floor_ms {
            runtime.poisoned = true;
            return Err(invalid("BFT V2 lease release clock floor regressed"));
        }
        runtime.verified = Some(verified.clone());
        runtime.observed_generation = snapshot.lease_generation;
        runtime.observed_time_floor_ms = snapshot.protected_time_floor_ms;
        if snapshot.lease_token_digest.is_empty() {
            runtime.active_lease = None;
        } else {
            runtime.active_lease = Some(ActiveLeaseV2 {
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
            "BFT V2 backend did not durably fence the released lease",
        ))
    }
}
