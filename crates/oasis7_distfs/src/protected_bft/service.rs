#[cfg(test)]
use std::sync::Arc;
use std::sync::Mutex;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use oasis7_consensus::tendermint_v1::{
    BftProposalIntentV1, BftVoteIntentV1, complete_reserved_bft_proposal_v1,
    complete_reserved_bft_vote_v1, replay_bft_signing_journal_v1,
};
use oasis7_proto::distributed_bft::{BftProposalHeaderV1, BftProposalV1, BftVoteV1};

use super::backend::{BftSignerBackendSnapshotV1, BftSignerBackendV1};
use super::binding::{
    BftSignerBindingV1, BftSignerError, BftSigningKeyHandleV1, decode_key, invalid,
};
use super::validation::{
    BftSignerCandidateV1, CheckpointProofInput, VerifiedBftSignerStateV1, candidate_event_time,
    validate_candidate_bounds, validate_height_timer_start_cached, validate_transition_cached,
};
use super::wire::{
    BFT_CHECKPOINT_DOMAIN_V1, BFT_REPLY_DOMAIN_V1, BFT_REQUEST_DIGEST_DOMAIN_V1,
    BftSignerCommandV1, BftSignerLeaseCredentialV1, BftSignerReplyBodyV1, BftSignerReplyKindV1,
    BftSignerRequestBodyV1, SignedBftSignerReplyV1, SignedBftSignerRequestV1, decode_bounded,
    digest_hex, encode_bounded,
};

#[path = "service_helpers.rs"]
mod helpers;
#[cfg(test)]
pub(crate) use helpers::SoftwareBftSigningKeyV1;
use helpers::{
    ReplyData, acquire_conflict_reply, bounded_error, find_signed_proposal, find_signed_vote,
    random_token, sign_body, state_reply, validate_lease_credential, validate_request,
};

#[path = "service_connection.rs"]
mod connection;
#[path = "service_snapshot_validation.rs"]
mod snapshot_validation;
use snapshot_validation::{
    validate_acquired_snapshot, validate_backend_snapshot, validate_backend_snapshot_against_cache,
    validate_monotonic_state_extension,
};

/// Callable BFT provider endpoint. The concrete backend must implement
/// transactionally durable generations, journal CAS, and protected time; this
/// service never provisions missing state or replaces a key.
pub struct BftSignerServiceV1 {
    binding: BftSignerBindingV1,
    provider_key: Box<dyn BftSigningKeyHandleV1>,
    validator_key: Box<dyn BftSigningKeyHandleV1>,
    runtime: Mutex<BftSignerServiceRuntimeV1>,
    #[cfg(test)]
    history_certificate_replays: Arc<AtomicUsize>,
}

struct BftSignerServiceRuntimeV1 {
    backend: Box<dyn BftSignerBackendV1>,
    poisoned: bool,
    verified: Option<VerifiedBftSignerStateV1>,
    active_lease: Option<ActiveLeaseV1>,
    observed_clock_floor_ms: i64,
}

struct ActiveLeaseV1 {
    generation: u64,
    token_digest: String,
}

impl PartialEq for ActiveLeaseV1 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && self.token_digest == other.token_digest
    }
}

impl Eq for ActiveLeaseV1 {}

struct BftSignerMutationContextV1<'a> {
    backend: &'a mut dyn BftSignerBackendV1,
    poisoned: &'a mut bool,
    verified: &'a mut Option<VerifiedBftSignerStateV1>,
    observed_clock_floor_ms: &'a mut i64,
}

impl BftSignerMutationContextV1<'_> {
    fn protected_time_ms(
        &mut self,
        lease: &BftSignerLeaseCredentialV1,
        token_digest: &str,
    ) -> Result<i64, BftSignerError> {
        let now = self
            .backend
            .protected_time_ms(lease.generation, token_digest)?;
        if now < *self.observed_clock_floor_ms || now < 0 {
            *self.poisoned = true;
            return Err(invalid("BFT signer protected clock rollback"));
        }
        *self.observed_clock_floor_ms = now;
        Ok(now)
    }

    fn verified_state(&self) -> Result<&VerifiedBftSignerStateV1, BftSignerError> {
        self.verified
            .as_ref()
            .ok_or_else(|| invalid("BFT signer verified state cache is unavailable"))
    }
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
        let schedule = validate_backend_snapshot(&binding, &snapshot)?;
        #[cfg(test)]
        let history_certificate_replays = Arc::new(AtomicUsize::new(snapshot.state.history.len()));
        Ok(Self {
            binding,
            provider_key,
            validator_key,
            runtime: Mutex::new(BftSignerServiceRuntimeV1 {
                backend,
                poisoned: false,
                verified: Some(VerifiedBftSignerStateV1 {
                    state: snapshot.state,
                    schedule,
                }),
                active_lease: None,
                observed_clock_floor_ms: snapshot.protected_time_floor_ms,
            }),
            #[cfg(test)]
            history_certificate_replays,
        })
    }

    fn validate_backend_snapshot(
        &self,
        snapshot: &BftSignerBackendSnapshotV1,
    ) -> Result<oasis7_consensus::tendermint_v1::BftHeightProposerScheduleV1, BftSignerError> {
        let schedule = validate_backend_snapshot(&self.binding, snapshot)?;
        #[cfg(test)]
        self.history_certificate_replays
            .fetch_add(snapshot.state.history.len(), Ordering::Relaxed);
        Ok(schedule)
    }

    fn validate_backend_snapshot_extending(
        &self,
        snapshot: &BftSignerBackendSnapshotV1,
        previous: &VerifiedBftSignerStateV1,
    ) -> Result<VerifiedBftSignerStateV1, BftSignerError> {
        let schedule = self.validate_backend_snapshot(snapshot)?;
        let next = VerifiedBftSignerStateV1 {
            state: snapshot.state.clone(),
            schedule,
        };
        validate_monotonic_state_extension(&self.binding, previous, &next)?;
        Ok(next)
    }

    #[cfg(test)]
    pub(crate) fn history_certificate_replay_counter(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.history_certificate_replays)
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
        let reply = self.process_command(&mut runtime, session, &request.body);
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

    fn process_command(
        &self,
        runtime: &mut BftSignerServiceRuntimeV1,
        session: &mut BftSignerSessionV1,
        request: &BftSignerRequestBodyV1,
    ) -> Result<ReplyData, BftSignerError> {
        let BftSignerServiceRuntimeV1 {
            backend,
            poisoned,
            verified,
            active_lease,
            observed_clock_floor_ms,
        } = runtime;
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
                let previous_verified = verified
                    .as_ref()
                    .ok_or_else(|| invalid("BFT signer verified state cache is unavailable"))?;
                let next_verified =
                    match self.validate_backend_snapshot_extending(&snapshot, previous_verified) {
                        Ok(next) => next,
                        Err(error) => {
                            *poisoned = true;
                            return Err(invalid(format!(
                                "BFT signer reacquire snapshot is not a verified extension: {error}"
                            )));
                        }
                    };
                if snapshot.protected_time_floor_ms < *observed_clock_floor_ms {
                    *poisoned = true;
                    return Err(invalid("BFT signer backend clock floor rollback"));
                }
                *observed_clock_floor_ms = snapshot.protected_time_floor_ms;
                *verified = Some(next_verified);
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
                let cached_before_acquire = verified
                    .as_ref()
                    .cloned()
                    .ok_or_else(|| invalid("BFT signer verified state cache is unavailable"))?;
                let previous = backend.load_provisioned(&self.binding.digest()?)?;
                let previous_verified = match self
                    .validate_backend_snapshot_extending(&previous, &cached_before_acquire)
                {
                    Ok(next) => next,
                    Err(error) => {
                        *poisoned = true;
                        return Err(invalid(format!(
                            "BFT signer reacquire snapshot is not a verified extension: {error}"
                        )));
                    }
                };
                if previous.protected_time_floor_ms < *observed_clock_floor_ms {
                    *poisoned = true;
                    return Err(invalid("BFT signer backend clock floor rollback"));
                }
                *observed_clock_floor_ms = previous.protected_time_floor_ms;
                *verified = Some(previous_verified);
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
                        let current_verified = match self.validate_backend_snapshot_extending(
                            &current,
                            verified.as_ref().ok_or_else(|| {
                                invalid("BFT signer verified state cache is unavailable")
                            })?,
                        ) {
                            Ok(next) => next,
                            Err(validation_error) => {
                                *poisoned = true;
                                return Err(invalid(format!(
                                    "BFT signer acquire outcome is uncertain; reloaded state is invalid: {validation_error}"
                                )));
                            }
                        };
                        if current.protected_time_floor_ms < *observed_clock_floor_ms {
                            *poisoned = true;
                            return Err(invalid("BFT signer backend clock floor rollback"));
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
                                    *verified = Some(current_verified);
                                    *observed_clock_floor_ms = current.protected_time_floor_ms;
                                    current
                                } else {
                                    *poisoned = true;
                                    return Err(invalid(
                                        "BFT signer acquire outcome is uncertain; committed lease snapshot differs",
                                    ));
                                }
                            } else {
                                *verified = Some(current_verified);
                                *observed_clock_floor_ms = current.protected_time_floor_ms;
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
                let next_verified = match self.validate_backend_snapshot_extending(
                    &snapshot,
                    verified
                        .as_ref()
                        .ok_or_else(|| invalid("BFT signer verified state cache is unavailable"))?,
                ) {
                    Ok(next) => next,
                    Err(error) => {
                        *poisoned = true;
                        return Err(invalid(format!(
                            "BFT signer acquire outcome is uncertain; acquired snapshot is invalid: {error}"
                        )));
                    }
                };
                if snapshot.protected_time_floor_ms < *observed_clock_floor_ms {
                    *poisoned = true;
                    return Err(invalid("BFT signer backend clock floor rollback"));
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
                *verified = Some(next_verified);
                *active_lease = Some(ActiveLeaseV1 {
                    generation: snapshot.lease_generation,
                    token_digest: lease_token_digest,
                });
                *observed_clock_floor_ms = snapshot.protected_time_floor_ms;
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
                if *active_lease
                    != Some(ActiveLeaseV1 {
                        generation: lease.generation,
                        token_digest: token_digest.clone(),
                    })
                {
                    return Err(invalid(
                        "BFT signer lease cache is not bound to this session",
                    ));
                }
                let cached = verified
                    .as_ref()
                    .ok_or_else(|| invalid("BFT signer verified state cache is unavailable"))?;
                if matches!(&request.command, BftSignerCommandV1::ProtectedTime) {
                    let now = backend.protected_time_ms(lease.generation, &token_digest)?;
                    if now < *observed_clock_floor_ms || now < 0 {
                        *poisoned = true;
                        return Err(invalid("BFT signer protected clock rollback"));
                    }
                    *observed_clock_floor_ms = now;
                    return Ok(ReplyData {
                        state: None,
                        state_revision: cached.state.revision,
                        state_digest: cached.state.state_digest.clone(),
                        generation: lease.generation,
                        lease_token: None,
                        result: BftSignerReplyKindV1::ProtectedTime { time_ms: now },
                    });
                }
                let snapshot = backend.read_for_lease(lease.generation, &token_digest)?;
                if let Err(error) = validate_backend_snapshot_against_cache(
                    &self.binding,
                    &snapshot,
                    cached,
                    lease.generation,
                    &token_digest,
                    *observed_clock_floor_ms,
                ) {
                    *poisoned = true;
                    return Err(error);
                }
                if snapshot.lease_generation != lease.generation
                    || snapshot.lease_token_digest != token_digest
                {
                    return Err(invalid("BFT signer lease was fenced"));
                }
                self.process_leased_command(
                    &mut BftSignerMutationContextV1 {
                        backend: backend.as_mut(),
                        poisoned,
                        verified,
                        observed_clock_floor_ms,
                    },
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
        context: &mut BftSignerMutationContextV1<'_>,
        lease: &BftSignerLeaseCredentialV1,
        token_digest: &str,
        snapshot: BftSignerBackendSnapshotV1,
        command: &BftSignerCommandV1,
    ) -> Result<ReplyData, BftSignerError> {
        match command {
            BftSignerCommandV1::PrepareAcquire { .. } => {
                Err(invalid("BFT signer prepare cannot use an active lease"))
            }
            BftSignerCommandV1::Checkpoint => {
                Ok(state_reply(snapshot, BftSignerReplyKindV1::Checkpoint))
            }
            BftSignerCommandV1::ProtectedTime => Err(invalid(
                "BFT protected time must use the verified-state fast path",
            )),
            BftSignerCommandV1::StartHeightTimer { expected_revision } => {
                self.start_height_timer(context, lease, token_digest, snapshot, *expected_revision)
            }
            BftSignerCommandV1::Advance { .. } => {
                let next = self.advance_state(context, lease, token_digest, &snapshot, command)?;
                Ok(state_reply(next, BftSignerReplyKindV1::Advanced))
            }
            BftSignerCommandV1::SignProposal {
                expected_revision,
                intent,
            } => self.sign_proposal(
                context,
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
                context,
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
        let verified = context.verified_state()?.clone();
        if verified.state != previous.state {
            *context.poisoned = true;
            return Err(invalid(
                "BFT signer cached state differs from lease snapshot",
            ));
        }
        let protected_now = context.protected_time_ms(lease, token_digest)?;
        let next = validate_height_timer_start_cached(&self.binding, &verified, protected_now)?;
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
        let verified = context.verified_state()?.clone();
        if verified.state != previous.state {
            *context.poisoned = true;
            return Err(invalid(
                "BFT signer cached state differs from lease snapshot",
            ));
        }
        let protected_now = context.protected_time_ms(lease, token_digest)?;
        let next = validate_transition_cached(&self.binding, &verified, candidate, protected_now)?;
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
        let verified = context.verified_state()?.clone();
        if verified.state != snapshot.state {
            *context.poisoned = true;
            return Err(invalid(
                "BFT signer cached state differs from lease snapshot",
            ));
        }
        let protected_now = context.protected_time_ms(lease, token_digest)?;
        let set = self.binding.validate()?;
        let schedule = &verified.schedule;
        let machine = replay_bft_signing_journal_v1(
            &snapshot.state.journal,
            &set,
            schedule,
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
            schedule,
            self.binding.timeouts,
        )
        .map_err(|error| invalid(format!("complete proposal reservation: {error}")))?;
        let candidate = BftSignerCandidateV1 {
            history: snapshot.state.history.clone(),
            journal,
        };
        let next = validate_transition_cached(&self.binding, &verified, &candidate, protected_now)?;
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
        let verified = context.verified_state()?.clone();
        if verified.state != snapshot.state {
            *context.poisoned = true;
            return Err(invalid(
                "BFT signer cached state differs from lease snapshot",
            ));
        }
        let protected_now = context.protected_time_ms(lease, token_digest)?;
        let set = self.binding.validate()?;
        let schedule = &verified.schedule;
        let machine = replay_bft_signing_journal_v1(
            &snapshot.state.journal,
            &set,
            schedule,
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
            schedule,
            self.binding.timeouts,
        )
        .map_err(|error| invalid(format!("complete vote reservation: {error}")))?;
        let candidate = BftSignerCandidateV1 {
            history: snapshot.state.history.clone(),
            journal,
        };
        let next = validate_transition_cached(&self.binding, &verified, &candidate, protected_now)?;
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
        next: VerifiedBftSignerStateV1,
        minimum_event_time_ms: Option<i64>,
    ) -> Result<BftSignerBackendSnapshotV1, BftSignerError> {
        let previous_verified = context.verified_state()?.clone();
        if previous_verified.state != previous.state {
            *context.poisoned = true;
            return Err(invalid(
                "BFT signer cached state differs before journal CAS",
            ));
        }
        let snapshot = match context.backend.compare_and_swap(
            lease.generation,
            token_digest,
            previous.state.revision,
            &previous.state.state_digest,
            minimum_event_time_ms,
            next.state.clone(),
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                *context.poisoned = true;
                return Err(invalid(format!(
                    "BFT signer journal CAS outcome is uncertain; service poisoned: {error}"
                )));
            }
        };
        if let Err(error) = validate_backend_snapshot_against_cache(
            &self.binding,
            &snapshot,
            &next,
            lease.generation,
            token_digest,
            *context.observed_clock_floor_ms,
        ) {
            *context.poisoned = true;
            return Err(invalid(format!(
                "BFT signer journal CAS returned invalid state; service poisoned: {error}"
            )));
        }
        if snapshot.lease_generation != lease.generation
            || snapshot.lease_token_digest != token_digest
            || snapshot.protected_time_floor_ms < previous.protected_time_floor_ms
            || snapshot.protected_time_floor_ms < *context.observed_clock_floor_ms
            || snapshot.state != next.state
        {
            *context.poisoned = true;
            return Err(invalid(
                "BFT signer journal CAS returned a different decision or regressed clock floor; service poisoned",
            ));
        }
        *context.observed_clock_floor_ms = snapshot.protected_time_floor_ms;
        *context.verified = Some(next);
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
