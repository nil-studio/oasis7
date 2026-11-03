//! Provider transport construction and authenticated acquire envelopes.
use super::*;

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
            private_retention_attestor: None,
        })
    }

    pub(super) fn connect(&self) -> Result<TcpStream, BftSignerErrorV3> {
        let stream = TcpStream::connect_timeout(&self.address, self.timeout)
            .map_err(|error| invalid(format!("connect BFT V3 signer provider: {error}")))?;
        stream
            .set_read_timeout(Some(self.timeout))
            .and_then(|_| stream.set_write_timeout(Some(self.timeout)))
            .map_err(|error| invalid(format!("configure BFT V3 signer timeout: {error}")))?;
        Ok(stream)
    }

    pub(super) fn exchange(
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

    pub(super) fn checkpoint_from_reply(
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
        let request_digest = super::super::wire::request_digest(request)?;
        private_retention::validate_head_role(
            &checkpoint.state,
            self.private_retention_attestor.as_ref(),
            &self.binding,
            &self.trust.provider_public_key,
        )?;
        if checkpoint.state.active_window.is_none() {
            checkpoint.validate(
                &self.binding,
                &request.body.challenge,
                &request_digest,
                expected_generation,
                &self.trust.provider_public_key,
            )?;
        } else {
            checkpoint.authenticate(
                &self.binding,
                &request.body.challenge,
                &request_digest,
                expected_generation,
                &self.trust.provider_public_key,
            )?;
        }
        if checkpoint.state.revision != reply.body.state_revision
            || checkpoint.state.state_digest != reply.body.state_digest
        {
            return Err(invalid("BFT V3 reply and checkpoint state differ"));
        }
        Ok(checkpoint.as_ref().clone())
    }

    pub(super) fn checked_result(
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
                    let mut lease = RemoteBftSignerLeaseV3 {
                        stream,
                        binding: self.binding.clone(),
                        requester_key: self.requester_key.clone(),
                        trust: self.trust.clone(),
                        private_retention_attestor: self.private_retention_attestor.clone(),
                        private_retention_transfer_floor: 0,
                        private_retention_latest_seal: None,
                        credential: BftSignerLeaseCredentialV3 {
                            generation: expected_next,
                            token: token.clone(),
                        },
                        checkpoint,
                        archive_observation: None,
                        active_context: None,
                        archive_attempt_timeout: self.timeout.min(Duration::from_secs(120)),
                        signing_result_deadline: None,
                    };
                    lease.validate_checkpoint_history(&lease.checkpoint.clone())?;
                    lease.archive_checkpoint_floor(&lease.checkpoint)?;
                    return Ok(Box::new(lease));
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
