use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use ed25519_dalek::SigningKey;
use oasis7_consensus::tendermint_v3::BftPublicProposalV3;
use oasis7_consensus::tendermint_v3::{BftProposalIntentV3, BftVoteIntentV3};
use oasis7_formal_protocol::bft::FormalBftCertifiedHeaderV3;
use oasis7_proto::distributed_bft::v3::BftVoteV3;

use super::binding::{BftSignerBindingV3, BftSignerErrorV3, BftSignerTrustV3, invalid};
use super::state::{BftSignerCandidateV3, BftSignerCheckpointV3};
use super::wire::{
    BftSignerCommandV3, BftSignerLeaseCredentialV3, BftSignerReplyKindV3, SignedBftSignerReplyV3,
    SignedBftSignerRequestV3, encode_frame, read_frame, validate_reply,
};

use super::install::*;

const MAX_ACQUIRE_CONFLICT_RETRIES: usize = 3;

/// A remote provider pinned to one immutable V3 binding and provider identity.
/// The requester key is used only for protocol authentication and local
/// installation acknowledgements; it is distinct from every validator key.
pub struct RemoteBftSignerProviderV3 {
    address: SocketAddr,
    timeout: Duration,
    binding: BftSignerBindingV3,
    requester_key: SigningKey,
    trust: BftSignerTrustV3,
}

pub trait BftSignerProviderV3: Send + Sync {
    fn acquire(
        &self,
        binding: &BftSignerBindingV3,
    ) -> Result<Box<dyn BftSignerLeaseV3>, BftSignerErrorV3>;
}

/// One connection-bound provider lease. The same TCP stream is retained for
/// the lifetime of the lease, so copied request frames cannot move sessions.
pub trait BftSignerLeaseV3: Send {
    fn enable_membership(
        &mut self,
        _expected_revision: u64,
        _expected_state_digest: &str,
    ) -> Result<BftSignerCheckpointV3, BftSignerErrorV3> {
        Err(invalid("V3 membership extension unsupported"))
    }
    fn prepare_transition_install(
        &mut self,
        _expected_revision: u64,
        _expected_state_digest: &str,
        _request_id: String,
        _transition_source_height: u64,
        _runtime_instance_id: String,
    ) -> Result<BftTransitionInstallPreparationV3, BftSignerErrorV3> {
        Err(invalid("V3 membership extension unsupported"))
    }
    fn complete_transition_install(
        &mut self,
        _intent: BftTransitionInstallIntentV3,
        _evidence: BftLocalInstallEvidenceV3,
    ) -> Result<BftTransitionInstallResultV3, BftSignerErrorV3> {
        Err(invalid("V3 membership extension unsupported"))
    }

    /// Typed key-possession proof only, never read permission or voting install.
    fn sign_candidate_read_request(
        &mut self,
        _body: &oasis7_proto::distributed_bft::v3::BftCandidateReadBodyV3,
    ) -> Result<oasis7_proto::distributed_bft::v3::BftCandidateReadProofV3, BftSignerErrorV3> {
        Err(invalid(
            "candidate read key proofs are unsupported by this lease",
        ))
    }
    fn checkpoint(&mut self) -> Result<BftSignerCheckpointV3, BftSignerErrorV3>;
    fn protected_time_ms(&mut self) -> Result<i64, BftSignerErrorV3>;
    fn start_height_timer(
        &mut self,
        expected_revision: u64,
    ) -> Result<BftSignerCheckpointV3, BftSignerErrorV3>;
    fn advance(
        &mut self,
        expected_revision: u64,
        expected_state_digest: &str,
        candidate: BftSignerCandidateV3,
    ) -> Result<BftSignerCheckpointV3, BftSignerErrorV3>;
    fn sign_proposal(
        &mut self,
        expected_revision: u64,
        intent: &BftProposalIntentV3,
    ) -> Result<(BftPublicProposalV3, BftSignerCheckpointV3), BftSignerErrorV3>;
    fn sign_vote(
        &mut self,
        expected_revision: u64,
        intent: &BftVoteIntentV3,
    ) -> Result<(BftVoteV3, BftSignerCheckpointV3), BftSignerErrorV3>;
    fn confirm_applied(
        &mut self,
        expected_revision: u64,
        expected_state_digest: &str,
        height: u64,
        header_id: &str,
        driver_install_digest: &str,
    ) -> Result<BftSignerCheckpointV3, BftSignerErrorV3>;
    fn append_committed_certificate(
        &mut self,
        expected_revision: u64,
        expected_state_digest: &str,
        certificate: FormalBftCertifiedHeaderV3,
    ) -> Result<BftSignerCheckpointV3, BftSignerErrorV3>;
}

impl RemoteBftSignerProviderV3 {
    pub fn new(
        address: SocketAddr,
        timeout: Duration,
        binding: BftSignerBindingV3,
        requester_key: SigningKey,
        trust: BftSignerTrustV3,
    ) -> Result<Self, BftSignerErrorV3> {
        binding.validate()?;
        trust.validate_for(&binding)?;
        if timeout.is_zero()
            || hex::encode(requester_key.verifying_key().to_bytes())
                != binding.requester_auth_public_key
        {
            return Err(invalid(
                "BFT V3 requester key or timeout does not match binding",
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

    fn connect(&self) -> Result<TcpStream, BftSignerErrorV3> {
        let stream = TcpStream::connect_timeout(&self.address, self.timeout)
            .map_err(|error| invalid(format!("connect BFT V3 signer provider: {error}")))?;
        stream
            .set_read_timeout(Some(self.timeout))
            .and_then(|_| stream.set_write_timeout(Some(self.timeout)))
            .map_err(|error| invalid(format!("configure BFT V3 signer timeout: {error}")))?;
        Ok(stream)
    }

    fn exchange(
        &self,
        stream: &mut TcpStream,
        lease: Option<BftSignerLeaseCredentialV3>,
        command: BftSignerCommandV3,
    ) -> Result<(SignedBftSignerRequestV3, SignedBftSignerReplyV3), BftSignerErrorV3> {
        let request = SignedBftSignerRequestV3::sign(
            &self.binding,
            &self.requester_key,
            lease,
            command,
            random_nonce()?,
        )?;
        let frame = encode_frame(&request)?;
        std::io::Write::write_all(stream, &frame)
            .map_err(|error| invalid(format!("write BFT V3 signer request: {error}")))?;
        let reply: SignedBftSignerReplyV3 = read_frame(stream)?;
        validate_reply(&self.binding, &self.trust, &request, &reply)?;
        Ok((request, reply))
    }

    fn checkpoint_from_reply(
        &self,
        request: &SignedBftSignerRequestV3,
        reply: &SignedBftSignerReplyV3,
        expected_generation: u64,
    ) -> Result<BftSignerCheckpointV3, BftSignerErrorV3> {
        let checkpoint = reply
            .body
            .checkpoint
            .as_ref()
            .ok_or_else(|| invalid("BFT V3 provider omitted its signed checkpoint"))?;
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
            return Err(invalid("BFT V3 reply and checkpoint state differ"));
        }
        Ok(checkpoint.as_ref().clone())
    }

    fn checked_result(
        reply: &SignedBftSignerReplyV3,
    ) -> Result<&BftSignerReplyKindV3, BftSignerErrorV3> {
        if let BftSignerReplyKindV3::Rejected { message } = &reply.body.result {
            return Err(invalid(format!(
                "BFT V3 provider rejected request: {message}"
            )));
        }
        Ok(&reply.body.result)
    }
}

impl BftSignerProviderV3 for RemoteBftSignerProviderV3 {
    fn acquire(
        &self,
        binding: &BftSignerBindingV3,
    ) -> Result<Box<dyn BftSignerLeaseV3>, BftSignerErrorV3> {
        if binding.digest()? != self.binding.digest()? {
            return Err(invalid("BFT V3 acquire binding differs from provider pin"));
        }
        for attempt in 0..=MAX_ACQUIRE_CONFLICT_RETRIES {
            let mut stream = self.connect()?;
            let (_, prepare_reply) = self.exchange(
                &mut stream,
                None,
                BftSignerCommandV3::PrepareAcquire {
                    binding_digest: binding.digest()?,
                },
            )?;
            let (expected_generation, prepare_nonce) = match Self::checked_result(&prepare_reply)? {
                BftSignerReplyKindV3::Prepared {
                    expected_generation,
                    prepare_nonce,
                } if prepare_reply.body.checkpoint.is_none()
                    && prepare_reply.body.lease_token.is_none() =>
                {
                    (*expected_generation, prepare_nonce.clone())
                }
                _ => return Err(invalid("BFT V3 prepare response has the wrong shape")),
            };
            if prepare_reply.body.lease_generation != expected_generation {
                return Err(invalid("BFT V3 prepare generation is inconsistent"));
            }
            let (request, reply) = self.exchange(
                &mut stream,
                None,
                BftSignerCommandV3::Acquire {
                    binding_digest: binding.digest()?,
                    expected_generation,
                    prepare_nonce,
                },
            )?;
            match Self::checked_result(&reply)? {
                BftSignerReplyKindV3::Acquired => {
                    let expected_next = expected_generation
                        .checked_add(1)
                        .ok_or_else(|| invalid("BFT V3 lease generation overflow"))?;
                    let token = reply
                        .body
                        .lease_token
                        .as_ref()
                        .ok_or_else(|| invalid("BFT V3 acquire omitted lease token"))?;
                    validate_token(token)?;
                    let checkpoint = self.checkpoint_from_reply(&request, &reply, expected_next)?;
                    return Ok(Box::new(RemoteBftSignerLeaseV3 {
                        stream,
                        binding: self.binding.clone(),
                        requester_key: self.requester_key.clone(),
                        trust: self.trust.clone(),
                        credential: BftSignerLeaseCredentialV3 {
                            generation: expected_next,
                            token: token.clone(),
                        },
                        checkpoint,
                    }));
                }
                BftSignerReplyKindV3::AcquireConflict {
                    current_generation,
                    lease_active,
                } if reply.body.checkpoint.is_none() && reply.body.lease_token.is_none() => {
                    if *current_generation != reply.body.lease_generation
                        || *current_generation < expected_generation
                        || (*current_generation == expected_generation && !*lease_active)
                    {
                        return Err(invalid(
                            "BFT V3 acquire conflict did not advance generation",
                        ));
                    }
                    if *lease_active || attempt == MAX_ACQUIRE_CONFLICT_RETRIES {
                        return Err(invalid("BFT V3 provider already has an active owner"));
                    }
                }
                _ => return Err(invalid("BFT V3 acquire response has the wrong shape")),
            }
        }
        Err(invalid("BFT V3 acquire retry bound exhausted"))
    }
}

pub struct RemoteBftSignerLeaseV3 {
    stream: TcpStream,
    binding: BftSignerBindingV3,
    requester_key: SigningKey,
    trust: BftSignerTrustV3,
    credential: BftSignerLeaseCredentialV3,
    checkpoint: BftSignerCheckpointV3,
}

impl RemoteBftSignerLeaseV3 {
    fn command(
        &mut self,
        command: BftSignerCommandV3,
    ) -> Result<(SignedBftSignerRequestV3, SignedBftSignerReplyV3), BftSignerErrorV3> {
        let request = SignedBftSignerRequestV3::sign(
            &self.binding,
            &self.requester_key,
            Some(self.credential.clone()),
            command,
            random_nonce()?,
        )?;
        let frame = encode_frame(&request)?;
        std::io::Write::write_all(&mut self.stream, &frame)
            .map_err(|error| invalid(format!("write BFT V3 lease request: {error}")))?;
        let reply: SignedBftSignerReplyV3 = read_frame(&mut self.stream)?;
        validate_reply(&self.binding, &self.trust, &request, &reply)?;
        if reply.body.lease_generation != self.credential.generation
            || reply
                .body
                .lease_token
                .as_deref()
                .is_some_and(|token| token != self.credential.token)
        {
            return Err(invalid(
                "BFT V3 leased reply changed its generation or token",
            ));
        }
        Ok((request, reply))
    }

    fn result_checkpoint(
        &mut self,
        request: &SignedBftSignerRequestV3,
        reply: &SignedBftSignerReplyV3,
        expected: fn(&BftSignerReplyKindV3) -> bool,
    ) -> Result<BftSignerCheckpointV3, BftSignerErrorV3> {
        RemoteBftSignerProviderV3::checked_result(reply)?;
        if !expected(&reply.body.result) {
            return Err(invalid("BFT V3 signer response has the wrong result kind"));
        }
        let checkpoint = self.verified_checkpoint(request, reply, self.credential.generation)?;
        self.checkpoint = checkpoint;
        Ok(self.checkpoint.clone())
    }

    fn verified_checkpoint(
        &self,
        request: &SignedBftSignerRequestV3,
        reply: &SignedBftSignerReplyV3,
        expected_generation: u64,
    ) -> Result<BftSignerCheckpointV3, BftSignerErrorV3> {
        let checkpoint = reply
            .body
            .checkpoint
            .as_ref()
            .ok_or_else(|| invalid("BFT V3 signer response omitted checkpoint"))?;
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
                "BFT V3 leased reply checkpoint differs from response",
            ));
        }
        Ok(checkpoint.as_ref().clone())
    }
}

impl BftSignerLeaseV3 for RemoteBftSignerLeaseV3 {
    fn enable_membership(
        &mut self,
        expected_revision: u64,
        expected_state_digest: &str,
    ) -> Result<BftSignerCheckpointV3, BftSignerErrorV3> {
        let (request, reply) = self.command(BftSignerCommandV3::EnableMembership {
            expected_revision,
            expected_state_digest: expected_state_digest.into(),
        })?;
        let checkpoint = self.result_checkpoint(&request, &reply, |r| {
            matches!(r, BftSignerReplyKindV3::MembershipEnabled)
        })?;
        if checkpoint.state.membership_install.is_none() {
            return Err(invalid("V3 enable response omitted extension"));
        }
        self.checkpoint = checkpoint.clone();
        Ok(checkpoint)
    }
    fn prepare_transition_install(
        &mut self,
        expected_revision: u64,
        expected_state_digest: &str,
        request_id: String,
        transition_source_height: u64,
        runtime_instance_id: String,
    ) -> Result<BftTransitionInstallPreparationV3, BftSignerErrorV3> {
        let (request, reply) = self.command(BftSignerCommandV3::PrepareTransitionInstall {
            expected_revision,
            expected_state_digest: expected_state_digest.to_string(),
            request_id: request_id.clone(),
            transition_source_height,
            runtime_instance_id: runtime_instance_id.clone(),
        })?;
        RemoteBftSignerProviderV3::checked_result(&reply)?;
        match &reply.body.result {
            BftSignerReplyKindV3::InstallPreparedPending { intent } => {
                let checkpoint = self.result_checkpoint(&request, &reply, |kind| {
                    matches!(kind, BftSignerReplyKindV3::InstallPreparedPending { .. })
                })?;
                intent.verify_for(&self.binding, &self.trust.provider_public_key)?;
                let body = &intent.body;
                if body.expected_revision != expected_revision
                    || body.expected_state_digest != expected_state_digest
                    || body.request_id != request_id
                    || body.transition_source_height != transition_source_height
                    || body.runtime_instance_id != runtime_instance_id
                    || checkpoint
                        .state
                        .membership_install
                        .as_ref()
                        .and_then(|m| m.pending.as_ref())
                        != Some(intent.as_ref())
                {
                    return Err(invalid("V3 prepared intent differs from original request"));
                }
                Ok(BftTransitionInstallPreparationV3::Pending {
                    intent: intent.as_ref().clone(),
                    checkpoint,
                })
            }
            BftSignerReplyKindV3::InstallPreparedCompleted { receipt } => {
                let checkpoint = self.result_checkpoint(&request, &reply, |kind| {
                    matches!(kind, BftSignerReplyKindV3::InstallPreparedCompleted { .. })
                })?;
                receipt.verify_for(&self.binding, &self.trust.provider_public_key)?;
                let body = &receipt.body;
                if body.installed_revision.checked_sub(2) != Some(expected_revision)
                    || body.pre_install_state_digest != expected_state_digest
                    || body.request_id != request_id
                    || body.source_height != transition_source_height
                    || body.runtime_instance_id != runtime_instance_id
                    || checkpoint
                        .state
                        .membership_install
                        .as_ref()
                        .is_none_or(|m| !m.receipts.contains(receipt.as_ref()))
                {
                    return Err(invalid(
                        "V3 completed preparation differs from original request",
                    ));
                }
                Ok(BftTransitionInstallPreparationV3::Completed {
                    receipt: receipt.as_ref().clone(),
                    checkpoint,
                })
            }
            _ => Err(invalid(
                "BFT V3 install preparation response has wrong kind",
            )),
        }
    }

    fn complete_transition_install(
        &mut self,
        intent: BftTransitionInstallIntentV3,
        evidence: BftLocalInstallEvidenceV3,
    ) -> Result<BftTransitionInstallResultV3, BftSignerErrorV3> {
        let acknowledgement = BftSignedTransitionInstallAckV3::sign(
            &intent,
            evidence.clone(),
            &self.binding,
            &self.trust.provider_public_key,
            &self.requester_key,
        )?;
        let (request, reply) = self.command(BftSignerCommandV3::CompleteTransitionInstall {
            intent: Box::new(intent.clone()),
            acknowledgement: Box::new(acknowledgement.clone()),
        })?;
        let checkpoint = self.result_checkpoint(&request, &reply, |r| {
            matches!(r, BftSignerReplyKindV3::InstallPreparedCompleted { .. })
        })?;
        let BftSignerReplyKindV3::InstallPreparedCompleted { receipt } = &reply.body.result else {
            return Err(invalid("V3 completion wrong result"));
        };
        receipt.verify_for(&self.binding, &self.trust.provider_public_key)?;
        let b = &receipt.body;
        if b.intent_digest != intent.digest()?
            || b.acknowledgement_digest != acknowledgement.digest()?
            || b.request_id != intent.body.request_id
            || b.wal_install_digest != evidence.wal_install_digest
            || b.driver_install_digest != evidence.driver_install_digest
            || b.installed_head_height != evidence.installed_head_height
            || b.installed_head_commit_ref != evidence.installed_head_commit_ref
            || b.installed_state_root != evidence.installed_state_root
            || b.installed_manifest_hash != evidence.installed_manifest_hash
            || b.active_validator_set_hash != evidence.installed_active_set_hash
            || b.authority_epoch != evidence.installed_authority_epoch
            || b.local_signing_public_key != evidence.local_signing_public_key
            || b.runtime_instance_id != evidence.runtime_instance_id
            || checkpoint
                .state
                .membership_install
                .as_ref()
                .is_none_or(|m| !m.receipts.contains(receipt.as_ref()))
        {
            return Err(invalid("V3 completion original evidence mismatch"));
        }
        self.checkpoint = checkpoint.clone();
        Ok(BftTransitionInstallResultV3 {
            receipt: receipt.as_ref().clone(),
            checkpoint,
        })
    }

    fn sign_candidate_read_request(
        &mut self,
        body: &oasis7_proto::distributed_bft::v3::BftCandidateReadBodyV3,
    ) -> Result<oasis7_proto::distributed_bft::v3::BftCandidateReadProofV3, BftSignerErrorV3> {
        body.validate().map_err(invalid)?;
        let (request, reply) = self.command(BftSignerCommandV3::SignCandidateRead {
            body: Box::new(body.clone()),
        })?;
        let BftSignerReplyKindV3::CandidateReadSigned { proof } =
            RemoteBftSignerProviderV3::checked_result(&reply)?
        else {
            return Err(invalid("candidate read signature reply has wrong kind"));
        };
        self.result_checkpoint(&request, &reply, |kind| {
            matches!(kind, BftSignerReplyKindV3::CandidateReadSigned { .. })
        })?;
        proof
            .verify_for(body, &body.validator_public_key_hex)
            .map_err(invalid)?;
        Ok(proof.as_ref().clone())
    }

    fn checkpoint(&mut self) -> Result<BftSignerCheckpointV3, BftSignerErrorV3> {
        let (request, reply) = self.command(BftSignerCommandV3::Checkpoint)?;
        self.result_checkpoint(&request, &reply, |kind| {
            matches!(kind, BftSignerReplyKindV3::Checkpoint)
        })
    }

    fn protected_time_ms(&mut self) -> Result<i64, BftSignerErrorV3> {
        let (_, reply) = self.command(BftSignerCommandV3::ProtectedTime)?;
        RemoteBftSignerProviderV3::checked_result(&reply)?;
        match reply.body.result {
            BftSignerReplyKindV3::ProtectedTime { time_ms }
                if reply.body.checkpoint.is_none()
                    && reply.body.lease_token.is_none()
                    && reply.body.state_revision == self.checkpoint.state.revision
                    && reply.body.state_digest == self.checkpoint.state.state_digest =>
            {
                Ok(time_ms)
            }
            _ => Err(invalid(
                "BFT V3 protected-time response shape or checkpoint mismatch",
            )),
        }
    }

    fn start_height_timer(
        &mut self,
        expected_revision: u64,
    ) -> Result<BftSignerCheckpointV3, BftSignerErrorV3> {
        let (request, reply) =
            self.command(BftSignerCommandV3::StartHeightTimer { expected_revision })?;
        self.result_checkpoint(&request, &reply, |kind| {
            matches!(kind, BftSignerReplyKindV3::HeightTimerStarted)
        })
    }

    fn advance(
        &mut self,
        expected_revision: u64,
        expected_state_digest: &str,
        candidate: BftSignerCandidateV3,
    ) -> Result<BftSignerCheckpointV3, BftSignerErrorV3> {
        let (request, reply) = self.command(BftSignerCommandV3::Advance {
            expected_revision,
            expected_state_digest: expected_state_digest.to_string(),
            candidate: Box::new(candidate),
        })?;
        self.result_checkpoint(&request, &reply, |kind| {
            matches!(kind, BftSignerReplyKindV3::Advanced)
        })
    }

    fn sign_proposal(
        &mut self,
        expected_revision: u64,
        intent: &BftProposalIntentV3,
    ) -> Result<(BftPublicProposalV3, BftSignerCheckpointV3), BftSignerErrorV3> {
        let (request, reply) = self.command(BftSignerCommandV3::SignProposal {
            expected_revision,
            intent: Box::new(intent.clone()),
        })?;
        RemoteBftSignerProviderV3::checked_result(&reply)?;
        let BftSignerReplyKindV3::ProposalSigned { proposal } = &reply.body.result else {
            return Err(invalid("BFT V3 proposal response has the wrong kind"));
        };
        let checkpoint = self.result_checkpoint(&request, &reply, |kind| {
            matches!(kind, BftSignerReplyKindV3::ProposalSigned { .. })
        })?;
        Ok((proposal.as_ref().clone(), checkpoint))
    }

    fn sign_vote(
        &mut self,
        expected_revision: u64,
        intent: &BftVoteIntentV3,
    ) -> Result<(BftVoteV3, BftSignerCheckpointV3), BftSignerErrorV3> {
        let (request, reply) = self.command(BftSignerCommandV3::SignVote {
            expected_revision,
            intent: Box::new(intent.clone()),
        })?;
        RemoteBftSignerProviderV3::checked_result(&reply)?;
        let BftSignerReplyKindV3::VoteSigned { vote } = &reply.body.result else {
            return Err(invalid("BFT V3 vote response has the wrong kind"));
        };
        let checkpoint = self.result_checkpoint(&request, &reply, |kind| {
            matches!(kind, BftSignerReplyKindV3::VoteSigned { .. })
        })?;
        Ok((vote.as_ref().clone(), checkpoint))
    }

    fn confirm_applied(
        &mut self,
        expected_revision: u64,
        expected_state_digest: &str,
        height: u64,
        header_id: &str,
        driver_install_digest: &str,
    ) -> Result<BftSignerCheckpointV3, BftSignerErrorV3> {
        let (request, reply) = self.command(BftSignerCommandV3::ConfirmApplied {
            expected_revision,
            expected_state_digest: expected_state_digest.into(),
            height,
            header_id: header_id.into(),
            driver_install_digest: driver_install_digest.into(),
        })?;
        if !matches!(
            RemoteBftSignerProviderV3::checked_result(&reply)?,
            BftSignerReplyKindV3::AppliedConfirmed
        ) {
            return Err(invalid("BFT V3 applied confirmation reply has wrong shape"));
        }
        self.result_checkpoint(&request, &reply, |result| {
            matches!(result, BftSignerReplyKindV3::AppliedConfirmed)
        })
    }
    fn append_committed_certificate(
        &mut self,
        expected_revision: u64,
        expected_state_digest: &str,
        certificate: FormalBftCertifiedHeaderV3,
    ) -> Result<BftSignerCheckpointV3, BftSignerErrorV3> {
        let (request, reply) = self.command(BftSignerCommandV3::AppendCommittedCertificate {
            expected_revision,
            expected_state_digest: expected_state_digest.to_string(),
            certificate: Box::new(certificate),
        })?;
        self.result_checkpoint(&request, &reply, |kind| {
            matches!(kind, BftSignerReplyKindV3::CertificateAppended)
        })
    }
}

fn random_nonce() -> Result<String, BftSignerErrorV3> {
    #[cfg(unix)]
    {
        use std::io::Read;
        let mut bytes = [0u8; 32];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(&mut bytes))
            .map_err(|error| invalid(format!("BFT V3 client CSPRNG unavailable: {error}")))?;
        Ok(hex::encode(bytes))
    }
    #[cfg(not(unix))]
    {
        Err(invalid(
            "BFT V3 remote client currently requires a Unix CSPRNG",
        ))
    }
}

fn validate_token(token: &str) -> Result<(), BftSignerErrorV3> {
    if token.len() != 64
        || token
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(invalid("BFT V3 lease token is noncanonical"));
    }
    Ok(())
}
