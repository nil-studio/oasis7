use std::io::{Read, Write};
use std::sync::Mutex;

#[cfg(test)]
use ed25519_dalek::SigningKey;
use oasis7_consensus::tendermint_v1::{
    BftProposalIntentV1, BftSigningJournalEventV1, BftVoteIntentV1,
    complete_reserved_bft_proposal_v1, complete_reserved_bft_vote_v1,
    replay_bft_signing_journal_v1,
};
use oasis7_proto::distributed_bft::{BftProposalHeaderV1, BftProposalV1, BftVoteV1};
use serde::Serialize;

use super::backend::{BftSignerBackendSnapshotV1, BftSignerBackendV1};
use super::binding::{
    BftSignerBindingV1, BftSignerError, BftSigningKeyHandleV1, decode_key, invalid,
    verify_signature,
};
use super::validation::{
    BftSignerCandidateV1, BftSignerStateV1, CheckpointProofInput, candidate_event_time,
    validate_candidate_bounds, validate_height_timer_start, validate_state, validate_transition,
};
use super::wire::{
    BFT_CHECKPOINT_DOMAIN_V1, BFT_REPLY_DOMAIN_V1, BFT_REQUEST_DIGEST_DOMAIN_V1,
    BFT_REQUEST_DOMAIN_V1, BFT_SIGNER_MAX_SIGNING_BYTES_V1, BftSignerCommandV1,
    BftSignerLeaseCredentialV1, BftSignerReplyBodyV1, BftSignerReplyKindV1, BftSignerRequestBodyV1,
    SignedBftSignerReplyV1, SignedBftSignerRequestV1, decode_bounded, digest_hex, encode_bounded,
    framed_cbor,
};

/// Callable BFT provider endpoint. The concrete backend must implement
/// transactionally durable generations, journal CAS, and protected time; this
/// service never provisions missing state or replaces a key.
pub struct BftSignerServiceV1 {
    binding: BftSignerBindingV1,
    provider_key: Box<dyn BftSigningKeyHandleV1>,
    validator_key: Box<dyn BftSigningKeyHandleV1>,
    runtime: Mutex<BftSignerServiceRuntimeV1>,
}

struct BftSignerServiceRuntimeV1 {
    backend: Box<dyn BftSignerBackendV1>,
    poisoned: bool,
}

struct BftSignerMutationContextV1<'a> {
    backend: &'a mut dyn BftSignerBackendV1,
    poisoned: &'a mut bool,
}

#[derive(Default)]
struct BftSignerSessionV1 {
    prepared_acquire: Option<PreparedAcquireV1>,
    lease: Option<BftSignerLeaseCredentialV1>,
}

struct PreparedAcquireV1 {
    expected_generation: u64,
    prepare_nonce: String,
}

impl BftSignerServiceV1 {
    pub fn open(
        binding: BftSignerBindingV1,
        provider_key: Box<dyn BftSigningKeyHandleV1>,
        validator_key: Box<dyn BftSigningKeyHandleV1>,
        mut backend: Box<dyn BftSignerBackendV1>,
    ) -> Result<Self, BftSignerError> {
        binding.validate()?;
        let digest = binding.digest()?;
        if provider_key.public_key_hex() == binding.local_signing_public_key
            || provider_key.public_key_hex() == binding.requester_auth_public_key
            || validator_key.public_key_hex() != binding.local_signing_public_key
        {
            return Err(invalid(
                "BFT signer provider, requester, and validator keys must be distinct",
            ));
        }
        decode_key(&provider_key.public_key_hex())?;
        let snapshot = backend.load_provisioned(&digest)?;
        validate_backend_snapshot(&binding, &snapshot)?;
        Ok(Self {
            binding,
            provider_key,
            validator_key,
            runtime: Mutex::new(BftSignerServiceRuntimeV1 {
                backend,
                poisoned: false,
            }),
        })
    }

    fn handle_session_frame(
        &self,
        session: &mut BftSignerSessionV1,
        bytes: &[u8],
    ) -> Result<Vec<u8>, BftSignerError> {
        let request: SignedBftSignerRequestV1 = decode_bounded(bytes)?;
        validate_request(&self.binding, &request)?;
        let request_digest = digest_hex(BFT_REQUEST_DIGEST_DOMAIN_V1, &request.body)?;
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| invalid("BFT signer service lock poisoned"))?;
        if runtime.poisoned {
            return Err(invalid(
                "BFT signer service is poisoned after uncertain release",
            ));
        }
        let reply = {
            let BftSignerServiceRuntimeV1 { backend, poisoned } = &mut *runtime;
            self.process_command(backend.as_mut(), poisoned, session, &request.body)
        };
        let reply = match reply {
            Ok(reply) => reply,
            Err(error) => {
                let generation = request
                    .body
                    .lease
                    .as_ref()
                    .map(|lease| lease.generation)
                    .or_else(|| session.lease.as_ref().map(|lease| lease.generation))
                    .or_else(|| {
                        session
                            .prepared_acquire
                            .as_ref()
                            .map(|prepared| prepared.expected_generation)
                    })
                    .unwrap_or(0);
                ReplyData {
                    state: None,
                    state_revision: 0,
                    state_digest: String::new(),
                    generation,
                    lease_token: None,
                    result: BftSignerReplyKindV1::Rejected {
                        message: bounded_error(error),
                    },
                }
            }
        };
        self.sign_reply(&request.body, &request_digest, reply)
    }

    /// Serves requests until the peer closes the stream. Callers should set a
    /// read/write timeout and run this only on a protected provider endpoint.
    pub fn serve_connection<S: Read + Write>(&self, stream: &mut S) -> Result<(), BftSignerError> {
        let mut session = BftSignerSessionV1::default();
        let result = self.serve_session_loop(stream, &mut session);
        let release = self.release_session_lease(&mut session);
        match (result, release) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), Ok(())) => Err(error),
            (_, Err(error)) => Err(error),
        }
    }

    fn serve_session_loop<S: Read + Write>(
        &self,
        stream: &mut S,
        session: &mut BftSignerSessionV1,
    ) -> Result<(), BftSignerError> {
        loop {
            let mut length = [0u8; 4];
            match stream.read(&mut length[..1]) {
                Ok(0) => return Ok(()),
                Ok(1) => {}
                Ok(_) => return Err(invalid("BFT signer stream read exceeded one-byte header")),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(invalid(format!("BFT signer request header: {error}"))),
            }
            stream.read_exact(&mut length[1..]).map_err(|error| {
                invalid(format!("truncated BFT signer request header: {error}"))
            })?;
            let frame_len = usize::try_from(u32::from_be_bytes(length))
                .map_err(|_| invalid("BFT signer frame length conversion"))?;
            if frame_len == 0 || frame_len > super::validation::BFT_SIGNER_MAX_FRAME_BYTES_V1 {
                return Err(invalid("BFT signer frame length bound"));
            }
            let mut frame = Vec::new();
            frame
                .try_reserve_exact(frame_len)
                .map_err(|_| invalid("BFT signer frame allocation failed"))?;
            frame.resize(frame_len, 0);
            stream
                .read_exact(&mut frame)
                .map_err(|error| invalid(format!("BFT signer request body: {error}")))?;
            let response = self.handle_session_frame(session, &frame)?;
            let response_len = u32::try_from(response.len())
                .map_err(|_| invalid("BFT signer response exceeds TCP frame range"))?;
            stream
                .write_all(&response_len.to_be_bytes())
                .and_then(|_| stream.write_all(&response))
                .map_err(|error| invalid(format!("BFT signer response write: {error}")))?;
        }
    }

    fn release_session_lease(
        &self,
        session: &mut BftSignerSessionV1,
    ) -> Result<(), BftSignerError> {
        let Some(lease) = session.lease.as_ref() else {
            return Ok(());
        };
        let token_digest = match digest_hex("oasis7.bft.protected-signer.v1.lease", &lease.token) {
            Ok(digest) => digest,
            Err(error) => {
                if let Ok(mut runtime) = self.runtime.lock() {
                    runtime.poisoned = true;
                }
                return Err(invalid(format!(
                    "BFT signer lease release is uncertain; service poisoned: {error}"
                )));
            }
        };
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| invalid("BFT signer service lock poisoned"))?;
        if runtime.poisoned {
            return Err(invalid(
                "BFT signer service is poisoned after uncertain release",
            ));
        }
        let release = runtime
            .backend
            .release_lease(lease.generation, &token_digest)
            .and_then(|snapshot| {
                validate_backend_snapshot(&self.binding, &snapshot)?;
                let next_generation = lease
                    .generation
                    .checked_add(1)
                    .ok_or_else(|| invalid("BFT signer lease generation overflow"))?;
                if snapshot.lease_generation == next_generation
                    && snapshot.lease_token_digest.is_empty()
                {
                    return Ok(());
                }
                if snapshot.lease_generation > lease.generation
                    && snapshot.lease_token_digest != token_digest
                {
                    return Ok(());
                }
                Err(invalid(
                    "BFT signer backend did not durably fence released lease",
                ))
            });
        if let Err(error) = release {
            runtime.poisoned = true;
            return Err(invalid(format!(
                "BFT signer lease release is uncertain; service poisoned: {error}"
            )));
        }
        session.lease = None;
        Ok(())
    }

    fn process_command(
        &self,
        backend: &mut dyn BftSignerBackendV1,
        poisoned: &mut bool,
        session: &mut BftSignerSessionV1,
        request: &BftSignerRequestBodyV1,
    ) -> Result<ReplyData, BftSignerError> {
        match &request.command {
            BftSignerCommandV1::PrepareAcquire { binding } => {
                if request.lease.is_some()
                    || session.lease.is_some()
                    || binding.as_ref() != &self.binding
                {
                    return Err(invalid(
                        "BFT signer prepare binding or existing lease mismatch",
                    ));
                }
                let snapshot = backend.load_provisioned(&self.binding.digest()?)?;
                validate_backend_snapshot(&self.binding, &snapshot)?;
                let prepare_nonce = random_token()?;
                session.prepared_acquire = Some(PreparedAcquireV1 {
                    expected_generation: snapshot.lease_generation,
                    prepare_nonce: prepare_nonce.clone(),
                });
                Ok(ReplyData {
                    state: None,
                    state_revision: snapshot.state.revision,
                    state_digest: snapshot.state.state_digest,
                    generation: snapshot.lease_generation,
                    lease_token: None,
                    result: BftSignerReplyKindV1::Prepared {
                        expected_generation: snapshot.lease_generation,
                        prepare_nonce,
                    },
                })
            }
            BftSignerCommandV1::Acquire {
                binding,
                expected_generation,
                prepare_nonce,
            } => {
                let prepared_matches = session.prepared_acquire.as_ref().is_some_and(|prepared| {
                    prepared.expected_generation == *expected_generation
                        && prepared.prepare_nonce == *prepare_nonce
                });
                if request.lease.is_some()
                    || session.lease.is_some()
                    || binding.as_ref() != &self.binding
                    || !prepared_matches
                {
                    return Err(invalid(
                        "BFT signer acquire must match this connection's prepared generation",
                    ));
                }
                session.prepared_acquire = None;
                let previous = backend.load_provisioned(&self.binding.digest()?)?;
                validate_backend_snapshot(&self.binding, &previous)?;
                if previous.lease_generation != *expected_generation {
                    if previous.lease_generation > *expected_generation {
                        return Ok(acquire_conflict_reply(previous));
                    }
                    *poisoned = true;
                    return Err(invalid("BFT signer acquire observed generation rollback"));
                }
                let next_generation = expected_generation
                    .checked_add(1)
                    .ok_or_else(|| invalid("BFT signer lease generation overflow"))?;
                let lease_token = random_token()?;
                let lease_token_digest =
                    digest_hex("oasis7.bft.protected-signer.v1.lease", &lease_token)?;
                let snapshot = match backend.acquire_lease(
                    &self.binding.digest()?,
                    *expected_generation,
                    &lease_token_digest,
                ) {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        let current = match backend.load_provisioned(&self.binding.digest()?) {
                            Ok(snapshot) => snapshot,
                            Err(reload_error) => {
                                *poisoned = true;
                                return Err(invalid(format!(
                                    "BFT signer acquire outcome is uncertain; reload failed: {reload_error}"
                                )));
                            }
                        };
                        if let Err(validation_error) =
                            validate_backend_snapshot(&self.binding, &current)
                        {
                            *poisoned = true;
                            return Err(invalid(format!(
                                "BFT signer acquire outcome is uncertain; reloaded state is invalid: {validation_error}"
                            )));
                        }
                        if current.lease_generation > *expected_generation {
                            if current.lease_token_digest == lease_token_digest {
                                if current.lease_generation == next_generation {
                                    if let Err(error) = validate_acquired_snapshot(
                                        &self.binding,
                                        &previous,
                                        &current,
                                        next_generation,
                                        &lease_token_digest,
                                    ) {
                                        *poisoned = true;
                                        return Err(invalid(format!(
                                            "BFT signer acquire outcome is uncertain; committed lease snapshot is invalid: {error}"
                                        )));
                                    }
                                    current
                                } else {
                                    *poisoned = true;
                                    return Err(invalid(
                                        "BFT signer acquire outcome is uncertain; committed lease snapshot differs",
                                    ));
                                }
                            } else {
                                return Ok(acquire_conflict_reply(current));
                            }
                        } else if current.lease_generation == *expected_generation
                            && current == previous
                        {
                            return Err(error);
                        } else {
                            *poisoned = true;
                            return Err(invalid(format!(
                                "BFT signer acquire outcome is uncertain; generation did not match prior snapshot: {error}"
                            )));
                        }
                    }
                };
                if let Err(error) = validate_backend_snapshot(&self.binding, &snapshot) {
                    *poisoned = true;
                    return Err(invalid(format!(
                        "BFT signer acquire outcome is uncertain; acquired snapshot is invalid: {error}"
                    )));
                }
                if let Err(error) = validate_acquired_snapshot(
                    &self.binding,
                    &previous,
                    &snapshot,
                    next_generation,
                    &lease_token_digest,
                ) {
                    *poisoned = true;
                    return Err(invalid(format!(
                        "BFT signer backend did not return a valid acquired lease: {error}"
                    )));
                }
                let credential = BftSignerLeaseCredentialV1 {
                    generation: snapshot.lease_generation,
                    token: lease_token.clone(),
                };
                session.lease = Some(credential);
                let mut reply = state_reply(snapshot, BftSignerReplyKindV1::Acquired);
                reply.lease_token = Some(lease_token);
                Ok(reply)
            }
            command => {
                let lease = request
                    .lease
                    .as_ref()
                    .ok_or_else(|| invalid("BFT signer command requires active lease"))?;
                validate_lease_credential(lease)?;
                if session.lease.as_ref() != Some(lease) {
                    return Err(invalid("BFT signer lease is not bound to this connection"));
                }
                let token_digest =
                    digest_hex("oasis7.bft.protected-signer.v1.lease", &lease.token)?;
                let snapshot = backend.read_for_lease(lease.generation, &token_digest)?;
                validate_backend_snapshot(&self.binding, &snapshot)?;
                if snapshot.lease_generation != lease.generation
                    || snapshot.lease_token_digest != token_digest
                {
                    return Err(invalid("BFT signer lease was fenced"));
                }
                self.process_leased_command(
                    backend,
                    poisoned,
                    lease,
                    &token_digest,
                    snapshot,
                    command,
                )
            }
        }
    }

    fn process_leased_command(
        &self,
        backend: &mut dyn BftSignerBackendV1,
        poisoned: &mut bool,
        lease: &BftSignerLeaseCredentialV1,
        token_digest: &str,
        snapshot: BftSignerBackendSnapshotV1,
        command: &BftSignerCommandV1,
    ) -> Result<ReplyData, BftSignerError> {
        let mut context = BftSignerMutationContextV1 { backend, poisoned };
        match command {
            BftSignerCommandV1::PrepareAcquire { .. } => {
                Err(invalid("BFT signer prepare cannot use an active lease"))
            }
            BftSignerCommandV1::Checkpoint => {
                Ok(state_reply(snapshot, BftSignerReplyKindV1::Checkpoint))
            }
            BftSignerCommandV1::ProtectedTime => {
                let now = context
                    .backend
                    .protected_time_ms(lease.generation, token_digest)?;
                if now < snapshot.protected_time_floor_ms || now < 0 {
                    return Err(invalid("BFT signer protected clock rollback"));
                }
                Ok(ReplyData {
                    state: None,
                    state_revision: snapshot.state.revision,
                    state_digest: snapshot.state.state_digest.clone(),
                    generation: lease.generation,
                    lease_token: None,
                    result: BftSignerReplyKindV1::ProtectedTime { time_ms: now },
                })
            }
            BftSignerCommandV1::StartHeightTimer { expected_revision } => self.start_height_timer(
                &mut context,
                lease,
                token_digest,
                snapshot,
                *expected_revision,
            ),
            BftSignerCommandV1::Advance { .. } => {
                let next =
                    self.advance_state(&mut context, lease, token_digest, &snapshot, command)?;
                Ok(state_reply(next, BftSignerReplyKindV1::Advanced))
            }
            BftSignerCommandV1::SignProposal {
                expected_revision,
                intent,
            } => self.sign_proposal(
                &mut context,
                lease,
                token_digest,
                snapshot,
                *expected_revision,
                intent,
            ),
            BftSignerCommandV1::SignVote {
                expected_revision,
                intent,
            } => self.sign_vote(
                &mut context,
                lease,
                token_digest,
                snapshot,
                *expected_revision,
                intent,
            ),
            BftSignerCommandV1::Acquire { .. } => {
                Err(invalid("BFT signer acquire cannot use an active lease"))
            }
        }
    }

    fn start_height_timer(
        &self,
        context: &mut BftSignerMutationContextV1<'_>,
        lease: &BftSignerLeaseCredentialV1,
        token_digest: &str,
        previous: BftSignerBackendSnapshotV1,
        expected_revision: u64,
    ) -> Result<ReplyData, BftSignerError> {
        if previous.state.journal.height_started_at_ms.is_some() {
            return Ok(state_reply(
                previous,
                BftSignerReplyKindV1::HeightTimerStarted,
            ));
        }
        if expected_revision != previous.state.revision {
            return Err(invalid("BFT height timer start has stale revision"));
        }
        let protected_now = context
            .backend
            .protected_time_ms(lease.generation, token_digest)?;
        if protected_now < previous.protected_time_floor_ms || protected_now < 0 {
            return Err(invalid("BFT signer protected clock rollback"));
        }
        let next = validate_height_timer_start(&self.binding, &previous.state, protected_now)?;
        let committed = self.persist_next_state(
            context,
            lease,
            token_digest,
            &previous,
            next,
            Some(protected_now),
        )?;
        Ok(state_reply(
            committed,
            BftSignerReplyKindV1::HeightTimerStarted,
        ))
    }

    fn advance_state(
        &self,
        context: &mut BftSignerMutationContextV1<'_>,
        lease: &BftSignerLeaseCredentialV1,
        token_digest: &str,
        previous: &BftSignerBackendSnapshotV1,
        command: &BftSignerCommandV1,
    ) -> Result<BftSignerBackendSnapshotV1, BftSignerError> {
        let BftSignerCommandV1::Advance {
            expected_revision,
            expected_state_digest,
            candidate,
        } = command
        else {
            return Err(invalid("BFT signer advance command mismatch"));
        };
        if *expected_revision != previous.state.revision
            || expected_state_digest != &previous.state.state_digest
        {
            return Err(invalid("BFT signer CAS revision or digest mismatch"));
        }
        validate_candidate_bounds(candidate)?;
        if candidate.history == previous.state.history
            && candidate.journal == previous.state.journal
        {
            return Ok(previous.clone());
        }
        let protected_now = context
            .backend
            .protected_time_ms(lease.generation, token_digest)?;
        if protected_now < previous.protected_time_floor_ms || protected_now < 0 {
            return Err(invalid("BFT signer protected clock rollback"));
        }
        let next = validate_transition(&self.binding, &previous.state, candidate, protected_now)?;
        self.persist_next_state(
            context,
            lease,
            token_digest,
            previous,
            next,
            candidate_event_time(&previous.state, candidate),
        )
    }

    fn sign_proposal(
        &self,
        context: &mut BftSignerMutationContextV1<'_>,
        lease: &BftSignerLeaseCredentialV1,
        token_digest: &str,
        snapshot: BftSignerBackendSnapshotV1,
        expected_revision: u64,
        intent: &BftProposalIntentV1,
    ) -> Result<ReplyData, BftSignerError> {
        if let Some(proposal) = find_signed_proposal(&snapshot.state.journal, intent)? {
            return Ok(state_reply(
                snapshot,
                BftSignerReplyKindV1::ProposalSigned {
                    proposal: Box::new(proposal),
                },
            ));
        }
        if expected_revision != snapshot.state.revision {
            return Err(invalid("BFT signer proposal retry has stale revision"));
        }
        if snapshot.state.journal.pending_proposal_intent() != Some(intent) {
            return Err(invalid("BFT signer proposal was not durably reserved"));
        }
        let protected_now = context
            .backend
            .protected_time_ms(lease.generation, token_digest)?;
        if protected_now < snapshot.protected_time_floor_ms || protected_now < 0 {
            return Err(invalid("BFT signer protected clock rollback"));
        }
        let set = self.binding.validate()?;
        let schedule = validate_state(&self.binding, &snapshot.state)?;
        let machine = replay_bft_signing_journal_v1(
            &snapshot.state.journal,
            &set,
            &schedule,
            self.binding.timeouts,
        )
        .map_err(|error| invalid(format!("replay proposal signing state: {error}")))?;
        let expected = machine
            .proposal_intent(intent.block.clone(), &intent.header.proposer_id)
            .map_err(|error| invalid(format!("proposal intent check: {error}")))?;
        if expected != *intent {
            return Err(invalid(
                "BFT signer proposal intent differs from deterministic state",
            ));
        }
        let proposal = self.sign_intent_proposal(intent)?;
        let journal = complete_reserved_bft_proposal_v1(
            &snapshot.state.journal,
            proposal.clone(),
            protected_now,
            &set,
            &schedule,
            self.binding.timeouts,
        )
        .map_err(|error| invalid(format!("complete proposal reservation: {error}")))?;
        let candidate = BftSignerCandidateV1 {
            history: snapshot.state.history.clone(),
            journal,
        };
        let next = validate_transition(&self.binding, &snapshot.state, &candidate, protected_now)?;
        let committed = self.persist_next_state(
            context,
            lease,
            token_digest,
            &snapshot,
            next,
            Some(protected_now),
        )?;
        Ok(state_reply(
            committed,
            BftSignerReplyKindV1::ProposalSigned {
                proposal: Box::new(proposal),
            },
        ))
    }

    fn sign_vote(
        &self,
        context: &mut BftSignerMutationContextV1<'_>,
        lease: &BftSignerLeaseCredentialV1,
        token_digest: &str,
        snapshot: BftSignerBackendSnapshotV1,
        expected_revision: u64,
        intent: &BftVoteIntentV1,
    ) -> Result<ReplyData, BftSignerError> {
        if let Some(vote) = find_signed_vote(&snapshot.state.journal, intent)? {
            return Ok(state_reply(
                snapshot,
                BftSignerReplyKindV1::VoteSigned {
                    vote: Box::new(vote),
                },
            ));
        }
        if expected_revision != snapshot.state.revision {
            return Err(invalid("BFT signer vote retry has stale revision"));
        }
        if snapshot.state.journal.pending_vote_intent() != Some(intent) {
            return Err(invalid("BFT signer vote was not durably reserved"));
        }
        let protected_now = context
            .backend
            .protected_time_ms(lease.generation, token_digest)?;
        if protected_now < snapshot.protected_time_floor_ms || protected_now < 0 {
            return Err(invalid("BFT signer protected clock rollback"));
        }
        let set = self.binding.validate()?;
        let schedule = validate_state(&self.binding, &snapshot.state)?;
        let machine = replay_bft_signing_journal_v1(
            &snapshot.state.journal,
            &set,
            &schedule,
            self.binding.timeouts,
        )
        .map_err(|error| invalid(format!("replay vote signing state: {error}")))?;
        let expected = machine
            .vote_intent(
                intent.validator_id.clone(),
                intent.phase,
                intent.value.clone(),
                intent.round,
            )
            .map_err(|error| invalid(format!("vote intent check: {error}")))?;
        if expected.as_ref() != Some(intent) {
            return Err(invalid(
                "BFT signer vote intent differs from deterministic state",
            ));
        }
        let vote = self.sign_intent_vote(intent)?;
        let journal = complete_reserved_bft_vote_v1(
            &snapshot.state.journal,
            vote.clone(),
            protected_now,
            &set,
            &schedule,
            self.binding.timeouts,
        )
        .map_err(|error| invalid(format!("complete vote reservation: {error}")))?;
        let candidate = BftSignerCandidateV1 {
            history: snapshot.state.history.clone(),
            journal,
        };
        let next = validate_transition(&self.binding, &snapshot.state, &candidate, protected_now)?;
        let committed = self.persist_next_state(
            context,
            lease,
            token_digest,
            &snapshot,
            next,
            Some(protected_now),
        )?;
        Ok(state_reply(
            committed,
            BftSignerReplyKindV1::VoteSigned {
                vote: Box::new(vote),
            },
        ))
    }

    fn sign_intent_proposal(
        &self,
        intent: &BftProposalIntentV1,
    ) -> Result<BftProposalV1, BftSignerError> {
        let mut header = BftProposalHeaderV1 {
            signature_hex: String::new(),
            ..intent.header.clone()
        };
        header.public_key_hex = self.binding.local_signing_public_key.clone();
        let signing_bytes = header
            .signing_payload()
            .map_err(|error| invalid(format!("BFT proposal signing payload: {error}")))?;
        let signature = self.validator_key.sign_preimage(&signing_bytes)?;
        header.signature_hex = hex::encode(signature);
        header
            .verify_signature()
            .map_err(|error| invalid(format!("BFT proposal signature self-check: {error}")))?;
        let proposal = BftProposalV1 {
            header,
            block: intent.block.clone(),
            valid_round_evidence: intent.valid_round_evidence.clone(),
        };
        proposal
            .validate_binding()
            .map_err(|error| invalid(format!("BFT signed proposal binding: {error}")))?;
        Ok(proposal)
    }

    fn sign_intent_vote(&self, intent: &BftVoteIntentV1) -> Result<BftVoteV1, BftSignerError> {
        let mut vote = BftVoteV1 {
            protocol_version: oasis7_proto::distributed_bft::BFT_PROTOCOL_VERSION_V1,
            scope: self.binding.scope.clone(),
            height: intent.height,
            round: intent.round,
            phase: intent.phase,
            validator_set_hash: self.binding.validate()?.set_hash().to_string(),
            value: intent.value.clone(),
            validator_id: intent.validator_id.clone(),
            public_key_hex: self.binding.local_signing_public_key.clone(),
            signature_hex: String::new(),
        };
        let signing_bytes = vote
            .signing_payload()
            .map_err(|error| invalid(format!("BFT vote signing payload: {error}")))?;
        vote.signature_hex = hex::encode(self.validator_key.sign_preimage(&signing_bytes)?);
        vote.verify_signature()
            .map_err(|error| invalid(format!("BFT vote signature self-check: {error}")))?;
        Ok(vote)
    }

    fn persist_next_state(
        &self,
        context: &mut BftSignerMutationContextV1<'_>,
        lease: &BftSignerLeaseCredentialV1,
        token_digest: &str,
        previous: &BftSignerBackendSnapshotV1,
        next: BftSignerStateV1,
        minimum_event_time_ms: Option<i64>,
    ) -> Result<BftSignerBackendSnapshotV1, BftSignerError> {
        let snapshot = match context.backend.compare_and_swap(
            lease.generation,
            token_digest,
            previous.state.revision,
            &previous.state.state_digest,
            minimum_event_time_ms,
            next.clone(),
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                *context.poisoned = true;
                return Err(invalid(format!(
                    "BFT signer journal CAS outcome is uncertain; service poisoned: {error}"
                )));
            }
        };
        if let Err(error) = validate_backend_snapshot(&self.binding, &snapshot) {
            *context.poisoned = true;
            return Err(invalid(format!(
                "BFT signer journal CAS returned invalid state; service poisoned: {error}"
            )));
        }
        if snapshot.lease_generation != lease.generation
            || snapshot.lease_token_digest != token_digest
            || snapshot.protected_time_floor_ms < previous.protected_time_floor_ms
            || snapshot.state != next
        {
            *context.poisoned = true;
            return Err(invalid(
                "BFT signer journal CAS returned a different decision or regressed clock floor; service poisoned",
            ));
        }
        Ok(snapshot)
    }

    fn sign_reply(
        &self,
        request: &BftSignerRequestBodyV1,
        request_digest: &str,
        data: ReplyData,
    ) -> Result<Vec<u8>, BftSignerError> {
        let body = BftSignerReplyBodyV1 {
            schema_version: 1,
            binding_digest: self.binding.digest()?,
            request_digest: request_digest.to_string(),
            challenge: request.challenge.clone(),
            lease_generation: data.generation,
            lease_token: data.lease_token,
            state_revision: data.state_revision,
            state_digest: data.state_digest,
            state: data.state.map(Box::new),
            result: data.result,
        };
        let checkpoint_signature_hex = body
            .state
            .as_ref()
            .map(|state| {
                let checkpoint = CheckpointProofInput {
                    state,
                    lease_generation: body.lease_generation,
                    challenge: &body.challenge,
                    request_digest: &body.request_digest,
                };
                sign_body(
                    BFT_CHECKPOINT_DOMAIN_V1,
                    &checkpoint,
                    self.provider_key.as_ref(),
                )
            })
            .transpose()?;
        let provider_signature_hex =
            sign_body(BFT_REPLY_DOMAIN_V1, &body, self.provider_key.as_ref())?;
        encode_bounded(&SignedBftSignerReplyV1 {
            body,
            provider_signature_hex,
            checkpoint_signature_hex,
        })
    }
}

struct ReplyData {
    state: Option<BftSignerStateV1>,
    state_revision: u64,
    state_digest: String,
    generation: u64,
    lease_token: Option<String>,
    result: BftSignerReplyKindV1,
}

fn state_reply(snapshot: BftSignerBackendSnapshotV1, result: BftSignerReplyKindV1) -> ReplyData {
    ReplyData {
        state_revision: snapshot.state.revision,
        state_digest: snapshot.state.state_digest.clone(),
        state: Some(snapshot.state),
        generation: snapshot.lease_generation,
        lease_token: None,
        result,
    }
}

fn acquire_conflict_reply(snapshot: BftSignerBackendSnapshotV1) -> ReplyData {
    ReplyData {
        state: None,
        state_revision: snapshot.state.revision,
        state_digest: snapshot.state.state_digest,
        generation: snapshot.lease_generation,
        lease_token: None,
        result: BftSignerReplyKindV1::AcquireConflict {
            current_generation: snapshot.lease_generation,
            lease_active: !snapshot.lease_token_digest.is_empty(),
        },
    }
}

fn validate_acquired_snapshot(
    binding: &BftSignerBindingV1,
    previous: &BftSignerBackendSnapshotV1,
    acquired: &BftSignerBackendSnapshotV1,
    expected_generation: u64,
    expected_token_digest: &str,
) -> Result<(), BftSignerError> {
    if acquired.lease_generation != expected_generation
        || acquired.lease_token_digest != expected_token_digest
        || acquired.protected_time_floor_ms < previous.protected_time_floor_ms
        || acquired.state.revision < previous.state.revision
        || (acquired.state.revision == previous.state.revision && acquired.state != previous.state)
    {
        return Err(invalid(
            "acquired lease does not extend the pre-CAS backend snapshot",
        ));
    }
    validate_backend_snapshot(binding, acquired)
}

fn validate_request(
    binding: &BftSignerBindingV1,
    request: &SignedBftSignerRequestV1,
) -> Result<(), BftSignerError> {
    if request.body.schema_version != 1 || request.body.binding_digest != binding.digest()? {
        return Err(invalid("BFT signer request schema or binding mismatch"));
    }
    validate_nonce(&request.body.challenge)?;
    if request.authentication_public_key != binding.requester_auth_public_key {
        return Err(invalid("BFT signer requester authentication key mismatch"));
    }
    verify_signature(
        BFT_REQUEST_DOMAIN_V1,
        &request.body,
        &request.signature_hex,
        &request.authentication_public_key,
    )
}

fn validate_backend_snapshot(
    binding: &BftSignerBindingV1,
    snapshot: &BftSignerBackendSnapshotV1,
) -> Result<(), BftSignerError> {
    snapshot.state.validate(binding)?;
    let expected_digest = binding.digest()?;
    if snapshot.state.binding_digest != expected_digest || snapshot.protected_time_floor_ms < 0 {
        return Err(invalid("BFT signer backend state binding or clock floor"));
    }
    if snapshot.lease_generation == 0 {
        if !snapshot.lease_token_digest.is_empty() {
            return Err(invalid("unleased BFT signer state has a lease token"));
        }
    } else if !snapshot.lease_token_digest.is_empty() {
        validate_hex_digest(&snapshot.lease_token_digest)?;
    }
    let latest_event_time = snapshot
        .state
        .journal
        .events()
        .last()
        .map(|event| event.event_time_ms())
        .into_iter()
        .chain(snapshot.state.journal.height_started_at_ms())
        .max()
        .unwrap_or(0);
    if snapshot.protected_time_floor_ms < latest_event_time {
        return Err(invalid(
            "BFT signer clock floor is behind durable journal events",
        ));
    }
    Ok(())
}

fn validate_hex_digest(value: &str) -> Result<(), BftSignerError> {
    if value.len() != 64
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(invalid("BFT signer digest must be canonical lowercase hex"));
    }
    Ok(())
}

fn validate_lease_credential(lease: &BftSignerLeaseCredentialV1) -> Result<(), BftSignerError> {
    if lease.generation == 0 || lease.token.len() != 64 {
        return Err(invalid("BFT signer lease credential"));
    }
    if lease
        .token
        .bytes()
        .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(invalid("BFT signer lease token encoding"));
    }
    Ok(())
}

fn validate_nonce(nonce: &str) -> Result<(), BftSignerError> {
    if nonce.len() != 64
        || nonce
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(invalid(
            "BFT signer challenge must be 32-byte lowercase hex",
        ));
    }
    Ok(())
}

fn bounded_error(error: BftSignerError) -> String {
    const MAX_ERROR_BYTES: usize = 512;
    let message = error.to_string();
    if message.len() <= MAX_ERROR_BYTES {
        return message;
    }
    let mut end = MAX_ERROR_BYTES;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    message[..end].to_string()
}

fn random_token() -> Result<String, BftSignerError> {
    #[cfg(unix)]
    {
        let mut bytes = [0u8; 32];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(&mut bytes))
            .map_err(|error| invalid(format!("BFT signer secure random: {error}")))?;
        Ok(hex::encode(bytes))
    }
    #[cfg(not(unix))]
    {
        Err(invalid(
            "BFT protected signer requires a secure random source",
        ))
    }
}

fn sign_body<T: Serialize>(
    domain: &str,
    body: &T,
    key: &dyn BftSigningKeyHandleV1,
) -> Result<String, BftSignerError> {
    let preimage = framed_cbor(domain, body, BFT_SIGNER_MAX_SIGNING_BYTES_V1)?;
    Ok(hex::encode(key.sign_preimage(&preimage)?))
}

fn find_signed_proposal(
    journal: &oasis7_consensus::tendermint_v1::BftSigningJournalV1,
    intent: &BftProposalIntentV1,
) -> Result<Option<BftProposalV1>, BftSignerError> {
    for event in journal.events() {
        if let BftSigningJournalEventV1::Proposal {
            proposal,
            local: true,
            ..
        } = event
            && proposal.header.height == intent.header.height
            && proposal.header.round == intent.header.round
            && proposal.header.proposer_id == intent.header.proposer_id
        {
            let mut header = proposal.header.clone();
            header.signature_hex.clear();
            if header == intent.header
                && proposal.block == intent.block
                && proposal.valid_round_evidence == intent.valid_round_evidence
            {
                proposal
                    .header
                    .verify_signature()
                    .map_err(|error| invalid(format!("stored proposal signature: {error}")))?;
                return Ok(Some(proposal.as_ref().clone()));
            }
            return Err(invalid(
                "BFT signer proposal slot contains another decision",
            ));
        }
    }
    Ok(None)
}

fn find_signed_vote(
    journal: &oasis7_consensus::tendermint_v1::BftSigningJournalV1,
    intent: &BftVoteIntentV1,
) -> Result<Option<BftVoteV1>, BftSignerError> {
    for event in journal.events() {
        if let BftSigningJournalEventV1::Vote {
            vote, local: true, ..
        } = event
            && vote.height == intent.height
            && vote.round == intent.round
            && vote.phase == intent.phase
            && vote.validator_id == intent.validator_id
        {
            let mut unsigned = vote.clone();
            unsigned.signature_hex.clear();
            if unsigned.height == intent.height
                && unsigned.round == intent.round
                && unsigned.phase == intent.phase
                && unsigned.value == intent.value
                && unsigned.validator_id == intent.validator_id
            {
                vote.verify_signature()
                    .map_err(|error| invalid(format!("stored vote signature: {error}")))?;
                return Ok(Some(vote.as_ref().clone()));
            }
            return Err(invalid("BFT signer vote slot contains another decision"));
        }
    }
    Ok(None)
}

/// A test-only software implementation can wrap `SigningKey`; production
/// providers should implement the key-handle contract with protected custody.
#[cfg(test)]
pub(crate) struct SoftwareBftSigningKeyV1(SigningKey);

#[cfg(test)]
impl SoftwareBftSigningKeyV1 {
    pub(crate) fn new(key: SigningKey) -> Self {
        Self(key)
    }
}

#[cfg(test)]
impl BftSigningKeyHandleV1 for SoftwareBftSigningKeyV1 {
    fn public_key_hex(&self) -> String {
        hex::encode(self.0.verifying_key().to_bytes())
    }

    fn sign_preimage(&self, preimage: &[u8]) -> Result<[u8; 64], BftSignerError> {
        use ed25519_dalek::Signer;
        Ok(self.0.sign(preimage).to_bytes())
    }
}

#[cfg(test)]
fn _test_key_handle_is_only_a_wrapper(_: SigningKey) {}
