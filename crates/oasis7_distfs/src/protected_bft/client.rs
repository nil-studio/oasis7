use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use ed25519_dalek::{Signer, SigningKey};
use oasis7_consensus::tendermint_v1::{BftProposalIntentV1, BftVoteIntentV1};
use oasis7_proto::distributed_bft::{BftProposalV1, BftVoteV1};

use super::binding::{
    BftSignerBindingV1, BftSignerError, BftSignerTrustV1, invalid, verify_signature,
};
use super::validation::{BftProtectedCheckpointV1, BftSignerCandidateV1};
use super::wire::{
    BFT_REPLY_DOMAIN_V1, BFT_REQUEST_DIGEST_DOMAIN_V1, BFT_REQUEST_DOMAIN_V1, BftSignerCommandV1,
    BftSignerLeaseCredentialV1, BftSignerReplyKindV1, BftSignerRequestBodyV1,
    SignedBftSignerReplyV1, SignedBftSignerRequestV1, decode_bounded, digest_hex, encode_bounded,
    framed_cbor,
};

const BFT_SIGNER_MAX_ACQUIRE_ATTEMPTS_V1: usize = 3;

/// Client-side connection factory. Its requester authentication key is a
/// separate key from the validator signer and must match the pinned binding.
pub struct RemoteBftSignerProviderV1 {
    address: SocketAddr,
    timeout: Duration,
    binding: BftSignerBindingV1,
    authentication_key: SigningKey,
    trust: BftSignerTrustV1,
}

impl RemoteBftSignerProviderV1 {
    pub fn new(
        address: SocketAddr,
        timeout: Duration,
        binding: BftSignerBindingV1,
        authentication_key: SigningKey,
        trust: BftSignerTrustV1,
    ) -> Result<Self, BftSignerError> {
        if timeout.is_zero() || timeout > Duration::from_secs(60) {
            return Err(invalid("BFT signer transport timeout must be 1ms..60s"));
        }
        binding.validate()?;
        trust.validate_for(&binding)?;
        if hex::encode(authentication_key.verifying_key().to_bytes())
            != binding.requester_auth_public_key
        {
            return Err(invalid(
                "BFT signer client authentication key differs from binding",
            ));
        }
        Ok(Self {
            address,
            timeout,
            binding,
            authentication_key,
            trust,
        })
    }

    fn acquire_inner<F>(
        &self,
        binding: &BftSignerBindingV1,
        mut after_prepare: F,
    ) -> Result<Box<dyn BftSignerLeaseV1>, BftSignerError>
    where
        F: FnMut(),
    {
        if binding != &self.binding {
            return Err(invalid(
                "BFT signer acquire differs from configured binding",
            ));
        }
        let mut stream = TcpStream::connect_timeout(&self.address, self.timeout)
            .map_err(|error| invalid(format!("connect BFT signer provider: {error}")))?;
        stream
            .set_read_timeout(Some(self.timeout))
            .and_then(|_| stream.set_write_timeout(Some(self.timeout)))
            .map_err(|error| invalid(format!("configure BFT signer transport: {error}")))?;
        stream
            .set_nodelay(true)
            .map_err(|error| invalid(format!("configure BFT signer TCP: {error}")))?;
        for attempt in 0..BFT_SIGNER_MAX_ACQUIRE_ATTEMPTS_V1 {
            let prepared = exchange(
                &mut stream,
                &self.binding,
                &self.authentication_key,
                &self.trust,
                None,
                BftSignerCommandV1::PrepareAcquire {
                    binding: Box::new(self.binding.clone()),
                },
            )?;
            let (expected_generation, prepare_nonce) = match &prepared.body.result {
                BftSignerReplyKindV1::Prepared {
                    expected_generation,
                    prepare_nonce,
                } if prepared.body.state.is_none()
                    && prepared.body.lease_token.is_none()
                    && prepared.body.lease_generation == *expected_generation =>
                {
                    (*expected_generation, prepare_nonce.clone())
                }
                BftSignerReplyKindV1::Rejected { message } => {
                    return Err(invalid(message.clone()));
                }
                _ => return Err(invalid("BFT signer prepare response mismatch")),
            };
            after_prepare();

            let expected_next_generation = expected_generation
                .checked_add(1)
                .ok_or_else(|| invalid("BFT signer lease generation overflow"))?;
            let reply = exchange(
                &mut stream,
                &self.binding,
                &self.authentication_key,
                &self.trust,
                None,
                BftSignerCommandV1::Acquire {
                    binding: Box::new(self.binding.clone()),
                    expected_generation,
                    prepare_nonce,
                },
            )?;
            match &reply.body.result {
                BftSignerReplyKindV1::AcquireConflict {
                    current_generation,
                    lease_active,
                } => {
                    if reply.body.state.is_some()
                        || reply.body.lease_token.is_some()
                        || reply.body.lease_generation != *current_generation
                        || *current_generation <= expected_generation
                    {
                        return Err(invalid("BFT signer acquire conflict response mismatch"));
                    }
                    if *lease_active {
                        return Err(invalid("BFT signer acquire lost to a newer active lease"));
                    }
                    if attempt + 1 == BFT_SIGNER_MAX_ACQUIRE_ATTEMPTS_V1 {
                        return Err(invalid(
                            "BFT signer acquire generation kept advancing during bounded retries",
                        ));
                    }
                    continue;
                }
                BftSignerReplyKindV1::Rejected { message } => {
                    return Err(invalid(message.clone()));
                }
                BftSignerReplyKindV1::Acquired
                    if reply.body.lease_generation == expected_next_generation => {}
                _ => return Err(invalid("BFT signer acquire response mismatch")),
            }
            let lease_token = reply
                .body
                .lease_token
                .clone()
                .ok_or_else(|| invalid("BFT signer acquire omitted lease token"))?;
            if reply.body.lease_generation == 0 || lease_token.len() != 64 {
                return Err(invalid("BFT signer acquire returned invalid lease"));
            }
            let checkpoint = checkpoint_from_reply(&reply, &self.binding, &self.trust)?;
            return Ok(Box::new(RemoteBftSignerLeaseV1 {
                stream,
                binding: self.binding.clone(),
                authentication_key: self.authentication_key.clone(),
                trust: self.trust.clone(),
                lease: BftSignerLeaseCredentialV1 {
                    generation: reply.body.lease_generation,
                    token: lease_token,
                },
                checkpoint,
                protected_time_floor_ms: 0,
            }));
        }
        Err(invalid("BFT signer acquire attempts exhausted"))
    }

    #[cfg(test)]
    pub(super) fn acquire_with_prepare_interleave<F>(
        &self,
        binding: &BftSignerBindingV1,
        after_prepare: F,
    ) -> Result<Box<dyn BftSignerLeaseV1>, BftSignerError>
    where
        F: FnMut(),
    {
        self.acquire_inner(binding, after_prepare)
    }
}

pub trait BftSignerProviderV1: Send + Sync {
    fn acquire(
        &self,
        binding: &BftSignerBindingV1,
    ) -> Result<Box<dyn BftSignerLeaseV1>, BftSignerError>;
}

pub trait BftSignerLeaseV1: Send {
    fn checkpoint(&mut self) -> Result<BftProtectedCheckpointV1, BftSignerError>;
    fn protected_time_ms(&mut self) -> Result<i64, BftSignerError>;
    fn start_height_timer(
        &mut self,
        expected_revision: u64,
    ) -> Result<BftProtectedCheckpointV1, BftSignerError>;
    fn advance(
        &mut self,
        expected_revision: u64,
        expected_state_digest: &str,
        candidate: BftSignerCandidateV1,
    ) -> Result<BftProtectedCheckpointV1, BftSignerError>;
    fn sign_reserved_proposal(
        &mut self,
        expected_revision: u64,
        intent: &BftProposalIntentV1,
    ) -> Result<(BftProposalV1, BftProtectedCheckpointV1), BftSignerError>;
    fn sign_reserved_vote(
        &mut self,
        expected_revision: u64,
        intent: &BftVoteIntentV1,
    ) -> Result<(BftVoteV1, BftProtectedCheckpointV1), BftSignerError>;
}

impl BftSignerProviderV1 for RemoteBftSignerProviderV1 {
    fn acquire(
        &self,
        binding: &BftSignerBindingV1,
    ) -> Result<Box<dyn BftSignerLeaseV1>, BftSignerError> {
        self.acquire_inner(binding, || {})
    }
}

struct RemoteBftSignerLeaseV1 {
    stream: TcpStream,
    binding: BftSignerBindingV1,
    authentication_key: SigningKey,
    trust: BftSignerTrustV1,
    lease: BftSignerLeaseCredentialV1,
    checkpoint: BftProtectedCheckpointV1,
    protected_time_floor_ms: i64,
}

impl RemoteBftSignerLeaseV1 {
    fn send(
        &mut self,
        command: BftSignerCommandV1,
    ) -> Result<SignedBftSignerReplyV1, BftSignerError> {
        let reply = exchange(
            &mut self.stream,
            &self.binding,
            &self.authentication_key,
            &self.trust,
            Some(self.lease.clone()),
            command,
        )?;
        if reply.body.lease_generation != self.lease.generation {
            return Err(invalid(
                "BFT signer reply generation differs from active lease",
            ));
        }
        Ok(reply)
    }

    fn accept_checkpoint(
        &mut self,
        reply: &SignedBftSignerReplyV1,
    ) -> Result<BftProtectedCheckpointV1, BftSignerError> {
        let checkpoint = checkpoint_from_reply(reply, &self.binding, &self.trust)?;
        if checkpoint.state.revision < self.checkpoint.state.revision {
            return Err(invalid("BFT signer checkpoint revision regressed"));
        }
        if checkpoint.state.revision == self.checkpoint.state.revision
            && (checkpoint.state.state_digest != self.checkpoint.state.state_digest
                || checkpoint.state != self.checkpoint.state)
        {
            return Err(invalid(
                "BFT signer checkpoint changed at the same revision",
            ));
        }
        self.checkpoint = checkpoint.clone();
        Ok(checkpoint)
    }

    fn expect_result(
        reply: &SignedBftSignerReplyV1,
        predicate: impl FnOnce(&BftSignerReplyKindV1) -> bool,
    ) -> Result<(), BftSignerError> {
        match &reply.body.result {
            BftSignerReplyKindV1::Rejected { message } => Err(invalid(message.clone())),
            result if predicate(result) => Ok(()),
            _ => Err(invalid(
                "BFT signer returned a response for another command",
            )),
        }
    }
}

impl BftSignerLeaseV1 for RemoteBftSignerLeaseV1 {
    fn checkpoint(&mut self) -> Result<BftProtectedCheckpointV1, BftSignerError> {
        let reply = self.send(BftSignerCommandV1::Checkpoint)?;
        Self::expect_result(&reply, |result| {
            matches!(result, BftSignerReplyKindV1::Checkpoint)
        })?;
        self.accept_checkpoint(&reply)
    }

    fn protected_time_ms(&mut self) -> Result<i64, BftSignerError> {
        let reply = self.send(BftSignerCommandV1::ProtectedTime)?;
        Self::expect_result(&reply, |result| {
            matches!(result, BftSignerReplyKindV1::ProtectedTime { .. })
        })?;
        if reply.body.state.is_some()
            || reply.body.state_revision != self.checkpoint.state.revision
            || reply.body.state_digest != self.checkpoint.state.state_digest
        {
            return Err(invalid(
                "BFT protected-time sample is not bound to current checkpoint",
            ));
        }
        let BftSignerReplyKindV1::ProtectedTime { time_ms } = reply.body.result else {
            return Err(invalid("BFT protected-time result kind mismatch"));
        };
        if time_ms < self.protected_time_floor_ms || time_ms < 0 {
            return Err(invalid("BFT protected-time sample regressed"));
        }
        self.protected_time_floor_ms = time_ms;
        Ok(time_ms)
    }

    fn advance(
        &mut self,
        expected_revision: u64,
        expected_state_digest: &str,
        candidate: BftSignerCandidateV1,
    ) -> Result<BftProtectedCheckpointV1, BftSignerError> {
        let reply = self.send(BftSignerCommandV1::Advance {
            expected_revision,
            expected_state_digest: expected_state_digest.to_string(),
            candidate: Box::new(candidate),
        })?;
        Self::expect_result(&reply, |result| {
            matches!(result, BftSignerReplyKindV1::Advanced)
        })?;
        self.accept_checkpoint(&reply)
    }

    fn start_height_timer(
        &mut self,
        expected_revision: u64,
    ) -> Result<BftProtectedCheckpointV1, BftSignerError> {
        let reply = self.send(BftSignerCommandV1::StartHeightTimer { expected_revision })?;
        Self::expect_result(&reply, |result| {
            matches!(result, BftSignerReplyKindV1::HeightTimerStarted)
        })?;
        self.accept_checkpoint(&reply)
    }

    fn sign_reserved_proposal(
        &mut self,
        expected_revision: u64,
        intent: &BftProposalIntentV1,
    ) -> Result<(BftProposalV1, BftProtectedCheckpointV1), BftSignerError> {
        let reply = self.send(BftSignerCommandV1::SignProposal {
            expected_revision,
            intent: Box::new(intent.clone()),
        })?;
        Self::expect_result(&reply, |result| {
            matches!(result, BftSignerReplyKindV1::ProposalSigned { .. })
        })?;
        let BftSignerReplyKindV1::ProposalSigned { proposal } = &reply.body.result else {
            return Err(invalid("BFT proposal signing result kind mismatch"));
        };
        let checkpoint = self.accept_checkpoint(&reply)?;
        Ok((proposal.as_ref().clone(), checkpoint))
    }

    fn sign_reserved_vote(
        &mut self,
        expected_revision: u64,
        intent: &BftVoteIntentV1,
    ) -> Result<(BftVoteV1, BftProtectedCheckpointV1), BftSignerError> {
        let reply = self.send(BftSignerCommandV1::SignVote {
            expected_revision,
            intent: Box::new(intent.clone()),
        })?;
        Self::expect_result(&reply, |result| {
            matches!(result, BftSignerReplyKindV1::VoteSigned { .. })
        })?;
        let BftSignerReplyKindV1::VoteSigned { vote } = &reply.body.result else {
            return Err(invalid("BFT vote signing result kind mismatch"));
        };
        let checkpoint = self.accept_checkpoint(&reply)?;
        Ok((vote.as_ref().clone(), checkpoint))
    }
}

fn exchange(
    stream: &mut TcpStream,
    binding: &BftSignerBindingV1,
    authentication_key: &SigningKey,
    trust: &BftSignerTrustV1,
    lease: Option<BftSignerLeaseCredentialV1>,
    command: BftSignerCommandV1,
) -> Result<SignedBftSignerReplyV1, BftSignerError> {
    let body = BftSignerRequestBodyV1 {
        schema_version: 1,
        binding_digest: binding.digest()?,
        challenge: fresh_challenge()?,
        lease,
        command,
    };
    let signature_hex = sign_request(&body, authentication_key)?;
    let request = SignedBftSignerRequestV1 {
        body,
        authentication_public_key: hex::encode(authentication_key.verifying_key().to_bytes()),
        signature_hex,
    };
    let encoded = encode_bounded(&request)?;
    let frame_len = u32::try_from(encoded.len())
        .map_err(|_| invalid("BFT signer request exceeds transport frame range"))?;
    stream
        .write_all(&frame_len.to_be_bytes())
        .and_then(|_| stream.write_all(&encoded))
        .map_err(|error| invalid(format!("write BFT signer request: {error}")))?;
    let mut length = [0u8; 4];
    stream
        .read_exact(&mut length)
        .map_err(|error| invalid(format!("read BFT signer response header: {error}")))?;
    let response_len = usize::try_from(u32::from_be_bytes(length))
        .map_err(|_| invalid("BFT signer response length conversion"))?;
    if response_len == 0 || response_len > super::validation::BFT_SIGNER_MAX_FRAME_BYTES_V1 {
        return Err(invalid("BFT signer response frame bound"));
    }
    let mut response = Vec::new();
    response
        .try_reserve_exact(response_len)
        .map_err(|_| invalid("BFT signer response allocation failed"))?;
    response.resize(response_len, 0);
    stream
        .read_exact(&mut response)
        .map_err(|error| invalid(format!("read BFT signer response body: {error}")))?;
    let reply: SignedBftSignerReplyV1 = decode_bounded(&response)?;
    validate_reply(&request, &reply, trust)?;
    Ok(reply)
}

fn validate_reply(
    request: &SignedBftSignerRequestV1,
    reply: &SignedBftSignerReplyV1,
    trust: &BftSignerTrustV1,
) -> Result<(), BftSignerError> {
    let request_digest = digest_hex(BFT_REQUEST_DIGEST_DOMAIN_V1, &request.body)?;
    if reply.body.schema_version != 1
        || reply.body.binding_digest != request.body.binding_digest
        || reply.body.challenge != request.body.challenge
        || reply.body.request_digest != request_digest
    {
        return Err(invalid(
            "BFT signer reply freshness or request binding mismatch",
        ));
    }
    verify_signature(
        BFT_REPLY_DOMAIN_V1,
        &reply.body,
        &reply.provider_signature_hex,
        &trust.provider_public_key,
    )?;
    Ok(())
}

fn checkpoint_from_reply(
    reply: &SignedBftSignerReplyV1,
    binding: &BftSignerBindingV1,
    trust: &BftSignerTrustV1,
) -> Result<BftProtectedCheckpointV1, BftSignerError> {
    let state = reply
        .body
        .state
        .as_ref()
        .ok_or_else(|| invalid("BFT signer response omitted checkpoint state"))?
        .as_ref()
        .clone();
    if state.revision != reply.body.state_revision || state.state_digest != reply.body.state_digest
    {
        return Err(invalid("BFT signer response state summary mismatch"));
    }
    let checkpoint = BftProtectedCheckpointV1 {
        state,
        lease_generation: reply.body.lease_generation,
        challenge: reply.body.challenge.clone(),
        request_digest: reply.body.request_digest.clone(),
        provider_public_key: trust.provider_public_key.clone(),
        provider_signature_hex: reply
            .checkpoint_signature_hex
            .clone()
            .ok_or_else(|| invalid("BFT signer response omitted checkpoint proof"))?,
    };
    checkpoint.validate(
        binding,
        &reply.body.challenge,
        &reply.body.request_digest,
        reply.body.lease_generation,
        &trust.provider_public_key,
    )?;
    Ok(checkpoint)
}

fn sign_request(body: &BftSignerRequestBodyV1, key: &SigningKey) -> Result<String, BftSignerError> {
    let preimage = framed_cbor(
        BFT_REQUEST_DOMAIN_V1,
        body,
        super::wire::BFT_SIGNER_MAX_SIGNING_BYTES_V1,
    )?;
    Ok(hex::encode(key.sign(&preimage).to_bytes()))
}

fn fresh_challenge() -> Result<String, BftSignerError> {
    #[cfg(unix)]
    {
        let mut nonce = [0u8; 32];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(&mut nonce))
            .map_err(|error| invalid(format!("BFT signer challenge source: {error}")))?;
        Ok(hex::encode(nonce))
    }
    #[cfg(not(unix))]
    {
        Err(invalid("BFT signer requires a cryptographic random source"))
    }
}
