use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use ed25519_dalek::SigningKey;
use oasis7_consensus::tendermint_v2::{BftProposalIntentV2, BftVoteIntentV2};
use oasis7_proto::distributed_bft::v2::{BftCommitCertificateV2, BftProposalV2, BftVoteV2};

use super::binding::{BftSignerBindingV2, BftSignerErrorV2, BftSignerTrustV2, invalid};
use super::install::{
    BftLocalInstallEvidenceV2, BftSignedTransitionInstallAckV2, BftTransitionInstallIntentV2,
    BftTransitionInstallPreparationV2, BftTransitionInstallResultV2,
};
use super::state::{BftSignerCandidateV2, BftSignerCheckpointV2};
use super::wire::{
    BftSignerCommandV2, BftSignerLeaseCredentialV2, BftSignerReplyKindV2, SignedBftSignerReplyV2,
    SignedBftSignerRequestV2, encode_frame, read_frame, validate_reply,
};

const MAX_ACQUIRE_CONFLICT_RETRIES: usize = 3;

/// A remote provider pinned to one immutable V2 binding and provider identity.
/// The requester key is used only for protocol authentication and local
/// installation acknowledgements; it is distinct from every validator key.
pub struct RemoteBftSignerProviderV2 {
    address: SocketAddr,
    timeout: Duration,
    binding: BftSignerBindingV2,
    requester_key: SigningKey,
    trust: BftSignerTrustV2,
}

pub trait BftSignerProviderV2: Send + Sync {
    fn acquire(
        &self,
        binding: &BftSignerBindingV2,
    ) -> Result<Box<dyn BftSignerLeaseV2>, BftSignerErrorV2>;
}

/// One connection-bound provider lease. The same TCP stream is retained for
/// the lifetime of the lease, so copied request frames cannot move sessions.
pub trait BftSignerLeaseV2: Send {
    fn checkpoint(&mut self) -> Result<BftSignerCheckpointV2, BftSignerErrorV2>;
    fn protected_time_ms(&mut self) -> Result<i64, BftSignerErrorV2>;
    fn start_height_timer(
        &mut self,
        expected_revision: u64,
    ) -> Result<BftSignerCheckpointV2, BftSignerErrorV2>;
    fn advance(
        &mut self,
        expected_revision: u64,
        expected_state_digest: &str,
        candidate: BftSignerCandidateV2,
    ) -> Result<BftSignerCheckpointV2, BftSignerErrorV2>;
    fn sign_proposal(
        &mut self,
        expected_revision: u64,
        intent: &BftProposalIntentV2,
    ) -> Result<(BftProposalV2, BftSignerCheckpointV2), BftSignerErrorV2>;
    fn sign_vote(
        &mut self,
        expected_revision: u64,
        intent: &BftVoteIntentV2,
    ) -> Result<(BftVoteV2, BftSignerCheckpointV2), BftSignerErrorV2>;
    fn append_committed_certificate(
        &mut self,
        expected_revision: u64,
        expected_state_digest: &str,
        certificate: BftCommitCertificateV2,
    ) -> Result<BftSignerCheckpointV2, BftSignerErrorV2>;
    fn prepare_transition_install(
        &mut self,
        expected_revision: u64,
        expected_state_digest: &str,
        request_id: String,
        transition_source_height: u64,
        runtime_instance_id: String,
    ) -> Result<BftTransitionInstallPreparationV2, BftSignerErrorV2>;
    fn complete_transition_install(
        &mut self,
        intent: BftTransitionInstallIntentV2,
        evidence: BftLocalInstallEvidenceV2,
    ) -> Result<BftTransitionInstallResultV2, BftSignerErrorV2>;
}

impl RemoteBftSignerProviderV2 {
    pub fn new(
        address: SocketAddr,
        timeout: Duration,
        binding: BftSignerBindingV2,
        requester_key: SigningKey,
        trust: BftSignerTrustV2,
    ) -> Result<Self, BftSignerErrorV2> {
        binding.validate()?;
        trust.validate_for(&binding)?;
        if timeout.is_zero()
            || hex::encode(requester_key.verifying_key().to_bytes())
                != binding.requester_auth_public_key
        {
            return Err(invalid(
                "BFT V2 requester key or timeout does not match binding",
            ));
        }
        Ok(Self {
            address,
            timeout,
            binding,
            requester_key,
            trust,
        })
    }

    fn connect(&self) -> Result<TcpStream, BftSignerErrorV2> {
        let stream = TcpStream::connect_timeout(&self.address, self.timeout)
            .map_err(|error| invalid(format!("connect BFT V2 signer provider: {error}")))?;
        stream
            .set_read_timeout(Some(self.timeout))
            .and_then(|_| stream.set_write_timeout(Some(self.timeout)))
            .map_err(|error| invalid(format!("configure BFT V2 signer timeout: {error}")))?;
        Ok(stream)
    }

    fn exchange(
        &self,
        stream: &mut TcpStream,
        lease: Option<BftSignerLeaseCredentialV2>,
        command: BftSignerCommandV2,
    ) -> Result<(SignedBftSignerRequestV2, SignedBftSignerReplyV2), BftSignerErrorV2> {
        let request = SignedBftSignerRequestV2::sign(
            &self.binding,
            &self.requester_key,
            lease,
            command,
            random_nonce()?,
        )?;
        let frame = encode_frame(&request)?;
        std::io::Write::write_all(stream, &frame)
            .map_err(|error| invalid(format!("write BFT V2 signer request: {error}")))?;
        let reply: SignedBftSignerReplyV2 = read_frame(stream)?;
        validate_reply(&self.binding, &self.trust, &request, &reply)?;
        Ok((request, reply))
    }

    fn checkpoint_from_reply(
        &self,
        request: &SignedBftSignerRequestV2,
        reply: &SignedBftSignerReplyV2,
        expected_generation: u64,
    ) -> Result<BftSignerCheckpointV2, BftSignerErrorV2> {
        let checkpoint = reply
            .body
            .checkpoint
            .as_ref()
            .ok_or_else(|| invalid("BFT V2 provider omitted its signed checkpoint"))?;
        let request_digest = super::wire::request_digest(request)?;
        let verified = checkpoint.validate(
            &self.binding,
            &request.body.challenge,
            &request_digest,
            expected_generation,
            &self.trust.provider_public_key,
        )?;
        if checkpoint.state.revision != reply.body.state_revision
            || checkpoint.state.state_digest != reply.body.state_digest
            || verified.state != checkpoint.state
        {
            return Err(invalid("BFT V2 reply and checkpoint state differ"));
        }
        Ok(checkpoint.as_ref().clone())
    }

    fn checked_result(
        reply: &SignedBftSignerReplyV2,
    ) -> Result<&BftSignerReplyKindV2, BftSignerErrorV2> {
        if let BftSignerReplyKindV2::Rejected { message } = &reply.body.result {
            return Err(invalid(format!(
                "BFT V2 provider rejected request: {message}"
            )));
        }
        Ok(&reply.body.result)
    }
}

impl BftSignerProviderV2 for RemoteBftSignerProviderV2 {
    fn acquire(
        &self,
        binding: &BftSignerBindingV2,
    ) -> Result<Box<dyn BftSignerLeaseV2>, BftSignerErrorV2> {
        if binding != &self.binding {
            return Err(invalid("BFT V2 acquire binding differs from provider pin"));
        }
        for attempt in 0..=MAX_ACQUIRE_CONFLICT_RETRIES {
            let mut stream = self.connect()?;
            let (_, prepare_reply) = self.exchange(
                &mut stream,
                None,
                BftSignerCommandV2::PrepareAcquire {
                    binding: Box::new(binding.clone()),
                },
            )?;
            let (expected_generation, prepare_nonce) = match Self::checked_result(&prepare_reply)? {
                BftSignerReplyKindV2::Prepared {
                    expected_generation,
                    prepare_nonce,
                } if prepare_reply.body.checkpoint.is_none()
                    && prepare_reply.body.lease_token.is_none() =>
                {
                    (*expected_generation, prepare_nonce.clone())
                }
                _ => return Err(invalid("BFT V2 prepare response has the wrong shape")),
            };
            if prepare_reply.body.lease_generation != expected_generation {
                return Err(invalid("BFT V2 prepare generation is inconsistent"));
            }
            let (request, reply) = self.exchange(
                &mut stream,
                None,
                BftSignerCommandV2::Acquire {
                    binding: Box::new(binding.clone()),
                    expected_generation,
                    prepare_nonce,
                },
            )?;
            match Self::checked_result(&reply)? {
                BftSignerReplyKindV2::Acquired => {
                    let expected_next = expected_generation
                        .checked_add(1)
                        .ok_or_else(|| invalid("BFT V2 lease generation overflow"))?;
                    let token = reply
                        .body
                        .lease_token
                        .as_ref()
                        .ok_or_else(|| invalid("BFT V2 acquire omitted lease token"))?;
                    validate_token(token)?;
                    let checkpoint = self.checkpoint_from_reply(&request, &reply, expected_next)?;
                    return Ok(Box::new(RemoteBftSignerLeaseV2 {
                        stream,
                        binding: self.binding.clone(),
                        requester_key: self.requester_key.clone(),
                        trust: self.trust.clone(),
                        credential: BftSignerLeaseCredentialV2 {
                            generation: expected_next,
                            token: token.clone(),
                        },
                        checkpoint,
                    }));
                }
                BftSignerReplyKindV2::AcquireConflict {
                    current_generation,
                    lease_active,
                } if reply.body.checkpoint.is_none() && reply.body.lease_token.is_none() => {
                    if *current_generation != reply.body.lease_generation
                        || *current_generation <= expected_generation
                    {
                        return Err(invalid(
                            "BFT V2 acquire conflict did not advance generation",
                        ));
                    }
                    if *lease_active || attempt == MAX_ACQUIRE_CONFLICT_RETRIES {
                        return Err(invalid("BFT V2 provider already has an active owner"));
                    }
                }
                _ => return Err(invalid("BFT V2 acquire response has the wrong shape")),
            }
        }
        Err(invalid("BFT V2 acquire retry bound exhausted"))
    }
}

pub struct RemoteBftSignerLeaseV2 {
    stream: TcpStream,
    binding: BftSignerBindingV2,
    requester_key: SigningKey,
    trust: BftSignerTrustV2,
    credential: BftSignerLeaseCredentialV2,
    checkpoint: BftSignerCheckpointV2,
}

impl RemoteBftSignerLeaseV2 {
    fn command(
        &mut self,
        command: BftSignerCommandV2,
    ) -> Result<(SignedBftSignerRequestV2, SignedBftSignerReplyV2), BftSignerErrorV2> {
        let request = SignedBftSignerRequestV2::sign(
            &self.binding,
            &self.requester_key,
            Some(self.credential.clone()),
            command,
            random_nonce()?,
        )?;
        let frame = encode_frame(&request)?;
        std::io::Write::write_all(&mut self.stream, &frame)
            .map_err(|error| invalid(format!("write BFT V2 lease request: {error}")))?;
        let reply: SignedBftSignerReplyV2 = read_frame(&mut self.stream)?;
        validate_reply(&self.binding, &self.trust, &request, &reply)?;
        let install_rotates_lease = matches!(
            (&request.body.command, &reply.body.result),
            (
                BftSignerCommandV2::CompleteTransitionInstall { .. },
                BftSignerReplyKindV2::TransitionInstalled { .. }
            )
        );
        if install_rotates_lease {
            let next_generation = self
                .credential
                .generation
                .checked_add(1)
                .ok_or_else(|| invalid("BFT V2 install lease generation overflow"))?;
            let token =
                reply.body.lease_token.as_deref().ok_or_else(|| {
                    invalid("BFT V2 install response omitted rotated lease token")
                })?;
            validate_token(token)?;
            if reply.body.lease_generation != next_generation || token == self.credential.token {
                return Err(invalid("BFT V2 install response did not rotate its lease"));
            }
        } else if reply.body.lease_generation != self.credential.generation
            || reply
                .body
                .lease_token
                .as_deref()
                .is_some_and(|token| token != self.credential.token)
        {
            return Err(invalid(
                "BFT V2 leased reply changed its generation or token",
            ));
        }
        Ok((request, reply))
    }

    fn result_checkpoint(
        &mut self,
        request: &SignedBftSignerRequestV2,
        reply: &SignedBftSignerReplyV2,
        expected: fn(&BftSignerReplyKindV2) -> bool,
    ) -> Result<BftSignerCheckpointV2, BftSignerErrorV2> {
        RemoteBftSignerProviderV2::checked_result(reply)?;
        if !expected(&reply.body.result) {
            return Err(invalid("BFT V2 signer response has the wrong result kind"));
        }
        let checkpoint = self.verified_checkpoint(request, reply, self.credential.generation)?;
        self.checkpoint = checkpoint;
        Ok(self.checkpoint.clone())
    }

    fn verified_checkpoint(
        &self,
        request: &SignedBftSignerRequestV2,
        reply: &SignedBftSignerReplyV2,
        expected_generation: u64,
    ) -> Result<BftSignerCheckpointV2, BftSignerErrorV2> {
        let checkpoint = reply
            .body
            .checkpoint
            .as_ref()
            .ok_or_else(|| invalid("BFT V2 signer response omitted checkpoint"))?;
        let digest = super::wire::request_digest(request)?;
        let verified = checkpoint.validate(
            &self.binding,
            &request.body.challenge,
            &digest,
            expected_generation,
            &self.trust.provider_public_key,
        )?;
        if checkpoint.state.revision != reply.body.state_revision
            || checkpoint.state.state_digest != reply.body.state_digest
            || verified.state != checkpoint.state
        {
            return Err(invalid(
                "BFT V2 leased reply checkpoint differs from response",
            ));
        }
        Ok(checkpoint.as_ref().clone())
    }
}

impl BftSignerLeaseV2 for RemoteBftSignerLeaseV2 {
    fn checkpoint(&mut self) -> Result<BftSignerCheckpointV2, BftSignerErrorV2> {
        let (request, reply) = self.command(BftSignerCommandV2::Checkpoint)?;
        self.result_checkpoint(&request, &reply, |kind| {
            matches!(kind, BftSignerReplyKindV2::Checkpoint)
        })
    }

    fn protected_time_ms(&mut self) -> Result<i64, BftSignerErrorV2> {
        let (_, reply) = self.command(BftSignerCommandV2::ProtectedTime)?;
        RemoteBftSignerProviderV2::checked_result(&reply)?;
        match reply.body.result {
            BftSignerReplyKindV2::ProtectedTime { time_ms }
                if reply.body.checkpoint.is_none()
                    && reply.body.lease_token.is_none()
                    && reply.body.state_revision == self.checkpoint.state.revision
                    && reply.body.state_digest == self.checkpoint.state.state_digest =>
            {
                Ok(time_ms)
            }
            _ => Err(invalid(
                "BFT V2 protected-time response shape or checkpoint mismatch",
            )),
        }
    }

    fn start_height_timer(
        &mut self,
        expected_revision: u64,
    ) -> Result<BftSignerCheckpointV2, BftSignerErrorV2> {
        let (request, reply) =
            self.command(BftSignerCommandV2::StartHeightTimer { expected_revision })?;
        self.result_checkpoint(&request, &reply, |kind| {
            matches!(kind, BftSignerReplyKindV2::HeightTimerStarted)
        })
    }

    fn advance(
        &mut self,
        expected_revision: u64,
        expected_state_digest: &str,
        candidate: BftSignerCandidateV2,
    ) -> Result<BftSignerCheckpointV2, BftSignerErrorV2> {
        let (request, reply) = self.command(BftSignerCommandV2::Advance {
            expected_revision,
            expected_state_digest: expected_state_digest.to_string(),
            candidate: Box::new(candidate),
        })?;
        self.result_checkpoint(&request, &reply, |kind| {
            matches!(kind, BftSignerReplyKindV2::Advanced)
        })
    }

    fn sign_proposal(
        &mut self,
        expected_revision: u64,
        intent: &BftProposalIntentV2,
    ) -> Result<(BftProposalV2, BftSignerCheckpointV2), BftSignerErrorV2> {
        let (request, reply) = self.command(BftSignerCommandV2::SignProposal {
            expected_revision,
            intent: Box::new(intent.clone()),
        })?;
        RemoteBftSignerProviderV2::checked_result(&reply)?;
        let BftSignerReplyKindV2::ProposalSigned { proposal } = &reply.body.result else {
            return Err(invalid("BFT V2 proposal response has the wrong kind"));
        };
        let checkpoint = self.result_checkpoint(&request, &reply, |kind| {
            matches!(kind, BftSignerReplyKindV2::ProposalSigned { .. })
        })?;
        Ok((proposal.as_ref().clone(), checkpoint))
    }

    fn sign_vote(
        &mut self,
        expected_revision: u64,
        intent: &BftVoteIntentV2,
    ) -> Result<(BftVoteV2, BftSignerCheckpointV2), BftSignerErrorV2> {
        let (request, reply) = self.command(BftSignerCommandV2::SignVote {
            expected_revision,
            intent: Box::new(intent.clone()),
        })?;
        RemoteBftSignerProviderV2::checked_result(&reply)?;
        let BftSignerReplyKindV2::VoteSigned { vote } = &reply.body.result else {
            return Err(invalid("BFT V2 vote response has the wrong kind"));
        };
        let checkpoint = self.result_checkpoint(&request, &reply, |kind| {
            matches!(kind, BftSignerReplyKindV2::VoteSigned { .. })
        })?;
        Ok((vote.as_ref().clone(), checkpoint))
    }

    fn append_committed_certificate(
        &mut self,
        expected_revision: u64,
        expected_state_digest: &str,
        certificate: BftCommitCertificateV2,
    ) -> Result<BftSignerCheckpointV2, BftSignerErrorV2> {
        let (request, reply) = self.command(BftSignerCommandV2::AppendCommittedCertificate {
            expected_revision,
            expected_state_digest: expected_state_digest.to_string(),
            certificate: Box::new(certificate),
        })?;
        self.result_checkpoint(&request, &reply, |kind| {
            matches!(kind, BftSignerReplyKindV2::CertificateAppended)
        })
    }

    fn prepare_transition_install(
        &mut self,
        expected_revision: u64,
        expected_state_digest: &str,
        request_id: String,
        transition_source_height: u64,
        runtime_instance_id: String,
    ) -> Result<BftTransitionInstallPreparationV2, BftSignerErrorV2> {
        let (request, reply) = self.command(BftSignerCommandV2::PrepareTransitionInstall {
            expected_revision,
            expected_state_digest: expected_state_digest.to_string(),
            request_id,
            transition_source_height,
            runtime_instance_id,
        })?;
        RemoteBftSignerProviderV2::checked_result(&reply)?;
        match &reply.body.result {
            BftSignerReplyKindV2::InstallPreparedPending { intent } => {
                let checkpoint = self.result_checkpoint(&request, &reply, |kind| {
                    matches!(kind, BftSignerReplyKindV2::InstallPreparedPending { .. })
                })?;
                intent.verify_for(&self.binding, &self.trust.provider_public_key)?;
                Ok(BftTransitionInstallPreparationV2::Pending {
                    intent: intent.as_ref().clone(),
                    checkpoint,
                })
            }
            BftSignerReplyKindV2::InstallPreparedCompleted { receipt } => {
                let checkpoint = self.result_checkpoint(&request, &reply, |kind| {
                    matches!(kind, BftSignerReplyKindV2::InstallPreparedCompleted { .. })
                })?;
                receipt.verify_for(&self.binding, &self.trust.provider_public_key)?;
                Ok(BftTransitionInstallPreparationV2::Completed {
                    receipt: receipt.as_ref().clone(),
                    checkpoint,
                })
            }
            _ => Err(invalid(
                "BFT V2 install preparation response has wrong kind",
            )),
        }
    }

    fn complete_transition_install(
        &mut self,
        intent: BftTransitionInstallIntentV2,
        evidence: BftLocalInstallEvidenceV2,
    ) -> Result<BftTransitionInstallResultV2, BftSignerErrorV2> {
        intent.verify_for(&self.binding, &self.trust.provider_public_key)?;
        evidence.validate_for(&intent, &self.binding)?;
        let acknowledgement = BftSignedTransitionInstallAckV2::sign(
            &intent,
            evidence.clone(),
            &self.binding,
            &self.trust.provider_public_key,
            &self.requester_key,
        )?;
        let expected_intent_digest = intent.digest()?;
        let expected_acknowledgement_digest = acknowledgement.digest()?;
        let (request, reply) = self.command(BftSignerCommandV2::CompleteTransitionInstall {
            intent: Box::new(intent.clone()),
            acknowledgement: Box::new(acknowledgement.clone()),
        })?;
        RemoteBftSignerProviderV2::checked_result(&reply)?;
        let (receipt, checkpoint, next_credential) = match &reply.body.result {
            BftSignerReplyKindV2::TransitionInstalled { receipt } => {
                let generation = self
                    .credential
                    .generation
                    .checked_add(1)
                    .ok_or_else(|| invalid("BFT V2 lease generation overflow"))?;
                let token = reply.body.lease_token.as_ref().ok_or_else(|| {
                    invalid("BFT V2 install response omitted rotated lease token")
                })?;
                validate_token(token)?;
                let checkpoint = self.verified_checkpoint(&request, &reply, generation)?;
                if reply.body.lease_generation != generation || token == &self.credential.token {
                    return Err(invalid("BFT V2 install response did not rotate its lease"));
                }
                (
                    receipt,
                    checkpoint,
                    Some(BftSignerLeaseCredentialV2 {
                        generation,
                        token: token.clone(),
                    }),
                )
            }
            BftSignerReplyKindV2::InstallPreparedCompleted { receipt } => {
                if reply.body.lease_generation != self.credential.generation
                    || reply
                        .body
                        .lease_token
                        .as_deref()
                        .is_some_and(|token| token != self.credential.token)
                {
                    return Err(invalid(
                        "BFT V2 completed install response changed its lease",
                    ));
                }
                let checkpoint =
                    self.verified_checkpoint(&request, &reply, self.credential.generation)?;
                (receipt, checkpoint, None)
            }
            _ => return Err(invalid("BFT V2 install completion response has wrong kind")),
        };
        validate_install_completion_receipt(
            receipt,
            &intent,
            &acknowledgement,
            &evidence,
            &checkpoint,
            &self.binding,
            &self.trust.provider_public_key,
            &expected_intent_digest,
            &expected_acknowledgement_digest,
            next_credential.is_some(),
        )?;
        if let Some(credential) = next_credential {
            self.credential = credential;
        }
        self.checkpoint = checkpoint.clone();
        Ok(BftTransitionInstallResultV2 {
            receipt: receipt.as_ref().clone(),
            checkpoint,
        })
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_install_completion_receipt(
    receipt: &super::install::BftTransitionInstallReceiptV2,
    intent: &BftTransitionInstallIntentV2,
    acknowledgement: &BftSignedTransitionInstallAckV2,
    evidence: &BftLocalInstallEvidenceV2,
    checkpoint: &BftSignerCheckpointV2,
    binding: &BftSignerBindingV2,
    provider_public_key: &str,
    intent_digest: &str,
    acknowledgement_digest: &str,
    just_installed: bool,
) -> Result<(), BftSignerErrorV2> {
    receipt.verify_for(binding, provider_public_key)?;
    acknowledgement.verify_for(intent, binding)?;
    if intent.digest()? != intent_digest
        || acknowledgement.digest()? != acknowledgement_digest
        || acknowledgement.body.evidence != *evidence
        || receipt.body.binding_digest != binding.digest()?
        || receipt.body.request_id != intent.body.request_id
        || receipt.body.intent_digest != intent_digest
        || receipt.body.acknowledgement_digest != acknowledgement_digest
        || receipt.body.source_height != intent.body.transition_source_height
        || receipt.body.transition_source_parent_commit_ref
            != intent.body.transition_source_parent_commit_ref
        || receipt.body.transition_block_id != intent.body.transition_block_id
        || receipt.body.transition_digest != intent.body.transition_digest
        || receipt.body.confirmation_height != intent.body.confirmation_height
        || receipt.body.confirmation_block_id != intent.body.confirmation_block_id
        || receipt.body.activation_height != intent.body.activation_height
        || evidence.installed_head_height != intent.body.install_head_height
        || evidence.installed_active_set_hash != intent.body.install_active_set_hash
        || receipt.body.old_validator_set_hash != intent.body.old_validator_set_hash
        || receipt.body.new_validator_set_hash != intent.body.new_validator_set_hash
        || receipt.body.active_validator_set_hash != evidence.installed_active_set_hash
        || receipt.body.local_validator_id != evidence.local_validator_id
        || receipt.body.local_signing_public_key != evidence.local_signing_public_key
        || receipt.body.authority_epoch != evidence.installed_authority_epoch
        || receipt.body.runtime_instance_id != evidence.runtime_instance_id
        || receipt.body.installed_head_height != evidence.installed_head_height
        || receipt.body.installed_head_commit_ref != evidence.installed_head_commit_ref
        || receipt.body.installed_state_root != evidence.installed_state_root
        || receipt.body.installed_manifest_hash != evidence.installed_manifest_hash
        || receipt.body.wal_install_digest != evidence.wal_install_digest
        || receipt.body.driver_install_digest != evidence.driver_install_digest
        || receipt.body.pre_install_state_digest != intent.body.expected_state_digest
        || !checkpoint
            .state
            .install_receipts
            .iter()
            .any(|stored| stored == receipt)
        || (just_installed
            && (receipt.body.lease_generation != checkpoint.lease_generation
                || receipt.body.installed_revision != checkpoint.state.revision))
    {
        return Err(invalid(
            "BFT V2 install receipt differs from submitted intent, ACK, evidence, or checkpoint",
        ));
    }
    Ok(())
}

fn random_nonce() -> Result<String, BftSignerErrorV2> {
    #[cfg(unix)]
    {
        use std::io::Read;
        let mut bytes = [0u8; 32];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(&mut bytes))
            .map_err(|error| invalid(format!("BFT V2 client CSPRNG unavailable: {error}")))?;
        Ok(hex::encode(bytes))
    }
    #[cfg(not(unix))]
    {
        Err(invalid(
            "BFT V2 remote client currently requires a Unix CSPRNG",
        ))
    }
}

fn validate_token(token: &str) -> Result<(), BftSignerErrorV2> {
    if token.len() != 64
        || token
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(invalid("BFT V2 lease token is noncanonical"));
    }
    Ok(())
}
